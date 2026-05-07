//! Single-row backup schedule. Read by the boot-time tick loop and
//! written by the Settings → Backups schedule form. Validation lives
//! in the storage layer so HTTP and tick paths share one source of
//! truth for what shapes are allowed.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::Db;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupFrequency {
    Daily,
    Weekly,
}

impl BackupFrequency {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "daily" => Ok(Self::Daily),
            "weekly" => Ok(Self::Weekly),
            other => Err(Error::BadRequest(format!(
                "unknown backup frequency: {other} (expected daily or weekly)"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSchedule {
    pub enabled: bool,
    pub frequency: BackupFrequency,
    pub hour: u32,
    pub minute: u32,
    /// 0 = Sunday … 6 = Saturday. Only honoured when frequency=Weekly.
    pub day_of_week: u32,
    /// How many local tarballs to keep after a successful backup.
    /// 0 = unlimited (no pruning).
    pub retention_count: u32,
    /// Unix seconds of last auto-fire. Used to debounce within the
    /// scheduled minute window.
    pub last_run_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct UpdateBackupSchedule {
    pub enabled: bool,
    pub frequency: BackupFrequency,
    pub hour: u32,
    pub minute: u32,
    pub day_of_week: u32,
    pub retention_count: u32,
}

impl UpdateBackupSchedule {
    /// Same shape-checks the SQL CHECK constraints would produce, but
    /// surfaced as user-friendly BadRequest errors instead of an
    /// opaque sqlite constraint violation.
    pub fn validate(&self) -> Result<()> {
        if self.hour > 23 {
            return Err(Error::BadRequest("hour must be 0..=23".into()));
        }
        if self.minute > 59 {
            return Err(Error::BadRequest("minute must be 0..=59".into()));
        }
        if self.day_of_week > 6 {
            return Err(Error::BadRequest("day_of_week must be 0..=6".into()));
        }
        if self.retention_count > 365 {
            return Err(Error::BadRequest(
                "retention_count must be 0..=365 — keep more by archiving off-site".into(),
            ));
        }
        Ok(())
    }
}

impl Db {
    pub fn get_backup_schedule(&self) -> Result<BackupSchedule> {
        let conn = self.pool().get()?;
        conn.query_row(
            "SELECT enabled, frequency, hour, minute, day_of_week, retention_count, last_run_at
             FROM backup_schedule WHERE id = 1",
            [],
            |r| {
                let frequency: String = r.get(1)?;
                Ok(BackupSchedule {
                    enabled: r.get::<_, i64>(0)? != 0,
                    frequency: BackupFrequency::parse(&frequency).unwrap_or(BackupFrequency::Daily),
                    hour: r.get::<_, i64>(2)? as u32,
                    minute: r.get::<_, i64>(3)? as u32,
                    day_of_week: r.get::<_, i64>(4)? as u32,
                    retention_count: r.get::<_, i64>(5)? as u32,
                    last_run_at: r.get(6)?,
                })
            },
        )
        .map_err(|e| Error::Other(anyhow::anyhow!("read backup_schedule: {e}")))
    }

    pub fn update_backup_schedule(&self, new: &UpdateBackupSchedule) -> Result<BackupSchedule> {
        new.validate()?;
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE backup_schedule
             SET enabled = ?1, frequency = ?2, hour = ?3, minute = ?4,
                 day_of_week = ?5, retention_count = ?6
             WHERE id = 1",
            params![
                i32::from(new.enabled),
                new.frequency.as_str(),
                new.hour,
                new.minute,
                new.day_of_week,
                new.retention_count,
            ],
        )?;
        self.get_backup_schedule()
    }

    pub fn record_backup_schedule_fired(&self, at: i64) -> Result<()> {
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE backup_schedule SET last_run_at = ?1 WHERE id = 1",
            params![at],
        )?;
        Ok(())
    }
}

/// Pure decision: given a schedule and a current time, should we
/// fire the backup right now?
///
/// Rules:
///   * `enabled` must be true.
///   * Current minute must equal scheduled (hour, minute).
///   * For `Weekly`, current weekday must equal `day_of_week`.
///   * Must not have fired within the last 60 minutes (debounce so
///     a 60s tick during the scheduled minute window doesn't fire twice).
///
/// Tested without touching the DB by feeding hand-rolled `chrono`
/// values — see `tests` below.
pub fn should_fire_now(
    sched: &BackupSchedule,
    now: chrono::DateTime<chrono::Local>,
) -> bool {
    use chrono::{Datelike, Timelike};
    if !sched.enabled {
        return false;
    }
    if now.hour() != sched.hour || now.minute() != sched.minute {
        return false;
    }
    if sched.frequency == BackupFrequency::Weekly {
        // chrono's `weekday().num_days_from_sunday()` is 0=Sunday … 6=Saturday,
        // matching our schema. `weekday()` itself uses ISO 1=Mon … 7=Sun.
        if now.weekday().num_days_from_sunday() != sched.day_of_week {
            return false;
        }
    }
    if let Some(last) = sched.last_run_at {
        // 60-minute debounce — covers an OS clock jump or a tick
        // that runs slightly before/after the target minute.
        let now_unix = now.timestamp();
        if now_unix - last < 60 * 60 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sched_at(hour: u32, minute: u32, freq: BackupFrequency, dow: u32) -> BackupSchedule {
        BackupSchedule {
            enabled: true,
            frequency: freq,
            hour,
            minute,
            day_of_week: dow,
            retention_count: 7,
            last_run_at: None,
        }
    }

    fn local(y: i32, m: u32, d: u32, h: u32, min: u32) -> chrono::DateTime<chrono::Local> {
        chrono::Local
            .with_ymd_and_hms(y, m, d, h, min, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn does_not_fire_when_disabled() {
        let mut s = sched_at(3, 0, BackupFrequency::Daily, 0);
        s.enabled = false;
        let now = local(2026, 4, 26, 3, 0);
        assert!(!should_fire_now(&s, now));
    }

    #[test]
    fn fires_on_exact_daily_minute() {
        let s = sched_at(3, 0, BackupFrequency::Daily, 0);
        let now = local(2026, 4, 26, 3, 0); // any day, 03:00
        assert!(should_fire_now(&s, now));
    }

    #[test]
    fn does_not_fire_on_wrong_minute() {
        let s = sched_at(3, 0, BackupFrequency::Daily, 0);
        let now = local(2026, 4, 26, 3, 1); // 03:01 — minute mismatch
        assert!(!should_fire_now(&s, now));
    }

    #[test]
    fn weekly_only_fires_on_matching_weekday() {
        // 2026-04-26 was a Sunday (day_of_week=0).
        let s_sunday = sched_at(3, 0, BackupFrequency::Weekly, 0);
        let s_monday = sched_at(3, 0, BackupFrequency::Weekly, 1);
        let sunday_03 = local(2026, 4, 26, 3, 0);
        assert!(should_fire_now(&s_sunday, sunday_03));
        assert!(!should_fire_now(&s_monday, sunday_03));
    }

    #[test]
    fn does_not_double_fire_within_an_hour() {
        let mut s = sched_at(3, 0, BackupFrequency::Daily, 0);
        let now = local(2026, 4, 26, 3, 0);
        s.last_run_at = Some(now.timestamp() - 30); // 30s ago
        assert!(!should_fire_now(&s, now));
    }

    #[test]
    fn fires_when_last_run_was_yesterday() {
        let mut s = sched_at(3, 0, BackupFrequency::Daily, 0);
        let now = local(2026, 4, 26, 3, 0);
        s.last_run_at = Some(now.timestamp() - 24 * 3600); // 24h ago
        assert!(should_fire_now(&s, now));
    }

    #[test]
    fn frequency_parse_round_trips() {
        assert_eq!(
            BackupFrequency::parse("daily").unwrap().as_str(),
            "daily"
        );
        assert_eq!(
            BackupFrequency::parse("weekly").unwrap().as_str(),
            "weekly"
        );
        assert!(BackupFrequency::parse("yearly").is_err());
    }

    #[test]
    fn update_validates_field_ranges() {
        let mut u = UpdateBackupSchedule {
            enabled: true,
            frequency: BackupFrequency::Daily,
            hour: 25,
            minute: 0,
            day_of_week: 0,
            retention_count: 7,
        };
        assert!(u.validate().is_err());
        u.hour = 0;
        u.minute = 60;
        assert!(u.validate().is_err());
        u.minute = 0;
        u.day_of_week = 7;
        assert!(u.validate().is_err());
        u.day_of_week = 0;
        u.retention_count = 1000;
        assert!(u.validate().is_err());
        u.retention_count = 7;
        assert!(u.validate().is_ok());
    }
}
