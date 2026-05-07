mod accounts;
mod ai;
mod api;
mod audit;
mod avatar;
pub(crate) mod backup;
mod body;
mod caldav;
mod cookie;
mod curated;
#[cfg(feature = "pro")]
mod devices;
mod folders;
mod import;
mod lockdown;
mod messages;
mod notes;
mod oauth_sessions;
mod outbox;
mod pgp;
mod reminders;
mod rules;
mod search;
mod send;
mod static_assets;
mod threads;
mod tier;
mod trusted_senders;
// Updates: license-gated on pro, anonymous GitHub-Releases check on free.
// Both implementations expose the same /api/updates/* + /api/license*
// surface so the web UI binds to a single API.
#[cfg(feature = "pro")]
mod updates;
#[cfg(not(feature = "pro"))]
#[path = "updates_free.rs"]
mod updates;
mod vault;
#[cfg(feature = "pro")]
mod vpn;

use std::{path::PathBuf, sync::Arc};

use axum::{
    extract::{DefaultBodyLimit, Request},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    Router,
};
use tower_http::trace::TraceLayer;

use crate::{
    backup::BackupJobs,
    llm::LlmHolder,
    privacy::ImageProxy,
    storage::{BlobStore, Db},
    sync::{purge::PurgeJobs, Scheduler},
    vault::Vault,
    vpn::VpnManager,
};

pub use oauth_sessions::OauthSessions;

/// Wide bag of dependencies shared by every axum handler.
///
/// Fields are grouped by role rather than alphabetically — the
/// groupings (storage, security, services, in-flight job registries)
/// make it easier to reason about which subsystems a handler should
/// reach into versus which subsystems should be hidden behind a
/// service. New fields belong in the section that matches their role,
/// not appended to the end.
#[derive(Clone)]
pub struct AppState {
    // ── Storage ────────────────────────────────────────────────
    pub db: Arc<Db>,
    pub blobs: Arc<BlobStore>,

    // ── Security ───────────────────────────────────────────────
    pub vault: Vault,
    pub vpn: VpnManager,

    // ── Services ───────────────────────────────────────────────
    pub scheduler: Scheduler,
    pub proxy: ImageProxy,

    // ── In-flight job registries ──────────────────────────────
    // Mark which expensive operations are currently running so the UI
    // can render progress and concurrent triggers can short-circuit.
    pub purge_jobs: PurgeJobs,
    pub backup_jobs: BackupJobs,

    // ── Transient OAuth state ─────────────────────────────────
    pub oauth_sessions: OauthSessions,

    // ── AI ────────────────────────────────────────────────────
    // Holds the chat + embed providers behind an async lock so
    // Settings → AI can hot-swap them without a server restart.
    // Empty (both providers `None`) when the build has
    // `tier::ALLOW_AI = false`, when the user has disabled AI in
    // settings, or when no backend is reachable at boot. AI
    // handlers detect a missing chat provider and 503 cleanly.
    pub llm: LlmHolder,
}

impl AppState {
    pub fn new(
        db: Arc<Db>,
        blobs: Arc<BlobStore>,
        scheduler: Scheduler,
        proxy: ImageProxy,
        vpn: VpnManager,
        vault: Vault,
        llm: LlmHolder,
    ) -> Self {
        Self {
            db,
            blobs,
            scheduler,
            proxy,
            vpn,
            vault,
            purge_jobs: PurgeJobs::new(),
            backup_jobs: BackupJobs::new(),
            oauth_sessions: OauthSessions::new(),
            llm,
        }
    }
}

