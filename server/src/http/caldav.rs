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
    error::{Error, Result},
    storage::{
        CalAccount, CalCalendar, CalEvent, NewCalAccount, NewLocalEvent, PatchLocalEvent,
    },
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
        .route(
            "/cal/events",
            get(list_events_in_range).post(create_local_event),
        )
        .route(
            "/cal/events/:id",
            get(get_event).patch(update_local_event).delete(delete_local_event),
        )
}

async fn list_accounts(State(s): State<AppState>) -> Result<Json<Vec<CalAccount>>> {
    s.vault.require_unlocked()?;
    // Provision the default local account on first call. Idempotent —
    // a row already present is a no-op. Doing this on the read path
    // (rather than at server boot) means new vaults converge to the
    // expected state the moment a client touches the calendar UI,
    // without any code change to the bootstrap sequence.
    if let Err(e) = s.db.ensure_local_cal_account() {
        tracing::warn!(error = %e, "calendar: local-account bootstrap failed");
    }
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

#[derive(Debug, Deserialize)]
struct CreateLocalEventBody {
    /// Optional — when omitted, falls back to the default local
    /// calendar (created on first /cal/accounts load). Lets the SPA
    /// stay simple: it can post `{summary, dtstart_utc}` and trust
    /// the server to pick a sensible target.
    calendar_id: Option<i64>,
    summary: Option<String>,
    description: Option<String>,
    location: Option<String>,
    dtstart_utc: i64,
    dtend_utc: Option<i64>,
    #[serde(default)]
    all_day: bool,
    rrule: Option<String>,
}

async fn create_local_event(
    State(s): State<AppState>,
    Json(b): Json<CreateLocalEventBody>,
) -> Result<Json<CalEvent>> {
    s.vault.require_unlocked()?;
    let calendar_id = match b.calendar_id {
        Some(id) => {
            let kind = s.db.cal_calendar_account_kind(id)?;
            if kind != "local" {
                return Err(Error::BadRequest(
                    "events can only be written to a local calendar — \
                     CalDAV calendars are read-only in this build"
                        .into(),
                ));
            }
            id
        }
        None => {
            // Fall back to the default local calendar, creating it
            // on demand. The bootstrap normally runs in
            // list_accounts so this branch is rare, but it makes
            // the endpoint robust against UI flows that POST without
            // first GETting the accounts.
            let (_account_id, calendar_id) = s.db.ensure_local_cal_account()?;
            calendar_id
        }
    };
    let new = NewLocalEvent {
        calendar_id,
        summary: b.summary.as_deref(),
        description: b.description.as_deref(),
        location: b.location.as_deref(),
        dtstart_utc: b.dtstart_utc,
        dtend_utc: b.dtend_utc,
        all_day: b.all_day,
        rrule: b.rrule.as_deref(),
    };
    let id = s.db.cal_event_create_local(&new)?;
    Ok(Json(s.db.get_cal_event(id)?))
}

#[derive(Debug, Deserialize, Default)]
struct PatchLocalEventBody {
    summary: Option<String>,
    description: Option<String>,
    location: Option<String>,
    dtstart_utc: Option<i64>,
    dtend_utc: Option<i64>,
    all_day: Option<bool>,
    rrule: Option<String>,
}

async fn update_local_event(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<PatchLocalEventBody>,
) -> Result<Json<CalEvent>> {
    s.vault.require_unlocked()?;
    let existing = s.db.get_cal_event(id)?;
    let kind = s.db.cal_calendar_account_kind(existing.calendar_id)?;
    if kind != "local" {
        return Err(Error::BadRequest(
            "this event lives on a CalDAV calendar; edit it on the source server".into(),
        ));
    }
    let patch = PatchLocalEvent {
        summary: b.summary.as_deref(),
        description: b.description.as_deref(),
        location: b.location.as_deref(),
        dtstart_utc: b.dtstart_utc,
        dtend_utc: b.dtend_utc,
        all_day: b.all_day,
        rrule: b.rrule.as_deref(),
    };
    s.db.cal_event_update_local(id, &patch)?;
    Ok(Json(s.db.get_cal_event(id)?))
}

async fn delete_local_event(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    s.vault.require_unlocked()?;
    let existing = s.db.get_cal_event(id)?;
    let kind = s.db.cal_calendar_account_kind(existing.calendar_id)?;
    if kind != "local" {
        return Err(Error::BadRequest(
            "this event lives on a CalDAV calendar; delete it on the source server".into(),
        ));
    }
    let removed = s.db.cal_event_delete(id)?;
    Ok(Json(serde_json::json!({ "deleted": id, "removed": removed })))
}
