//! Lockdown mode — server-side hard kill-switch.
//!
//! When the install-wide flag is on, every mutating / outbound
//! route 403s before its handler runs. This is the *real* guarantee
//! that nothing the AI suggests, nothing a session hijacker could
//! click, and nothing a stale browser tab could replay can produce
//! a destructive or off-box effect. Reading mail and asking Datas
//! questions still work — the user is on a privacy-first email
//! client, not an air-gapped one.
//!
//! Routes covered (the implementation lives in the middleware
//! below, but here's the full list for review):
//!
//! - POST /api/send                                — outbound mail
//! - POST /api/messages/:id/send-receipt           — MDN dispatch
//! - POST /api/messages/:id/spam | not-spam        — IMAP move
//! - POST /api/messages/:id/trash                  — IMAP move
//! - POST /api/messages/:id/archive                — IMAP move
//! - POST /api/messages/:id/move                   — IMAP move
//! - POST /api/messages/folder-action              — bulk delete / mark
//! - POST /api/messages/bulk                       — bulk action
//! - POST /api/messages/bulk/move-to               — bulk move
//! - POST /api/outbox/:id/reschedule               — outbox dispatch
//! - DELETE /api/outbox/:id                        — outbox cancel (read-only? keep open — cancelling is benign)
//!
//! Plus body-rendering side: a remote=1 query on
//! /api/messages/:id/body.html is forced to remote=0 so no remote
//! image / font / CSS fetch happens while lockdown is on. That
//! check lives in the body handler itself, not this middleware,
//! because the route returns 200 either way (just with stripped
//! content).

