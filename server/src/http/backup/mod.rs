use std::path::PathBuf;

use tokio::io::AsyncWriteExt;

use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, State},
    routing::{get, post},
    Json, Router,
};

use axum::body::Body;
use axum::http::header;
use axum::response::Response;
use tokio_util::io::ReaderStream;

use super::AppState;
use crate::{
    backup::{
        self, gdrive,
        orchestrator::{self},
        restore::{self, ValidationSummary},
        BackupJob, BackupReport,
    },
    error::{Error, Result},
    storage::{BackupSchedule, UpdateBackupSchedule},
};
mod destinations;
mod gdrive_oauth;

/// Cap multipart uploads at 10 GiB. Mailbox tarballs run 50–500 MB
/// for typical users; the cap is here so a runaway client can't
/// fill the disk, not as a feature limit.
const RESTORE_UPLOAD_LIMIT: usize = 10 * 1024 * 1024 * 1024;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/backups", get(list))
        .route("/backups/create", post(create))
        .route("/backups/status", get(status))
        .route("/backups/:filename", axum::routing::delete(delete))
        .route("/backups/:filename/download", get(download))
        .route(
            "/backups/restore/upload",
            post(restore_upload).layer(DefaultBodyLimit::max(RESTORE_UPLOAD_LIMIT)),
        )
        .route(
            "/backups/restore/from-existing",
            post(restore_from_existing),
        )
        .route("/backups/restore/from-gdrive", post(restore_from_gdrive))
        .route("/backups/restore/validate", post(restore_validate))
        .route("/backups/restore/apply", post(restore_apply))
        .route(
            "/backups/restore/staging/:id",
            axum::routing::delete(restore_cancel),
        )
        // Off-site destinations.
        .route(
            "/backups/destinations",
            get(destinations::list_destinations).post(destinations::create_destination),
        )
        .route(
            "/backups/destinations/:id",
            axum::routing::patch(destinations::update_destination)
                .delete(destinations::delete_destination),
        )
        .route(
            "/backups/destinations/:id/test",
            post(destinations::test_destination),
        )
        .route(
            "/backups/destinations/:id/push",
            post(destinations::push_destination),
        )
        .route(
            "/backups/destinations/:id/cloud-backups",
            get(list_cloud_backups),
        )
        .route(
            "/backups/destinations/:id/forget-fingerprint",
            post(destinations::forget_destination_fingerprint),
        )
        // Schedule.
        .route("/backups/schedule", get(get_schedule).post(set_schedule))
        // GDrive OAuth.
        .route("/backups/integrations", get(gdrive_oauth::integrations))
        .route(
            "/backups/destinations/oauth/google/start",
            get(gdrive_oauth::gdrive_oauth_start),
        )
        .route(
            "/backups/oauth/google/callback",
            get(gdrive_oauth::gdrive_oauth_callback),
        )
}

pub(super) fn dirs(_s: &AppState) -> (PathBuf, PathBuf) {
    let data_dir = std::env::var("POSTERN_DATA_DIR")
        .map_or_else(|_| PathBuf::from("/var/lib/postern/data"), PathBuf::from);
    let backup_dir = std::env::var("POSTERN_BACKUP_DIR")
        .map_or_else(|_| PathBuf::from("/var/lib/postern/backups"), PathBuf::from);
    (data_dir, backup_dir)
}

async fn list(State(s): State<AppState>) -> Result<Json<Vec<BackupReport>>> {
    let (_, backup_dir) = dirs(&s);
    Ok(Json(backup::list_backups(&backup_dir)?))
}

/// Kick off a backup job and return immediately. Backups can take
/// well over a minute on real mailboxes (VACUUM INTO + blob copy +
/// gzip), and any client behind Cloudflare's default 100s gateway
/// timeout — i.e. anyone hitting the public hostname through a free-
/// plan tunnel — would see a 524 long before the work finishes. The
/// HTTP request returns in milliseconds; the UI polls
/// `GET /backups/status` for progress.
async fn create(State(s): State<AppState>) -> Result<Json<BackupJob>> {
    s.vault.require_unlocked()?;
    if s.backup_jobs.is_running() {
        return Err(Error::BadRequest(
            "a backup is already running — wait for it to finish".into(),
        ));
    }
    let initial = BackupJob::running();
    s.backup_jobs.set(initial.clone());

    // Snapshot VPN state at job-start time. Same pattern as the IMAP
    // sync scheduler — if the kill-switch is engaged we route off-
    // site uploads through wg0, otherwise via the default route.
    let bind_iface = s.vpn.bind_iface();
    let (data_dir, backup_dir) = dirs(&s);
    let db = s.db.clone();
    let vault = s.vault.clone();
    let jobs = s.backup_jobs.clone();
    tokio::spawn(orchestrator::run_backup(
        db, vault, jobs, bind_iface, data_dir, backup_dir, "",
    ));

    Ok(Json(initial))
}

async fn status(State(s): State<AppState>) -> Result<Json<Option<BackupJob>>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.backup_jobs.current()))
}

