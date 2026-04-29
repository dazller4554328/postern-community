//! Calendar HTTP endpoints.
//!
//! Everything here requires the vault unlocked — calendar bodies live
//! in the encrypted DB, same as mail. The vault-lock guard in
//! `http::router` already enforces this for `/api/*` paths; handlers
//! still call `require_unlocked` for defense in depth.

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::{
    caldav::{expand_rrule_in_range, sync_account, SyncReport},
    error::Result,
    storage::{CalAccount, CalCalendar, CalEvent, NewCalAccount},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/cal/accounts", get(list_accounts).post(create_account))
        .route(
            "/cal/accounts/:id",
            axum::routing::delete(delete_account),
        )
        .route("/cal/accounts/:id/sync", post(trigger_sync))
        .route("/cal/calendars", get(list_calendars))
        .route("/cal/events", get(list_events_in_range))
        .route("/cal/events/:id", get(get_event))
}

async fn list_accounts(State(s): State<AppState>) -> Result<Json<Vec<CalAccount>>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.list_cal_accounts()?))
}

async fn create_account(
    State(s): State<AppState>,
    Json(new): Json<NewCalAccount>,
) -> Result<Json<CalAccount>> {
    s.vault.require_unlocked()?;
    let account = s.db.insert_cal_account(&new, &s.vault)?;
    // Kick off an initial sync in the background so the calendar
    // starts populating right away. We don't wait — the frontend
    // polls /cal/events after adding.
    let db = s.db.clone();
    let vault = s.vault.clone();
    let id = account.id;
    tokio::spawn(async move {
        sync_account(db, vault, id).await;
    });
    Ok(Json(account))
}

async fn delete_account(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    s.db.delete_cal_account(id)?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

async fn trigger_sync(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<SyncReport>> {
    s.vault.require_unlocked()?;
    // Confirm the account exists — surface a 404 here rather than an
    // opaque error from the sync loop.
    let _ = s.db.get_cal_account(id)?;
    let db = s.db.clone();
    let vault = s.vault.clone();
    let report = sync_account(db, vault, id).await;
    Ok(Json(report))
}

#[derive(Debug, Deserialize)]
struct CalendarsQuery {
    account_id: Option<i64>,
}

async fn list_calendars(
    State(s): State<AppState>,
    Query(q): Query<CalendarsQuery>,
) -> Result<Json<Vec<CalCalendar>>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.list_cal_calendars(q.account_id)?))
}

/// Query parameters: `?from=<unix>&to=<unix>`. Both inclusive on
/// boundaries — an event starting at `to` still appears.
#[derive(Debug, Deserialize)]
struct RangeQuery {
    from: i64,
    to: i64,
}

/// Occurrence-flavoured event projection — one row per concrete
/// occurrence. Recurring events fan out; non-recurring events come
/// through with `is_recurring=false`.
#[derive(Debug, Clone, Serialize)]
struct EventOccurrence {
    id: i64,
    calendar_id: i64,
    uid: String,
    summary: Option<String>,
    description: Option<String>,
    location: Option<String>,
    dtstart_utc: i64,
    dtend_utc: Option<i64>,
    all_day: bool,
    is_recurring: bool,
    /// The index of this occurrence in its recurring series (0 for
    /// non-recurring or for the original). Useful when a future UI
    /// wants to jump to "week 3 of the standup".
    occurrence_index: u32,
}

async fn list_events_in_range(
    State(s): State<AppState>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<EventOccurrence>>> {
    s.vault.require_unlocked()?;
    if q.to < q.from {
        return Err(crate::error::Error::BadRequest(
            "from must be ≤ to".into(),
        ));
    }

    let rows: Vec<CalEvent> = s.db.list_cal_events_in_range(q.from, q.to)?;
    let mut out: Vec<EventOccurrence> = Vec::with_capacity(rows.len());
    for ev in rows {
        let duration = ev
            .dtend_utc
            .map(|e| (e - ev.dtstart_utc).max(0))
            .unwrap_or(0);
        if let Some(rrule) = ev.rrule.as_deref() {
            let occurrences = expand_rrule_in_range(ev.dtstart_utc, rrule, q.from, q.to);
            for (idx, start) in occurrences.into_iter().enumerate() {
                out.push(EventOccurrence {
                    id: ev.id,
                    calendar_id: ev.calendar_id,
                    uid: ev.uid.clone(),
                    summary: ev.summary.clone(),
                    description: ev.description.clone(),
                    location: ev.location.clone(),
                    dtstart_utc: start,
                    dtend_utc: Some(start + duration),
                    all_day: ev.all_day,
                    is_recurring: true,
                    occurrence_index: idx as u32,
                });
            }
            // Some RRULEs may exclude the DTSTART itself — e.g.
            // `FREQ=WEEKLY;BYDAY=MO` when DTSTART is Tuesday. The
            // expander handles that. Nothing else to do.
            continue;
        }
        // Non-recurring: emit only if it overlaps the window.
        let end = ev.dtend_utc.unwrap_or(ev.dtstart_utc + 3600);
        if ev.dtstart_utc < q.to && end >= q.from {
            out.push(EventOccurrence {
                id: ev.id,
                calendar_id: ev.calendar_id,
                uid: ev.uid.clone(),
                summary: ev.summary.clone(),
                description: ev.description.clone(),
                location: ev.location.clone(),
                dtstart_utc: ev.dtstart_utc,
                dtend_utc: ev.dtend_utc,
                all_day: ev.all_day,
                is_recurring: false,
                occurrence_index: 0,
            });
        }
    }
    out.sort_by_key(|e| e.dtstart_utc);
    Ok(Json(out))
}

async fn get_event(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<CalEvent>> {
    s.vault.require_unlocked()?;
    Ok(Json(s.db.get_cal_event(id)?))
}
