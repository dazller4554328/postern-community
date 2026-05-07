//! Orchestration layer that sits above the per-driver primitives in
//! `crate::backup` (local), `crate::backup_destinations` (SFTP) and
//! `crate::gdrive` (Google Drive).
//!
//! Three responsibilities live here:
//!
//!   - [`run_backup`] is the single source of truth for "make a
//!     backup, update the job registry, log the event, prune
//!     retention, then auto-push to every enabled off-site
//!     destination". Both the manual `/backups/create` handler and
//!     the periodic scheduler tick call it.
//!
//!   - [`push_one`] / [`test_one`] dispatch by `dest.kind` and hide
//!     the credential-loading / token-refresh / TOFU-pin plumbing
//!     from HTTP handlers. Returning typed [`PushOutcome`] /
//!     [`TestOutcome`] keeps the JSON-shaping in the handler where it
//!     belongs.
//!
//!   - [`push_to_all_enabled`] fans out a finished backup to every
//!     enabled destination row, swallowing per-destination failures
//!     so one broken SFTP server can't keep the rest from receiving
//!     copies.
//!
//! Side-effect contract: these functions persist what they did to
//! the database (push-ok marker, refreshed tokens, pinned host key)
//! but leave `log_event` calls to the caller — manual operator
//! actions and auto-push fan-out want different event messages.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    backup::{self, BackupJob, BackupJobs, BackupReport},
    backup_destinations as sftp_driver,
    error::{Error, Result},
    gdrive,
    storage::{BackupDestination, Db, GDrivePublicConfig, SftpPublicConfig},
    vault::Vault,
};

/// Result of a successful push to a single destination. Lets HTTP
/// handlers render whichever subset of fields they want without the
/// orchestrator having to know about JSON shapes.
pub struct PushOutcome {
    /// Stable string identifier shown back to the operator.
    /// SFTP: absolute remote path. GDrive: `drive:<file_id>`.
    pub remote_ref: String,
    /// Set on SFTP pushes. GDrive's chunked-upload protocol doesn't
    /// give us a count back from the final commit.
    pub bytes_uploaded: Option<u64>,
    /// Set on GDrive uploads.
    pub gdrive_file_id: Option<String>,
    /// Set when this push pinned a host key for the first time
    /// (i.e. the destination row had no fingerprint yet).
    pub pinned_fingerprint: Option<String>,
}

/// Result of a successful destination probe. Per-kind so callers can
/// surface kind-specific data (fingerprint, account email) without
/// the orchestrator imposing a lowest-common-denominator shape.
pub enum TestOutcome {
    Sftp { fingerprint: String, first_use: bool },
    Gdrive { account_email: String, folder_name: String },
}

/// Push a tarball to a single destination, dispatching by kind. On
/// success persists `record_destination_push_ok` and any token
/// refresh / first-use fingerprint pin. Errors propagate; callers
/// decide whether to log_event and whether to call
/// `record_destination_push_err`.
pub async fn push_one(
    db: &Db,
    vault: &Vault,
    bind_iface: Option<&str>,
    dest: &BackupDestination,
    local_path: &Path,
    filename: &str,
) -> Result<PushOutcome> {
    match dest.kind.as_str() {
        "sftp" => push_sftp(db, vault, dest, local_path, filename).await,
        "gdrive" => push_gdrive(db, vault, bind_iface, dest, local_path, filename).await,
        other => Err(Error::BadRequest(format!(
            "unknown destination kind: {other}"
        ))),
    }
}

async fn push_sftp(
    db: &Db,
    vault: &Vault,
    dest: &BackupDestination,
    local_path: &Path,
    filename: &str,
) -> Result<PushOutcome> {
    let public: SftpPublicConfig = serde_json::from_value(dest.public_config.clone())
        .map_err(|e| Error::Other(anyhow::anyhow!("decode sftp public: {e}")))?;
    let credential = db.get_sftp_credential(dest.id, vault)?;
    let (outcome, fingerprint) = sftp_driver::push(
        &public,
        &credential,
        local_path,
        filename,
        dest.server_fingerprint.as_deref(),
    )
    .await?;
    let pinned = if dest.server_fingerprint.is_none() {
        db.set_backup_destination_fingerprint(dest.id, &fingerprint)?;
        Some(fingerprint)
    } else {
        None
    };
    db.record_destination_push_ok(dest.id, &outcome.remote_path)?;
    Ok(PushOutcome {
        remote_ref: outcome.remote_path,
        bytes_uploaded: Some(outcome.bytes_uploaded),
        gdrive_file_id: None,
        pinned_fingerprint: pinned,
    })
}

