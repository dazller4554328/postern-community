use std::collections::HashSet;

use rusqlite::{params, params_from_iter, OptionalExtension};
use serde::Serialize;

use super::Db;
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct NewMessage {
    pub account_id: i64,
    pub message_id: String,
    pub thread_id: Option<String>,
    pub subject: Option<String>,
    pub from_addr: Option<String>,
    pub to_addrs: Option<String>,
    pub cc_addrs: Option<String>,
    pub date_utc: i64,
    pub blob_sha256: String,
    pub size_bytes: i64,
    pub snippet: Option<String>,
    pub body_text: Option<String>,
    pub has_attachments: bool,
    pub is_read: bool,
    pub is_encrypted: bool,
    /// Address from `Disposition-Notification-To:`. None when the
    /// sender didn't request a read receipt.
    pub receipt_to: Option<String>,
    pub label_names: Vec<String>,
    /// Normalized subject for JWZ step 4 (merge orphan replies by
    /// subject when threading headers are missing).
    pub subject_key: Option<String>,
    /// True when the raw subject had a Re:/Fwd: prefix. Gates the
    /// subject-based parent-thread lookup in `upsert_message`.
    pub has_reply_prefix: bool,
    /// True when the message had no `References:` / `In-Reply-To:` —
    /// i.e. `thread_id` defaulted to the message's own ID. Combined
    /// with `has_reply_prefix`, this tells us we should look for an
    /// existing thread to merge into.
    pub is_thread_orphan: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub id: i64,
    pub account_id: i64,
    pub message_id: String,
    pub thread_id: Option<String>,
    pub subject: Option<String>,
    pub from_addr: Option<String>,
    pub to_addrs: Option<String>,
    pub cc_addrs: Option<String>,
    pub date_utc: i64,
    pub snippet: Option<String>,
    pub has_attachments: bool,
    pub is_read: bool,
    pub is_starred: bool,
    pub is_encrypted: bool,
    /// `Disposition-Notification-To` address — present when the sender
    /// requested a read receipt. Drives the manual-MDN banner in the
    /// read view.
    pub receipt_to: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageListItem {
    #[serde(flatten)]
    pub message: Message,
    pub account_email: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageDetail {
    #[serde(flatten)]
    pub message: Message,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    #[serde(flatten)]
    pub item: MessageListItem,
    /// `highlight()`-annotated snippet with `<mark>` wrapping matched terms.
    pub match_snippet: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThreadSummary {
    pub thread_id: String,
    pub subject: Option<String>,
    pub participants: Vec<String>,
    pub message_count: i64,
    pub unread_count: i64,
    pub has_attachments: bool,
    pub latest_date_utc: i64,
    pub latest_snippet: Option<String>,
    pub latest_from: Option<String>,
    /// Set of distinct `account_email`s that appear in the thread — used
    /// by the UI to tag cross-account conversations.
    pub account_emails: Vec<String>,
}

impl Db {
    pub fn upsert_message(&self, m: &NewMessage) -> Result<bool> {
        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;

        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM messages WHERE account_id = ?1 AND message_id = ?2",
                params![m.account_id, m.message_id],
                |r| r.get(0),
            )
            .optional()?;

        let msg_id = if let Some(id) = existing {
            // Sync may be reporting that the server has since marked this
            // message \Seen. Bump is_read up — but never back down, since a
            // later Postern-side "mark unread" shouldn't be clobbered by a
            // stale flag snapshot.
            if m.is_read {
                tx.execute(
                    "UPDATE messages SET is_read = 1 WHERE id = ?1 AND is_read = 0",
                    params![id],
                )?;
            }
            id
        } else {
            // JWZ step 4: if the message lost its threading headers but
            // still looks like a reply (Re:/Fwd: prefix), try to merge
            // it into an existing thread by normalized subject within
            // the same account. We only do this for "orphans" — adding
            // one to a message with real References would override a
            // known-good chain root with a weaker signal.
            let effective_thread_id = if m.is_thread_orphan && m.has_reply_prefix {
                if let Some(key) = m.subject_key.as_deref() {
                    let parent: Option<String> = tx
                        .query_row(
                            "SELECT thread_id FROM messages
                             WHERE account_id = ?1 AND subject_key = ?2
                               AND thread_id IS NOT NULL
                             ORDER BY date_utc ASC LIMIT 1",
                            params![m.account_id, key],
                            |r| r.get::<_, Option<String>>(0),
                        )
                        .optional()?
                        .flatten();
                    parent.or_else(|| m.thread_id.clone())
                } else {
                    m.thread_id.clone()
                }
            } else {
                m.thread_id.clone()
            };

            tx.execute(
                "INSERT INTO messages
                   (account_id, message_id, thread_id, subject, from_addr, to_addrs, cc_addrs,
                    date_utc, blob_sha256, size_bytes, snippet, body_text,
                    has_attachments, is_read, is_encrypted, subject_key, receipt_to)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
                params![
                    m.account_id,
                    m.message_id,
                    effective_thread_id,
                    m.subject,
                    m.from_addr,
                    m.to_addrs,
                    m.cc_addrs,
                    m.date_utc,
                    m.blob_sha256,
                    m.size_bytes,
                    m.snippet,
                    m.body_text,
                    i32::from(m.has_attachments),
                    i32::from(m.is_read),
                    i32::from(m.is_encrypted),
                    m.subject_key,
                    m.receipt_to,
                ],
            )?;
            let id = tx.last_insert_rowid();

            tx.execute(
                "INSERT INTO messages_fts(rowid, subject, from_addr, to_addrs, body_text)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id,
                    m.subject.as_deref().unwrap_or(""),
                    m.from_addr.as_deref().unwrap_or(""),
                    m.to_addrs.as_deref().unwrap_or(""),
                    m.body_text.as_deref().unwrap_or(""),
                ],
            )?;
            id
        };

        // Accumulate labels across sync passes. Gmail exposes the same
        // message under multiple folders (INBOX + [Gmail]/All Mail +
        // custom labels), each with its own UID. Earlier code wholesale-
        // DELETEd before re-inserting, so whichever folder synced last
        // won and the earlier labels vanished — making cross-account
        // "unified Inbox" queries return nothing for Gmail accounts.
        // INSERT OR IGNORE lets labels stack; label removal on the
        // server side isn't reflected, which matches how the cursor
        // sync already treats edits in general.
        for name in &m.label_names {
            let label_id: i64 = tx
                .query_row(
                    "SELECT id FROM labels WHERE account_id = ?1 AND name = ?2",
                    params![m.account_id, name],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| {
                    crate::error::Error::Other(anyhow::anyhow!("label not registered: {name}"))
                })?;
            tx.execute(
                "INSERT OR IGNORE INTO message_labels(message_id, label_id) VALUES (?1, ?2)",
                params![msg_id, label_id],
            )?;
        }

        tx.commit()?;

        // Feed the contacts table — but only on the insert path, not
        // when this call was a re-sync of a known message (otherwise
        // message_count would tick on every metadata refresh, not
        // per actual message). Best-effort: failures here mustn't
        // unwind the just-committed message insert, so we log and
        // move on. Display name plumbing is deferred — the parser
        // currently strips it, so phase 1 contacts are address-only.
        let inserted = existing.is_none();
        if inserted {
            let now = chrono::Utc::now().timestamp();
            let seen = if m.date_utc > 0 { m.date_utc } else { now };
            let feed = |raw: Option<&str>| {
                let Some(s) = raw else { return };
                for piece in s.split(',') {
                    let piece = piece.trim();
                    if piece.is_empty() {
                        continue;
                    }
                    // Defensive parse for "Name <addr>" survivors.
                    let addr = match (piece.rfind('<'), piece.rfind('>')) {
                        (Some(open), Some(close)) if close > open + 1 => {
                            piece[open + 1..close].trim().to_string()
                        }
                        _ => piece.to_string(),
                    };
                    let _ = self.upsert_contact(&addr, None, seen);
                }
            };
            feed(m.from_addr.as_deref());
            feed(m.to_addrs.as_deref());
            feed(m.cc_addrs.as_deref());
        }

        Ok(inserted)
    }

    /// Replace all labels on a message with the given list. Used by
    /// the spam/move endpoints to update local state after IMAP MOVE.
    ///
    /// **Atomic**: the DELETE + INSERTs run in a single transaction.
    /// Earlier this method ran them on a pooled connection without a
    /// transaction wrapper, so a pool eviction or a panic between the
    /// DELETE and the first INSERT would leave the message with zero
    /// labels permanently — invisible in every folder view. Since
    /// callers (move handler, spam handler, archive handler) reach
    /// here on the optimistic path before the IMAP MOVE confirms,
    /// that crash window was a real label-loss risk.
    pub fn relabel_message(&self, id: i64, account_id: i64, label_names: &[&str]) -> Result<()> {
        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM message_labels WHERE message_id = ?1",
            params![id],
        )?;
        for name in label_names {
            let label_id: Option<i64> = tx
                .query_row(
                    "SELECT id FROM labels WHERE account_id = ?1 AND name = ?2",
                    params![account_id, name],
                    |r| r.get(0),
                )
                .optional()?;
            let lid = match label_id {
                Some(lid) => lid,
                None => {
                    // Label doesn't exist yet — create as user so it
                    // shows in the sidebar tree. System labels are
                    // for known IMAP folders only (INBOX, Sent,
                    // Trash, etc). Doing the insert inline here
                    // (rather than calling upsert_label) keeps the
                    // operation inside this transaction.
                    tx.execute(
                        "INSERT OR IGNORE INTO labels(account_id, name, kind) VALUES (?1, ?2, 'user')",
                        params![account_id, name],
                    )?;
                    tx.query_row(
                        "SELECT id FROM labels WHERE account_id = ?1 AND name = ?2",
                        params![account_id, name],
                        |r| r.get::<_, i64>(0),
                    )?
                }
            };
            tx.execute(
                "INSERT OR IGNORE INTO message_labels(message_id, label_id) VALUES (?1, ?2)",
                params![id, lid],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Flip the read flag on a local message row. Does not propagate to the
    /// IMAP server yet — that's a follow-up once we switch `examine` → `select`
    /// and drive IMAP STORE. Caller handles "message not found" via rows == 0.

    pub fn set_message_read(&self, id: i64, read: bool) -> Result<bool> {
        let conn = self.pool().get()?;
        let rows = conn.execute(
            "UPDATE messages SET is_read = ?1 WHERE id = ?2",
            params![i32::from(read), id],
        )?;
        Ok(rows > 0)
    }

    /// Batched backfill of `body_text` for messages stored before migration
    /// 0002. Returns the number of rows updated. The caller supplies the
    /// parsing function so storage stays free of mail-parser dependencies.
    pub fn backfill_bodies<F>(&self, batch: usize, parse_blob: F) -> Result<usize>
    where
        F: Fn(&str) -> Option<String>,
    {
        let conn = self.pool().get()?;
        let mut stmt =
            conn.prepare("SELECT id, blob_sha256 FROM messages WHERE body_text IS NULL LIMIT ?1")?;
        let candidates: Vec<(i64, String)> = stmt
            .query_map(params![batch as i64], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        let mut updated = 0;
        for (id, hash) in candidates {
            let Some(body) = parse_blob(&hash) else {
                continue;
            };
            conn.execute(
                "UPDATE messages SET body_text = ?1 WHERE id = ?2",
                params![body, id],
            )?;
            // Refresh the FTS entry in case it's stale from the contentless era.
            conn.execute("DELETE FROM messages_fts WHERE rowid = ?1", params![id])?;
            conn.execute(
                "INSERT INTO messages_fts(rowid, subject, from_addr, to_addrs, body_text)
                 SELECT id,
                        COALESCE(subject, ''), COALESCE(from_addr, ''),
                        COALESCE(to_addrs, ''), ?2
                 FROM messages WHERE id = ?1",
                params![id, body],
            )?;
            updated += 1;
        }
        Ok(updated)
    }

    /// Populate `subject_key` for messages ingested before migration
    /// 0018. The key is what powers the JWZ-step-4 merge: replies that
    /// lost their `References:` header still get clustered by
    /// normalized subject. Batched so a big legacy mailbox doesn't
    /// lock the pool for long.
    pub fn backfill_subject_keys<F>(&self, batch: usize, compute: F) -> Result<usize>
    where
        F: Fn(&str) -> Option<String>,
    {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, blob_sha256 FROM messages
             WHERE subject_key IS NULL
             LIMIT ?1",
        )?;
        let candidates: Vec<(i64, String)> = stmt
            .query_map(params![batch as i64], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        if candidates.is_empty() {
            return Ok(0);
        }

        let mut updated = 0;
        for (id, hash) in candidates {
            // None is a valid outcome (empty/all-prefix subject) — and
            // we still need to mark the row so the backfill terminates.
            // Use the sentinel empty string for "normalized to nothing".
            let key = compute(&hash).unwrap_or_default();
            conn.execute(
                "UPDATE messages SET subject_key = ?1 WHERE id = ?2",
                params![key, id],
            )?;
            updated += 1;
        }
        Ok(updated)
    }

    /// Rewrite thread_ids for messages that were ingested before the JWZ
    /// threading logic existed. `compute` is passed the raw blob and
    /// returns the new thread_id. Batched to keep pool contention low.
    pub fn backfill_threads<F>(&self, batch: usize, compute: F) -> Result<usize>
    where
        F: Fn(&str) -> Option<String>,
    {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, blob_sha256, thread_id, message_id
             FROM messages
             WHERE thread_id IS NULL OR thread_id LIKE 'imap-%'
             LIMIT ?1",
        )?;
        let candidates: Vec<(i64, String, Option<String>, String)> = stmt
            .query_map(params![batch as i64], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        let mut updated = 0;
        for (id, hash, _prev, self_mid) in candidates {
            let new_tid = compute(&hash).unwrap_or(self_mid);
            conn.execute(
                "UPDATE messages SET thread_id = ?1 WHERE id = ?2",
                params![new_tid, id],
            )?;
            updated += 1;
        }
        Ok(updated)
    }

    /// `(account_id, label_id) -> (uid_validity, uid_next)`
    pub fn sync_state(&self, account_id: i64, label_id: i64) -> Result<(Option<u32>, Option<u32>)> {
        let conn = self.pool().get()?;
        Ok(conn
            .query_row(
                "SELECT uid_validity, uid_next FROM sync_state
                 WHERE account_id = ?1 AND label_id = ?2",
                params![account_id, label_id],
                |r| {
                    Ok((
                        r.get::<_, Option<i64>>(0)?.map(|v| v as u32),
                        r.get::<_, Option<i64>>(1)?.map(|v| v as u32),
                    ))
                },
            )
            .optional()?
            .unwrap_or((None, None)))
    }

    /// Wipe all sync cursors for an account. Next sync starts from UID 1
    /// for every folder. Returns number of rows cleared.
    pub fn reset_sync_state_for_account(&self, account_id: i64) -> Result<usize> {
        let conn = self.pool().get()?;
        let rows = conn.execute(
            "DELETE FROM sync_state WHERE account_id = ?1",
            params![account_id],
        )?;
        Ok(rows)
    }

    pub fn update_sync_state(
        &self,
        account_id: i64,
        label_id: i64,
        uid_validity: u32,
        uid_next: u32,
    ) -> Result<()> {
        let conn = self.pool().get()?;
        conn.execute(
            "INSERT INTO sync_state(account_id, label_id, uid_validity, uid_next, last_sync_utc)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(account_id, label_id) DO UPDATE SET
                uid_validity = excluded.uid_validity,
                uid_next = excluded.uid_next,
                last_sync_utc = excluded.last_sync_utc",
            params![
                account_id,
                label_id,
                uid_validity as i64,
                uid_next as i64,
                chrono::Utc::now().timestamp()
            ],
        )?;
        Ok(())
    }

    /// Minimal shape needed to drive the auto-archive IMAP move.
    /// Separate from MessageListItem because we don't need the joined
    /// account email here and we add the date the bucket is computed from.
    pub fn list_auto_archive_candidates(
        &self,
        account_id: i64,
        cutoff_utc: i64,
        read_only: bool,
        archive_base: &str,
        limit: i64,
    ) -> Result<Vec<AutoArchiveCandidate>> {
        let conn = self.pool().get()?;
        // INBOX label + older than cutoff + (optionally read) + not already
        // under the archive base. We match archive labels by prefix — any
        // bucket below the user's base (e.g. `Archive/2026/03`) counts as
        // already archived and is skipped.
        let pattern = format!("{archive_base}%");
        let mut sql = String::from(
            "SELECT m.id, m.message_id, m.date_utc, m.is_read
             FROM messages m
             WHERE m.account_id = ?1
               AND m.date_utc <= ?2
               AND m.id IN (
                 SELECT ml.message_id FROM message_labels ml
                 JOIN labels l ON l.id = ml.label_id
                 WHERE l.name = 'INBOX' AND l.account_id = ?1
               )
               AND m.id NOT IN (
                 SELECT ml.message_id FROM message_labels ml
                 JOIN labels l ON l.id = ml.label_id
                 WHERE l.account_id = ?1 AND l.name LIKE ?3
               )",
        );
        if read_only {
            sql.push_str(" AND m.is_read = 1");
        }
        sql.push_str(" ORDER BY m.date_utc ASC LIMIT ?4");

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![account_id, cutoff_utc, pattern, limit], |r| {
            Ok(AutoArchiveCandidate {
                id: r.get(0)?,
                message_id: r.get(1)?,
                date_utc: r.get(2)?,
                is_read: r.get::<_, i64>(3)? != 0,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Row id for a (account_id, message_id) pair. Returns Err when
    /// the pool or query fails — previous callers swallowed pool
    /// errors via `.ok()` and silently skipped downstream work
    /// (rule application, etc.) whenever the pool was momentarily
    /// exhausted. Propagating the error lets the scheduler log and
    /// retry on the next cycle instead of silently mis-processing.
    pub fn message_row_id(&self, account_id: i64, message_id: &str) -> Result<Option<i64>> {
        let conn = self.pool().get()?;
        let id = conn
            .query_row(
                "SELECT id FROM messages WHERE account_id = ?1 AND message_id = ?2",
                params![account_id, message_id],
                |r| r.get::<_, i64>(0),
            )
            .optional()?;
        Ok(id)
    }

    /// Bulk Message-ID presence check. Returns the subset of `message_ids`
    /// that exist in the local DB for this account. Used by the
    /// server-purge precheck to confirm Postern has every message
    /// before authorising a server-side delete; running per-UID
    /// `message_row_id` calls would mean one SQLite hit per message in
    /// a years-deep mailbox. Empty input → empty output (no DB hit).
    ///
    /// Uses a single `WHERE message_id IN (?, ?, ?, …)` query with a
    /// parameter cap of 900 per chunk to stay safely under SQLite's
    /// default `SQLITE_MAX_VARIABLE_NUMBER` of 999.
    pub fn messages_present(
        &self,
        account_id: i64,
        message_ids: &[String],
    ) -> Result<HashSet<String>> {
        if message_ids.is_empty() {
            return Ok(HashSet::new());
        }
        let conn = self.pool().get()?;
        let mut found = HashSet::with_capacity(message_ids.len());
        for chunk in message_ids.chunks(900) {
            let placeholders: String = (0..chunk.len())
                .map(|i| format!("?{}", i + 2))
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT message_id FROM messages \
                 WHERE account_id = ?1 AND message_id IN ({placeholders})"
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(chunk.len() + 1);
            params.push(&account_id);
            for mid in chunk {
                params.push(mid);
            }
            let rows = stmt.query_map(params_from_iter(params.iter().copied()), |r| {
                r.get::<_, String>(0)
            })?;
            for row in rows {
                found.insert(row?);
            }
        }
        Ok(found)
    }

    /// Add labels to a message looked up by its RFC822 Message-ID.
    /// Silently no-ops if the message isn't in this account's local
    /// store — useful for the Gmail label-rescan path, which walks the
    /// server's All Mail and only cares about messages Postern already
    /// Count messages the retention sweep would delete from the server
    /// on the next pass. Scope is intentionally narrow: INBOX label only,
    /// older than cutoff, not starred. Matches the live IMAP search used
    /// by the sweeper (`UID SEARCH BEFORE <date> NOT FLAGGED`), so the
    /// preview tracks what will actually happen.
    pub fn count_retention_candidates(&self, account_id: i64, cutoff_utc: i64) -> Result<i64> {
        let conn = self.pool().get()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM messages m
             WHERE m.account_id = ?1
               AND m.date_utc <= ?2
               AND m.is_starred = 0
               AND m.id IN (
                 SELECT ml.message_id FROM message_labels ml
                 JOIN labels l ON l.id = ml.label_id
                 WHERE l.name = 'INBOX' AND l.account_id = ?1
               )",
            params![account_id, cutoff_utc],
            |r| r.get(0),
        )?;
        Ok(count)
    }

    /// Count-only variant — drives the "N messages would be archived"
    /// preview card in the UI without materializing the rows.
    pub fn count_auto_archive_candidates(
        &self,
        account_id: i64,
        cutoff_utc: i64,
        read_only: bool,
        archive_base: &str,
    ) -> Result<i64> {
        let conn = self.pool().get()?;
        let pattern = format!("{archive_base}%");
        let mut sql = String::from(
            "SELECT COUNT(*) FROM messages m
             WHERE m.account_id = ?1
               AND m.date_utc <= ?2
               AND m.id IN (
                 SELECT ml.message_id FROM message_labels ml
                 JOIN labels l ON l.id = ml.label_id
                 WHERE l.name = 'INBOX' AND l.account_id = ?1
               )
               AND m.id NOT IN (
                 SELECT ml.message_id FROM message_labels ml
                 JOIN labels l ON l.id = ml.label_id
                 WHERE l.account_id = ?1 AND l.name LIKE ?3
               )",
        );
        if read_only {
            sql.push_str(" AND m.is_read = 1");
        }
        let count: i64 =
            conn.query_row(&sql, params![account_id, cutoff_utc, pattern], |r| r.get(0))?;
        Ok(count)
    }
}

#[derive(Debug, Clone)]
pub struct AutoArchiveCandidate {
    pub id: i64,
    pub message_id: String,
    pub date_utc: i64,
    pub is_read: bool,
}

// Old sanitize_fts_query / rewrite_column_prefixes helpers removed —
// superseded by storage::search_query::parse which handles the same
// prefix-rewriting plus a much larger operator set. Tests migrated
// alongside the new module.

#[cfg(test)]
mod tests {
    use super::*;

    /// Set up a migrated DB with a single account row inserted via raw
    /// SQL — bypasses the Vault wrapper that `Db::insert_account` uses,
    /// keeping the unit tests focused on the lookup paths.
    fn db_with_account() -> (tempfile::TempDir, Db, i64) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("t.db")).unwrap();
        db.migrate().unwrap();
        let conn = db.pool().get().unwrap();
        conn.execute(
            "INSERT INTO accounts(kind, email, imap_host, imap_port, credential_ref, created_at)
             VALUES ('imap', 'a@b.test', 'imap.example.com', 993, 'acct:a@b.test', 0)",
            [],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        (dir, db, id)
    }

    fn insert_message(db: &Db, account_id: i64, message_id: &str) {
        let conn = db.pool().get().unwrap();
        conn.execute(
            "INSERT INTO messages(account_id, message_id, thread_id, date_utc,
                                  blob_sha256, size_bytes, body_text, has_attachments,
                                  is_read, is_encrypted)
             VALUES (?1, ?2, NULL, 0, '', 0, NULL, 0, 0, 0)",
            params![account_id, message_id],
        )
        .unwrap();
    }

    #[test]
    fn messages_present_returns_empty_for_empty_input() {
        let (_t, db, account_id) = db_with_account();
        insert_message(&db, account_id, "<a@local>");

        let result = db.messages_present(account_id, &[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn messages_present_returns_only_matching_message_ids() {
        let (_t, db, account_id) = db_with_account();
        insert_message(&db, account_id, "<a@local>");
        insert_message(&db, account_id, "<b@local>");

        let queried = vec!["<a@local>".into(), "<missing@local>".into(), "<b@local>".into()];
        let result = db.messages_present(account_id, &queried).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.contains("<a@local>"));
        assert!(result.contains("<b@local>"));
        assert!(!result.contains("<missing@local>"));
    }

    #[test]
    fn messages_present_scopes_to_account() {
        // A message-id that exists for one account must NOT match for another.
        // This is the key safety invariant — purging account A must never
        // be authorised by account B's local copy.
        let (_t, db, account_a) = db_with_account();
        let conn = db.pool().get().unwrap();
        conn.execute(
            "INSERT INTO accounts(kind, email, imap_host, imap_port, credential_ref, created_at)
             VALUES ('imap', 'c@d.test', 'imap.example.com', 993, 'acct:c@d.test', 0)",
            [],
        )
        .unwrap();
        let account_b = conn.last_insert_rowid();
        drop(conn);

        insert_message(&db, account_a, "<shared@local>");

        assert_eq!(
            db.messages_present(account_a, &vec!["<shared@local>".into()])
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            db.messages_present(account_b, &vec!["<shared@local>".into()])
                .unwrap()
                .len(),
            0
        );
    }

    fn upsert_label_for(db: &Db, account_id: i64, name: &str) -> i64 {
        // Mirror the production label upsert path enough to seed test
        // data. Real callers go through `Db::upsert_label` which has
        // a kind classifier; here we just need the row to exist.
        let conn = db.pool().get().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO labels(account_id, name, kind) VALUES (?1, ?2, 'system')",
            params![account_id, name],
        )
        .unwrap();
        conn.query_row(
            "SELECT id FROM labels WHERE account_id = ?1 AND name = ?2",
            params![account_id, name],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn link_label(db: &Db, account_id: i64, message_id: &str, label_name: &str) {
        let lid = upsert_label_for(db, account_id, label_name);
        let conn = db.pool().get().unwrap();
        let mid: i64 = conn
            .query_row(
                "SELECT id FROM messages WHERE account_id = ?1 AND message_id = ?2",
                params![account_id, message_id],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO message_labels(message_id, label_id) VALUES (?1, ?2)",
            params![mid, lid],
        )
        .unwrap();
    }

    fn label_set_for(db: &Db, account_id: i64, message_id: &str) -> Vec<String> {
        let conn = db.pool().get().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT l.name
                 FROM labels l
                 JOIN message_labels ml ON ml.label_id = l.id
                 JOIN messages m ON m.id = ml.message_id
                 WHERE m.account_id = ?1 AND m.message_id = ?2
                 ORDER BY l.name",
            )
            .unwrap();
        stmt.query_map(params![account_id, message_id], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn remove_label_drops_only_named_label() {
        let (_t, db, account_id) = db_with_account();
        insert_message(&db, account_id, "<a@local>");
        link_label(&db, account_id, "<a@local>", "INBOX");
        link_label(&db, account_id, "<a@local>", "[Gmail]/Trash");

        let removed = db
            .remove_label_from_messages(account_id, &["<a@local>".into()], "INBOX")
            .unwrap();
        assert_eq!(removed, 1);
        assert_eq!(label_set_for(&db, account_id, "<a@local>"), vec!["[Gmail]/Trash".to_string()]);
    }

    #[test]
    fn remove_label_is_no_op_when_label_absent() {
        let (_t, db, account_id) = db_with_account();
        insert_message(&db, account_id, "<a@local>");
        link_label(&db, account_id, "<a@local>", "[Gmail]/Trash");

        let removed = db
            .remove_label_from_messages(account_id, &["<a@local>".into()], "INBOX")
            .unwrap();
        assert_eq!(removed, 0);
        assert_eq!(label_set_for(&db, account_id, "<a@local>"), vec!["[Gmail]/Trash".to_string()]);
    }

    #[test]
    fn remove_label_scopes_to_account() {
        // Same fix invariant as messages_present_scopes_to_account: a
        // label drop on account A must never touch account B's rows.
        let (_t, db, account_a) = db_with_account();
        let conn = db.pool().get().unwrap();
        conn.execute(
            "INSERT INTO accounts(kind, email, imap_host, imap_port, credential_ref, created_at)
             VALUES ('imap', 'b@b.test', 'h', 1, 'r', 0)",
            [],
        )
        .unwrap();
        let account_b = conn.last_insert_rowid();
        drop(conn);
        insert_message(&db, account_a, "<a@local>");
        insert_message(&db, account_b, "<a@local>");
        link_label(&db, account_a, "<a@local>", "INBOX");
        link_label(&db, account_b, "<a@local>", "INBOX");

        db.remove_label_from_messages(account_a, &["<a@local>".into()], "INBOX")
            .unwrap();
        assert_eq!(label_set_for(&db, account_a, "<a@local>"), Vec::<String>::new());
        assert_eq!(label_set_for(&db, account_b, "<a@local>"), vec!["INBOX".to_string()]);
    }

    #[test]
    fn messages_present_chunks_above_sqlite_variable_limit() {
        // SQLite's default SQLITE_MAX_VARIABLE_NUMBER is 999; we chunk
        // at 900 to stay safely under. Insert 1500 messages and query
        // for all of them in one call — must round-trip cleanly.
        let (_t, db, account_id) = db_with_account();
        let mut ids = Vec::with_capacity(1500);
        for i in 0..1500 {
            let mid = format!("<{i}@local>");
            insert_message(&db, account_id, &mid);
            ids.push(mid);
        }

        let result = db.messages_present(account_id, &ids).unwrap();
        assert_eq!(result.len(), 1500);
    }
}
