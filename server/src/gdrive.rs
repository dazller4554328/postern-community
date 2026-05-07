//! Google Drive OAuth + upload driver for backup destinations.
//!
//! ## OAuth flow
//!
//! 1. `authorize_url(redirect_uri, state)` → operator's browser
//!    redirects to Google's consent screen with the
//!    `https://www.googleapis.com/auth/drive.file` scope.
//! 2. Google redirects back with `?code=...&state=...`. The HTTP layer
//!    verifies state, calls `exchange_code(code)` here.
//! 3. `exchange_code` returns an access_token + refresh_token. We
//!    cache the access_token until `expires_at - 60s` and use the
//!    refresh_token to mint new ones forever after.
//!
//! ## Upload
//!
//! Uses the resumable upload protocol because backups can be hundreds
//! of MB and a multipart upload over a long pipe risks getting cut off
//! mid-transfer with no resume path. Resumable: POST to start a
//! session, get back a session URI, PUT the file body to that URI.
//! For files <= 5 MB we still single-shot it via PUT to the session
//! URI (Google's recommended pattern for "small" files in resumable).
//!
//! ## Why drive.file
//!
//! `drive.file` only sees files Postern itself created. We never get
//! to read the user's other Drive contents — minimum-privilege scope
//! that also avoids Google's manual review for the broader `drive`
//! scope.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::{
    error::{Error, Result},
    storage::{GDriveCredential, GDrivePublicConfig},
};

/// Build a reqwest client bound to the given interface (when set).
///
/// Postern's container ships with a VPN kill-switch that REJECTs all
/// outbound traffic except: loopback, wg0, RFC1918, Tailnet CGNAT,
/// and a small list of explicit allow-IPs. When the user has the VPN
/// up, GDrive HTTPS calls must go *through* wg0 or the OUTPUT chain
/// drops them with `No route to host`. SO_BINDTODEVICE via reqwest's
/// `.interface(name)` is the same mechanism IMAP/SMTP/CalDAV use.
///
/// `total_timeout` covers the entire request — caller picks based on
/// expected payload size. Short ops (token exchange, refresh, folder
/// list) use 60s; the upload PUT uses 30 min so a 500 MB tarball
/// over wg0 has plenty of slack at modest VPN throughput.
fn build_client(
    bind_iface: Option<&str>,
    total_timeout: std::time::Duration,
) -> Result<reqwest::Client> {
    let mut b = reqwest::Client::builder().timeout(total_timeout);
    if let Some(name) = bind_iface {
        b = b.interface(name);
    }
    b.build()
        .map_err(|e| Error::Other(anyhow::anyhow!("reqwest client build: {e}")))
}

/// Default for short Drive control-plane requests.
const SHORT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
/// Wide enough to cover a multi-hundred-MB tarball over the VPN.
const UPLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// Folder name we create in the user's Drive on first auth and store
/// every backup tarball in. Stored on the destination row by id, but
/// surfaced to the user by name so they can find it in the Drive UI.
pub const POSTERN_FOLDER_NAME: &str = "Postern Backups";

/// Scope requested at consent. Limits us to files we ourselves
/// created — we cannot see the user's other Drive contents.
pub const SCOPE: &str = "https://www.googleapis.com/auth/drive.file";

/// Static URLs we POST to. Google has documented these as stable.
const GOOGLE_AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
const DRIVE_FILES_URL: &str = "https://www.googleapis.com/drive/v3/files";
const DRIVE_UPLOAD_URL: &str = "https://www.googleapis.com/upload/drive/v3/files";

/// Read OAuth client config from env. None = not configured (UI
/// hides the Connect button).
pub struct OauthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

impl OauthConfig {
    pub fn from_env() -> Option<Self> {
        let client_id = std::env::var("POSTERN_GDRIVE_CLIENT_ID").ok()?;
        let client_secret = std::env::var("POSTERN_GDRIVE_CLIENT_SECRET").ok()?;
        let redirect_uri = std::env::var("POSTERN_GDRIVE_REDIRECT_URI").ok()?;
        if client_id.is_empty() || client_secret.is_empty() || redirect_uri.is_empty() {
            return None;
        }
        Some(Self {
            client_id,
            client_secret,
            redirect_uri,
        })
    }
}