async fn push_gdrive(
    db: &Db,
    vault: &Vault,
    bind_iface: Option<&str>,
    dest: &BackupDestination,
    local_path: &Path,
    filename: &str,
) -> Result<PushOutcome> {
    let cfg = gdrive::OauthConfig::from_env()
        .ok_or_else(|| Error::BadRequest("Google Drive OAuth client not configured".into()))?;
    let public: GDrivePublicConfig = serde_json::from_value(dest.public_config.clone())
        .map_err(|e| Error::Other(anyhow::anyhow!("decode gdrive public: {e}")))?;
    let mut credential = db.get_gdrive_credential(dest.id, vault)?;
    let refreshed = gdrive::refresh_if_expiring(&cfg, &mut credential, bind_iface).await?;
    if refreshed {
        let blob = serde_json::to_vec(&credential)
            .map_err(|e| Error::Other(anyhow::anyhow!("encode credential: {e}")))?;
        db.update_destination_credential(dest.id, &blob, vault)?;
    }
    let file_id = gdrive::upload_tarball(
        &credential.access_token,
        &public,
        local_path,
        filename,
        bind_iface,
    )
    .await?;
    let remote_ref = format!("drive:{file_id}");
    db.record_destination_push_ok(dest.id, &remote_ref)?;
    Ok(PushOutcome {
        remote_ref,
        bytes_uploaded: None,
        gdrive_file_id: Some(file_id),
        pinned_fingerprint: None,
    })
}

/// Probe a destination's reachability + credentials. Persists token
/// refresh and TOFU-pin side-effects.
pub async fn test_one(
    db: &Db,
    vault: &Vault,
    bind_iface: Option<&str>,
    dest: &BackupDestination,
) -> Result<TestOutcome> {
    match dest.kind.as_str() {
        "sftp" => {
            let public: SftpPublicConfig = serde_json::from_value(dest.public_config.clone())
                .map_err(|e| Error::Other(anyhow::anyhow!("decode sftp public: {e}")))?;
            let credential = db.get_sftp_credential(dest.id, vault)?;
            let probe =
                sftp_driver::test(&public, &credential, dest.server_fingerprint.as_deref())
                    .await?;
            let first_use = dest.server_fingerprint.is_none();
            if first_use {
                db.set_backup_destination_fingerprint(dest.id, &probe.fingerprint)?;
            }
            Ok(TestOutcome::Sftp {
                fingerprint: probe.fingerprint,
                first_use,
            })
        }
        "gdrive" => {
            let cfg = gdrive::OauthConfig::from_env().ok_or_else(|| {
                Error::BadRequest("Google Drive OAuth client not configured".into())
            })?;
            let public: GDrivePublicConfig = serde_json::from_value(dest.public_config.clone())
                .map_err(|e| Error::Other(anyhow::anyhow!("decode gdrive public: {e}")))?;
            let mut credential = db.get_gdrive_credential(dest.id, vault)?;
            // refresh-if-needed lives inside gdrive::test; persist
            // the new tokens here so a stale refresh isn't repeated.
            let before_expiry = credential.expires_at;
            gdrive::test(&cfg, &mut credential, &public, bind_iface).await?;
            if credential.expires_at != before_expiry {
                let blob = serde_json::to_vec(&credential)
                    .map_err(|e| Error::Other(anyhow::anyhow!("encode credential: {e}")))?;
                db.update_destination_credential(dest.id, &blob, vault)?;
            }
            Ok(TestOutcome::Gdrive {
                account_email: public.account_email,
                folder_name: public.folder_name,
            })
        }
        other => Err(Error::BadRequest(format!("unknown kind {other}"))),
    }
}

