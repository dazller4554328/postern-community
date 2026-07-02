mod accounts;
mod ai;
mod api;
mod audit;
mod avatar;
pub(crate) mod backup;
mod body;
mod caldav;
mod contacts;
mod cookie;
#[cfg(feature = "pro")]
mod devices;
mod folders;
mod import;
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
    http::{header, HeaderValue, StatusCode},
    middleware,
    response::IntoResponse,
    Router,
};
use tower_http::{set_header::SetResponseHeaderLayer, trace::TraceLayer};

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
    /// Per-account counter of in-flight IMAP MOVE tasks fired by
    /// `spawn_move`. Empty-folder polls this before purging so a
    /// freshly-trashed message doesn't get re-imported on the next
    /// sync after a racy expunge.
    pub move_jobs: crate::mail::MoveJobs,

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
            move_jobs: crate::mail::MoveJobs::new(),
            oauth_sessions: OauthSessions::new(),
            llm,
        }
    }
}

/// Paths reachable without an unlocked vault AND without a device
/// session. Both the lock-guard and session-guard middlewares share
/// this single source of truth so the two security allow-lists can't
/// silently drift apart; each guard ORs its own narrow extras on top.
///
/// What's here and why:
/// - `/api/vault/*` — vault lifecycle (status/init/unlock/lock); the
///   unlock screen itself.
/// - `/api/auth/totp/status` — the unlock screen needs to know whether
///   to show the 2FA field before submitting. Leaks no secrets.
/// - `/health`, `/version` — Docker / uptime probes, pre-unlock.
/// - `/img-proxy/*` — inline images in already-opened, sandboxed
///   iframes; exposes nothing the vault wasn't already exposing.
/// - `/api/updates/status|version` — the update banner must keep polling
///   across the mid-install container restart, or the install hangs
///   visually. Payload is host-file status, no secrets.
/// - `/api/tier` — compile-time constant; the unlock screen renders
///   "Postern" vs "Postern Community" from it.
fn is_public_base_path(path: &str) -> bool {
    path.starts_with("/api/vault/")
        || path == "/api/auth/totp/status"
        || path == "/health"
        || path == "/version"
        || path.starts_with("/img-proxy/")
        || path == "/api/updates/status"
        || path == "/api/updates/version"
        || path == "/api/tier"
}

pub fn router(state: AppState, static_dir: Option<PathBuf>) -> Router {
    let mut api_routes = Router::new()
        .merge(api::routes())
        .merge(accounts::routes())
        .merge(messages::routes())
        .merge(contacts::routes())
        .merge(body::routes())
        .merge(folders::routes())
        .merge(search::routes())
        .merge(send::routes())
        .merge(outbox::routes())
        .merge(caldav::routes())
        .merge(reminders::routes())
        .merge(notes::routes())
        .merge(updates::routes())
        .merge(rules::routes())
        .merge(trusted_senders::routes())
        .merge(audit::routes())
        .merge(backup::routes())
        .merge(vault::routes())
        .merge(pgp::routes())
        .merge(tier::routes())
        .route("/avatar", axum::routing::get(avatar::get_avatar))
        .route(
            "/settings/remote-avatars",
            axum::routing::get(avatar::get_remote_avatars_setting)
                .post(avatar::set_remote_avatars_setting),
        );

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
        api_routes = api_routes.merge(vpn::routes()).merge(devices::routes());
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
            // Always-allowed endpoints (see is_public_base_path):
            // vault lifecycle, health/version, image proxy, update
            // status, tier. Everything else is behind-the-wall when
            // locked. The lock guard adds no extras beyond the base.
            let allow_while_locked = is_public_base_path(path);
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
    // Pro-only middleware. Holds copies of the vault, db, and LLM
    // holder — captured once and cloned per-request inside the
    // closure. The LLM clone is for the IP-change-lock path; when the
    // auto-lock fires, we drop AI providers in the same beat so
    // in-memory keys can't keep talking to OpenAI after the vault
    // locks.
    #[cfg(feature = "pro")]
    let ip_guard = {
        let vault_for_mw = state.vault.clone();
        let db_for_mw = state.db.clone();
        let llm_for_mw = state.llm.clone();
        middleware::from_fn(move |req: Request, next: middleware::Next| {
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
                        .map(std::borrow::ToOwned::to_owned);

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
                                llm.replace(None).await;
                                tracing::info!("ai: providers released after IP-change auto-lock");
                            }
                        }
                    }
                }
                next.run(req).await
            }
        })
    };

    // Session-cookie required for any non-public API path. Pro-only:
    // closes the same-IP-different-browser bypass where a second
    // browser on the same network would inherit the unlocked vault
    // state without performing its own unlock. The allow-list mirrors
    // lock_guard so the unlock screen + healthcheck endpoints can
    // still serve.
    #[cfg(feature = "pro")]
    let session_guard = {
        let state_for_session = state.clone();
        middleware::from_fn(move |req: Request, next: middleware::Next| {
            let s = state_for_session.clone();
            async move {
                let path = req.uri().path();
                // Shared base (see is_public_base_path) PLUS one
                // session-guard-only extra: OAuth callbacks arrive as
                // cross-site redirects (Google → Postern), and our
                // SameSite=Strict session cookie is stripped on those.
                // The OAuth handler validates a server-side state token
                // instead — the proper CSRF defense for OAuth flows.
                // Without this, the callback 401s mid-Drive-setup.
                let allow =
                    is_public_base_path(path) || path == "/api/backups/oauth/google/callback";
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
                        // Locked carries the user-facing reason ("session
                        // expired", …); anything else stays server-side.
                        let msg = match &e {
                            crate::error::Error::Locked(reason) => reason.clone(),
                            _ => "session check failed".to_string(),
                        };
                        return (StatusCode::UNAUTHORIZED, msg).into_response();
                    }
                }
                next.run(req).await
            }
        })
    };

    let mut app = Router::new()
        .nest("/api", api_routes)
        .merge(api::root_routes())
        .merge(body::img_proxy_routes())
        // 50 MiB ceiling on request bodies. Covers legitimate
        // attachments (a 30 MiB PDF in base64 is ~40 MiB); anything
        // bigger is almost certainly abuse and we'd rather 413 early
        // than deserialize megabytes pre-auth.
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
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

    // Global security headers on every response (SPA shell + API). The
    // email-body iframe and img-proxy set their own stricter CSP per
    // handler; these are the baseline backstop. HSTS is safe because
    // Postern is only reached over TLS (Cloudflare Tunnel / reverse
    // proxy). frame-ancestors 'none' + X-Frame-Options block clickjacking
    // of the authenticated UI.
    app.with_state(state)
        // if_not_present, not overriding: the email-body iframe handler
        // sets SAMEORIGIN so the SPA can frame it; everything else gets
        // DENY.
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("x-permitted-cross-domain-policies"),
            HeaderValue::from_static("none"),
        ))
        .layer(TraceLayer::new_for_http())
}

