//! Contacts storage. Address-keyed rows fed by the message-insert
//! hook + a one-time boot backfill from existing messages, surfaced
//! by the recipient autocomplete and (phase 2) a Contacts UI.
//!
//! Why we have this separate from the messages corpus: autocomplete
//! against `messages.from_addr` LIKE '%foo%' DOES work, but it
//! re-scans the corpus on every keystroke and silently returns
//! nothing on a fresh install with no synced mail yet. A small
//! address-keyed table is faster, cleaner, and survives the user
//! pruning old mail.

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::Db;
use crate::error::Result;

#[derive(Debug, Clone, Serialize)]
pub struct Contact {
    pub id: i64,
    pub address: String,
    pub display_name: Option<String>,
    pub first_seen_utc: i64,
    pub last_seen_utc: i64,
    pub message_count: i64,
    pub is_favorite: bool,
    pub notes: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Db {
    /// Upsert one address into the contacts table. Bumps message_count
    /// + last_seen on hits, inserts on miss. Display name overrides
    /// any existing one when supplied — most-recent-header-wins.
    /// Treats empty / whitespace-only addresses as no-ops.
    pub fn upsert_contact(
        &self,
        address: &str,
        display_name: Option<&str>,
        seen_at_utc: i64,
    ) -> Result<()> {
        let addr = address.trim();
        if addr.is_empty() || !addr.contains('@') {
            return Ok(());
        }
        let conn = self.pool().get()?;
        // INSERT ... ON CONFLICT ... DO UPDATE handles both paths in
        // one round-trip. message_count + last_seen always update;
        // display_name only updates when we have a non-empty new
        // value (so a future bare-address hit doesn't blank out a
        // good existing name).
        let now = chrono::Utc::now().timestamp();
        let display_clean = display_name
            .map(str::trim)
            .filter(|s| !s.is_empty());
        conn.execute(
            "INSERT INTO contacts(
                address, display_name, first_seen_utc, last_seen_utc,
                message_count, is_favorite, notes, created_at, updated_at
             )
             VALUES (?1, ?2, ?3, ?3, 1, 0, NULL, ?4, ?4)
             ON CONFLICT(address) DO UPDATE SET
               display_name = COALESCE(excluded.display_name, contacts.display_name),
               last_seen_utc = MAX(contacts.last_seen_utc, excluded.last_seen_utc),
               first_seen_utc = MIN(contacts.first_seen_utc, excluded.last_seen_utc),
               message_count = contacts.message_count + 1,
               updated_at = ?4",
            params![addr, display_clean, seen_at_utc, now],
        )?;
        Ok(())
    }

    /// Backfill from the existing messages table. Idempotent — only
    /// inserts addresses that aren't yet in the contacts table.
    /// Returns (messages_scanned, unique_addresses, rows_inserted)
    /// so the boot routine can log the breakdown when things look
    /// off. Called once at every unlock; the boot routine logs the
    /// numbers so a "0 inserted" can be distinguished from "0 scanned"
    /// or "0 unique addresses found".
    pub fn backfill_contacts_diag(&self) -> Result<(usize, usize, usize)> {
        let conn = self.pool().get()?;
        let now = chrono::Utc::now().timestamp();
        let mut stmt = conn.prepare(
            "SELECT
                COALESCE(from_addr, '') AS from_addr,
                COALESCE(to_addrs, '') AS to_addrs,
                COALESCE(cc_addrs, '') AS cc_addrs,
                date_utc
             FROM messages",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        let scanned = rows.len();

        use std::collections::HashMap;
        struct Agg {
            display: String,
            first_seen: i64,
            last_seen: i64,
            count: i64,
        }
        let mut agg: HashMap<String, Agg> = HashMap::new();
        for (from, to, cc, date) in rows {
            for raw in [&from, &to, &cc] {
                for piece in raw.split(',') {
                    let piece = piece.trim();
                    if piece.is_empty() || !piece.contains('@') {
                        continue;
                    }
                    let addr = match (piece.rfind('<'), piece.rfind('>')) {
                        (Some(open), Some(close)) if close > open + 1 => {
                            piece[open + 1..close].trim().to_string()
                        }
                        _ => piece.to_string(),
                    };
                    if !addr.contains('@') {
                        continue;
                    }
                    let key = addr.to_lowercase();
                    agg.entry(key)
                        .and_modify(|a| {
                            a.first_seen = a.first_seen.min(date);
                            a.last_seen = a.last_seen.max(date);
                            a.count += 1;
                        })
                        .or_insert(Agg {
                            display: addr,
                            first_seen: date,
                            last_seen: date,
                            count: 1,
                        });
                }
            }
        }
        let unique = agg.len();

        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;
        let mut created = 0usize;
        for (_, a) in agg {
            let n = tx.execute(
                "INSERT INTO contacts(
                    address, display_name, first_seen_utc, last_seen_utc,
                    message_count, is_favorite, notes, created_at, updated_at
                 )
                 VALUES (?1, NULL, ?2, ?3, ?4, 0, NULL, ?5, ?5)
                 ON CONFLICT(address) DO NOTHING",
                params![a.display, a.first_seen, a.last_seen, a.count, now],
            )?;
            if n > 0 {
                created += 1;
            }
        }
        tx.commit()?;
        Ok((scanned, unique, created))
    }

