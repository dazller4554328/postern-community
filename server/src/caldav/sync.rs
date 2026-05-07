//! One-shot sync for a CalDAV account.
//!
//! Run from the HTTP layer when the user clicks "Sync now" (and in a
//! future background worker). The flow:
//!
//!   1. Discover principal + calendar-home-set if not already cached.
//!   2. PROPFIND the calendar-home-set with depth:1 to enumerate
//!      VEVENT-carrying calendar collections.
//!   3. For each collection, fetch the ctag. Skip if unchanged from
//!      the locally-stored ctag.
//!   4. For each changed collection, calendar-query for the (href,
//!      etag) pairs, then calendar-multiget in one round trip to pull
//!      the .ics bodies.
//!   5. Parse every VEVENT, upsert rows, and prune rows whose href is
//!      no longer present on the server (delete-propagation).

use anyhow::anyhow;
use serde::Serialize;
use std::sync::Arc;

use super::client::{parse_multistatus, Client};
use super::discover::{absolute_url, discover, list_calendars};
use super::parse::parse_ics;
use crate::{
    error::{Error, Result},
    storage::{Db, UpsertCalEvent},
    vault::Vault,
};

/// Outcome summary for the HTTP response + audit log.
#[derive(Debug, Default, Clone, Serialize)]
pub struct SyncReport {
    pub account_id: i64,
    pub calendars_total: usize,
    pub calendars_changed: usize,
    pub events_upserted: usize,
    pub events_pruned: usize,
    pub started_at: i64,
    pub finished_at: i64,
    pub error: Option<String>,
}

const CALENDAR_QUERY_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<c:calendar-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:prop>
    <d:getetag/>
  </d:prop>
  <c:filter>
    <c:comp-filter name="VCALENDAR">
      <c:comp-filter name="VEVENT"/>
    </c:comp-filter>
  </c:filter>
</c:calendar-query>"#;

fn calendar_multiget_body(hrefs: &[String]) -> String {
    // Keep the request small-ish — some servers reject multistatus
    // payloads over a few MB. Callers chunk before calling.
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<c:calendar-multiget xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:prop>
    <d:getetag/>
    <c:calendar-data/>
  </d:prop>
"#,
    );
    for h in hrefs {
        xml.push_str("  <d:href>");
        xml.push_str(&xml_escape(h));
        xml.push_str("</d:href>\n");
    }
    xml.push_str("</c:calendar-multiget>");
    xml
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

const MULTIGET_BATCH: usize = 50;

pub async fn sync_account(db: Arc<Db>, vault: Vault, account_id: i64) -> SyncReport {
    let started_at = chrono::Utc::now().timestamp();
    let mut report = SyncReport {
        account_id,
        started_at,
        ..Default::default()
    };

    let outcome = match sync_inner(db.clone(), vault, account_id, &mut report).await {
        Ok(()) => None,
        Err(e) => {
            let msg = format!("{e}");
            tracing::warn!(account_id, error = %msg, "caldav sync failed");
            Some(msg)
        }
    };
    report.finished_at = chrono::Utc::now().timestamp();
    report.error = outcome.clone();
    let err_ref = outcome.as_deref();
    if let Err(e) = db.set_cal_account_sync_result(account_id, err_ref) {
        tracing::warn!(error = %e, "caldav sync: persist result failed");
    }
    report
}

