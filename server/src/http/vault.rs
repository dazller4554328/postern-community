use axum::{
    extract::State,
    http::{header::SET_COOKIE, HeaderMap, HeaderValue},
    response::{AppendHeaders, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use base64::Engine;
use rand::RngCore;
use serde::Deserialize;

use super::{cookie, AppState};
use crate::{
    error::{Error, Result},
    vault::VaultStatus,
};

/// Cookie name covering both auth flavours (session + persistent).
/// Single name keeps the lock / cookie-check paths simple; the
/// persistence distinction lives in the row's `expires_at` server-side
/// and the absence/presence of `Max-Age` on the Set-Cookie line.
const DEVICE_COOKIE: &str = "pstn_dev";
/// Persistent ("Remember this device") TTL — 30 days. Strikes a
/// balance between "phone stops bothering me" and "rotate often
/// enough that a stolen cookie goes stale before long".
const DEVICE_TTL_SECS: i64 = 30 * 24 * 3600;
/// Session-cookie TTL on the server side. The client cookie has no
/// Max-Age so the browser deletes it on close, but we keep a
/// matching DB row for 24h as a hard upper bound — that way an
/// attacker who captures a cookie can't replay it for longer than a
/// day even if the user's browser stays open.
const SESSION_TTL_SECS: i64 = 24 * 3600;
/// Threshold above which a cookie's remaining lifetime counts as
/// "persistent" for the UI hint (`trusted_device` in StatusReply).
/// Any session cookie's TTL is ≤ 24h; anything ≥ 7 days is
/// unambiguously the remember-me path.
const PERSISTENT_THRESHOLD_SECS: i64 = 7 * 24 * 3600;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/vault/status", get(status))
        .route("/vault/init", post(init))
        .route("/vault/unlock", post(unlock))
        .route("/vault/lock", post(lock))
        .route("/vault/change-password", post(change_password))
        .route("/auth/totp/status", get(totp_status))
        .route("/auth/totp/init", post(totp_init))
        .route("/auth/totp/confirm", post(totp_confirm))
        .route("/auth/totp/disable", post(totp_disable))
}

#[derive(serde::Serialize)]
struct StatusReply {
    state: VaultStatus,
    /// True when the request already carries a valid trusted-device
    /// cookie. The login UI uses this to hide the "remember this device
    /// for 30 days" checkbox when the box is effectively already ticked.
    trusted_device: bool,
}

// Trusted-device operations are pro-only (the whole mechanism exists
// so a single Postern VPS can keep a roaming phone signed in across
// IP changes). Community builds bind to localhost — no trust
// boundary to cross — so these all collapse to "never trusted, no
// token persistence". Gating here instead of at every call site keeps
// the unlock/lock flow readable.

/// True when the request carries a *persistent* (30-day "remember
/// me") cookie. Drives both the UI hint (pre-tick the "remember"
/// box) AND the step-up-auth gate in `unlock` (a fresh persistent
/// cookie skips 2FA, matching Square / NordVPN behaviour).
///
/// Distinguishes session vs persistent by the cookie's *original*
/// TTL (`expires_at - created_at`), not its remaining lifetime —
/// otherwise a real 30-day cookie at day 24 looks like a session
/// cookie because only 6 days remain, and the user gets unexpectedly
/// re-prompted for 2FA before the cookie has actually expired.
#[cfg(feature = "pro")]
fn has_trusted_cookie(s: &AppState, headers: &HeaderMap) -> bool {
    cookie::get(headers, DEVICE_COOKIE)
        .and_then(|tok| s.db.trusted_device_original_ttl_secs(&tok).unwrap_or(None))
        .is_some_and(|ttl| ttl >= PERSISTENT_THRESHOLD_SECS)
}

#[cfg(not(feature = "pro"))]
fn has_trusted_cookie(_s: &AppState, _headers: &HeaderMap) -> bool {
    false
}

/// True when the request has *any* valid cookie (session or
/// persistent) AND that cookie's in-memory session is still active —
/// not idle past `SESSION_IDLE_SECS`, not past the hard cap, not
/// closed by an explicit /vault/lock. Used by the session_guard
/// middleware to decide whether to let API requests through. Free
/// builds always return true — they bind to localhost, no trust
/// boundary to enforce.
///
/// The DB-row check (`is_trusted_device_token_valid`) and the
/// in-memory session check are both required: the row says "this
/// device is enrolled," the in-memory session says "this device has
/// re-authenticated recently." A stale row with no session triggers
/// the unlock screen exactly as expected.
#[cfg(feature = "pro")]
pub fn has_valid_session_cookie(s: &AppState, headers: &HeaderMap) -> bool {
    let Some(tok) = cookie::get(headers, DEVICE_COOKIE) else {
        return false;
    };
    if !s.db.is_trusted_device_token_valid(&tok).unwrap_or(false) {
        return false;
    }
    let hash = crate::storage::hash_session_token(&tok);
    s.vault.session_is_valid(&hash)
}

#[cfg(not(feature = "pro"))]
pub fn has_valid_session_cookie(_s: &AppState, _headers: &HeaderMap) -> bool {
    true
}

#[cfg(feature = "pro")]
fn remember_device(
    s: &AppState,
    token: &str,
    ua: Option<&str>,
    ip: &str,
    ttl_secs: i64,
) -> Result<()> {
    s.db.insert_trusted_device(token, ua, Some(ip), ttl_secs)?;
    Ok(())
}

#[cfg(not(feature = "pro"))]
fn remember_device(
    _s: &AppState,
    _token: &str,
    _ua: Option<&str>,
    _ip: &str,
    _ttl_secs: i64,
) -> Result<()> {
    Ok(())
}

#[cfg(feature = "pro")]
fn forget_device(s: &AppState, token: &str) {
    let _ = s.db.delete_trusted_device_by_token(token);
}

#[cfg(not(feature = "pro"))]
fn forget_device(_s: &AppState, _token: &str) {}

async fn status(State(s): State<AppState>, headers: HeaderMap) -> Result<Json<StatusReply>> {
    let trusted_device = has_trusted_cookie(&s, &headers);
    // In pro builds, "vault unlocked server-side" alone isn't auth —
    // the request must also carry a valid session cookie. Without
    // one we report Locked so the SPA shows the unlock screen
    // instead of dropping the user into the inbox of a vault that
    // the server-side state happens to have unlocked for someone
    // else on the same IP. Free builds (localhost-only) skip this
    // check because there's no separate "someone else" to defend
    // against.
    let raw_state = s.vault.status();
    let state = if cfg!(feature = "pro")
        && raw_state == VaultStatus::Unlocked
        && !has_valid_session_cookie(&s, &headers)
    {
        VaultStatus::Locked
    } else {
        raw_state
    };
    Ok(Json(StatusReply {
        state,
        trusted_device,
    }))
}

#[derive(Deserialize)]
struct PasswordBody {
    password: String,
    /// When true, issue a trusted-device token so this browser skips
    /// the IP-change auto-lock on subsequent requests. Defaults false
    /// so existing clients and desktop users aren't affected.
    #[serde(default)]
    remember_device: bool,
    /// 6-digit TOTP code from the user's authenticator app. Required
    /// when the vault has 2FA enrolled; ignored otherwise. Sending
    /// `recovery_code` instead lets the user in if their authenticator
    /// is lost.
    #[serde(default)]
    totp_code: Option<String>,
    /// Single-use recovery code, used in place of `totp_code` when
    /// the user has lost their authenticator device. Burned on use.
    #[serde(default)]
    recovery_code: Option<String>,
}

fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(256).collect::<String>())
}

