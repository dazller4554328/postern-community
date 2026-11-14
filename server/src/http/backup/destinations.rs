//! Off-site backup destinations: CRUD over SFTP / Google Drive targets,
//! the connection test, and the manual on-demand push. The actual
//! transfer drivers live in crate::backup::{destinations, gdrive,
//! orchestrator}; this is just the HTTP surface over them.

use axum::{
    extract::{Path, State},
    Json,
};

use super::{dirs, AppState};
use crate::{
    backup::{
        self, destinations as dest_driver,
        orchestrator::{self, PushOutcome, TestOutcome},
    },
    error::{Error, Result},
    storage::{BackupDestination, NewBackupDestination, SftpCredential, SftpPublicConfig},
};

// Off-site destinations: CRUD + test + manual push.
//
// Auth on every endpoint requires the vault to be unlocked because
// SFTP credentials are vault-encrypted at rest, and any test/push
// has to decrypt them.
// ---------------------------------------------------------------------

#[derive(serde::Deserialize)]
pub(super) struct NewDestinationBody {
    label: String,
    /// Currently always "sftp".
    kind: String,
    sftp: Option<SftpFormConfig>,
}

#[derive(serde::Deserialize)]
struct SftpFormConfig {
    host: String,
    port: u16,
    username: String,
    remote_dir: String,
    /// "password" or "key".
    auth: String,
    /// Set when auth=password.
    password: Option<String>,
    /// Set when auth=key (OpenSSH-format private key).
    key_pem: Option<String>,
    /// Optional passphrase for an encrypted private key.
    passphrase: Option<String>,
}

fn extract_sftp_form(body: &NewDestinationBody) -> Result<(SftpPublicConfig, SftpCredential)> {
    let f = body
        .sftp
        .as_ref()
        .ok_or_else(|| Error::BadRequest("missing sftp config".into()))?;
    if f.host.trim().is_empty() {
        return Err(Error::BadRequest("host is required".into()));
    }
    if f.username.trim().is_empty() {
        return Err(Error::BadRequest("username is required".into()));
    }
    if f.remote_dir.trim().is_empty() {
        return Err(Error::BadRequest("remote_dir is required".into()));
    }
    let public = SftpPublicConfig {
        host: f.host.trim().into(),
        port: f.port,
        username: f.username.trim().into(),
        remote_dir: f.remote_dir.trim().into(),
    };
    let credential = match f.auth.as_str() {
        "password" => {
            let pw = f
                .password
                .as_ref()
                .ok_or_else(|| Error::BadRequest("password required".into()))?;
            SftpCredential::Password {
                password: pw.clone(),
            }
        }
        "key" => {
            let key_pem = f
                .key_pem
                .as_ref()
                .ok_or_else(|| Error::BadRequest("key_pem required".into()))?;
            SftpCredential::Key {
                key_pem: key_pem.clone(),
                passphrase: f.passphrase.clone().filter(|s| !s.is_empty()),
            }
        }
        other => {
            return Err(Error::BadRequest(format!(
                "unknown auth type: {other} (expected password or key)"
            )))
        }
    };
    Ok((public, credential))
}

pub(super) async fn list_destinations(
    State(s): State<AppState>,
) -> Result<Json<Vec<BackupDestination>>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.list_backup_destinations()?))
}

pub(super) async fn create_destination(
    State(s): State<AppState>,
    Json(b): Json<NewDestinationBody>,
) -> Result<Json<BackupDestination>> {
    s.vault.require_unlocked()?;
    if b.kind != "sftp" {
        return Err(Error::BadRequest(format!(
            "only sftp destinations supported in this build (got {})",
            b.kind
        )));
    }
    let (public, credential) = extract_sftp_form(&b)?;

    // Test connectivity before persisting — fail fast with a useful
    // error rather than save a row that won't work. TOFU mode: no
    // expected fingerprint yet, capture what we see and pin it on
    // the new row.
    let probe = dest_driver::test(&public, &credential, None).await?;

    let new = NewBackupDestination {
        kind: "sftp".into(),
        label: b.label.trim().into(),
        public_config: serde_json::to_value(&public)
            .map_err(|e| Error::Other(anyhow::anyhow!("encode public: {e}")))?,
        credential_json: serde_json::to_vec(&credential)
            .map_err(|e| Error::Other(anyhow::anyhow!("encode credential: {e}")))?,
    };
    let created = s.db.insert_backup_destination(&new, &s.vault)?;
    s.db.set_backup_destination_fingerprint(created.id, &probe.fingerprint)?;
    let _ = s.db.log_event(
        "backup_destination_added",
        Some(&format!(
            "{} ({}) — pinned hostkey {}",
            created.label, created.kind, probe.fingerprint
        )),
        None,
    );
    // Re-read so the response includes the fingerprint we just pinned.
    Ok(Json(s.db.get_backup_destination(created.id)?))
}

