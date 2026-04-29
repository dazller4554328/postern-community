//! Calendar storage — CalDAV accounts, their discovered collections,
//! and the VEVENTs synced into each collection.
//!
//! Mirrors the structure of [`accounts`] so readers who've seen the
//! IMAP side don't need to re-learn the conventions.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::Db;
use crate::error::{Error, Result};
use crate::vault::Vault;

#[derive(Debug, Clone, Serialize)]
pub struct CalAccount {
    pub id: i64,
    pub label: String,
    pub server_url: String,
    pub username: String,
    pub principal_url: Option<String>,
    pub calendar_home_url: Option<String>,
    pub last_sync_at: Option<i64>,
    pub last_sync_error: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewCalAccount {
    pub label: String,
    pub server_url: String,
    pub username: String,
    pub app_password: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalCalendar {
    pub id: i64,
    pub account_id: i64,
    pub dav_url: String,
    pub name: String,
    pub ctag: Option<String>,
    pub color: Option<String>,
    pub read_only: bool,
    pub hidden: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalEvent {
    pub id: i64,
    pub calendar_id: i64,
    pub dav_href: String,
    pub dav_etag: Option<String>,
    pub uid: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub dtstart_utc: i64,
    pub dtend_utc: Option<i64>,
    pub all_day: bool,
    pub rrule: Option<String>,
    pub raw_ics: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Decomposed VEVENT fields, used by the CalDAV sync path when
/// upserting. Kept separate from `CalEvent` so callers don't have to
/// fabricate synthetic `id` / `created_at` values.
#[derive(Debug, Clone)]
pub struct UpsertCalEvent<'a> {
    pub dav_href: &'a str,
    pub dav_etag: Option<&'a str>,
    pub uid: &'a str,
    pub summary: Option<&'a str>,
    pub description: Option<&'a str>,
    pub location: Option<&'a str>,
    pub dtstart_utc: i64,
    pub dtend_utc: Option<i64>,
    pub all_day: bool,
    pub rrule: Option<&'a str>,
    pub raw_ics: &'a str,
}

impl Db {
    // ------ Accounts ------

    pub fn insert_cal_account(&self, new: &NewCalAccount, vault: &Vault) -> Result<CalAccount> {
        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;

        let created_at = chrono::Utc::now().timestamp();
        tx.execute(
            "INSERT INTO cal_accounts (label, server_url, username, credential_ref, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                new.label.trim(),
                new.server_url.trim(),
                new.username.trim(),
                "", // placeholder — rewritten below so the ref embeds the id
                created_at,
            ],
        )?;
        let id = tx.last_insert_rowid();
        let cred_ref = format!("cal:{id}");
        let wrapped = vault.encrypt(new.app_password.as_bytes())?;
        tx.execute(
            "INSERT INTO secrets(ref, ciphertext) VALUES (?1, ?2)
             ON CONFLICT(ref) DO UPDATE SET ciphertext = excluded.ciphertext",
            params![cred_ref, wrapped],
        )?;
        tx.execute(
            "UPDATE cal_accounts SET credential_ref = ?1 WHERE id = ?2",
            params![cred_ref, id],
        )?;
        tx.commit()?;
        self.get_cal_account(id)
    }

    pub fn list_cal_accounts(&self) -> Result<Vec<CalAccount>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, label, server_url, username, principal_url, calendar_home_url,
                    last_sync_at, last_sync_error, created_at
               FROM cal_accounts
               ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], row_to_cal_account)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_cal_account(&self, id: i64) -> Result<CalAccount> {
        let conn = self.pool().get()?;
        conn.query_row(
            "SELECT id, label, server_url, username, principal_url, calendar_home_url,
                    last_sync_at, last_sync_error, created_at
               FROM cal_accounts WHERE id = ?1",
            params![id],
            row_to_cal_account,
        )
        .optional()?
        .ok_or(Error::NotFound)
    }