use axum::{
    body::Body,
    extract::{Request, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::error::Result;

pub fn routes() -> Router<AppState> {
    Router::new().route("/lockdown", get(get_status).post(post_set))
}

#[derive(Serialize)]
struct LockdownStatus {
    enabled: bool,
}

async fn get_status(State(s): State<AppState>) -> Result<Json<LockdownStatus>> {
    Ok(Json(LockdownStatus {
        enabled: s.db.lockdown_enabled()?,
    }))
}

#[derive(Deserialize)]
struct SetBody {
    enabled: bool,
}

async fn post_set(
    State(s): State<AppState>,
    Json(body): Json<SetBody>,
) -> Result<Json<LockdownStatus>> {
    s.vault.require_unlocked()?;
    let saved = s.db.set_lockdown_enabled(body.enabled)?;
    let _ = s.db.log_event(
        if saved { "lockdown_enabled" } else { "lockdown_disabled" },
        None,
        None,
    );
    tracing::info!(enabled = saved, "lockdown mode toggled");
    Ok(Json(LockdownStatus { enabled: saved }))
}

/// Routes that are blocked while lockdown is on. Matched against
/// `(method, path)` — exact path is too restrictive (we have
/// /messages/:id/trash etc), so each entry is a (method, prefix,
/// suffix?) tuple. The `match_route` helper folds this into a
/// single decision.
struct BlockedRoute {
    method: Method,
    /// Path prefix. The middleware compares using `starts_with`
    /// after stripping the trailing element when `suffix` is set.
    prefix: &'static str,
    /// Path suffix (e.g. "/trash" for `/api/messages/:id/trash`).
    /// None means "match prefix on whole path".
    suffix: Option<&'static str>,
}

const BLOCKED: &[BlockedRoute] = &[
    // Outbound mail.
    BlockedRoute {
        method: Method::POST,
        prefix: "/api/send",
        suffix: None,
    },
    // MDN dispatch — sends an email back to the requester. Same
    // egress concern as /send.
    BlockedRoute {
        method: Method::POST,
        prefix: "/api/messages/",
        suffix: Some("/send-receipt"),
    },
    // IMAP folder mutations on a single message.
    BlockedRoute {
        method: Method::POST,
        prefix: "/api/messages/",
        suffix: Some("/spam"),
    },
    BlockedRoute {
        method: Method::POST,
        prefix: "/api/messages/",
        suffix: Some("/not-spam"),
    },
    BlockedRoute {
        method: Method::POST,
        prefix: "/api/messages/",
        suffix: Some("/trash"),
    },
    BlockedRoute {
        method: Method::POST,
        prefix: "/api/messages/",
        suffix: Some("/archive"),
    },
    BlockedRoute {
        method: Method::POST,
        prefix: "/api/messages/",
        suffix: Some("/move"),
    },
    // Bulk + folder-level actions.
    BlockedRoute {
        method: Method::POST,
        prefix: "/api/messages/folder-action",
        suffix: None,
    },
    BlockedRoute {
        method: Method::POST,
        prefix: "/api/messages/bulk",
        suffix: None,
    },
    BlockedRoute {
        method: Method::POST,
        prefix: "/api/messages/bulk/move-to",
        suffix: None,
    },
    // Outbox dispatch knobs. Cancelling an outbox row (DELETE) is
    // intentionally NOT blocked — cancelling a queued send is the
    // *safer* direction. Rescheduling is the dangerous one because
    // it can change a "scheduled for 2026-12-01" send into "send
    // now".
    BlockedRoute {
        method: Method::POST,
        prefix: "/api/outbox/",
        suffix: Some("/reschedule"),
    },
];

/// Match `(method, path)` against the blocked list. Returns true
/// when the request would mutate state lockdown is meant to
/// prevent.
fn match_route(method: &Method, path: &str) -> bool {
    BLOCKED.iter().any(|b| {
        if &b.method != method {
            return false;
        }
        match b.suffix {
            None => path == b.prefix || path.starts_with(b.prefix),
            Some(suffix) => path.starts_with(b.prefix) && path.ends_with(suffix),
        }
    })
}

/// Axum middleware: 403 with a structured error body when lockdown
/// is on AND the route is in the blocked set. Otherwise pass
/// through. The error body uses Postern's standard `{ "error":
/// "..." }` envelope so the frontend's existing error toasts pick
/// it up without special-casing.
pub async fn middleware(
    State(s): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_owned();
    if match_route(&method, &path) {
        // Read the flag inside the match branch so the unblocked
        // happy path doesn't even hit the DB. Failure to read is
        // treated as "not in lockdown" — better to allow the action
        // and surface a different error than to lock the user out
        // of their mailbox if the DB hiccups.
        if matches!(s.db.lockdown_enabled(), Ok(true)) {
            tracing::info!(
                method = %method,
                path = %path,
                "lockdown: blocked mutating request"
            );
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "Lockdown mode is active. This action is disabled. Turn lockdown off in Settings → AI to use it."
                })),
            )
                .into_response();
        }
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_blocked_send() {
        assert!(match_route(&Method::POST, "/api/send"));
    }

    #[test]
    fn matches_blocked_per_message_actions() {
        assert!(match_route(&Method::POST, "/api/messages/42/trash"));
        assert!(match_route(&Method::POST, "/api/messages/42/archive"));
        assert!(match_route(&Method::POST, "/api/messages/42/move"));
        assert!(match_route(&Method::POST, "/api/messages/42/spam"));
        assert!(match_route(&Method::POST, "/api/messages/42/not-spam"));
        assert!(match_route(&Method::POST, "/api/messages/42/send-receipt"));
    }

    #[test]
    fn matches_blocked_bulk_and_folder() {
        assert!(match_route(&Method::POST, "/api/messages/bulk"));
        assert!(match_route(&Method::POST, "/api/messages/bulk/move-to"));
        assert!(match_route(&Method::POST, "/api/messages/folder-action"));
    }

    #[test]
    fn does_not_match_reads() {
        // Read endpoints stay open under lockdown — that's the
        // whole point.
        assert!(!match_route(&Method::GET, "/api/messages"));
        assert!(!match_route(&Method::GET, "/api/messages/42"));
        assert!(!match_route(&Method::GET, "/api/messages/42/forensics"));
    }

    #[test]
    fn does_not_match_outbox_cancel() {
        // DELETE on outbox is the SAFE direction — pulling a
        // queued send back. Don't block.
        assert!(!match_route(&Method::DELETE, "/api/outbox/42"));
    }

    #[test]
    fn matches_outbox_reschedule() {
        assert!(match_route(&Method::POST, "/api/outbox/42/reschedule"));
    }

    #[test]
    fn does_not_match_method_mismatch() {
        // A GET to /api/send (which would 405 normally anyway)
        // shouldn't be classified as blocked, since the middleware
        // is supposed to pass it through to the real handler.
        assert!(!match_route(&Method::GET, "/api/send"));
    }
}
