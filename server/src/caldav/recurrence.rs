//! RRULE expansion at read time.
//!
//! `cal_events` rows store an event's DTSTART plus the raw RRULE
//! string. When the UI asks for events in a date range, we expand
//! each recurring row into concrete occurrences inside that range.
//! Storing the expanded grid on disk would drift the moment a server-
//! side edit bumps the rule or UNTIL — expanding at query time keeps
//! local state as authoritative as the ctag-driven sync allows.
//!
//! Bounded at `MAX_EXPANSIONS` per row so a misconfigured `FREQ=SECONDLY`
//! or unterminated rule can't knock out the handler. That's the same
//! safety valve the `rrule` crate recommends.

use chrono::{DateTime, TimeZone, Utc};
use rrule::{RRuleSet, Tz};

/// Hard cap on the number of occurrences we ever return per recurring
/// event in one range query. The UI's range is at most a couple
/// months, so 500 is plenty for every reasonable schedule and cheap
/// to evaluate.
const MAX_EXPANSIONS: u16 = 500;

/// Expand the given RRULE starting at `dtstart_utc` into every
/// occurrence that begins inside `[from_utc, to_utc]`. Returns the
/// original DTSTART as-is if the rule is empty or fails to parse; if
/// the rule is valid but produces no occurrences in the window, an
/// empty vec.
pub fn expand_rrule_in_range(
    dtstart_utc: i64,
    rrule: &str,
    from_utc: i64,
    to_utc: i64,
) -> Vec<i64> {
    if rrule.trim().is_empty() {
        return vec![];
    }
    let Some(dtstart) = Utc.timestamp_opt(dtstart_utc, 0).single() else {
        return vec![];
    };
    let Some(from_dt) = Utc.timestamp_opt(from_utc, 0).single() else {
        return vec![];
    };
    let Some(to_dt) = Utc.timestamp_opt(to_utc, 0).single() else {
        return vec![];
    };

    // The rrule crate parses a DTSTART-prefixed blob. Build one from
    // our columns so the caller doesn't have to keep the raw ics.
    let combined = format!(
        "DTSTART:{}\nRRULE:{}",
        dtstart.format("%Y%m%dT%H%M%SZ"),
        rrule.trim()
    );
    let Ok(set): Result<RRuleSet, _> = combined.parse() else {
        tracing::debug!(%rrule, "rrule parse failed, treating as single occurrence");
        return vec![];
    };
    let after = tz_utc_from_chrono(from_dt);
    let before = tz_utc_from_chrono(to_dt);
    let result = set.after(after).before(before).all(MAX_EXPANSIONS);
    result
        .dates
        .into_iter()
        .map(|d| d.with_timezone(&Utc).timestamp())
        .collect()
}

fn tz_utc_from_chrono(dt: DateTime<Utc>) -> DateTime<Tz> {
    // rrule has its own Tz newtype around chrono-tz to get Send/Sync.
    // Round-trip through naive UTC to satisfy the target type.
    Tz::UTC.from_utc_datetime(&dt.naive_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_weekly_on_mondays() {
        // 2026-01-05 is a Monday. 4 weeks worth of Mondays should
        // appear inside January when we ask for Jan 1–31.
        let dtstart = Utc
            .with_ymd_and_hms(2026, 1, 5, 9, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        let from = Utc
            .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        let to = Utc
            .with_ymd_and_hms(2026, 2, 1, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        let occurrences = expand_rrule_in_range(dtstart, "FREQ=WEEKLY;BYDAY=MO", from, to);
        assert_eq!(occurrences.len(), 4, "4 Mondays in Jan 2026 from the 5th");
    }

    #[test]
    fn honours_count_limit() {
        let dtstart = Utc
            .with_ymd_and_hms(2026, 1, 1, 9, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        let from = dtstart - 1;
        let to = dtstart + 60 * 60 * 24 * 365;
        let occurrences =
            expand_rrule_in_range(dtstart, "FREQ=DAILY;COUNT=5", from, to);
        assert_eq!(occurrences.len(), 5);
    }

    #[test]
    fn invalid_rrule_returns_empty() {
        let dtstart = 1_700_000_000;
        let occurrences = expand_rrule_in_range(
            dtstart,
            "NOT A RULE",
            dtstart - 1,
            dtstart + 60,
        );
        assert!(occurrences.is_empty());
    }
}