    pub fn delete_cal_account(&self, id: i64) -> Result<()> {
        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;
        let cred_ref: Option<String> = tx
            .query_row(
                "SELECT credential_ref FROM cal_accounts WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?;
        tx.execute("DELETE FROM cal_accounts WHERE id = ?1", params![id])?;
        if let Some(r) = cred_ref {
            tx.execute("DELETE FROM secrets WHERE ref = ?1", params![r])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn cal_account_password(&self, id: i64, vault: &Vault) -> Result<String> {
        let conn = self.pool().get()?;
        let cred_ref: String = conn.query_row(
            "SELECT credential_ref FROM cal_accounts WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        let ciphertext: Vec<u8> = conn.query_row(
            "SELECT ciphertext FROM secrets WHERE ref = ?1",
            params![cred_ref],
            |r| r.get(0),
        )?;
        let plaintext = vault.decrypt(&ciphertext)?;
        String::from_utf8(plaintext)
            .map_err(|e| Error::Other(anyhow::anyhow!("cal password decode: {e}")))
    }

    pub fn set_cal_account_discovery(
        &self,
        id: i64,
        principal: Option<&str>,
        calendar_home: Option<&str>,
    ) -> Result<()> {
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE cal_accounts
                SET principal_url = COALESCE(?1, principal_url),
                    calendar_home_url = COALESCE(?2, calendar_home_url)
              WHERE id = ?3",
            params![principal, calendar_home, id],
        )?;
        Ok(())
    }

    pub fn set_cal_account_sync_result(&self, id: i64, error: Option<&str>) -> Result<()> {
        let conn = self.pool().get()?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE cal_accounts
                SET last_sync_at = ?1, last_sync_error = ?2
              WHERE id = ?3",
            params![now, error, id],
        )?;
        Ok(())
    }

    // ------ Calendars ------

    /// Upsert a discovered calendar collection. Returns the row id.
    pub fn upsert_cal_calendar(
        &self,
        account_id: i64,
        dav_url: &str,
        name: &str,
        ctag: Option<&str>,
        color: Option<&str>,
        read_only: bool,
    ) -> Result<i64> {
        let conn = self.pool().get()?;
        conn.execute(
            "INSERT INTO cal_calendars (account_id, dav_url, name, ctag, color, read_only)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(account_id, dav_url) DO UPDATE SET
                name = excluded.name,
                ctag = excluded.ctag,
                color = excluded.color,
                read_only = excluded.read_only",
            params![account_id, dav_url, name, ctag, color, i32::from(read_only)],
        )?;
        let id: i64 = conn.query_row(
            "SELECT id FROM cal_calendars WHERE account_id = ?1 AND dav_url = ?2",
            params![account_id, dav_url],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    pub fn list_cal_calendars(&self, account_id: Option<i64>) -> Result<Vec<CalCalendar>> {
        let conn = self.pool().get()?;
        let sql = if account_id.is_some() {
            "SELECT id, account_id, dav_url, name, ctag, color, read_only, hidden, created_at
               FROM cal_calendars WHERE account_id = ?1 ORDER BY name"
        } else {
            "SELECT id, account_id, dav_url, name, ctag, color, read_only, hidden, created_at
               FROM cal_calendars ORDER BY account_id, name"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = if let Some(aid) = account_id {
            stmt.query_map(params![aid], row_to_cal_calendar)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            stmt.query_map([], row_to_cal_calendar)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        Ok(rows)
    }

    pub fn set_cal_calendar_ctag(&self, id: i64, ctag: Option<&str>) -> Result<()> {
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE cal_calendars SET ctag = ?1 WHERE id = ?2",
            params![ctag, id],
        )?;
        Ok(())
    }

    /// Delete any events whose dav_href isn't in the authoritative set
    /// returned by the server. Called after a full REPORT listing to
    /// converge local state to server state.
    pub fn prune_cal_events(&self, calendar_id: i64, keep_hrefs: &[String]) -> Result<usize> {
        let conn = self.pool().get()?;
        if keep_hrefs.is_empty() {
            let n = conn.execute(
                "DELETE FROM cal_events WHERE calendar_id = ?1",
                params![calendar_id],
            )?;
            return Ok(n);
        }
        let placeholders = (0..keep_hrefs.len())
            .map(|i| format!("?{}", i + 2))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "DELETE FROM cal_events
               WHERE calendar_id = ?1 AND dav_href NOT IN ({placeholders})"
        );
        let mut args: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(keep_hrefs.len() + 1);
        args.push(&calendar_id);
        for h in keep_hrefs {
            args.push(h);
        }
        let n = conn.execute(&sql, args.as_slice())?;
        Ok(n)
    }

    // ------ Events ------

    pub fn upsert_cal_event(
        &self,
        calendar_id: i64,
        ev: &UpsertCalEvent<'_>,
    ) -> Result<i64> {
        let conn = self.pool().get()?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO cal_events
                (calendar_id, dav_href, dav_etag, uid, summary, description, location,
                 dtstart_utc, dtend_utc, all_day, rrule, raw_ics, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)
             ON CONFLICT(calendar_id, dav_href) DO UPDATE SET
                dav_etag = excluded.dav_etag,
                uid = excluded.uid,
                summary = excluded.summary,
                description = excluded.description,
                location = excluded.location,
                dtstart_utc = excluded.dtstart_utc,
                dtend_utc = excluded.dtend_utc,
                all_day = excluded.all_day,
                rrule = excluded.rrule,
                raw_ics = excluded.raw_ics,
                updated_at = excluded.updated_at",
            params![
                calendar_id,
                ev.dav_href,
                ev.dav_etag,
                ev.uid,
                ev.summary,
                ev.description,
                ev.location,
                ev.dtstart_utc,
                ev.dtend_utc,
                i32::from(ev.all_day),
                ev.rrule,
                ev.raw_ics,
                now,
            ],
        )?;
        let id: i64 = conn.query_row(
            "SELECT id FROM cal_events WHERE calendar_id = ?1 AND dav_href = ?2",
            params![calendar_id, ev.dav_href],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    /// Events whose single (non-recurring) occurrence overlaps the
    /// requested range, OR whose row has an RRULE (expansion happens
    /// in the caller — we ship all candidate recurring events so the
    /// range filter can run against expanded occurrences).
    pub fn list_cal_events_in_range(
        &self,
        from_utc: i64,
        to_utc: i64,
    ) -> Result<Vec<CalEvent>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, calendar_id, dav_href, dav_etag, uid, summary, description, location,
                    dtstart_utc, dtend_utc, all_day, rrule, raw_ics, created_at, updated_at
               FROM cal_events
              WHERE rrule IS NOT NULL
                 OR (dtstart_utc < ?2 AND COALESCE(dtend_utc, dtstart_utc + 3600) >= ?1)
              ORDER BY dtstart_utc ASC",
        )?;
        let rows = stmt.query_map(params![from_utc, to_utc], row_to_cal_event)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_cal_event(&self, id: i64) -> Result<CalEvent> {
        let conn = self.pool().get()?;
        conn.query_row(
            "SELECT id, calendar_id, dav_href, dav_etag, uid, summary, description, location,
                    dtstart_utc, dtend_utc, all_day, rrule, raw_ics, created_at, updated_at
               FROM cal_events WHERE id = ?1",
            params![id],
            row_to_cal_event,
        )
        .optional()?
        .ok_or(Error::NotFound)
    }
}

fn row_to_cal_account(r: &rusqlite::Row) -> rusqlite::Result<CalAccount> {
    Ok(CalAccount {
        id: r.get(0)?,
        label: r.get(1)?,
        server_url: r.get(2)?,
        username: r.get(3)?,
        principal_url: r.get(4)?,
        calendar_home_url: r.get(5)?,
        last_sync_at: r.get(6)?,
        last_sync_error: r.get(7)?,
        created_at: r.get(8)?,
    })
}

fn row_to_cal_calendar(r: &rusqlite::Row) -> rusqlite::Result<CalCalendar> {
    Ok(CalCalendar {
        id: r.get(0)?,
        account_id: r.get(1)?,
        dav_url: r.get(2)?,
        name: r.get(3)?,
        ctag: r.get(4)?,
        color: r.get(5)?,
        read_only: r.get::<_, i64>(6)? != 0,
        hidden: r.get::<_, i64>(7)? != 0,
        created_at: r.get(8)?,
    })
}

fn row_to_cal_event(r: &rusqlite::Row) -> rusqlite::Result<CalEvent> {
    Ok(CalEvent {
        id: r.get(0)?,
        calendar_id: r.get(1)?,
        dav_href: r.get(2)?,
        dav_etag: r.get(3)?,
        uid: r.get(4)?,
        summary: r.get(5)?,
        description: r.get(6)?,
        location: r.get(7)?,
        dtstart_utc: r.get(8)?,
        dtend_utc: r.get(9)?,
        all_day: r.get::<_, i64>(10)? != 0,
        rrule: r.get(11)?,
        raw_ics: r.get(12)?,
        created_at: r.get(13)?,
        updated_at: r.get(14)?,
    })
}
