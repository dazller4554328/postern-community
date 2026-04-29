//! Local reminders storage.
//!
//! Plain CRUD over the `reminders` table plus the due-lookup the
//! background poller relies on. Recurring reminders advance in-place
//! when marked done — see `Db::mark_reminder_done`.
//!
//! Everything here operates on the vault-encrypted SQLCipher DB, so
//! the HTTP layer must have already passed `require_unlocked()`.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::Db;
use crate::error::{Error, Result};

/// Recurrence cadence. `None` is the default — one-shot reminders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReminderRepeat {
    None,
    Daily,
    Weekly,
    Monthly,
}

impl ReminderRepeat {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReminderRepeat::None => "none",
            ReminderRepeat::Daily => "daily",
            ReminderRepeat::Weekly => "weekly",
            ReminderRepeat::Monthly => "monthly",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "none" => ReminderRepeat::None,
            "daily" => ReminderRepeat::Daily,
            "weekly" => ReminderRepeat::Weekly,
            "monthly" => ReminderRepeat::Monthly,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Reminder {
    pub id: i64,
    pub title: String,
    pub notes: Option<String>,
    pub due_at_utc: i64,
    pub repeat: ReminderRepeat,
    pub done: bool,
    pub notified: bool,
    pub snoozed_until_utc: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewReminder {
    pub title: String,
    pub notes: Option<String>,
    pub due_at_utc: i64,
    #[serde(default = "default_repeat")]
    pub repeat: ReminderRepeat,
}

fn default_repeat() -> ReminderRepeat {
    ReminderRepeat::None
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateReminder {
    pub title: Option<String>,
    pub notes: Option<Option<String>>, // double-option so `null` can clear
    pub due_at_utc: Option<i64>,
    pub repeat: Option<ReminderRepeat>,
}

impl Db {
    pub fn insert_reminder(&self, new: &NewReminder) -> Result<Reminder> {
        let title = new.title.trim();
        if title.is_empty() {
            return Err(Error::BadRequest("title is required".into()));
        }
        let notes = new.notes.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let conn = self.pool().get()?;
        conn.execute(
            "INSERT INTO reminders (title, notes, due_at_utc, repeat)
             VALUES (?1, ?2, ?3, ?4)",
            params![title, notes, new.due_at_utc, new.repeat.as_str()],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        self.get_reminder(id)
    }

    pub fn list_reminders(&self, include_done: bool) -> Result<Vec<Reminder>> {
        let conn = self.pool().get()?;
        let sql = if include_done {
            "SELECT id, title, notes, due_at_utc, repeat, done, notified,
                    snoozed_until_utc, created_at, updated_at
               FROM reminders
               ORDER BY due_at_utc ASC"
        } else {
            "SELECT id, title, notes, due_at_utc, repeat, done, notified,
                    snoozed_until_utc, created_at, updated_at
               FROM reminders
               WHERE done = 0
               ORDER BY due_at_utc ASC"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], row_to_reminder)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Reminders overlapping [from_utc, to_utc]. Used by the calendar
    /// month/day grid. Includes done ones so a user who ticked
    /// something off still sees it on the day it was scheduled.
    pub fn list_reminders_in_range(&self, from_utc: i64, to_utc: i64) -> Result<Vec<Reminder>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, title, notes, due_at_utc, repeat, done, notified,
                    snoozed_until_utc, created_at, updated_at
               FROM reminders
              WHERE due_at_utc >= ?1 AND due_at_utc <= ?2
              ORDER BY due_at_utc ASC",
        )?;
        let rows = stmt.query_map(params![from_utc, to_utc], row_to_reminder)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_reminder(&self, id: i64) -> Result<Reminder> {
        let conn = self.pool().get()?;
        conn.query_row(
            "SELECT id, title, notes, due_at_utc, repeat, done, notified,
                    snoozed_until_utc, created_at, updated_at
               FROM reminders WHERE id = ?1",
            params![id],
            row_to_reminder,
        )
        .optional()?
        .ok_or(Error::NotFound)
    }

    pub fn update_reminder(&self, id: i64, patch: &UpdateReminder) -> Result<Reminder> {
        // Load-modify-save so the existing row's fields survive when
        // the patch leaves them unset.
        let current = self.get_reminder(id)?;

        let title = match &patch.title {
            Some(t) => {
                let trimmed = t.trim();
                if trimmed.is_empty() {
                    return Err(Error::BadRequest("title cannot be empty".into()));
                }
                trimmed.to_string()
            }
            None => current.title,
        };
        let notes: Option<String> = match &patch.notes {
            Some(None) => None,
            Some(Some(s)) => {
                let t = s.trim();
                if t.is_empty() { None } else { Some(t.to_string()) }
            }
            None => current.notes,
        };
        let due = patch.due_at_utc.unwrap_or(current.due_at_utc);
        let repeat = patch.repeat.unwrap_or(current.repeat);

        // Any field change clears the notified flag so the user gets
        // re-pinged for the new due time.
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE reminders
                SET title = ?1, notes = ?2, due_at_utc = ?3, repeat = ?4,
                    notified = 0, snoozed_until_utc = NULL,
                    updated_at = strftime('%s','now')
              WHERE id = ?5",
            params![title, notes, due, repeat.as_str(), id],
        )?;
        drop(conn);
        self.get_reminder(id)
    }

    pub fn delete_reminder(&self, id: i64) -> Result<()> {
        let conn = self.pool().get()?;
        let n = conn.execute("DELETE FROM reminders WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    /// Mark done. For recurring reminders, advance due_at_utc by one
    /// period and keep done=false so the next occurrence appears.
    pub fn mark_reminder_done(&self, id: i64) -> Result<Reminder> {
        let current = self.get_reminder(id)?;
        let conn = self.pool().get()?;
        match current.repeat {
            ReminderRepeat::None => {
                conn.execute(
                    "UPDATE reminders
                        SET done = 1, notified = 1,
                            snoozed_until_utc = NULL,
                            updated_at = strftime('%s','now')
                      WHERE id = ?1",
                    params![id],
                )?;
            }
            r => {
                let next = advance_due(current.due_at_utc, r);
                conn.execute(
                    "UPDATE reminders
                        SET due_at_utc = ?1, notified = 0,
                            snoozed_until_utc = NULL,
                            updated_at = strftime('%s','now')
                      WHERE id = ?2",
                    params![next, id],
                )?;
            }
        }
        drop(conn);
        self.get_reminder(id)
    }

    pub fn snooze_reminder(&self, id: i64, until_utc: i64) -> Result<Reminder> {
        // Ensure exists and clear notified so the snoozed-to time
        // triggers another fire.
        let _ = self.get_reminder(id)?;
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE reminders
                SET snoozed_until_utc = ?1, notified = 0,
                    updated_at = strftime('%s','now')
              WHERE id = ?2",
            params![until_utc, id],
        )?;
        drop(conn);
        self.get_reminder(id)
    }

    /// Reminders whose due time (or snooze end) has passed and which
    /// have not yet fired a notification. Used by the poller.
    pub fn list_due_reminders(&self, now_utc: i64) -> Result<Vec<Reminder>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, title, notes, due_at_utc, repeat, done, notified,
                    snoozed_until_utc, created_at, updated_at
               FROM reminders
              WHERE done = 0
                AND notified = 0
                AND MAX(due_at_utc, COALESCE(snoozed_until_utc, 0)) <= ?1
              ORDER BY due_at_utc ASC",
        )?;
        let rows = stmt.query_map(params![now_utc], row_to_reminder)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn mark_reminder_notified(&self, id: i64) -> Result<()> {
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE reminders
                SET notified = 1, updated_at = strftime('%s','now')
              WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }
}

/// Advance a due time by one period. Uses chrono for month arithmetic
/// so "Jan 31 monthly" lands on Feb 28/29, Mar 31, etc. instead of
/// silently drifting by 30 days.
fn advance_due(due_at_utc: i64, repeat: ReminderRepeat) -> i64 {
    use chrono::{DateTime, Datelike, Duration, Utc};
    let dt = DateTime::<Utc>::from_timestamp(due_at_utc, 0).unwrap_or_else(Utc::now);
    match repeat {
        ReminderRepeat::None => due_at_utc,
        ReminderRepeat::Daily => (dt + Duration::days(1)).timestamp(),
        ReminderRepeat::Weekly => (dt + Duration::weeks(1)).timestamp(),
        ReminderRepeat::Monthly => {
            // Roll month forward, then clamp day-of-month so e.g.
            // Jan 31 → Feb 28 instead of failing validation and
            // falling through to +30 days.
            let (ny, nm) = if dt.month() == 12 {
                (dt.year() + 1, 1)
            } else {
                (dt.year(), dt.month() + 1)
            };
            let day = dt.day().min(last_day_of_month(ny, nm));
            let date = chrono::NaiveDate::from_ymd_opt(ny, nm, day);
            let time = dt.time();
            date.and_then(|d| d.and_time(time).and_local_timezone(Utc).single())
                .map(|d| d.timestamp())
                .unwrap_or_else(|| (dt + Duration::days(30)).timestamp())
        }
    }
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    use chrono::{Datelike, NaiveDate};
    // First day of the *next* month, minus one day.
    let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    NaiveDate::from_ymd_opt(ny, nm, 1)
        .and_then(|d| d.pred_opt())
        .map(|d| d.day())
        .unwrap_or(28)
}

fn row_to_reminder(r: &rusqlite::Row) -> rusqlite::Result<Reminder> {
    let repeat_str: String = r.get(4)?;
    let repeat = ReminderRepeat::parse(&repeat_str).unwrap_or(ReminderRepeat::None);
    Ok(Reminder {
        id: r.get(0)?,
        title: r.get(1)?,
        notes: r.get(2)?,
        due_at_utc: r.get(3)?,
        repeat,
        done: r.get::<_, i64>(5)? != 0,
        notified: r.get::<_, i64>(6)? != 0,
        snoozed_until_utc: r.get(7)?,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn advance_daily_adds_one_day() {
        let t = Utc.with_ymd_and_hms(2026, 1, 15, 9, 0, 0).unwrap().timestamp();
        let next = advance_due(t, ReminderRepeat::Daily);
        assert_eq!(next - t, 86_400);
    }

    #[test]
    fn advance_weekly_adds_seven_days() {
        let t = Utc.with_ymd_and_hms(2026, 1, 15, 9, 0, 0).unwrap().timestamp();
        let next = advance_due(t, ReminderRepeat::Weekly);
        assert_eq!(next - t, 7 * 86_400);
    }

    #[test]
    fn advance_monthly_handles_short_months() {
        // Jan 31 → Feb 28 (2026 is not a leap year).
        let t = Utc.with_ymd_and_hms(2026, 1, 31, 9, 0, 0).unwrap().timestamp();
        let next = advance_due(t, ReminderRepeat::Monthly);
        let expected = Utc
            .with_ymd_and_hms(2026, 2, 28, 9, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(next, expected);
    }

    #[test]
    fn advance_monthly_wraps_year() {
        let t = Utc.with_ymd_and_hms(2026, 12, 15, 9, 0, 0).unwrap().timestamp();
        let next = advance_due(t, ReminderRepeat::Monthly);
        let expected = Utc
            .with_ymd_and_hms(2027, 1, 15, 9, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(next, expected);
    }

    #[test]
    fn repeat_roundtrip() {
        for r in [
            ReminderRepeat::None,
            ReminderRepeat::Daily,
            ReminderRepeat::Weekly,
            ReminderRepeat::Monthly,
        ] {
            assert_eq!(ReminderRepeat::parse(r.as_str()), Some(r));
        }
        assert_eq!(ReminderRepeat::parse("quarterly"), None);
    }
}