/// Generate a 32-byte random token, base64url-encoded (no padding).
/// Short enough to fit comfortably in a cookie, long enough that
/// guessing is infeasible (256 bits).
fn new_token() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

fn client_ip(headers: &HeaderMap) -> String {
    // `cf-connecting-ip` is Cloudflare's single-value header — trust
    // it as-is. `x-forwarded-for` is a chain: "<client>, <proxy1>,
    // <proxy2>". Only the left-most entry is plausibly the real
    // client; everything past the first comma was injected by an
    // intermediate hop and can be spoofed by a caller that sends a
    // pre-populated header. Audit-log the first token only.
    if let Some(cf) = headers
        .get("cf-connecting-ip")
        .and_then(|v| v.to_str().ok())
    {
        return cf.trim().to_string();
    }
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        return xff.split(',').next().unwrap_or("").trim().to_string();
    }
    "unknown".to_string()
}

async fn init(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<PasswordBody>,
) -> Result<Json<StatusReply>> {
    let ip = client_ip(&headers);
    let vault = s.vault.clone();
    tokio::task::spawn_blocking(move || vault.init(&b.password))
        .await
        .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("join: {e}")))??;
    s.vault.set_unlock_ip(ip.clone());
    let _ =
        s.db.log_event("vault_init", Some("master password set"), Some(&ip));
    Ok(Json(StatusReply {
        state: s.vault.status(),
        trusted_device: false,
    }))
}

