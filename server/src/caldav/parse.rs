//! iCalendar parsing — turns a raw .ics body into the fields we store.
//!
//! We only care about VEVENTs here; VTODO and VJOURNAL are out of
//! scope for the first cut. A single .ics resource can contain more
//! than one VEVENT (most commonly for recurring-event overrides —
//! RECURRENCE-ID matches back to the base event). We keep the first
//! event that has no RECURRENCE-ID as the canonical row; override
//! components are a follow-up.

use anyhow::anyhow;
use chrono::{Datelike, TimeZone, Utc};
use icalendar::{Calendar, CalendarComponent, Component, DatePerhapsTime, EventLike};

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct ParsedEvent {
    pub uid: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub dtstart_utc: i64,
    pub dtend_utc: Option<i64>,
    pub all_day: bool,
    pub rrule: Option<String>,
}

pub fn parse_ics(raw: &str) -> Result<ParsedEvent> {
    let cal: Calendar = raw
        .parse()
        .map_err(|e| Error::Other(anyhow!("ics parse: {e}")))?;

    // Prefer a VEVENT without RECURRENCE-ID (the base event). Fall
    // back to the first VEVENT if the resource contains only
    // overrides — rare, but possible.
    let mut base: Option<&icalendar::Event> = None;
    let mut first: Option<&icalendar::Event> = None;
    for comp in cal.components.iter() {
        if let CalendarComponent::Event(e) = comp {
            if first.is_none() {
                first = Some(e);
            }
            if e.property_value("RECURRENCE-ID").is_none() {
                base = Some(e);
                break;
            }
        }
    }
    let event = base
        .or(first)
        .ok_or_else(|| Error::Other(anyhow!("ics has no VEVENT")))?;

    let uid = event
        .get_uid()
        .ok_or_else(|| Error::Other(anyhow!("VEVENT missing UID")))?
        .to_owned();

    let dtstart = event
        .get_start()
        .ok_or_else(|| Error::Other(anyhow!("VEVENT missing DTSTART")))?;
    let (dtstart_utc, all_day) = to_unix(dtstart);
    let dtend_utc = event.get_end().map(|d| to_unix(d).0);

    let summary = event.get_summary().map(normalize_text);
    let description = event.get_description().map(normalize_text);
    let location = event.get_location().map(normalize_text);

    fn normalize_text(s: &str) -> String {
        // iCalendar line-folding escapes literal "\n" sequences. The
        // icalendar crate already unfolds on parse for most fields
        // but we still see backslash-escaped `\N` in descriptions.
        s.replace("\\n", "\n")
            .replace("\\N", "\n")
            .replace("\\,", ",")
    }
    let rrule = event.property_value("RRULE").map(|s| s.to_owned());

    Ok(ParsedEvent {
        uid,
        summary,
        description,
        location,
        dtstart_utc,
        dtend_utc,
        all_day,
        rrule,
    })
}

/// Convert a `DatePerhapsTime` to (unix_seconds, is_all_day). All-day
/// VEVENTs carry only a date — we pin them to 00:00 UTC of that date
/// and let the client render them in the local zone.
fn to_unix(dpt: DatePerhapsTime) -> (i64, bool) {
    match dpt {
        DatePerhapsTime::Date(d) => {
            let ts = Utc
                .with_ymd_and_hms(d.year(), d.month(), d.day(), 0, 0, 0)
                .single()
                .map(|t| t.timestamp())
                .unwrap_or(0);
            (ts, true)
        }
        DatePerhapsTime::DateTime(calendar_dt) => {
            // icalendar preserves Utc / Floating / WithTimezone. The
            // chrono-tz-backed helper resolves Utc + WithTimezone
            // cleanly; Floating (timezone-less) returns None and we
            // assume UTC as a pragmatic default for a client that
            // defaults to user-local rendering anyway.
            if let Some(utc) = calendar_dt.try_into_utc() {
                return (utc.timestamp(), false);
            }
            if let icalendar::CalendarDateTime::Floating(naive) = calendar_dt {
                return (Utc.from_utc_datetime(&naive).timestamp(), false);
            }
            (0, false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
BEGIN:VEVENT\r\n\
UID:abc-123\r\n\
DTSTAMP:20260101T000000Z\r\n\
DTSTART:20260101T090000Z\r\n\
DTEND:20260101T100000Z\r\n\
SUMMARY:Team sync\r\n\
DESCRIPTION:Weekly standup\\nbring coffee\r\n\
LOCATION:Room 2\r\n\
RRULE:FREQ=WEEKLY;BYDAY=MO\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

    #[test]
    fn parses_basic_vevent() {
        let p = parse_ics(SAMPLE).unwrap();
        assert_eq!(p.uid, "abc-123");
        assert_eq!(p.summary.as_deref(), Some("Team sync"));
        assert_eq!(p.location.as_deref(), Some("Room 2"));
        assert_eq!(p.description.as_deref(), Some("Weekly standup\nbring coffee"));
        assert_eq!(p.rrule.as_deref(), Some("FREQ=WEEKLY;BYDAY=MO"));
        assert!(!p.all_day);
        assert!(p.dtstart_utc > 0);
        assert_eq!(p.dtend_utc.unwrap() - p.dtstart_utc, 3600);
    }

    #[test]
    fn parses_all_day_event() {
        let raw = "BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
BEGIN:VEVENT\r\n\
UID:all-day\r\n\
DTSTAMP:20260101T000000Z\r\n\
DTSTART;VALUE=DATE:20260101\r\n\
SUMMARY:New Year\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";
        let p = parse_ics(raw).unwrap();
        assert!(p.all_day);
        assert_eq!(p.summary.as_deref(), Some("New Year"));
    }
}