#[derive(serde::Deserialize)]
pub(super) struct UpdateDestinationBody {
    label: Option<String>,
    enabled: Option<bool>,
}

pub(super) async fn update_destination(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<UpdateDestinationBody>,
) -> Result<Json<BackupDestination>> {
    s.vault.require_unlocked()?;
    if let Some(label) = b.label {
        s.db.update_backup_destination_label(id, label.trim())?;
    }
    if let Some(enabled) = b.enabled {
        s.db.update_backup_destination_enabled(id, enabled)?;
    }
    Ok(Json(s.db.get_backup_destination(id)?))
}

pub(super) async fn delete_destination(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let dest = s.db.get_backup_destination(id)?;
    s.db.delete_backup_destination(id)?;
    let _ = s.db.log_event(
        "backup_destination_removed",
        Some(&format!("{} ({})", dest.label, dest.kind)),
        None,
    );
    Ok(Json(serde_json::json!({ "deleted": id })))
}

pub(super) async fn forget_destination_fingerprint(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<BackupDestination>> {
    s.vault.require_unlocked()?;
    let dest = s.db.get_backup_destination(id)?;
    s.db.forget_backup_destination_fingerprint(id)?;
    let _ = s.db.log_event(
        "backup_destination_fingerprint_forgotten",
        Some(&format!(
            "{}: cleared {} (next connect will TOFU-pin a new key)",
            dest.label,
            dest.server_fingerprint.as_deref().unwrap_or("(none)")
        )),
        None,
    );
    Ok(Json(s.db.get_backup_destination(id)?))
}

pub(super) async fn test_destination(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let dest = s.db.get_backup_destination(id)?;
    let bind_iface = s.vpn.bind_iface();
    let outcome = orchestrator::test_one(&s.db, &s.vault, bind_iface.as_deref(), &dest).await?;
    Ok(Json(match outcome {
        TestOutcome::Sftp {
            fingerprint,
            first_use,
        } => serde_json::json!({
            "ok": true,
            "fingerprint": fingerprint,
            "first_use": first_use,
        }),
        TestOutcome::Gdrive {
            account_email,
            folder_name,
        } => serde_json::json!({
            "ok": true,
            "account_email": account_email,
            "folder_name": folder_name,
        }),
    }))
}

#[derive(serde::Deserialize, Default)]
pub(super) struct PushDestinationBody {
    /// When set, push this specific backup. Otherwise the latest.
    filename: Option<String>,
}

pub(super) async fn push_destination(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    body: Option<Json<PushDestinationBody>>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let dest = s.db.get_backup_destination(id)?;
    let (_, backup_dir) = dirs(&s);

    // Pick which tarball to push.
    let chosen = if let Some(name) = body.and_then(|Json(b)| b.filename) {
        backup::validate_backup_filename(&name)?;
        name
    } else {
        let backups = backup::list_backups(&backup_dir)?;
        backups
            .into_iter()
            .next()
            .map(|r| r.filename)
            .ok_or_else(|| Error::BadRequest("no backups to push".into()))?
    };
    let local_path = backup_dir.join(&chosen);
    let bind_iface = s.vpn.bind_iface();

    match orchestrator::push_one(
        &s.db,
        &s.vault,
        bind_iface.as_deref(),
        &dest,
        &local_path,
        &chosen,
    )
    .await
    {
        Ok(out) => {
            let _ = s.db.log_event(
                "backup_destination_pushed",
                Some(&format!("{} → {}", chosen, dest.label)),
                None,
            );
            Ok(Json(push_outcome_to_json(&dest, &out)))
        }
        Err(e) => {
            s.db.record_destination_push_err(id, &e.to_string())?;
            Err(e)
        }
    }
}

fn push_outcome_to_json(dest: &BackupDestination, out: &PushOutcome) -> serde_json::Value {
    match dest.kind.as_str() {
        "gdrive" => serde_json::json!({
            "ok": true,
            "file_id": out.gdrive_file_id,
            "remote_path": out.remote_ref,
        }),
        // SFTP and any future driver get the generic shape.
        _ => serde_json::json!({
            "ok": true,
            "remote_path": out.remote_ref,
            "bytes_uploaded": out.bytes_uploaded,
        }),
    }
}
