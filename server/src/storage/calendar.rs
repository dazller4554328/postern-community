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
    /// `"caldav"` for accounts that sync from a remote CalDAV server,
    /// `"local"` for accounts whose events live only in this DB.
    /// Drives both the unlock flow (locals skip credential decode) and
    /// the write-back guard (CalDAV calendars are read-only at the API
    /// surface until two-way sync ships).
    pub kind: String,
    pub label: String,
    /// NULL for local accounts.
    pub server_url: Option<String>,
    /// NULL for local accounts.
    pub username: Option<String>,
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
            "INSERT INTO cal_accounts
                (kind, label, server_url, username, credential_ref, created_at)
             VALUES ('caldav', ?1, ?2, ?3, ?4, ?5)",
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

    /// Idempotently make sure a single `kind='local'` account exists,
    /// with one default calendar inside it. First call creates them;
    /// subsequent calls are no-ops. Run on every calendar bootstrap so
    /// fresh installs land on a working surface without the user
    /// configuring an external server. Returns (account_id, calendar_id).
    pub fn ensure_local_cal_account(&self) -> Result<(i64, i64)> {
        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM cal_accounts WHERE kind = 'local' ORDER BY id ASC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let account_id = match existing {
            Some(id) => id,
            None => {
                tx.execute(
                    "INSERT INTO cal_accounts (kind, label) VALUES ('local', 'On this device')",
                    [],
                )?;
                tx.last_insert_rowid()
            }
        };
        let calendar_id: i64 = match tx
            .query_row(
                "SELECT id FROM cal_calendars WHERE account_id = ?1 ORDER BY id ASC LIMIT 1",
                params![account_id],
                |r| r.get(0),
            )
            .optional()?
        {
            Some(id) => id,
            None => {
                // Synthetic dav_url scoped to local: so it can never
                // collide with a real CalDAV collection URL and the
                // (account_id, dav_url) UNIQUE keeps holding.
                let dav_url = format!("local:account/{account_id}/calendar/default");
                tx.execute(
                    "INSERT INTO cal_calendars (account_id, dav_url, name, color, read_only)
                     VALUES (?1, ?2, 'My calendar', '#0d7a5a', 0)",
                    params![account_id, dav_url],
                )?;
                tx.last_insert_rowid()
            }
        };
        tx.commit()?;
        Ok((account_id, calendar_id))
    }

    /// Fetch only `kind` for a calendar's owning account. Cheaper than
    /// loading the full account row when all the caller wants is the
    /// "is this writable from the API?" decision.
    pub fn cal_calendar_account_kind(&self, calendar_id: i64) -> Result<String> {
        let conn = self.pool().get()?;
        let kind: String = conn.query_row(
            "SELECT a.kind
               FROM cal_calendars c
               JOIN cal_accounts a ON a.id = c.account_id
              WHERE c.id = ?1",
            params![calendar_id],
            |r| r.get(0),
        )?;
        Ok(kind)
    }

    pub fn list_cal_accounts(&self) -> Result<Vec<CalAccount>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, kind, label, server_url, username, principal_url, calendar_home_url,
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
            "SELECT id, kind, label, server_url, username, principal_url, calendar_home_url,
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

    /// Create an event in a local calendar. Caller is responsible for
    /// confirming the calendar's account is `kind='local'` — the
    /// HTTP layer enforces this. We synthesise a `dav_href` (just
    /// "local:event/<uuid>") so the (calendar_id, dav_href) UNIQUE
    /// invariant holds, and store a minimal raw_ics so a future "export
    /// to .ics" or "promote to a CalDAV calendar" path has something to
    /// hand off without reconstructing it from columns.
    pub fn cal_event_create_local(&self, e: &NewLocalEvent<'_>) -> Result<i64> {
        let uid = format!("postern-{}", uuid::Uuid::new_v4());
        let dav_href = format!("local:event/{uid}");
        let raw_ics = build_local_ics(&uid, e);
        let conn = self.pool().get()?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO cal_events
                (calendar_id, dav_href, dav_etag, uid, summary, description, location,
                 dtstart_utc, dtend_utc, all_day, rrule, raw_ics, created_at, updated_at)
             VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
            params![
                e.calendar_id,
                dav_href,
                uid,
                e.summary,
                e.description,
                e.location,
                e.dtstart_utc,
                e.dtend_utc,
                i32::from(e.all_day),
                e.rrule,
                raw_ics,
                now,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Patch a local event. Each field is optional — only non-`None`
    /// values are touched. Refuses (returns `Error::NotFound`) when
    /// the row doesn't exist; the HTTP layer additionally refuses
    /// when the row's calendar belongs to a CalDAV account.
    pub fn cal_event_update_local(&self, id: i64, p: &PatchLocalEvent<'_>) -> Result<()> {
        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;
        let existing: Option<CalEvent> = tx
            .query_row(
                "SELECT id, calendar_id, dav_href, dav_etag, uid, summary, description, location,
                        dtstart_utc, dtend_utc, all_day, rrule, raw_ics, created_at, updated_at
                   FROM cal_events WHERE id = ?1",
                params![id],
                row_to_cal_event,
            )
            .optional()?;
        let Some(cur) = existing else {
            return Err(Error::NotFound);
        };
        let summary = p.summary.map(|s| s.to_owned()).or(cur.summary);
        let description = p.description.map(|s| s.to_owned()).or(cur.description);
        let location = p.location.map(|s| s.to_owned()).or(cur.location);
        let dtstart = p.dtstart_utc.unwrap_or(cur.dtstart_utc);
        let dtend = p.dtend_utc.or(cur.dtend_utc);
        let all_day = p.all_day.unwrap_or(cur.all_day);
        let rrule = p.rrule.map(|s| s.to_owned()).or(cur.rrule);
        let new_event = NewLocalEvent {
            calendar_id: cur.calendar_id,
            summary: summary.as_deref(),
            description: description.as_deref(),
            location: location.as_deref(),
            dtstart_utc: dtstart,
            dtend_utc: dtend,
            all_day,
            rrule: rrule.as_deref(),
        };
        let raw_ics = build_local_ics(&cur.uid, &new_event);
        let now = chrono::Utc::now().timestamp();
        tx.execute(
            "UPDATE cal_events
                SET summary = ?1, description = ?2, location = ?3,
                    dtstart_utc = ?4, dtend_utc = ?5, all_day = ?6,
                    rrule = ?7, raw_ics = ?8, updated_at = ?9
              WHERE id = ?10",
            params![
                summary,
                description,
                location,
                dtstart,
                dtend,
                i32::from(all_day),
                rrule,
                raw_ics,
                now,
                id,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn cal_event_delete(&self, id: i64) -> Result<bool> {
        let conn = self.pool().get()?;
        let n = conn.execute("DELETE FROM cal_events WHERE id = ?1", params![id])?;
        Ok(n > 0)
    }
}

/// Inputs for creating a local event. References instead of owned
/// strings so the HTTP layer can pass slices straight from its
/// deserialised body without an intermediate copy.
#[derive(Debug, Clone)]
pub struct NewLocalEvent<'a> {
    pub calendar_id: i64,
    pub summary: Option<&'a str>,
    pub description: Option<&'a str>,
    pub location: Option<&'a str>,
    pub dtstart_utc: i64,
    pub dtend_utc: Option<i64>,
    pub all_day: bool,
    pub rrule: Option<&'a str>,
}

/// Patch — every field is `Option`, presence indicates "the caller
/// sent this; apply it." `dtstart_utc` is `Option` rather than required
/// so a "rename only" PATCH doesn't have to re-send the time.
#[derive(Debug, Clone, Default)]
pub struct PatchLocalEvent<'a> {
    pub summary: Option<&'a str>,
    pub description: Option<&'a str>,
    pub location: Option<&'a str>,
    pub dtstart_utc: Option<i64>,
    pub dtend_utc: Option<i64>,
    pub all_day: Option<bool>,
    pub rrule: Option<&'a str>,
}

/// Build a tiny VCALENDAR/VEVENT envelope so the row's `raw_ics` is a
/// valid iCalendar file and not an empty string. We keep it
/// intentionally minimal — full RFC 5545 compliance is the CalDAV
/// sync's job; this exists so a future "export local calendar" or
/// "migrate to a CalDAV server" feature has a real payload to send.
fn build_local_ics(uid: &str, e: &NewLocalEvent<'_>) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(256);
    let _ = writeln!(s, "BEGIN:VCALENDAR");
    let _ = writeln!(s, "VERSION:2.0");
    let _ = writeln!(s, "PRODID:-//Postern//Local//EN");
    let _ = writeln!(s, "BEGIN:VEVENT");
    let _ = writeln!(s, "UID:{uid}");
    let _ = writeln!(s, "DTSTAMP:{}", format_ical_utc(chrono::Utc::now().timestamp()));
    if e.all_day {
        let _ = writeln!(s, "DTSTART;VALUE=DATE:{}", format_ical_date(e.dtstart_utc));
        if let Some(end) = e.dtend_utc {
            let _ = writeln!(s, "DTEND;VALUE=DATE:{}", format_ical_date(end));
        }
    } else {
        let _ = writeln!(s, "DTSTART:{}", format_ical_utc(e.dtstart_utc));
        if let Some(end) = e.dtend_utc {
            let _ = writeln!(s, "DTEND:{}", format_ical_utc(end));
        }
    }
    if let Some(sum) = e.summary {
        let _ = writeln!(s, "SUMMARY:{}", escape_ics_text(sum));
    }
    if let Some(desc) = e.description {
        let _ = writeln!(s, "DESCRIPTION:{}", escape_ics_text(desc));
    }
    if let Some(loc) = e.location {
        let _ = writeln!(s, "LOCATION:{}", escape_ics_text(loc));
    }
    if let Some(rr) = e.rrule {
        let _ = writeln!(s, "RRULE:{rr}");
    }
    let _ = writeln!(s, "END:VEVENT");
    let _ = writeln!(s, "END:VCALENDAR");
    s
}

fn format_ical_utc(ts: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y%m%dT%H%M%SZ").to_string())
        .unwrap_or_else(|| "19700101T000000Z".into())
}

fn format_ical_date(ts: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y%m%d").to_string())
        .unwrap_or_else(|| "19700101".into())
}

/// RFC 5545 §3.3.11 — escape backslash, comma, semicolon, newline.
fn escape_ics_text(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace(',', "\\,")
        .replace(';', "\\;")
        .replace('\n', "\\n")
}

fn row_to_cal_account(r: &rusqlite::Row) -> rusqlite::Result<CalAccount> {
    Ok(CalAccount {
        id: r.get(0)?,
        kind: r.get(1)?,
        label: r.get(2)?,
        server_url: r.get(3)?,
        username: r.get(4)?,
        principal_url: r.get(5)?,
        calendar_home_url: r.get(6)?,
        last_sync_at: r.get(7)?,
        last_sync_error: r.get(8)?,
        created_at: r.get(9)?,
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
