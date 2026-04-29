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
    backup::{self, BackupJob, BackupReport},
    backup_destinations as dest_driver,
    backup_orchestrator::{self, PushOutcome, TestOutcome},
    error::{Error, Result},
    gdrive,
    restore::{self, ValidationSummary},
    storage::{
        BackupDestination, BackupSchedule, GDrivePublicConfig, NewBackupDestination,
        SftpCredential, SftpPublicConfig, UpdateBackupSchedule,
    },
};
use axum::extract::Query;
use axum::response::Redirect;

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
        .route("/backups/restore/from-existing", post(restore_from_existing))
        .route("/backups/restore/validate", post(restore_validate))
        .route("/backups/restore/apply", post(restore_apply))
        .route(
            "/backups/restore/staging/:id",
            axum::routing::delete(restore_cancel),
        )
        // Off-site destinations.
        .route(
            "/backups/destinations",
            get(list_destinations).post(create_destination),
        )
        .route(
            "/backups/destinations/:id",
            axum::routing::patch(update_destination).delete(delete_destination),
        )
        .route("/backups/destinations/:id/test", post(test_destination))
        .route("/backups/destinations/:id/push", post(push_destination))
        .route(
            "/backups/destinations/:id/forget-fingerprint",
            post(forget_destination_fingerprint),
        )
        // Schedule.
        .route(
            "/backups/schedule",
            get(get_schedule).post(set_schedule),
        )
        // GDrive OAuth.
        .route("/backups/integrations", get(integrations))
        .route(
            "/backups/destinations/oauth/google/start",
            get(gdrive_oauth_start),
        )
        .route(
            "/backups/oauth/google/callback",
            get(gdrive_oauth_callback),
        )
}

fn dirs(_s: &AppState) -> (PathBuf, PathBuf) {
    let data_dir = std::env::var("POSTERN_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/var/lib/postern/data"));
    let backup_dir = std::env::var("POSTERN_BACKUP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/var/lib/postern/backups"));
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
    tokio::spawn(backup_orchestrator::run_backup(
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
// Restore flow — see crate::restore for the storage-layer details.
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
    let (staging_id, _) =
        tokio::task::spawn_blocking(move || {
            restore::stage_existing_backup(&data_dir, &backup_dir, &b.filename)
        })
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("join: {e}")))??;

    Ok(Json(serde_json::json!({ "staging_id": staging_id })))
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

// ---------------------------------------------------------------------
// Off-site destinations: CRUD + test + manual push.
//
// Auth on every endpoint requires the vault to be unlocked because
// SFTP credentials are vault-encrypted at rest, and any test/push
// has to decrypt them.
// ---------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct NewDestinationBody {
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
            SftpCredential::Password { password: pw.clone() }
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

async fn list_destinations(
    State(s): State<AppState>,
) -> Result<Json<Vec<BackupDestination>>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.list_backup_destinations()?))
}

