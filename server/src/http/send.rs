//! POST /api/send — always enqueues through the outbox.
//!
//! Historically this called send_blocking synchronously and returned
//! SendReport. That path no longer exists: every send now goes through
//! the outbox queue so undo-send and send-later can share infrastructure
//! (see `crate::outbox`). Callers that want the final forensics poll
//! GET /api/outbox/:id after the queue settles.

use axum::{extract::State, routing::post, Json, Router};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::{error::Result, send::SendRequest};

pub fn routes() -> Router<AppState> {
    Router::new().route("/send", post(send))
}

#[derive(Debug, Clone, Deserialize)]
struct SendHttpRequest {
    #[serde(flatten)]
    inner: SendRequest,
    /// Unix epoch seconds at which the worker should dispatch. When
    /// omitted or ≤ now, the worker picks the row up on the next tick
    /// (≤ 2s). When set in the future, the send is held until then.
    /// Between enqueue and `scheduled_at`, the client can DELETE
    /// /api/outbox/:id to cancel (undo-send + cancel-scheduled).
    #[serde(default)]
    scheduled_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct SendHttpResponse {
    outbox_id: i64,
    scheduled_at: i64,
    /// True when scheduled_at was omitted or in the past — the worker
    /// will dispatch within a couple of seconds. UIs use this to pick
    /// between an "Undo" toast and a "Scheduled for X" pill.
    immediate: bool,
}

async fn send(
    State(s): State<AppState>,
    Json(req): Json<SendHttpRequest>,
) -> Result<Json<SendHttpResponse>> {
    s.vault.require_unlocked()?;
    if req.inner.to.is_empty() {
        return Err(crate::error::Error::BadRequest(
            "at least one recipient (To) required".into(),
        ));
    }
    // Confirm account exists up-front so bad requests fail here rather
    // than silently sitting in the queue.
    let _ = s.db.get_account(req.inner.account_id)?;

    let now = chrono::Utc::now().timestamp();
    let requested = req.scheduled_at.unwrap_or(now);
    let scheduled_at = requested.max(now);
    let immediate = scheduled_at <= now;

    // Tier cap on send-later horizon. Community builds only allow the
    // undo-send window — i.e., up to ~60s in the future — so the full
    // "send at 9am tomorrow" feature stays a paid differentiator. The
    // cap compares against `now` rather than `requested` so a request
    // for a scheduled_at in the past (which normally clamps up to now)
    // doesn't accidentally count against it.
    if let Some(max_delay) = crate::tier::MAX_SEND_DELAY_SECS {
        if scheduled_at - now > max_delay {
            return Err(crate::error::Error::BadRequest(format!(
                "scheduled send beyond {max_delay}s isn't available on Postern Community. \
                 Send now, or schedule within the undo-send window."
            )));
        }
    }

    let summary_to = req.inner.to.join(", ");
    let summary_subject = req.inner.subject.clone();
    let payload_json = serde_json::to_string(&req.inner)
        .map_err(|e| crate::error::Error::Other(anyhow::anyhow!("encode send payload: {e}")))?;

    let outbox_id = s.db.outbox_enqueue(
        req.inner.account_id,
        &payload_json,
        scheduled_at,
        &summary_to,
        &summary_subject,
    )?;

    Ok(Json(SendHttpResponse {
        outbox_id,
        scheduled_at,
        immediate,
    }))
}