async fn unlock(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<PasswordBody>,
) -> Result<impl IntoResponse> {
    let ip = client_ip(&headers);
    let ua = user_agent(&headers);
    let remember = b.remember_device;
    let password = b.password;
    let totp_code = b.totp_code.clone();
    let recovery_code = b.recovery_code.clone();
    let vault = s.vault.clone();
    let result = tokio::task::spawn_blocking(move || vault.unlock(&password))
        .await
        .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("join: {e}")))?;
    match result {
        Ok(()) => {
            // Password check passed. Now enforce 2FA if it's enabled
            // — must succeed before we hand out the cookie. Failure
            // here re-locks the vault so the password-derived keys
            // don't linger in memory after a "right password, wrong
            // 2FA" request.
            let totp_status = match s.db.get_auth_totp_status() {
                Ok(t) => t,
                Err(e) => {
                    s.vault.lock();
                    return Err(e);
                }
            };
            // Reconcile the unencrypted marker against the DB on
            // every unlock — keeps the unlock screen's pre-vault
            // 2FA-field decision accurate even for users who
            // enrolled before the marker existed.
            write_totp_marker(&s, totp_status.enabled);
            // Step-up auth: a valid persistent (30-day "remember
            // me") cookie skips 2FA — the cookie itself is the
            // second factor for this device. Standard pattern (Square,
            // NordVPN, etc.): password every login, second factor
            // only when the device isn't already trusted. Cookie
            // rows are server-bound, so a revoked or expired entry
            // immediately re-imposes the prompt.
            let already_trusted = has_trusted_cookie(&s, &headers);
            if totp_status.enabled && !already_trusted {
                let pass = match (&totp_code, &recovery_code) {
                    (_, Some(code)) if !code.trim().is_empty() => {
                        // Recovery codes burn even on the failure
                        // case? No — only on a successful match
                        // (consume_recovery_code returns true). A
                        // mistyped code shouldn't waste a slot.
                        s.db.consume_recovery_code(code.trim()).unwrap_or(false)
                    }
                    (Some(code), _) if !code.trim().is_empty() => {
                        match s.db.read_auth_totp_secret(&s.vault) {
                            Ok(Some(secret_b32)) => verify_totp(&secret_b32, code.trim()),
                            _ => false,
                        }
                    }
                    _ => false,
                };
                if !pass {
                    s.vault.lock();
                    let _ = s.db.log_event(
                        "vault_unlock_2fa_failed",
                        Some(if recovery_code.is_some() {
                            "bad recovery code"
                        } else if totp_code.is_some() {
                            "bad totp code"
                        } else {
                            "missing 2fa code"
                        }),
                        Some(&ip),
                    );
                    return Err(Error::BadRequest(
                        "wrong or missing two-factor code".into(),
                    ));
                }
            } else if totp_status.enabled && already_trusted {
                // Audit-trail the skip so a log review can answer
                // "did this login bypass 2FA, and why?"
                let _ = s.db.log_event(
                    "vault_unlock_2fa_skipped",
                    Some("trusted device cookie"),
                    Some(&ip),
                );
            }
            s.vault.set_unlock_ip(ip.clone());
            let _ = s.db.log_event("vault_unlock", None, Some(&ip));
            // Mint the cookie token first so we can record this
            // device's session against its hash. Generating the token
            // here (vs the legacy code path that minted it after the
            // event log) keeps cookie issuance and session bookkeeping
            // adjacent — easier to follow, and the new session_open
            // call is the only thing that gates HTTP requests once the
            // session_guard middleware switches over.
            let token = new_token();
            s.vault.session_open(
                crate::storage::hash_session_token(&token),
                ip.clone(),
            );
            // Re-seed the AI provider holder. After a fresh boot
            // the API key couldn't be decrypted (vault was locked),
            // so init_llm_holder built empty providers and AI was
            // effectively dead until the user manually re-saved
            // Settings → AI. Now we rebuild here as soon as the
            // vault unlock succeeds, gated by the auto_start
            // preference. Failures are non-fatal — a misconfigured
            // provider just leaves the holder empty and the user
            // can fix it from Settings → AI.
            if let Err(e) = rehydrate_ai_on_unlock(&s).await {
                tracing::warn!(error = %e, "ai: rehydrate on vault unlock failed");
            }
            // Curated-view backfill: on the first unlock after the
            // 0054 migration, scan existing sent mail to seed the
            // engagement table so the Curated tab has signal
            // immediately instead of waiting for new sends. No-op on
            // every subsequent unlock (table-non-empty guard).
            {
                let db_for_curated = s.db.clone();
                tokio::task::spawn_blocking(move || {
                    match db_for_curated.backfill_engagement_if_empty() {
                        Ok(0) => {} // already populated, normal case
                        Ok(n) => tracing::info!(seeded = n, "curated: engagement backfill seeded senders"),
                        Err(e) => tracing::warn!(error = %e, "curated: engagement backfill failed"),
                    }
                });
            }
            // Fire one /check against the license server in the
            // background. The validator atomic-binds the install_id
            // on the first non-empty /check, so this is what claims
            // the seat for users who installed via the bootstrap and
            // haven't visited Settings → Updates yet. Best-effort —
            // network blips and license-server outages just leave the
            // seat unbound; the user (or the next unlock) retries.
            //
            // Pro-only: the `updates` http module is stripped from the
            // community build (no licence concept there).
            #[cfg(feature = "pro")]
            {
                let s_for_bind = s.clone();
                tokio::spawn(async move {
                    if let Err(e) = crate::http::updates::auto_check_and_bind(&s_for_bind).await {
                        tracing::warn!(error = %e, "license: auto seat-bind on vault unlock failed");
                    }
                });
            }
            // Always issue a cookie now — pre-this-change, only the
            // remember-me path got one and "vault unlocked + no
            // cookie + same IP" was the same-IP-different-browser
            // bypass. Browser-session cookies (no Max-Age) still
            // disappear when the tab closes, so the default UX is
            // unchanged for anyone who didn't tick the box; what
            // changes is that the API now requires the cookie too.
            let (cookie_value, ttl, audit_kind) = if remember {
                (
                    cookie::build(DEVICE_COOKIE, &token, DEVICE_TTL_SECS),
                    DEVICE_TTL_SECS,
                    "trusted_device_added",
                )
            } else {
                (
                    cookie::build_session(DEVICE_COOKIE, &token),
                    SESSION_TTL_SECS,
                    "session_started",
                )
            };
            let set_cookie = match remember_device(&s, &token, ua.as_deref(), &ip, ttl) {
                Ok(()) => {
                    let _ = s.db.log_event(audit_kind, ua.as_deref(), Some(&ip));
                    Some(cookie_value)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to persist trusted device");
                    None
                }
            };
            let trusted_device = remember || already_trusted;
            let body = Json(StatusReply {
                state: s.vault.status(),
                trusted_device,
            });
            let mut headers_out = Vec::new();
            if let Some(sc) = set_cookie {
                if let Ok(v) = HeaderValue::from_str(&sc) {
                    headers_out.push((SET_COOKIE, v));
                }
            }
            Ok((AppendHeaders(headers_out), body).into_response())
        }
        Err(ref e) if e.to_string().contains("wrong master password") => {
            let _ =
                s.db.log_event("vault_unlock_failed", Some("wrong password"), Some(&ip));
            result?;
            unreachable!()
        }
        Err(e) => Err(e),
    }
}