#[cfg(test)]
mod tests {
    use super::is_public_base_path;

    #[test]
    fn public_base_allows_unlock_screen_and_probes() {
        for p in [
            "/api/vault/status",
            "/api/vault/unlock",
            "/api/auth/totp/status",
            "/health",
            "/version",
            "/img-proxy/abc123",
            "/api/updates/status",
            "/api/updates/version",
            "/api/tier",
        ] {
            assert!(is_public_base_path(p), "{p} should be public while locked");
        }
    }

    #[test]
    fn public_base_blocks_mutation_and_data_paths() {
        // The base list must NOT leak any data/mutation endpoint — a
        // locked vault has to gate these.
        for p in [
            "/api/accounts",
            "/api/accounts/1/retention",
            "/api/messages",
            "/api/send",
            "/api/vpn/install",
            // The OAuth callback is a session-guard-only extra, NOT part
            // of the shared base — it must stay blocked while locked.
            "/api/backups/oauth/google/callback",
        ] {
            assert!(
                !is_public_base_path(p),
                "{p} must not be in the public base"
            );
        }
    }

    // ── Request-level integration tests ───────────────────────────
    // Build the real assembled Router over a fresh (locked) vault and
    // drive it with actual HTTP requests, so the lock-guard middleware
    // + is_public_base_path are exercised end-to-end — not just the
    // predicate in isolation.
    use super::{router, AppState};
    use crate::{
        llm::LlmHolder,
        privacy::ImageProxy,
        storage::{BlobStore, Db},
        sync::Scheduler,
        vault::Vault,
        vpn::VpnManager,
    };
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use std::sync::Arc;
    use tower::ServiceExt; // oneshot

    /// Assemble the production Router backed by throwaway storage with a
    /// locked, uninitialised vault. Returns the temp dir too so the
    /// caller keeps it alive for the duration of the test.
    fn locked_app() -> (axum::Router, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("POSTERN_DATA_DIR", tmp.path());
        let db = Arc::new(Db::open(&tmp.path().join("postern.db")).unwrap());
        let blobs = Arc::new(BlobStore::new(tmp.path().join("blobs")).unwrap());
        let proxy = ImageProxy::new();
        let vpn = VpnManager::new(db.clone(), proxy.clone());
        let vault = Vault::new(db.clone(), tmp.path().to_path_buf());
        let (scheduler, _handle) =
            Scheduler::start(db.clone(), blobs.clone(), vpn.clone(), vault.clone());
        let state = AppState::new(
            db,
            blobs,
            scheduler,
            proxy,
            vpn,
            vault,
            LlmHolder::default(),
        );
        (router(state, None), tmp)
    }

    async fn status_of(app: &axum::Router, uri: &str) -> StatusCode {
        app.clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn locked_vault_serves_public_paths() {
        let (app, _tmp) = locked_app();
        // Tier flag + healthcheck must answer even with the vault locked
        // (the unlock screen and uptime probes depend on it).
        assert_eq!(status_of(&app, "/api/tier").await, StatusCode::OK);
        assert_eq!(status_of(&app, "/health").await, StatusCode::OK);
    }

    #[tokio::test]
    async fn locked_vault_blocks_protected_paths() {
        let (app, _tmp) = locked_app();
        // A data endpoint must not be reachable while locked — the
        // guard 401s before any handler runs.
        assert_eq!(
            status_of(&app, "/api/accounts").await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn responses_carry_global_security_headers() {
        let (app, _tmp) = locked_app();
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let h = resp.headers();
        assert_eq!(h.get("x-frame-options").unwrap(), "DENY");
        assert_eq!(h.get("x-content-type-options").unwrap(), "nosniff");
        assert_eq!(h.get("referrer-policy").unwrap(), "no-referrer");
        assert!(h.contains_key("strict-transport-security"));
    }
}