    /// Thin wrapper for callers that just want the inserted count.
    pub fn backfill_contacts_from_messages(&self) -> Result<usize> {
        Ok(self.backfill_contacts_diag()?.2)
    }

    /// True when the contacts table has zero rows. Used by the boot
    /// routine to decide whether to run backfill.
    pub fn contacts_is_empty(&self) -> Result<bool> {
        let conn = self.pool().get()?;
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM contacts", [], |r| r.get(0))?;
        Ok(n == 0)
    }

    /// Fast prefix-style autocomplete. Matches against both the bare
    /// address AND the display name so typing a person's name finds
    /// their email. Ordered by recent activity so frequently-emailed
    /// contacts surface first.
    pub fn autocomplete_contacts(&self, q: &str, limit: i64) -> Result<Vec<String>> {
        let q = q.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let pattern = format!("%{}%", q.replace('%', "\\%"));
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT address, display_name FROM contacts
             WHERE address LIKE ?1 ESCAPE '\\'
                OR (display_name IS NOT NULL AND display_name LIKE ?1 ESCAPE '\\')
             ORDER BY is_favorite DESC, last_seen_utc DESC
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![pattern, limit], |r| {
                let addr: String = r.get(0)?;
                let name: Option<String> = r.get(1)?;
                // Reconstitute "Name <addr>" when we have both, plain
                // address otherwise. Keeps the UI consistent with how
                // users expect to see contacts in compose.
                Ok(match name {
                    Some(n) if !n.is_empty() => format!("{n} <{addr}>"),
                    _ => addr,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Paginated list — phase 2 UI driver. Empty `q` returns all,
    /// optional q does the same LIKE match autocomplete uses.
    pub fn list_contacts(
        &self,
        q: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Contact>> {
        let conn = self.pool().get()?;
        let limit = limit.clamp(1, 500);
        let offset = offset.max(0);
        if let Some(needle) = q.map(str::trim).filter(|s| !s.is_empty()) {
            let pattern = format!("%{}%", needle.replace('%', "\\%"));
            let mut stmt = conn.prepare(
                "SELECT id, address, display_name, first_seen_utc, last_seen_utc,
                        message_count, is_favorite, notes, created_at, updated_at
                 FROM contacts
                 WHERE address LIKE ?1 ESCAPE '\\'
                    OR (display_name IS NOT NULL AND display_name LIKE ?1 ESCAPE '\\')
                 ORDER BY is_favorite DESC, last_seen_utc DESC
                 LIMIT ?2 OFFSET ?3",
            )?;
            let iter = stmt.query_map(params![pattern, limit, offset], row_to_contact)?;
            Ok(iter.collect::<rusqlite::Result<Vec<_>>>()?)
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, address, display_name, first_seen_utc, last_seen_utc,
                        message_count, is_favorite, notes, created_at, updated_at
                 FROM contacts
                 ORDER BY is_favorite DESC, last_seen_utc DESC
                 LIMIT ?1 OFFSET ?2",
            )?;
            let iter = stmt.query_map(params![limit, offset], row_to_contact)?;
            Ok(iter.collect::<rusqlite::Result<Vec<_>>>()?)
        }
    }

    /// Read one contact by id. None when missing — caller maps to 404.
    pub fn get_contact(&self, id: i64) -> Result<Option<Contact>> {
        let conn = self.pool().get()?;
        let row = conn
            .query_row(
                "SELECT id, address, display_name, first_seen_utc, last_seen_utc,
                        message_count, is_favorite, notes, created_at, updated_at
                 FROM contacts WHERE id = ?1",
                params![id],
                row_to_contact,
            )
            .optional()?;
        Ok(row)
    }

    /// Manual-add path. Caller has already validated the address;
    /// we just enforce uniqueness here. Returns the inserted row's
    /// id, or an error containing "exists" if the address is taken
    /// — handler maps that to 409.
    pub fn create_contact(
        &self,
        address: &str,
        display_name: Option<&str>,
        notes: Option<&str>,
        is_favorite: bool,
    ) -> Result<i64> {
        let addr = address.trim();
        if addr.is_empty() || !addr.contains('@') {
            return Err(crate::error::Error::BadRequest(
                "address must contain @".into(),
            ));
        }
        let display_clean = display_name.map(str::trim).filter(|s| !s.is_empty());
        let notes_clean = notes.map(str::trim).filter(|s| !s.is_empty());
        let now = chrono::Utc::now().timestamp();
        let conn = self.pool().get()?;
        // Manual entries get message_count=0 to signal "we haven't
        // actually emailed this person, the user just typed them in".
        // Real upserts will bump it later as messages flow in.
        match conn.execute(
            "INSERT INTO contacts(
                address, display_name, first_seen_utc, last_seen_utc,
                message_count, is_favorite, notes, created_at, updated_at
             )
             VALUES (?1, ?2, ?3, ?3, 0, ?4, ?5, ?3, ?3)",
            params![addr, display_clean, now, i32::from(is_favorite), notes_clean],
        ) {
            Ok(_) => Ok(conn.last_insert_rowid()),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("UNIQUE") || msg.contains("constraint") {
                    Err(crate::error::Error::BadRequest(format!(
                        "contact already exists: {addr}"
                    )))
                } else {
                    Err(e.into())
                }
            }
        }
    }

    /// Patch a subset of fields. None means "leave as-is"; Some(value)
    /// means "set to this", including Some("") which clears nullable
    /// fields back to NULL. The address itself is immutable post-
    /// create — changing it would break the unique-key invariant the
    /// upsert hook relies on. Users who need a different address
    /// delete + re-add.
    pub fn update_contact(
        &self,
        id: i64,
        display_name: Option<Option<String>>,
        notes: Option<Option<String>>,
        is_favorite: Option<bool>,
    ) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let conn = self.pool().get()?;
        // Build the SET clause dynamically to avoid clobbering fields
        // the caller didn't ask to change. Quote-safe because the
        // field list is hand-rolled, not user-derived.
        let mut sets: Vec<&'static str> = Vec::new();
        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(d) = display_name {
            sets.push("display_name = ?");
            let cleaned = d.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
            binds.push(Box::new(cleaned));
        }
        if let Some(n) = notes {
            sets.push("notes = ?");
            let cleaned = n.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
            binds.push(Box::new(cleaned));
        }
        if let Some(f) = is_favorite {
            sets.push("is_favorite = ?");
            binds.push(Box::new(i32::from(f)));
        }
        if sets.is_empty() {
            // No-op — caller asked for nothing. Treat as "row exists?"
            return Ok(self.get_contact(id)?.is_some());
        }
        sets.push("updated_at = ?");
        binds.push(Box::new(now));
        binds.push(Box::new(id));
        let sql = format!(
            "UPDATE contacts SET {} WHERE id = ?",
            sets.join(", ")
        );
        let n = conn.execute(
            &sql,
            rusqlite::params_from_iter(binds.iter().map(|b| b.as_ref())),
        )?;
        Ok(n > 0)
    }

    pub fn delete_contact(&self, id: i64) -> Result<bool> {
        let conn = self.pool().get()?;
        let n = conn.execute("DELETE FROM contacts WHERE id = ?1", params![id])?;
        Ok(n > 0)
    }

    /// Total row count — paginator uses this for "Page X of Y".
    pub fn count_contacts(&self, q: Option<&str>) -> Result<i64> {
        let conn = self.pool().get()?;
        let n: i64 = if let Some(needle) = q.map(str::trim).filter(|s| !s.is_empty()) {
            let pattern = format!("%{}%", needle.replace('%', "\\%"));
            conn.query_row(
                "SELECT COUNT(*) FROM contacts
                 WHERE address LIKE ?1 ESCAPE '\\'
                    OR (display_name IS NOT NULL AND display_name LIKE ?1 ESCAPE '\\')",
                params![pattern],
                |r| r.get(0),
            )?
        } else {
            conn.query_row("SELECT COUNT(*) FROM contacts", [], |r| r.get(0))?
        };
        Ok(n)
    }
}

fn row_to_contact(r: &rusqlite::Row<'_>) -> rusqlite::Result<Contact> {
    Ok(Contact {
        id: r.get(0)?,
        address: r.get(1)?,
        display_name: r.get(2)?,
        first_seen_utc: r.get(3)?,
        last_seen_utc: r.get(4)?,
        message_count: r.get(5)?,
        is_favorite: r.get::<_, i64>(6)? != 0,
        notes: r.get(7)?,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
    })
}
