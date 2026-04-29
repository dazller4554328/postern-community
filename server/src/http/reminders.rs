//! Local reminders HTTP endpoints.
//!
//! Vault-gated via the router's lock guard; handlers still call
//! `require_unlocked` for defense in depth, same pattern as the
//! calendar endpoints.

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use super::AppState;
use crate::{
    error::{Error, Result},
    storage::{NewReminder, Reminder, UpdateReminder},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/reminders", get(list_reminders).post(create_reminder))
        .route("/reminders/range", get(list_reminders_in_range))
        .route("/reminders/due", get(list_due_reminders))
        .route(
            "/reminders/:id",
            get(get_reminder)
                .patch(update_reminder)
                .delete(delete_reminder),
        )
        .route("/reminders/:id/done", post(mark_done))
        .route("/reminders/:id/snooze", post(snooze))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default)]
    include_done: bool,
}

async fn list_reminders(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<Reminder>>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.list_reminders(q.include_done)?))
}

#[derive(Debug, Deserialize)]
struct RangeQuery {
    from: i64,
    to: i64,
}

async fn list_reminders_in_range(
    State(s): State<AppState>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<Reminder>>> {
    s.vault.require_unlocked()?;
    if q.to < q.from {
        return Err(Error::BadRequest("from must be ≤ to".into()));
    }
    Ok(Json(s.db.list_reminders_in_range(q.from, q.to)?))
}

async fn list_due_reminders(State(s): State<AppState>) -> Result<Json<Vec<Reminder>>> {
    s.vault.require_unlocked()?;
    let now = chrono::Utc::now().timestamp();
    let due = s.db.list_due_reminders(now)?;
    // Mark each as notified so the next poll doesn't return them
    // again. Any failures here are non-fatal — the worst case is the
    // user sees the same reminder twice in a row, not missed ones.
    for r in &due {
        if let Err(e) = s.db.mark_reminder_notified(r.id) {
            tracing::warn!(reminder_id = r.id, error = %e, "failed to mark reminder notified");
        }
    }
    Ok(Json(due))
}

async fn get_reminder(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Reminder>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.get_reminder(id)?))
}

async fn create_reminder(
    State(s): State<AppState>,
    Json(new): Json<NewReminder>,
) -> Result<Json<Reminder>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.insert_reminder(&new)?))
}

async fn update_reminder(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(patch): Json<UpdateReminder>,
) -> Result<Json<Reminder>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.update_reminder(id, &patch)?))
}

async fn delete_reminder(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    s.db.delete_reminder(id)?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

async fn mark_done(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Reminder>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.mark_reminder_done(id)?))
}

#[derive(Debug, Deserialize)]
struct SnoozeBody {
    /// One of: "5m", "1h", "tomorrow", or an explicit unix seconds
    /// timestamp. Anything else is rejected — we don't want the UI
    /// silently accepting free-form durations that may diverge from
    /// the snooze options we actually support.
    until: SnoozeUntil,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SnoozeUntil {
    Preset(String),
    Unix(i64),
}

async fn snooze(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<SnoozeBody>,
) -> Result<Json<Reminder>> {
    s.vault.require_unlocked()?;
    let now = chrono::Utc::now().timestamp();
    let until = match body.until {
        SnoozeUntil::Preset(p) => match p.as_str() {
            "5m" => now + 5 * 60,
            "1h" => now + 60 * 60,
            // "Tomorrow" = 9am local on the next calendar day, normalized
            // to UTC by the server. The exact local-9am will drift across
            // DST, which is fine for a snooze — it's not a precise alarm.
            "tomorrow" => tomorrow_9am_utc(now),
            other => {
                return Err(Error::BadRequest(format!("unknown snooze preset: {other}")));
            }
        },
        SnoozeUntil::Unix(ts) => {
            if ts <= now {
                return Err(Error::BadRequest("snooze target must be in the future".into()));
            }
            ts
        }
    };
    Ok(Json(s.db.snooze_reminder(id, until)?))
}

/// 09:00 UTC on the calendar day after `now`. We deliberately don't
/// try to honour the user's local timezone here — the UI can send an
/// explicit unix timestamp (via `SnoozeUntil::Unix`) when it wants
/// local-9am behaviour. Keeping the server's snooze logic TZ-free
/// avoids divergence when the client's zone changes between snooze
/// and fire.
fn tomorrow_9am_utc(now: i64) -> i64 {
    use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
    let today: DateTime<Utc> = DateTime::<Utc>::from_timestamp(now, 0).unwrap_or_else(Utc::now);
    let d = (today + Duration::days(1)).date_naive();
    Utc.with_ymd_and_hms(d.year(), d.month(), d.day(), 9, 0, 0)
        .single()
        .map(|dt| dt.timestamp())
        .unwrap_or(now + 24 * 3600)
}