async fn lock(State(s): State<AppState>, headers: HeaderMap) -> Result<impl IntoResponse> {
    let ip = client_ip(&headers);
    // Per-device lock: revoke this browser's trusted-device row, drop
    // its in-memory session, and expire its cookie. Other devices
    // keep their sessions. The global SQLCipher mount stays up so
    // background sync (outbox sender, IMAP fetch, indexer) keeps
    // running — locking your laptop should not silently kill mail
    // delivery for the rest of the household.
    //
    // AI providers stay loaded for the same reason: they're a
    // process-level resource, not a per-user one. Process restart or
    // a "lock everything" admin action would still tear them down.
    if let Some(tok) = cookie::get(&headers, DEVICE_COOKIE) {
        s.vault
            .session_close(&crate::storage::hash_session_token(&tok));
        forget_device(&s, &tok);
    }
    let _ = s.db.log_event("vault_lock", Some("device"), Some(&ip));
    let headers_out = vec![(
        SET_COOKIE,
        HeaderValue::from_str(&cookie::expire(DEVICE_COOKIE)).unwrap(),
    )];
    // Report Locked for *this device's* perspective regardless of
    // whether the global vault is still open — the SPA uses this to
    // navigate to the unlock screen, and it shouldn't be confused by
    // "vault is technically still unlocked because phone is using it."
    Ok((
        AppendHeaders(headers_out),
        Json(StatusReply {
            state: VaultStatus::Locked,
            trusted_device: false,
        }),
    )
        .into_response())
}