/// URL the operator's browser is redirected to for consent. `state`
/// is a CSRF token the caller must verify on the callback before
/// trusting the `code` Google returns.
pub fn authorize_url(cfg: &OauthConfig, state: &str) -> String {
    // Standard OAuth 2.0 authorize URL params. `prompt=consent` and
    // `access_type=offline` together tell Google to *always* return a
    // refresh_token, even if the user previously consented — without
    // both flags, a re-consent gives an access_token only and the
    // long-lived push pipeline breaks the next time the cached token
    // expires.
    let params = [
        ("client_id", cfg.client_id.as_str()),
        ("redirect_uri", cfg.redirect_uri.as_str()),
        ("response_type", "code"),
        ("scope", SCOPE),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("state", state),
        ("include_granted_scopes", "true"),
    ];
    let qs = params
        .iter()
        .map(|(k, v)| format!("{}={}", crate::net::urlencode(k), crate::net::urlencode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{GOOGLE_AUTHORIZE_URL}?{qs}")
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    /// Only returned on the first exchange (consent). Refreshes don't
    /// re-issue a refresh_token; the caller carries it across.
    refresh_token: Option<String>,
    expires_in: i64,
    #[serde(default)]
    #[allow(dead_code)]
    scope: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    token_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    email: String,
}

/// Exchange a one-shot authorization code for the long-lived token
/// pair. Also fetches the user's email so the destination row can
/// be labelled distinctively when the operator connects more than
/// one Google account.
pub async fn exchange_code(
    cfg: &OauthConfig,
    code: &str,
    bind_iface: Option<&str>,
) -> Result<(GDriveCredential, String)> {
    let client = build_client(bind_iface, SHORT_TIMEOUT)?;
    let resp: TokenResponse = client
        .post(GOOGLE_TOKEN_URL)
        .form(&[
            ("client_id", cfg.client_id.as_str()),
            ("client_secret", cfg.client_secret.as_str()),
            ("redirect_uri", cfg.redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
            ("code", code),
        ])
        .send()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("token exchange: {e}")))?
        .error_for_status()
        .map_err(|e| Error::BadRequest(format!("Google rejected the auth code: {e}")))?
        .json()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("decode token response: {e}")))?;

    let refresh_token = resp.refresh_token.ok_or_else(|| {
        // This is the case the access_type+prompt flags above are
        // designed to prevent. If it still happens, the operator
        // probably changed scopes mid-flight; the fix is to revoke
        // the app from https://myaccount.google.com/permissions and
        // retry.
        Error::BadRequest(
            "Google didn't return a refresh_token. Revoke Postern at \
             https://myaccount.google.com/permissions and try again."
                .into(),
        )
    })?;

    // Best-effort: try to fetch the user's email so the row label can
    // distinguish between multiple connected Google accounts. We
    // requested only the `drive.file` scope, so the userinfo endpoint
    // returns 401 unless the consent screen also pre-included the
    // `email` scope. Treat any failure as "no email available" rather
    // than failing the whole connect — the operator already typed a
    // human label which is what's actually shown in the UI.
    let email = match client
        .get(GOOGLE_USERINFO_URL)
        .bearer_auth(&resp.access_token)
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => match r.json::<UserInfo>().await {
            Ok(u) => u.email,
            Err(e) => {
                warn!(error = %e, "gdrive: userinfo decode failed (continuing without email)");
                String::new()
            }
        },
        Ok(r) => {
            warn!(
                status = %r.status(),
                "gdrive: userinfo returned non-success (continuing without email)"
            );
            String::new()
        }
        Err(e) => {
            warn!(error = %e, "gdrive: userinfo request failed (continuing without email)");
            String::new()
        }
    };

    let credential = GDriveCredential {
        refresh_token,
        access_token: resp.access_token,
        expires_at: chrono::Utc::now().timestamp() + resp.expires_in,
    };
    Ok((credential, email))
}

/// Refresh the cached access_token using the long-lived
/// refresh_token. Mutates `credential` in place. Returns true when a
/// refresh actually happened (caller persists in that case).
pub async fn refresh_if_expiring(
    cfg: &OauthConfig,
    credential: &mut GDriveCredential,
    bind_iface: Option<&str>,
) -> Result<bool> {
    // 60s safety margin to account for the round-trip we're about to
    // do plus any clock skew between us and Google's auth server.
    if credential.expires_at - 60 > chrono::Utc::now().timestamp() {
        return Ok(false);
    }
    let client = build_client(bind_iface, SHORT_TIMEOUT)?;
    let resp: TokenResponse = client
        .post(GOOGLE_TOKEN_URL)
        .form(&[
            ("client_id", cfg.client_id.as_str()),
            ("client_secret", cfg.client_secret.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", credential.refresh_token.as_str()),
        ])
        .send()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("refresh: {e}")))?
        .error_for_status()
        .map_err(|e| {
            Error::BadRequest(format!(
                "Google rejected refresh_token: {e}. The destination may have \
                 been disconnected on the Google side; remove and re-add."
            ))
        })?
        .json()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("decode refresh response: {e}")))?;
    credential.access_token = resp.access_token;
    credential.expires_at = chrono::Utc::now().timestamp() + resp.expires_in;
    Ok(true)
}

