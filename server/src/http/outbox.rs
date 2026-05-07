//! Outbox HTTP surface.
//!
//! The queue itself is filled by POST /api/send (see `http::send`).
//! These endpoints let the client observe the queue and cancel or
//! reschedule entries that haven't been dispatched yet. All routes
//! require the vault to be unlocked — otherwise someone with API
//! access could peek at pending message bodies.

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::{
    error::{Error, Result},
    storage::OutboxEntry,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/outbox", get(list))
        .route(
            "/outbox/recent-failures",
            get(recent_failures).delete(clear_failures),
        )
        .route(
            "/outbox/:id",
            get(get_one).delete(cancel_one),
        )
        .route("/outbox/:id/reschedule", post(reschedule))
}

/// Strip the huge payload_json from the list view — the UI only needs
/// the summary fields. Clients can GET /outbox/:id to pull the full
/// payload when editing a scheduled draft.
#[derive(Debug, Serialize)]
struct OutboxListItem {
    id: i64,
    account_id: i64,
    scheduled_at: i64,
    status: String,
    attempts: i64,
    last_error: Option<String>,
    summary_to: String,
    summary_subject: String,
    sent_message_id: Option<String>,
    created_at: i64,
    updated_at: i64,
}

impl From<OutboxEntry> for OutboxListItem {
    fn from(e: OutboxEntry) -> Self {
        Self {
            id: e.id,
            account_id: e.account_id,
            scheduled_at: e.scheduled_at,
            status: e.status,
            attempts: e.attempts,
            last_error: e.last_error,
            summary_to: e.summary_to,
            summary_subject: e.summary_subject,
            sent_message_id: e.sent_message_id,
            created_at: e.created_at,
            updated_at: e.updated_at,
        }
    }
}

async fn list(State(s): State<AppState>) -> Result<Json<Vec<OutboxListItem>>> {
    s.vault.require_unlocked()?;
    let rows = s.db.outbox_list_active()?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

async fn recent_failures(State(s): State<AppState>) -> Result<Json<Vec<OutboxListItem>>> {
    s.vault.require_unlocked()?;
    let rows = s.db.outbox_list_recent_failures(25)?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

/// Wipe every `status='failed'` row. Pending and sending entries are
/// untouched — the storage method scopes the DELETE so a
/// double-clicked button can't drop in-flight sends.
async fn clear_failures(State(s): State<AppState>) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let removed = s.db.outbox_clear_failed()?;
    Ok(Json(serde_json::json!({ "removed": removed })))
}

/// Includes payload_json and forensics_json — for the status-poll loop
/// driving the compose undo toast + forensics card, and for editing a
/// scheduled draft.
async fn get_one(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<OutboxEntry>> {
    s.vault.require_unlocked()?;
    let entry = s.db.outbox_get(id)?;
    Ok(Json(entry))
}

async fn cancel_one(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let transitioned = s.db.outbox_cancel(id)?;
    if !transitioned {
        // The row exists but is no longer pending — most likely the
        // worker already dispatched it, so undo is impossible. Return
        // a 409 so the UI can tell the user "too late".
        return Err(Error::Conflict(
            "outbox entry is no longer pending and cannot be cancelled".into(),
        ));
    }
    Ok(Json(serde_json::json!({ "cancelled": id })))
}

#[derive(Debug, Deserialize)]
struct RescheduleBody {
    scheduled_at: i64,
}

async fn reschedule(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<RescheduleBody>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let now = chrono::Utc::now().timestamp();
    let target = b.scheduled_at.max(now);
    let updated = s.db.outbox_reschedule(id, target)?;
    if !updated {
        return Err(Error::Conflict(
            "outbox entry is no longer pending".into(),
        ));
    }
    Ok(Json(serde_json::json!({
        "id": id,
        "scheduled_at": target,
    })))
}