/// Auto-push a finished backup to every enabled destination. Per-
/// destination failures are recorded and logged but never propagated
/// — the local tarball is the primary artefact and one broken
/// off-site target shouldn't fail the whole backup.
pub async fn push_to_all_enabled(
    db: &Db,
    vault: &Vault,
    bind_iface: Option<&str>,
    report: &BackupReport,
) {
    let destinations = match db.list_enabled_backup_destinations() {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(error = %e, "backup: list destinations failed (skipping push)");
            return;
        }
    };
    let local_path = PathBuf::from(&report.path);
    for dest in destinations {
        match push_one(db, vault, bind_iface, &dest, &local_path, &report.filename).await {
            Ok(_) => tracing::info!(
                destination_id = dest.id,
                label = %dest.label,
                "backup: pushed to off-site destination"
            ),
            Err(e) => {
                tracing::warn!(
                    destination_id = dest.id,
                    label = %dest.label,
                    error = %e,
                    "backup: off-site push failed"
                );
                let _ = db.record_destination_push_err(dest.id, &e.to_string());
                let _ = db.log_event(
                    "backup_destination_push_failed",
                    Some(&format!("{}: {e}", dest.label)),
                    None,
                );
            }
        }
    }
}

/// Single source of truth for "create a local backup, log it, prune
/// to retention, auto-push to enabled destinations".
///
/// `note` is appended to the success log_event so manual and
/// scheduled runs are distinguishable in the audit log. Use `""` for
/// manual runs and `"scheduled"` for the scheduler.
///
/// CPU-bound steps (tarball compression, retention prune) run on the
/// blocking pool; the auto-push fan-out runs on the async runtime.
pub async fn run_backup(
    db: Arc<Db>,
    vault: Vault,
    jobs: BackupJobs,
    bind_iface: Option<String>,
    data_dir: PathBuf,
    backup_dir: PathBuf,
    note: &'static str,
) {
    let report = {
        // Clone the Arc<Db> so the `move` closure owns one handle and
        // the outer `db` stays usable for error logging below.
        let db_for_blocking = db.clone();
        let data_dir_for_blocking = data_dir.clone();
        let backup_dir_for_blocking = backup_dir.clone();
        match tokio::task::spawn_blocking(move || {
            backup::create_backup(
                &db_for_blocking,
                &data_dir_for_blocking,
                &backup_dir_for_blocking,
            )
        })
        .await
        {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                tracing::error!(error = %e, "backup: failed");
                let _ = db.log_event("backup_failed", Some(&e.to_string()), None);
                let mut job = BackupJob::running();
                job.finish_failed(e.to_string());
                jobs.set(job);
                return;
            }
            Err(join_err) => {
                let msg = format!("backup task panicked: {join_err}");
                tracing::error!("{msg}");
                let _ = db.log_event("backup_failed", Some(&msg), None);
                let mut job = BackupJob::running();
                job.finish_failed(msg);
                jobs.set(job);
                return;
            }
        }
    };

    let detail = if note.is_empty() {
        format!(
            "{} ({}MB)",
            report.filename,
            report.size_bytes / (1024 * 1024)
        )
    } else {
        format!(
            "{} ({}MB) — {note}",
            report.filename,
            report.size_bytes / (1024 * 1024)
        )
    };
    let _ = db.log_event("backup_created", Some(&detail), None);

    let mut job = BackupJob::running();
    job.finish_success(report.clone());
    jobs.set(job);

    // Retention prune before push, so we don't ship tarballs we're
    // about to delete.
    if let Ok(s) = db.get_backup_schedule() {
        if s.retention_count > 0 {
            let backup_dir = backup_dir.clone();
            let keep = s.retention_count as usize;
            let _ = tokio::task::spawn_blocking(move || {
                if let Err(e) = backup::prune_to_keep(&backup_dir, keep) {
                    tracing::warn!(error = %e, "retention prune failed (continuing)");
                }
            })
            .await;
        }
    }

    push_to_all_enabled(&db, &vault, bind_iface.as_deref(), &report).await;
}