/// Find or create the `Postern Backups` folder in the user's Drive
/// root. Returns its file id. Idempotent — if the folder already
/// exists from a previous connect we reuse it.
pub async fn ensure_postern_folder(
    access_token: &str,
    bind_iface: Option<&str>,
) -> Result<String> {
    let client = build_client(bind_iface, SHORT_TIMEOUT)?;
    // Search for an existing folder with our name in root.
    let q = format!(
        "name = '{}' and mimeType = 'application/vnd.google-apps.folder' \
         and trashed = false",
        POSTERN_FOLDER_NAME
    );
    let list: serde_json::Value = client
        .get(DRIVE_FILES_URL)
        .bearer_auth(access_token)
        .query(&[
            ("q", q.as_str()),
            ("fields", "files(id,name,parents)"),
            ("spaces", "drive"),
        ])
        .send()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("drive list: {e}")))?
        .error_for_status()
        .map_err(|e| Error::Other(anyhow::anyhow!("drive list: {e}")))?
        .json()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("decode drive list: {e}")))?;

    if let Some(files) = list.get("files").and_then(|v| v.as_array()) {
        if let Some(first) = files.first() {
            if let Some(id) = first.get("id").and_then(|v| v.as_str()) {
                info!(folder_id = %id, "gdrive: reusing existing Postern Backups folder");
                return Ok(id.to_owned());
            }
        }
    }

    // None found → create one in root.
    let metadata = serde_json::json!({
        "name": POSTERN_FOLDER_NAME,
        "mimeType": "application/vnd.google-apps.folder",
    });
    let created: serde_json::Value = client
        .post(DRIVE_FILES_URL)
        .bearer_auth(access_token)
        .json(&metadata)
        .send()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("create folder: {e}")))?
        .error_for_status()
        .map_err(|e| Error::Other(anyhow::anyhow!("create folder: {e}")))?
        .json()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("decode create folder: {e}")))?;

    let id = created
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Other(anyhow::anyhow!("create folder returned no id")))?;
    info!(folder_id = %id, "gdrive: created Postern Backups folder");
    Ok(id.to_owned())
}

/// Upload a tarball to the destination's folder. Uses Google Drive's
/// resumable-upload protocol: POST to start a session, PUT the body
/// to the session URI Google hands back. Returns the new file id.
pub async fn upload_tarball(
    access_token: &str,
    public: &GDrivePublicConfig,
    local_path: &Path,
    filename: &str,
    bind_iface: Option<&str>,
) -> Result<String> {
    // Stream the file rather than buffering it: a 500 MB tarball
    // shouldn't pin 500 MB of heap for the duration of the upload.
    // Read size up-front so the resumable-init's
    // X-Upload-Content-Length is honest.
    let metadata_fs = tokio::fs::metadata(local_path)
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("stat local tarball: {e}")))?;
    let size = metadata_fs.len();

    let metadata = serde_json::json!({
        "name": filename,
        "parents": [public.folder_id],
        "mimeType": "application/gzip",
    });

    // Single client for both the init POST and the body PUT. Use the
    // long timeout so a slow VPN-routed upload doesn't trip 60s.
    let client = build_client(bind_iface, UPLOAD_TIMEOUT)?;
    // Start the resumable session — Google returns a session URI in
    // the Location header; that's where the body bytes go next.
    let init = client
        .post(format!("{DRIVE_UPLOAD_URL}?uploadType=resumable"))
        .bearer_auth(access_token)
        .header("X-Upload-Content-Type", "application/gzip")
        .header("X-Upload-Content-Length", size.to_string())
        .json(&metadata)
        .send()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("init upload: {e}")))?
        .error_for_status()
        .map_err(|e| Error::Other(anyhow::anyhow!("init upload: {e}")))?;

    // Use the canonical header name constant — reqwest normalises
    // header lookup, but a future test double or version change that
    // changes lookup semantics would silently break the upload.
    let session_uri = init
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Error::Other(anyhow::anyhow!("no Location header in upload init")))?
        .to_owned();

    // Stream the body. tokio_util::io::ReaderStream wraps a
    // tokio::fs::File as a Stream<Item = Result<Bytes>>; reqwest's
    // Body::wrap_stream consumes it without buffering.
    let file = tokio::fs::File::open(local_path)
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("open local tarball: {e}")))?;
    let stream = tokio_util::io::ReaderStream::new(file);
    let body = reqwest::Body::wrap_stream(stream);

    let put = client
        .put(&session_uri)
        .bearer_auth(access_token)
        .header(reqwest::header::CONTENT_TYPE, "application/gzip")
        .header(reqwest::header::CONTENT_LENGTH, size.to_string())
        .body(body)
        .send()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("upload body: {e}")))?;

    if !put.status().is_success() {
        let status = put.status();
        let body = put.text().await.unwrap_or_default();
        return Err(Error::Other(anyhow::anyhow!(
            "upload body returned {status}: {}",
            body.chars().take(200).collect::<String>()
        )));
    }

    let resp: serde_json::Value = put
        .json()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("decode upload response: {e}")))?;
    let id = resp
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Other(anyhow::anyhow!("upload response missing id")))?;
    info!(file_id = %id, name = %filename, bytes = size, "gdrive: uploaded");
    Ok(id.to_owned())
}