async fn delete(
    State(s): State<AppState>,
    Path(filename): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let (_, backup_dir) = dirs(&s);
    backup::delete_backup(&backup_dir, &filename)?;
    Ok(Json(serde_json::json!({ "deleted": filename })))
}

// ---------------------------------------------------------------------
// Restore flow — see crate::backup::restore for the storage-layer details.
//
// Three steps the UI walks the user through:
//   1. POST /backups/restore/upload      — multipart upload; returns staging_id
//   2. POST /backups/restore/validate    — { staging_id, password }; counts rows
//   3. POST /backups/restore/apply       — { staging_id }; sets marker, exits
//
// Plus DELETE /backups/restore/staging/:id to discard a staged upload.
//
// All four require the vault to be unlocked, i.e. the user is logged
// into the *current* install. The password supplied to /validate is
// the master password from the *backup* — these are independent and
// can differ (e.g. when restoring an old backup onto a fresh install).
// ---------------------------------------------------------------------

async fn restore_upload(
    State(s): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let (data_dir, _) = dirs(&s);

    let (staging_id, tarball_path) = restore::prepare_staging(&data_dir)?;

    // Stream the first file part to disk.
    let mut written = 0u64;
    let mut got_file = false;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| Error::BadRequest(format!("multipart: {e}")))?
    {
        if got_file {
            // Caller sent extra parts — ignore.
            continue;
        }
        got_file = true;
        // Tokio's async fs API — std::fs::File::create + write_all
        // would block the runtime thread for the duration of a multi-
        // hundred-MB upload, stalling every other request behind it.
        let mut file = tokio::fs::File::create(&tarball_path)
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("create tarball: {e}")))?;
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| Error::BadRequest(format!("chunk: {e}")))?
        {
            file.write_all(&chunk)
                .await
                .map_err(|e| Error::Other(anyhow::anyhow!("write tarball: {e}")))?;
            written += chunk.len() as u64;
        }
        file.flush()
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("flush tarball: {e}")))?;
    }

    if !got_file {
        let _ = restore::discard_staging(&data_dir, &staging_id);
        return Err(Error::BadRequest(
            "no file in multipart upload — pick a backup file to restore".into(),
        ));
    }

    Ok(Json(serde_json::json!({
        "staging_id": staging_id,
        "size_bytes": written,
    })))
}

#[derive(serde::Deserialize)]
struct FromExistingBody {
    /// Bare filename from the backups list (e.g.
    /// `postern-backup-20260425-130000.tar.gz`). Path-traversal
    /// rejected by `restore::stage_existing_backup`.
    filename: String,
}

async fn restore_from_existing(
    State(s): State<AppState>,
    Json(b): Json<FromExistingBody>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let (data_dir, backup_dir) = dirs(&s);

    // Filesystem work (hard-link or copy of a multi-hundred-MB file)
    // off the request thread.
    let (staging_id, _) = tokio::task::spawn_blocking(move || {
        restore::stage_existing_backup(&data_dir, &backup_dir, &b.filename)
    })
    .await
    .map_err(|e| Error::Other(anyhow::anyhow!("join: {e}")))??;

    Ok(Json(serde_json::json!({ "staging_id": staging_id })))
}