#[derive(Deserialize)]
struct ChangePasswordBody {
    old_password: String,
    new_password: String,
}

async fn change_password(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<ChangePasswordBody>,
) -> Result<Json<StatusReply>> {
    let ip = client_ip(&headers);
    let vault = s.vault.clone();
    tokio::task::spawn_blocking(move || vault.change_password(&b.old_password, &b.new_password))
        .await
        .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("join: {e}")))??;
    let _ = s.db.log_event("password_changed", None, Some(&ip));
    let trusted_device = has_trusted_cookie(&s, &headers);
    Ok(Json(StatusReply {
        state: s.vault.status(),
        trusted_device,
    }))
}

/// Re-seed the AI provider holder using the now-decryptable API
/// keys, gated on `ai_settings.auto_start`. No-ops in any of these
/// cases:
///   * AI features are tier-gated off (community build).
///   * Vault is somehow re-locked between unlock and this call.
///   * Settings has `enabled = false` (user explicitly disabled AI).
///   * Settings has `auto_start = false` (user opted out).
///   * Holder already has providers loaded (idempotent — covers the
///     case where the vault unlock fired multiple times in quick
///     succession, e.g. from a race between the IP-change re-lock
///     and the user re-entering the password).
async fn rehydrate_ai_on_unlock(s: &AppState) -> crate::error::Result<()> {
    if !crate::tier::ALLOW_AI {
        return Ok(());
    }
    let settings = s.db.get_ai_settings()?;
    if !settings.enabled || !settings.auto_start {
        return Ok(());
    }
    if s.llm.chat().await.is_some() || s.llm.embed().await.is_some() {
        // Already loaded — happens if init_llm_holder caught a
        // pre-unlocked vault (rare) or another request raced us.
        return Ok(());
    }
    let api_key = s.db.ai_api_key(&s.vault)?;
    let embed_api_key = s.db.ai_embed_api_key(&s.vault)?;
    let bind_iface = s.vpn.bind_iface();
    let chat = crate::llm::build_chat_provider(
        &settings,
        api_key.as_deref(),
        bind_iface.as_deref(),
    )?;
    let embed = crate::llm::build_embed_provider(
        &settings,
        api_key.as_deref(),
        embed_api_key.as_deref(),
        bind_iface.as_deref(),
    )?;
    let chat = chat.map(|p| crate::llm::ActivityLoggedProvider::wrap(p, s.db.clone()));
    let embed = embed.map(|p| crate::llm::ActivityLoggedProvider::wrap(p, s.db.clone()));
    s.llm.replace(chat, embed).await;
    tracing::info!("ai: providers rehydrated post-vault-unlock (auto_start=on)");
    Ok(())
}

// ─── TOTP (second factor at vault unlock) ───────────────────────────

/// Build a TOTP instance for our standard parameters: SHA-1 (the
/// algorithm every authenticator app speaks), 6-digit code, 30s
/// window, ±1 skew (90s tolerance). The label rendered in the
/// authenticator app combines an issuer "Postern" with a stable
/// account name "vault" — we don't know the user's email here and
/// the secret table is singleton anyway.
fn build_totp(secret_b32: &str) -> std::result::Result<totp_rs::TOTP, totp_rs::TotpUrlError> {
    let raw = totp_rs::Secret::Encoded(secret_b32.to_owned())
        .to_bytes()
        .map_err(|_| totp_rs::TotpUrlError::Secret(secret_b32.to_owned()))?;
    totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        raw,
        Some("Postern".to_owned()),
        "vault".to_owned(),
    )
}

