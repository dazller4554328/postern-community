//! Updates endpoint for the community build.
//!
//! Same route surface as the pro build (`/api/updates/*`,
//! `/api/license*`) so the web UI — UpdateBanner + UpdatesPanel —
//! works unchanged. Key differences:
//!
//!   - No license key. License endpoints always report the install
//!     as "community / active", no verification needed.
//!   - `/api/updates/check` polls the GitHub Releases API anonymously
//!     for the postern-community repo. Anyone can reach it; no token
//!     or license.
//!   - `/api/updates/apply` is a 409 with instructions. We don't
//!     ship a host-side updater on the free tier (users run plain
//!     docker-compose or a native binary and upgrade themselves).
//!
//! If the free user eventually wants one-click in-place updates they
//! can use the pro-side updater script from the main repo; keeping
//! it out of the default community build avoids needing `docker
//! compose` + systemd install on every personal laptop.

use std::env;

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::error::{Error, Result};

/// Upstream release manifest — public GitHub repository, no auth.
/// Flip the env var for dogfooding or internal test channels without
/// a rebuild.
fn releases_url() -> String {
    env::var("POSTERN_COMMUNITY_RELEASES_URL").unwrap_or_else(|_| {
        "https://api.github.com/repos/dazller4554328/postern-community/releases/latest"
            .to_string()
    })
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/license", get(get_license).post(set_license))
        .route("/license/verify", post(verify_license))
        .route("/updates/check", post(check_updates))
        .route("/updates/apply", post(apply_update))
        .route("/updates/status", get(update_status))
        .route("/updates/version", get(current_version))
}

// ---------------------------------------------------------------------
// License — stubs that report "community, no license needed"
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct LicenseInfo {
    install_id: String,
    license_key_masked: Option<String>,
    license_status: String,
    license_tier: Option<String>,
    license_verified_at_utc: Option<i64>,
}

fn community_license(install_id: String) -> LicenseInfo {
    LicenseInfo {
        install_id,
        license_key_masked: None,
        license_status: "active".to_string(),
        license_tier: Some("community".to_string()),
        license_verified_at_utc: Some(chrono::Utc::now().timestamp()),
    }
}

async fn get_license(State(s): State<AppState>) -> Result<Json<LicenseInfo>> {
    s.vault.require_unlocked()?;
    let meta = s.db.app_meta_get_or_init()?;
    Ok(Json(community_license(meta.install_id)))
}

/// Free builds reject any attempt to set a license key — the concept
/// doesn't apply. Return 400 rather than silently accepting so the
/// UI surfaces it clearly.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SetLicenseBody {
    license_key: Option<String>,
}

async fn set_license(
    State(_s): State<AppState>,
    Json(_body): Json<SetLicenseBody>,
) -> Result<Json<LicenseInfo>> {
    Err(Error::BadRequest(
        "license keys aren't used on Postern Community builds".into(),
    ))
}

#[derive(Debug, Serialize)]
struct VerifyResult {
    valid: bool,
    status: String,
    tier: Option<String>,
    message: Option<String>,
}

async fn verify_license(State(_s): State<AppState>) -> Result<Json<VerifyResult>> {
    Ok(Json(VerifyResult {
        valid: true,
        status: "active".to_string(),
        tier: Some("community".to_string()),
        message: Some("Postern Community — no license required.".to_string()),
    }))
}

// ---------------------------------------------------------------------
// Updates
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct UpdateCheck {
    current_commit: String,
    latest_commit: Option<String>,
    update_available: bool,
    release_date: Option<String>,
    release_notes: Option<String>,
    filename: Option<String>,
    sha256: Option<String>,
    size_bytes: Option<u64>,
    license_status: String,
    message: Option<String>,
}

/// GitHub Releases API payload — we pluck only what we need.
/// The full schema is huge; deserializing everything would force us
/// to chase schema drift every time GitHub adds a field.
#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
}

async fn check_updates(State(_s): State<AppState>) -> Result<Json<UpdateCheck>> {
    let current = current_commit_short();

    let client = reqwest::Client::builder()
        .user_agent("Postern-Community/0.1")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| Error::Other(anyhow::anyhow!("reqwest build: {e}")))?;

    let resp = client
        .get(releases_url())
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("GitHub Releases unreachable: {e}")))?;

    // A 404 just means "no releases yet" — don't log it as an error.
    if resp.status().as_u16() == 404 {
        return Ok(Json(UpdateCheck {
            current_commit: current,
            latest_commit: None,
            update_available: false,
            release_date: None,
            release_notes: None,
            filename: None,
            sha256: None,
            size_bytes: None,
            license_status: "active".to_string(),
            message: Some("No releases published yet.".to_string()),
        }));
    }
    if !resp.status().is_success() {
        return Err(Error::Other(anyhow::anyhow!(
            "GitHub Releases returned {}",
            resp.status()
        )));
    }

    let release: GithubRelease = resp
        .json()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("decode GitHub payload: {e}")))?;

    let latest = release.tag_name.trim_start_matches('v').to_string();
    // We compare substring-ish: the release tag is typically
    // "v0.2.0" or the short sha. Current commit comes from
    // GIT_COMMIT at build time. Good-enough equality: either side
    // contains the other.
    let update_available = !current.is_empty()
        && !latest.is_empty()
        && !(current.contains(&latest) || latest.contains(&current));

    Ok(Json(UpdateCheck {
        current_commit: current,
        latest_commit: Some(latest),
        update_available,
        release_date: release.published_at,
        release_notes: release.body,
        filename: release.html_url,
        sha256: None,
        size_bytes: None,
        license_status: "active".to_string(),
        message: None,
    }))
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct ApplyResult {
    queued: bool,
    message: String,
}

/// Free builds don't self-install. The user pulls the new image or
/// downloads a new binary themselves — which is fine because Postern
/// Community is meant to run on a personal machine where the user
/// already has full root. Surface the instructions over a clean 409
/// so the banner's error path can render it verbatim.
async fn apply_update(State(_s): State<AppState>) -> Result<Json<ApplyResult>> {
    Err(Error::Conflict(
        "Postern Community doesn't auto-install updates. Pull the latest \
         Docker image (docker compose pull && docker compose up -d) or \
         download the new binary from GitHub Releases, then restart."
            .into(),
    ))
}

#[derive(Debug, Serialize)]
struct UpdateStatus {
    state: String,
    message: Option<String>,
    finished_at: Option<i64>,
    trigger_pending: bool,
}

async fn update_status(State(_s): State<AppState>) -> Result<Json<UpdateStatus>> {
    // No host-side updater on community builds — status is always idle.
    // Response shape matches the pro API so the UI stays on one code path.
    Ok(Json(UpdateStatus {
        state: "idle".to_string(),
        message: None,
        finished_at: None,
        trigger_pending: false,
    }))
}

#[derive(Debug, Serialize)]
struct VersionInfo {
    commit: String,
}

async fn current_version() -> Json<VersionInfo> {
    Json(VersionInfo {
        commit: current_commit_short(),
    })
}

fn current_commit_short() -> String {
    let raw = env::var("GIT_COMMIT").unwrap_or_else(|_| "dev".to_string());
    let trimmed = raw.trim();
    if trimmed.len() > 12 {
        trimmed[..12].to_string()
    } else {
        trimmed.to_string()
    }
}
