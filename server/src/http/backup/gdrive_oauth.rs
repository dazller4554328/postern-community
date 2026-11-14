//! Google Drive OAuth flow for backup destinations: the consent
//! redirect, the callback that exchanges the code and persists a
//! `gdrive` destination row, and the integration-status probe.

use axum::{
    extract::{Query, State},
    response::Redirect,
    Json,
};

use super::AppState;
use crate::{
    backup::gdrive,
    error::{Error, Result},
    net::urlencode as urlencode_q,
    storage::{GDrivePublicConfig, NewBackupDestination},
};

pub(super) async fn integrations(State(_): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "google_drive": {
            "configured": gdrive::OauthConfig::from_env().is_some(),
        }
    }))
}

#[derive(serde::Deserialize)]
pub(super) struct OauthStartParams {
    /// Label the operator typed in the form. Carried through the OAuth
    /// detour via the in-memory session map (keyed by state token)
    /// so the callback can name the destination row correctly.
    label: String,
}

pub(super) async fn gdrive_oauth_start(
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
            crate::backup::gdrive::PendingOauth {
                label: p.label.trim().into(),
                created_at: chrono::Utc::now().timestamp(),
            },
        )
        .await;

    let url = gdrive::authorize_url(&cfg, &state);
    Ok(Redirect::to(&url))
}

#[derive(serde::Deserialize)]
pub(super) struct OauthCallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

pub(super) async fn gdrive_oauth_callback(
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

    let cfg = gdrive::OauthConfig::from_env()
        .ok_or_else(|| Error::BadRequest("Google Drive OAuth client not configured".into()))?;

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