/// Constant-time-ish verification of a 6-digit code against the
/// stored secret. Wraps the totp-rs check so callers don't have to
/// touch the upstream error type. Returns false on any failure path
/// (bad secret, system clock error, wrong code).
fn verify_totp(secret_b32: &str, code: &str) -> bool {
    match build_totp(secret_b32) {
        Ok(totp) => totp.check_current(code).unwrap_or(false),
        Err(_) => false,
    }
}

#[derive(serde::Serialize)]
struct TotpStatusReply {
    enabled: bool,
    /// True when an enrollment was started (secret stored) but the
    /// user hasn't yet confirmed with a code. UI uses this to resume
    /// vs offer a fresh setup.
    pending: bool,
    recovery_codes_remaining: i64,
}

/// Filename used as a "TOTP is enabled" marker, dropped in the
/// vault data dir alongside the sidecar. Its mere presence means
/// 2FA is on. Lives outside SQLCipher so the unlock screen can read
/// it before the vault is unlocked — without it, the 2FA field
/// only appears AFTER a failed unlock attempt seeds the
/// localStorage mirror.
const TOTP_MARKER_FILENAME: &str = ".totp_enabled";

fn totp_marker_path(s: &AppState) -> std::path::PathBuf {
    s.vault.data_dir().join(TOTP_MARKER_FILENAME)
}

fn write_totp_marker(s: &AppState, enabled: bool) {
    let path = totp_marker_path(s);
    if enabled {
        if let Err(e) = std::fs::write(&path, b"1") {
            tracing::warn!(?path, error = %e, "totp: marker write failed");
        }
    } else if path.exists() {
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!(?path, error = %e, "totp: marker delete failed");
        }
    }
}

async fn totp_status(State(s): State<AppState>) -> Result<Json<TotpStatusReply>> {
    // Try the encrypted DB first — that's authoritative. When it
    // succeeds, also reconcile the marker file so a previous
    // out-of-band wipe of the marker doesn't permanently mismatch
    // the DB.
    if let Ok(st) = s.db.get_auth_totp_status() {
        write_totp_marker(&s, st.enabled);
        return Ok(Json(TotpStatusReply {
            enabled: st.enabled,
            pending: st.pending,
            recovery_codes_remaining: st.recovery_codes_remaining,
        }));
    }
    // DB unreachable — almost always because the vault is locked
    // and the SQLCipher pool can't open. Fall back to the
    // unencrypted marker so the unlock screen can still decide
    // whether to render the 2FA field. We can't compute pending
    // or recovery_codes_remaining without the DB, so report
    // conservative defaults.
    Ok(Json(TotpStatusReply {
        enabled: totp_marker_path(&s).exists(),
        pending: false,
        recovery_codes_remaining: 0,
    }))
}

#[derive(serde::Serialize)]
struct TotpInitReply {
    /// Base32-encoded secret. Most authenticator apps accept the
    /// QR-scanned otpauth:// URL OR a manually-typed copy of this
    /// string. UI shows both.
    secret: String,
    /// otpauth://totp/Postern:vault?secret=...&issuer=Postern&...
    /// Some apps (1Password, Bitwarden) prefer this URL over a QR.
    otpauth_url: String,
    /// data:image/png;base64,... — the rendered QR ready to drop
    /// into an `<img src=...>` tag. Generated server-side via
    /// totp-rs's `qr` feature so the SPA doesn't need a QR library.
    qr_png_data_url: String,
}

async fn totp_init(State(s): State<AppState>) -> Result<Json<TotpInitReply>> {
    s.vault.require_unlocked()?;
    let secret = totp_rs::Secret::generate_secret().to_encoded();
    let secret_b32 = match secret {
        totp_rs::Secret::Encoded(s) => s,
        // generate_secret().to_encoded() always yields Encoded — guard
        // anyway so an upstream change doesn't silently break this.
        totp_rs::Secret::Raw(_) => {
            return Err(Error::Other(anyhow::anyhow!(
                "totp secret encoding failed"
            )));
        }
    };
    let totp = build_totp(&secret_b32)
        .map_err(|e| Error::Other(anyhow::anyhow!("totp setup: {e}")))?;
    let otpauth_url = totp.get_url();
    let qr_b64 = totp
        .get_qr_base64()
        .map_err(|e| Error::Other(anyhow::anyhow!("totp qr render: {e}")))?;
    let qr_png_data_url = format!("data:image/png;base64,{qr_b64}");
    s.db.store_auth_totp_secret(&s.vault, &secret_b32)?;
    let _ = s.db.log_event("totp_enrollment_started", None, None);
    Ok(Json(TotpInitReply {
        secret: secret_b32,
        otpauth_url,
        qr_png_data_url,
    }))
}