async fn create_destination(
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
    s.db
        .set_backup_destination_fingerprint(created.id, &probe.fingerprint)?;
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
struct UpdateDestinationBody {
    label: Option<String>,
    enabled: Option<bool>,
}

async fn update_destination(
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

async fn delete_destination(
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

async fn forget_destination_fingerprint(
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

async fn test_destination(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let dest = s.db.get_backup_destination(id)?;
    let bind_iface = s.vpn.bind_iface();
    let outcome =
        backup_orchestrator::test_one(&s.db, &s.vault, bind_iface.as_deref(), &dest).await?;
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
struct PushDestinationBody {
    /// When set, push this specific backup. Otherwise the latest.
    filename: Option<String>,
}

async fn push_destination(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    body: Option<Json<PushDestinationBody>>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let dest = s.db.get_backup_destination(id)?;
    let (_, backup_dir) = dirs(&s);

    // Pick which tarball to push.
    let chosen = match body.and_then(|Json(b)| b.filename) {
        Some(name) => {
            backup::validate_backup_filename(&name)?;
            name
        }
        None => {
            let backups = backup::list_backups(&backup_dir)?;
            backups
                .into_iter()
                .next()
                .map(|r| r.filename)
                .ok_or_else(|| Error::BadRequest("no backups to push".into()))?
        }
    };
    let local_path = backup_dir.join(&chosen);
    let bind_iface = s.vpn.bind_iface();

    match backup_orchestrator::push_one(
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

// ---------------------------------------------------------------------
// Integrations probe + Google Drive OAuth flow.
//
// The frontend hits /backups/integrations to decide which "Add"
// buttons to render — the GDrive button is hidden when env vars
// aren't set so the user isn't offered a feature they can't use.
//
// OAuth itself is two endpoints:
//
//   GET /backups/destinations/oauth/google/start?label=...
//       → 302 to Google's authorize URL after stashing a state token.
//
//   GET /backups/oauth/google/callback?code=X&state=Y
//       → exchange code, create folder, persist destination,
//         redirect back to /settings/backups.
// ---------------------------------------------------------------------

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

async fn integrations(State(_): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "google_drive": {
            "configured": gdrive::OauthConfig::from_env().is_some(),
        }
    }))
}

#[derive(serde::Deserialize)]
struct OauthStartParams {
    /// Label the operator typed in the form. Carried through the OAuth
    /// detour via the in-memory session map (keyed by state token)
    /// so the callback can name the destination row correctly.
    label: String,
}

async fn gdrive_oauth_start(
    State(s): State<AppState>,
    Query(p): Query<OauthStartParams>,
) -> Result<Redirect> {
    s.vault.require_unlocked()?;
    let cfg = gdrive::OauthConfig::from_env().ok_or_else(|| {
        Error::BadRequest(
            "Google Drive OAuth client not configured. \
             Set POSTERN_GDRIVE_CLIENT_ID, POSTERN_GDRIVE_CLIENT_SECRET, \
             and POSTERN_GDRIVE_REDIRECT_URI in the server env."
                .into(),
        )
    })?;

    if p.label.trim().is_empty() {
        return Err(Error::BadRequest("label is required".into()));
    }

    // Random 24-byte state token. CSRF defence + the key for our
    // pending-session map. Hex-encoded so it round-trips through
    // Google's redirect cleanly.
    use rand::RngCore;
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    let state = hex::encode(bytes);

    s.oauth_sessions
        .put(
            state.clone(),
            crate::gdrive::PendingOauth {
                label: p.label.trim().into(),
                created_at: chrono::Utc::now().timestamp(),
            },
        )
        .await;

    let url = gdrive::authorize_url(&cfg, &state);
    Ok(Redirect::to(&url))
}

#[derive(serde::Deserialize)]
struct OauthCallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn gdrive_oauth_callback(
    State(s): State<AppState>,
    Query(p): Query<OauthCallbackParams>,
) -> Result<Redirect> {
    s.vault.require_unlocked()?;

    if let Some(err) = p.error {
        // User clicked Deny in Google's consent screen, or an
        // upstream error happened. Bounce back to the settings page
        // with the error in the query string so the UI can surface
        // it.
        return Ok(Redirect::to(&format!(
            "/settings/backups?gdrive_error={}",
            urlencode_q(&err)
        )));
    }
    let code = p
        .code
        .ok_or_else(|| Error::BadRequest("missing code in callback".into()))?;
    let state = p
        .state
        .ok_or_else(|| Error::BadRequest("missing state in callback".into()))?;
    let pending = s
        .oauth_sessions
        .take(&state)
        .await
        .ok_or_else(|| Error::BadRequest("unknown or expired state token".into()))?;

    let cfg = gdrive::OauthConfig::from_env().ok_or_else(|| {
        Error::BadRequest("Google Drive OAuth client not configured".into())
    })?;

    let bind_iface = s.vpn.bind_iface();
    let (mut credential, account_email) =
        gdrive::exchange_code(&cfg, &code, bind_iface.as_deref()).await?;
    let folder_id =
        gdrive::ensure_postern_folder(&credential.access_token, bind_iface.as_deref()).await?;

    // Persist as a new destination row.
    let public = GDrivePublicConfig {
        folder_id: folder_id.clone(),
        folder_name: gdrive::POSTERN_FOLDER_NAME.into(),
        account_email: account_email.clone(),
    };
    let public_value = serde_json::to_value(&public)
        .map_err(|e| Error::Other(anyhow::anyhow!("encode gdrive public: {e}")))?;
    let credential_json = serde_json::to_vec(&credential)
        .map_err(|e| Error::Other(anyhow::anyhow!("encode credential: {e}")))?;
    let new = NewBackupDestination {
        kind: "gdrive".into(),
        label: pending.label,
        public_config: public_value,
        credential_json,
    };
    let created = s.db.insert_backup_destination(&new, &s.vault)?;
    let _ = s.db.log_event(
        "backup_destination_added",
        Some(&format!("{} (gdrive: {})", created.label, account_email)),
        None,
    );

    // Defensive: the access_token was just minted, so refresh isn't
    // needed yet, but call the refresher to make sure the predicate
    // logic is exercised once on the happy path. Quiet on no-op.
    let _ = gdrive::refresh_if_expiring(&cfg, &mut credential, bind_iface.as_deref()).await;

    // The flash banner displays whatever this query param contains.
    // Prefer the Google account email when we got it, otherwise fall
    // back to the operator's typed label so the success message stays
    // useful when userinfo was unavailable (e.g. drive.file scope only).
    let display = if account_email.is_empty() {
        created.label.clone()
    } else {
        account_email.clone()
    };
    Ok(Redirect::to(&format!(
        "/settings/backups?gdrive_connected={}",
        urlencode_q(&display)
    )))
}

use crate::net::urlencode as urlencode_q;