pub fn router(state: AppState, static_dir: Option<PathBuf>) -> Router {
    let mut api_routes = Router::new()
        .merge(api::routes())
        .merge(accounts::routes())
        .merge(messages::routes())
        .merge(body::routes())
        .merge(folders::routes())
        .merge(search::routes())
        .merge(send::routes())
        .merge(outbox::routes())
        .merge(caldav::routes())
        .merge(curated::routes())
        .merge(reminders::routes())
        .merge(notes::routes())
        .merge(updates::routes())
        .merge(rules::routes())
        .merge(trusted_senders::routes())
        .merge(audit::routes())
        .merge(backup::routes())
        .merge(vault::routes())
        .merge(pgp::routes())
        .merge(threads::routes())
        .merge(tier::routes())
        .merge(lockdown::routes())
        .route("/avatar", axum::routing::get(avatar::get_avatar));

    // Mail import is available in both tiers — the flag lives in
    // tier::ALLOW_MAIL_IMPORT so a future decision to gate it again
    // has one clean place to flip.
    if crate::tier::ALLOW_MAIL_IMPORT {
        api_routes = api_routes.merge(import::routes());
    }

    // AI surfaces — gated by the same constant the boot probe
    // checks. When ALLOW_AI is false the routes don't even
    // register, so a community build's HTTP surface stays
    // identical to today.
    if crate::tier::ALLOW_AI {
        api_routes = api_routes.merge(ai::routes());
    }

    #[cfg(feature = "pro")]
    {
        api_routes = api_routes
            .merge(vpn::routes())
            .merge(devices::routes());
    }

    // Vault-lock guard: when the vault is locked, allow only the
    // /vault/* routes that exist to unlock it. Every other /api route
    // — which includes every mutation endpoint (delete account,
    // set_retention, purge toggles, VPN install, send) — 401s with
    // an explicit "vault locked" response. Previously the individual
    // handlers were expected to call `require_unlocked` themselves;
    // several didn't, which let a locked vault still accept mutation
    // requests whose effects would be applied on the next unlocked
    // sync cycle.
    let vault_for_lock_guard = state.vault.clone();
    let lock_guard = middleware::from_fn(move |req: Request, next: middleware::Next| {
        let v = vault_for_lock_guard.clone();
        async move {
            let path = req.uri().path();
            // Always-allowed endpoints: vault lifecycle (status so the
            // UI can decide whether to show the unlock screen, plus
            // init/unlock/lock themselves). Everything else is
            // behind-the-wall when locked.
            let allow_while_locked = path.starts_with("/api/vault/")
                // The unlock screen needs to know whether to show
                // the 2FA field before the user submits credentials,
                // so the TOTP status endpoint is openly readable
                // while locked. Returns only {enabled, pending,
                // recovery_codes_remaining} — no secrets leak.
                || path == "/api/auth/totp/status"
                // Root healthcheck / version endpoints stay open so
                // Docker and uptime probes can function before unlock.
                || path == "/health"
                || path == "/version"
                // Inline image proxy is rendered inside sandboxed
                // iframes for already-opened messages and doesn't
                // expose anything the vault wasn't already exposing.
                || path.starts_with("/img-proxy/")
                // Update status + current version are safe while locked.
                // Keeping /api/updates/status reachable matters because
                // the container restarts mid-install — if the banner's
                // status poll 401s after the restart, the UI never sees
                // "success" and the install hangs visually. The payload
                // is just {state, message, finished_at, trigger_pending}
                // read from a host file. No secrets exposed.
                || path == "/api/updates/status"
                || path == "/api/updates/version"
                // Tier surfacing — compile-time constant, no secrets.
                // The vault-unlock screen needs it to render the right
                // copy ("Postern" vs "Postern Community").
                || path == "/api/tier";
            if !allow_while_locked && !v.is_unlocked() {
                return (StatusCode::UNAUTHORIZED, "vault is locked").into_response();
            }
            next.run(req).await
        }
    });

    // IP-based auto-lock + trusted-devices cookie check. Pro-only:
    // the whole mechanism exists so a single Postern VPS can be
    // reached by multiple roaming clients while enforcing "same IP
    // as unlock, unless on a trusted device". The community build
    // binds to localhost only — a single user, zero strangers —
    // so this middleware is dead weight there and the
    // `touch_trusted_device` + `check_ip` call sites don't exist in
    // the free binary.
    #[cfg(feature = "pro")]
    let vault_for_mw = state.vault.clone();
    #[cfg(feature = "pro")]
    let db_for_mw = state.db.clone();
    // Hold a copy of the LLM holder for the IP-change-lock path —
    // when the auto-lock fires, we want to drop AI providers in the
    // same beat so in-memory keys can't keep talking to OpenAI
    // after the vault is locked.
    #[cfg(feature = "pro")]
    let llm_for_mw = state.llm.clone();
    #[cfg(feature = "pro")]
    let ip_guard = middleware::from_fn(move |req: Request, next: middleware::Next| {
        let v = vault_for_mw.clone();
        let db = db_for_mw.clone();
        let llm = llm_for_mw.clone();
        async move {
            if v.is_unlocked() {
                // Only enforce the IP check when the request carries a
                // real client IP header — internal requests (Docker
                // health check, scheduler loopback) don't have one and
                // would false-positive to "unknown" → auto-lock.
                let real_ip = req
                    .headers()
                    .get("cf-connecting-ip")
                    .and_then(|v| v.to_str().ok())
                    .or_else(|| {
                        req.headers()
                            .get("x-forwarded-for")
                            .and_then(|v| v.to_str().ok())
                    })
                    .map(|s| s.to_owned());

                // Trusted-device shortcircuit: if the request carries a
                // valid pstn_dev cookie, skip the IP check entirely and
                // refresh the device's last-seen metadata. This is the
                // whole point — roaming phones stay signed in as long
                // as they hold a trusted token.
                let raw_cookie = cookie::get(req.headers(), "pstn_dev");
                let trusted = match &raw_cookie {
                    Some(tok) => match db.touch_trusted_device(tok, real_ip.as_deref()) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!(error = %e, "touch_trusted_device errored");
                            false
                        }
                    },
                    None => false,
                };
                // Demoted to debug — raise via RUST_LOG=postern=debug
                // if you need to see the per-request decision again.
                tracing::debug!(
                    path = %req.uri().path(),
                    has_cookie = raw_cookie.is_some(),
                    trusted,
                    ip = ?real_ip,
                    "trusted-device middleware"
                );

                if !trusted {
                    if let Some(ip) = &real_ip {
                        let was_unlocked_before = v.is_unlocked();
                        let _ = v.check_ip(ip);
                        // If check_ip just transitioned us from
                        // unlocked → locked (the IP-change auto-
                        // lock path), drop AI providers immediately
                        // so the in-memory keys stop being usable.
                        if was_unlocked_before && !v.is_unlocked() {
                            llm.replace(None, None).await;
                            tracing::info!(
                                "ai: providers released after IP-change auto-lock"
                            );
                        }
                    }
                }
            }
            next.run(req).await
        }
    });

    // Session-cookie required for any non-public API path. Pro-only:
    // closes the same-IP-different-browser bypass where a second
    // browser on the same network would inherit the unlocked vault
    // state without performing its own unlock. The allow-list mirrors
    // lock_guard so the unlock screen + healthcheck endpoints can
    // still serve.
    #[cfg(feature = "pro")]
    let state_for_session = state.clone();
    #[cfg(feature = "pro")]
    let session_guard = middleware::from_fn(move |req: Request, next: middleware::Next| {
        let s = state_for_session.clone();
        async move {
            let path = req.uri().path();
            let allow = path.starts_with("/api/vault/")
                || path == "/api/auth/totp/status"
                || path == "/health"
                || path == "/version"
                || path.starts_with("/img-proxy/")
                || path == "/api/updates/status"
                || path == "/api/updates/version"
                || path == "/api/tier"
                // OAuth callbacks arrive as cross-site redirects
                // (Google → Postern). Our session cookie is
                // SameSite=Strict so browsers strip it on those
                // redirects. The OAuth handler validates a
                // server-side state token instead — that's the
                // proper CSRF defense for OAuth flows. Without
                // this allow-listing, the callback 401s and the
                // user sees "session required" mid-Drive-setup.
                || path == "/api/backups/oauth/google/callback";
            if !allow {
                // Two checks: cookie + DB row exist, AND the in-memory
                // session is still active (idle/hard-cap not yet hit).
                // The DB row check stays so a token whose row was
                // revoked from another device drops out immediately.
                // session_check refreshes last_seen_at on the way in,
                // so an active tab keeps the idle window sliding.
                let Some(tok) = cookie::get(req.headers(), "pstn_dev") else {
                    return (
                        StatusCode::UNAUTHORIZED,
                        "session required — please sign in",
                    )
                        .into_response();
                };
                if !s.db.is_trusted_device_token_valid(&tok).unwrap_or(false) {
                    return (
                        StatusCode::UNAUTHORIZED,
                        "session required — please sign in",
                    )
                        .into_response();
                }
                let hash = crate::storage::hash_session_token(&tok);
                let ip = req
                    .headers()
                    .get("cf-connecting-ip")
                    .and_then(|v| v.to_str().ok())
                    .or_else(|| {
                        req.headers()
                            .get("x-forwarded-for")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|s| s.split(',').next())
                    })
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if let Err(e) = s.vault.session_check(&hash, &ip) {
                    return (StatusCode::UNAUTHORIZED, e.to_string()).into_response();
                }
            }
            next.run(req).await
        }
    });

    let mut app = Router::new()
        .nest("/api", api_routes)
        .merge(api::root_routes())
        .merge(body::img_proxy_routes())
        // 50 MiB ceiling on request bodies. Covers legitimate
        // attachments (a 30 MiB PDF in base64 is ~40 MiB); anything
        // bigger is almost certainly abuse and we'd rather 413 early
        // than deserialize megabytes pre-auth.
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        // Lockdown layer runs AFTER the lock_guard so we know the
        // vault is unlocked (which means the user is authenticated)
        // before we enforce a 403. Unauthenticated requests already
        // 401'd via lock_guard. This layer reads the singleton flag
        // from app_meta on each call — cheap, and avoids a stale
        // cached value when the user just toggled the flag from
        // another tab.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            lockdown::middleware,
        ))
        .layer(lock_guard);
    #[cfg(feature = "pro")]
    {
        // Order: ip_guard runs outermost (it touches the cookie's
        // last-seen + may auto-lock on IP change), then session_guard
        // enforces "no cookie = 401" on non-public paths, then
        // lock_guard checks the vault. session_guard runs after
        // ip_guard so a roaming-phone IP-change-then-unlock sequence
        // is handled by ip_guard's cookie short-circuit before
        // session_guard re-validates.
        app = app.layer(session_guard).layer(ip_guard);
    }

    if let Some(dir) = static_dir {
        app = app.merge(static_assets::routes(dir));
    }

    app.with_state(state).layer(TraceLayer::new_for_http())
}