#[derive(Deserialize)]
struct TotpConfirmBody {
    code: String,
}

#[derive(serde::Serialize)]
struct TotpConfirmReply {
    enabled: bool,
    /// 10 single-use recovery codes. Shown to the user ONCE — the
    /// server only retains hashes, so this response is the only
    /// time the plaintext exists outside the user's notebook.
    recovery_codes: Vec<String>,
}

async fn totp_confirm(
    State(s): State<AppState>,
    Json(b): Json<TotpConfirmBody>,
) -> Result<Json<TotpConfirmReply>> {
    s.vault.require_unlocked()?;
    let secret = s
        .db
        .read_auth_totp_secret(&s.vault)?
        .ok_or_else(|| Error::BadRequest("no enrollment in progress".into()))?;
    if !verify_totp(&secret, b.code.trim()) {
        return Err(Error::BadRequest(
            "wrong code — check your authenticator app's clock".into(),
        ));
    }
    let codes = generate_recovery_codes(10);
    s.db.store_recovery_codes(&codes)?;
    s.db.enable_auth_totp()?;
    write_totp_marker(&s, true);
    let _ = s.db.log_event("totp_enabled", None, None);
    Ok(Json(TotpConfirmReply {
        enabled: true,
        recovery_codes: codes,
    }))
}

#[derive(Deserialize)]
struct TotpDisableBody {
    /// One of code / recovery_code must be present and valid — we
    /// require the user to prove they still hold a factor before
    /// dropping the protection.
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    recovery_code: Option<String>,
}

async fn totp_disable(
    State(s): State<AppState>,
    Json(b): Json<TotpDisableBody>,
) -> Result<Json<TotpStatusReply>> {
    s.vault.require_unlocked()?;
    let st = s.db.get_auth_totp_status()?;
    if !st.enabled {
        // Already off — nothing to do, return current state. Lets the
        // SPA call this idempotently.
        return Ok(Json(TotpStatusReply {
            enabled: false,
            pending: st.pending,
            recovery_codes_remaining: st.recovery_codes_remaining,
        }));
    }
    let pass = match (&b.code, &b.recovery_code) {
        (_, Some(rc)) if !rc.trim().is_empty() => {
            s.db.consume_recovery_code(rc.trim()).unwrap_or(false)
        }
        (Some(c), _) if !c.trim().is_empty() => {
            match s.db.read_auth_totp_secret(&s.vault) {
                Ok(Some(secret)) => verify_totp(&secret, c.trim()),
                _ => false,
            }
        }
        _ => false,
    };
    if !pass {
        return Err(Error::BadRequest(
            "wrong code — current 2FA code or a recovery code is required to disable"
                .into(),
        ));
    }
    s.db.disable_auth_totp()?;
    write_totp_marker(&s, false);
    let _ = s.db.log_event("totp_disabled", None, None);
    Ok(Json(TotpStatusReply {
        enabled: false,
        pending: false,
        recovery_codes_remaining: 0,
    }))
}

/// Generate `n` recovery codes. Each is 16 random bytes, base32-
/// encoded (~26 chars), grouped in 4-character chunks with hyphens
/// for readability when the user copies them onto paper. Reuses
/// totp-rs's Secret::to_encoded() so we don't need a direct
/// dependency on the base32 crate.
fn generate_recovery_codes(n: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let mut buf = vec![0u8; 16];
        rand::thread_rng().fill_bytes(&mut buf);
        let encoded = match totp_rs::Secret::Raw(buf).to_encoded() {
            totp_rs::Secret::Encoded(s) => s,
            // to_encoded() of Raw always returns Encoded, but guard
            // anyway against an upstream surprise.
            totp_rs::Secret::Raw(_) => continue,
        };
        // Strip any '=' padding totp-rs may leave on, then group into
        // 4-char chunks with hyphens, e.g. ABCD-EFGH-IJKL-MNOP-QRST-UV.
        let trimmed: String = encoded.chars().filter(|c| *c != '=').collect();
        let grouped: Vec<String> = trimmed
            .as_bytes()
            .chunks(4)
            .map(|c| String::from_utf8_lossy(c).to_string())
            .collect();
        out.push(grouped.join("-"));
    }
    out
}