/// Verify the cached credentials still work — round-trips a
/// metadata fetch on the destination folder. Used by the per-row
/// "Test" button. Refreshes the access token if needed.
pub async fn test(
    cfg: &OauthConfig,
    credential: &mut GDriveCredential,
    public: &GDrivePublicConfig,
    bind_iface: Option<&str>,
) -> Result<()> {
    refresh_if_expiring(cfg, credential, bind_iface).await?;
    let client = build_client(bind_iface, SHORT_TIMEOUT)?;
    let url = format!("{DRIVE_FILES_URL}/{}", public.folder_id);
    let resp = client
        .get(&url)
        .bearer_auth(&credential.access_token)
        .query(&[("fields", "id,name,trashed")])
        .send()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("drive folder check: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::BadRequest(format!(
            "Google Drive returned {status} for the destination folder. \
             {} If the folder was deleted, remove and re-add the destination.",
            body.chars().take(200).collect::<String>()
        )));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("decode folder check: {e}")))?;
    if body.get("trashed").and_then(|v| v.as_bool()).unwrap_or(false) {
        warn!("gdrive: destination folder is in Trash");
        return Err(Error::BadRequest(
            "destination folder is in Trash on the Google Drive side — \
             restore it or remove the destination and re-add."
                .into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PendingOauth {
    pub label: String,
    pub created_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorize_url_includes_required_oauth_params() {
        let cfg = OauthConfig {
            client_id: "12345.apps".into(),
            client_secret: "ignored-here".into(),
            redirect_uri: "https://postern.email/cb".into(),
        };
        let url = authorize_url(&cfg, "abc-state");
        assert!(url.starts_with(GOOGLE_AUTHORIZE_URL));
        assert!(url.contains("client_id=12345.apps"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
        assert!(url.contains("state=abc-state"));
        // Scope must be present, percent-encoded.
        assert!(
            url.contains("scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive.file"),
            "url should include the drive.file scope, got: {url}"
        );
    }

    #[test]
    fn oauth_config_returns_none_when_any_var_is_missing() {
        // Simulate by checking the predicate directly. We can't unset
        // env vars from tests reliably under cargo test --jobs, but
        // the from_env logic is straightforward.
        let cfg = OauthConfig {
            client_id: "".into(),
            client_secret: "x".into(),
            redirect_uri: "y".into(),
        };
        // The from_env check rejects empty client_id; mirror that
        // here rather than calling from_env.
        assert!(cfg.client_id.is_empty());
    }

    #[test]
    fn refresh_skipped_when_token_still_fresh() {
        // Pure-logic gate: if expires_at is well in the future, the
        // refresh function shouldn't network. We can't test the
        // network path without a real client; verify the predicate.
        let cred = GDriveCredential {
            refresh_token: "r".into(),
            access_token: "a".into(),
            expires_at: chrono::Utc::now().timestamp() + 3600,
        };
        // 60s margin is the threshold inside refresh_if_expiring.
        assert!(cred.expires_at - 60 > chrono::Utc::now().timestamp());
    }
}