/// List the backup tarballs already sitting in a Google Drive
/// destination's folder, so a user on a fresh install can restore one
/// without first downloading the (multi-GB) file to their browser.
async fn list_cloud_backups(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<gdrive::DriveBackup>>> {
    s.vault.require_unlocked()?;
    let dest = s.db.get_backup_destination(id)?;
    let bind_iface = s.vpn.bind_iface();
    let list =
        orchestrator::list_cloud_backups(&s.db, &s.vault, bind_iface.as_deref(), &dest).await?;
    Ok(Json(list))
}

#[derive(serde::Deserialize)]
struct FromGdriveBody {
    destination_id: i64,
    file_id: String,
}

/// Stage a backup straight from Google Drive: the server downloads the
/// chosen file into the restore staging dir (fast server-side pipe, no
/// browser round-trip), then the normal validate → apply flow takes
/// over with the returned `staging_id`. This can take a while for a
/// multi-GB tarball; the request stays open for the duration, same as
/// the upload and validate steps.
async fn restore_from_gdrive(
    State(s): State<AppState>,
    Json(b): Json<FromGdriveBody>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let (data_dir, _) = dirs(&s);
    let dest = s.db.get_backup_destination(b.destination_id)?;
    let bind_iface = s.vpn.bind_iface();

    // Reuse the same staging the upload path uses, so validate/apply
    // are unchanged — we just fill the tarball from Drive instead of a
    // multipart body.
    let (staging_id, tarball) = restore::prepare_staging(&data_dir)?;
    let bytes = match orchestrator::download_cloud_backup(
        &s.db,
        &s.vault,
        bind_iface.as_deref(),
        &dest,
        &b.file_id,
        &tarball,
    )
    .await
    {
        Ok(n) => n,
        Err(e) => {
            // Don't leave a half-written staging dir behind.
            let _ = restore::discard_staging(&data_dir, &staging_id);
            return Err(e);
        }
    };

    let _ = s.db.log_event(
        "restore_staged_from_gdrive",
        Some(&format!("{} ({} bytes)", dest.label, bytes)),
        None,
    );
    Ok(Json(serde_json::json!({
        "staging_id": staging_id,
        "size_bytes": bytes,
    })))
}

#[derive(serde::Deserialize)]
struct ValidateBody {
    staging_id: String,
    password: String,
}

async fn restore_validate(
    State(s): State<AppState>,
    Json(b): Json<ValidateBody>,
) -> Result<Json<ValidationSummary>> {
    s.vault.require_unlocked()?;
    let (data_dir, _) = dirs(&s);

    // Run the (CPU-bound) Argon2 + tar extract + SQLCipher open on a
    // blocking thread so the async runtime stays responsive while
    // huge tarballs are extracted.
    let staging_id = b.staging_id.clone();
    let summary = tokio::task::spawn_blocking(move || {
        restore::validate_staged_backup(&data_dir, &staging_id, &b.password)
    })
    .await
    .map_err(|e| Error::Other(anyhow::anyhow!("join: {e}")))??;

    Ok(Json(summary))
}

#[derive(serde::Deserialize)]
struct ApplyBody {
    staging_id: String,
}

async fn restore_apply(
    State(s): State<AppState>,
    Json(b): Json<ApplyBody>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let (data_dir, _) = dirs(&s);

    restore::mark_for_boot_restore(&data_dir, &b.staging_id)?;
    let _ = s.db.log_event(
        "restore_scheduled",
        Some(&format!("staging_id={}", b.staging_id)),
        None,
    );

    // Schedule a graceful exit a few seconds out so this response can
    // flush. The orchestrator (docker compose `restart: always`,
    // systemd, etc.) restarts us, and `consume_pending_restore` runs
    // before the DB pool opens on the next boot.
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        tracing::info!("restore: exiting for boot-time application");
        std::process::exit(0);
    });

    Ok(Json(serde_json::json!({
        "scheduled": true,
        "restart_in_secs": 2,
    })))
}

async fn restore_cancel(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let (data_dir, _) = dirs(&s);
    restore::discard_staging(&data_dir, &id)?;
    Ok(Json(serde_json::json!({ "cancelled": id })))
}

// ---------------------------------------------------------------------
// Backup file download — streams the on-server tarball to the browser
// with Content-Disposition: attachment so the browser saves it. The
// filename guard mirrors `delete_backup` to block path traversal.
// ---------------------------------------------------------------------

async fn download(
    State(s): State<AppState>,
    Path(filename): Path<String>,
) -> Result<Response<Body>> {
    s.vault.require_unlocked()?;
    backup::validate_backup_filename(&filename)?;
    let (_, backup_dir) = dirs(&s);
    let path = backup_dir.join(&filename);
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::NotFound,
            _ => Error::Other(anyhow::anyhow!("open {path:?}: {e}")),
        })?;
    // Propagate errors here rather than .unwrap_or(0) — sending
    // Content-Length: 0 makes the browser think the file is empty
    // and silently truncates the download.
    let size = file
        .metadata()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("stat {path:?}: {e}")))?
        .len();
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let response = Response::builder()
        .header(header::CONTENT_TYPE, "application/gzip")
        .header(header::CONTENT_LENGTH, size)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .header(header::CACHE_CONTROL, "no-store")
        .body(body)
        .map_err(|e| Error::Other(anyhow::anyhow!("build response: {e}")))?;
    Ok(response)
}

async fn get_schedule(State(s): State<AppState>) -> Result<Json<BackupSchedule>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.get_backup_schedule()?))
}

#[derive(serde::Deserialize)]
struct SetScheduleBody {
    enabled: bool,
    /// "daily" | "weekly".
    frequency: String,
    hour: u32,
    minute: u32,
    /// 0 = Sunday … 6 = Saturday. Ignored when frequency is "daily".
    day_of_week: u32,
    retention_count: u32,
}

async fn set_schedule(
    State(s): State<AppState>,
    Json(b): Json<SetScheduleBody>,
) -> Result<Json<BackupSchedule>> {
    s.vault.require_unlocked()?;
    let frequency = crate::storage::BackupFrequency::parse(&b.frequency)?;
    let update = UpdateBackupSchedule {
        enabled: b.enabled,
        frequency,
        hour: b.hour,
        minute: b.minute,
        day_of_week: b.day_of_week,
        retention_count: b.retention_count,
    };
    let saved = s.db.update_backup_schedule(&update)?;
    let _ = s.db.log_event(
        "backup_schedule_updated",
        Some(&format!(
            "{} {}:{:02} (retention={}, enabled={})",
            saved.frequency.as_str(),
            saved.hour,
            saved.minute,
            saved.retention_count,
            saved.enabled,
        )),
        None,
    );
    Ok(Json(saved))
}