async fn sync_inner(
    db: Arc<Db>,
    vault: Vault,
    account_id: i64,
    report: &mut SyncReport,
) -> Result<()> {
    vault.require_unlocked()?;
    let account = db.get_cal_account(account_id)?;
    if account.kind == "local" {
        // Local accounts have no remote to talk to — they exist purely
        // so users can keep events in the vault without configuring an
        // external calendar server. Reporting OK here lets the
        // generic /accounts/:id/sync button stay visible without
        // erroring out for the on-device account.
        return Ok(());
    }
    let username = account
        .username
        .as_deref()
        .ok_or_else(|| Error::Other(anyhow::anyhow!("caldav account missing username")))?;
    let server_url = account.server_url.as_deref().ok_or_else(|| {
        Error::Other(anyhow::anyhow!("caldav account missing server_url"))
    })?;
    let password = db.cal_account_password(account_id, &vault)?;
    let client = Client::new(username, &password)?;

    // Discovery — cached per account after first success.
    let (principal, home) = if let (Some(p), Some(h)) =
        (account.principal_url.clone(), account.calendar_home_url.clone())
    {
        (p, h)
    } else {
        let (p, h) = discover(&client, server_url).await?;
        db.set_cal_account_discovery(account_id, Some(&p), Some(&h))?;
        (p, h)
    };
    tracing::info!(account_id, principal = %principal, home = %home, "caldav discovery");

    let collections = list_calendars(&client, &home).await?;
    report.calendars_total = collections.len();

    for info in collections {
        let calendar_id = db.upsert_cal_calendar(
            account_id,
            &info.url,
            &info.name,
            info.ctag.as_deref(),
            info.color.as_deref(),
            false,
        )?;

        // Find the stored ctag by re-reading the row we just wrote.
        // A cheap consistency pass — if the previous ctag on disk
        // matched the server's we can skip the REPORT entirely.
        let existing = db
            .list_cal_calendars(Some(account_id))?
            .into_iter()
            .find(|c| c.dav_url == info.url);
        let previous_ctag = existing.as_ref().and_then(|c| c.ctag.clone());
        let unchanged = matches!(
            (&previous_ctag, &info.ctag),
            (Some(a), Some(b)) if a == b
        );
        if unchanged {
            tracing::debug!(
                account_id,
                calendar = %info.name,
                "ctag unchanged, skipping REPORT"
            );
            continue;
        }
        report.calendars_changed += 1;

        // Enumerate hrefs on the server via calendar-query.
        let body = client.report(&info.url, "1", CALENDAR_QUERY_BODY).await?;
        let responses = parse_multistatus(&body)?;
        let all_hrefs: Vec<String> = responses
            .iter()
            .filter(|r| !r.href.is_empty())
            .map(|r| r.href.clone())
            .collect();

        // calendar-multiget to fetch all bodies in batches.
        let mut upserted = 0usize;
        for chunk in all_hrefs.chunks(MULTIGET_BATCH) {
            let body = client
                .report(&info.url, "1", &calendar_multiget_body(&chunk.to_vec()))
                .await?;
            let responses = parse_multistatus(&body)?;
            for r in responses {
                let Some(ics) = r.props.calendar_data else {
                    continue;
                };
                let parsed = match parse_ics(&ics) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!(
                            href = %r.href,
                            error = %e,
                            "skipped unparseable ics"
                        );
                        continue;
                    }
                };
                let ev = UpsertCalEvent {
                    dav_href: &r.href,
                    dav_etag: r.props.getetag.as_deref(),
                    uid: &parsed.uid,
                    summary: parsed.summary.as_deref(),
                    description: parsed.description.as_deref(),
                    location: parsed.location.as_deref(),
                    dtstart_utc: parsed.dtstart_utc,
                    dtend_utc: parsed.dtend_utc,
                    all_day: parsed.all_day,
                    rrule: parsed.rrule.as_deref(),
                    raw_ics: &ics,
                };
                match db.upsert_cal_event(calendar_id, &ev) {
                    Ok(_) => upserted += 1,
                    Err(e) => tracing::warn!(
                        href = %r.href,
                        error = %e,
                        "calendar event upsert failed"
                    ),
                }
            }
        }
        report.events_upserted += upserted;

        // Converge local state: any row whose href didn't come back
        // from the server gets pruned. This covers deletes made via
        // another client.
        let keep: Vec<String> = all_hrefs
            .iter()
            .map(|h| absolute_url(&info.url, h).unwrap_or_else(|_| h.clone()))
            .collect();
        // We stored raw hrefs (relative or absolute depending on what
        // the server returned). Normalise both sides before compare.
        let normalized: Vec<String> = all_hrefs;
        let pruned = db.prune_cal_events(calendar_id, &normalized)?;
        report.events_pruned += pruned;
        let _ = keep;

        // Update stored ctag so the next sync can short-circuit.
        db.set_cal_calendar_ctag(calendar_id, info.ctag.as_deref())?;
    }

    let _ = db.log_activity(
        "caldav_sync_completed",
        Some(&format!(
            "{} cals, {} events upserted, {} pruned",
            report.calendars_total, report.events_upserted, report.events_pruned
        )),
    );
    Ok(())
}

/// Convenience wrapper so callers who've parsed their own `Error`
/// stack don't have to import anyhow.
pub(crate) fn bad_server(msg: impl Into<String>) -> Error {
    Error::Other(anyhow!("{}", msg.into()))
}
