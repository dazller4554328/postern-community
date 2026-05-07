//! Label management. Labels in Postern are local-only organisational
//! tags — they reflect how the user (and the sync layer mirroring
//! folders/X-GM-LABELS) has bucketed messages, not an exact mirror
//! of server folder state. See `docs/STORAGE_INVARIANTS.md` for the
//! full sync rule.
//!
//! Operations split roughly two ways:
//!
//! - Label lifecycle (`upsert_label`, `list_labels`, `rename_label_tree`,
//!   `delete_label_tree`) — manage the `labels` table directly.
//! - Message-label junction (`add_labels_by_message_id`,
//!   `remove_label_from_messages`, `mark_label_all_read`,
//!   `hard_delete_by_label`) — operate on `message_labels` rows. These
//!   live here rather than in the message CRUD module because they're
//!   parameterised by label name and tend to be touched together with
//!   the lifecycle methods.

use rusqlite::{params, params_from_iter, OptionalExtension};
use serde::Serialize;

use super::Db;
use crate::error::Result;

#[derive(Debug, Clone, Serialize)]
pub struct Label {
    pub id: i64,
    pub account_id: i64,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FolderCount {
    pub label_id: i64,
    pub name: String,
    pub kind: String,
    pub total: i64,
    pub unread: i64,
    /// Sum of `messages.size_bytes` for the folder, in bytes. Useful
    /// for surfacing per-folder occupancy in the sidebar tooltip.
    pub size_bytes: i64,
}

impl Db {
    pub fn upsert_label(&self, account_id: i64, name: &str, kind: &str) -> Result<i64> {
        let conn = self.pool().get()?;
        if let Some(id) = conn
            .query_row(
                "SELECT id FROM labels WHERE account_id = ?1 AND name = ?2",
                params![account_id, name],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
        {
            return Ok(id);
        }
        conn.execute(
            "INSERT INTO labels(account_id, name, kind) VALUES (?1, ?2, ?3)",
            params![account_id, name, kind],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// True when a label already exists for this account, case-sensitive.
    pub fn label_exists(&self, account_id: i64, name: &str) -> Result<bool> {
        let conn = self.pool().get()?;
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM labels WHERE account_id = ?1 AND name = ?2",
            params![account_id, name],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// Set `is_read = 1` on every message currently labelled `name`
    /// for the given account. Returns the number of rows updated.
    /// Local-only — Postern doesn't propagate read state to IMAP (that
    /// lives behind the paid server-retention flag), so this just
    /// flips the UI indicator for a whole folder in one shot.
    pub fn mark_label_all_read(&self, account_id: i64, name: &str) -> Result<usize> {
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE messages
             SET is_read = 1
             WHERE account_id = ?1
               AND id IN (
                 SELECT ml.message_id
                 FROM message_labels ml
                 JOIN labels l ON l.id = ml.label_id
                 WHERE l.account_id = ?1 AND l.name = ?2
               )
               AND is_read = 0",
            params![account_id, name],
        )?;
        Ok(n)
    }

    /// Return the Message-Ids of every local message currently labelled
    /// `name` on `account_id`, then delete those rows. The caller is
    /// expected to use the returned message_ids to issue best-effort
    /// IMAP EXPUNGE server-side; the local deletion is already
    /// committed by the time this returns, so a failing EXPUNGE only
    /// means the remote copy lingers (and will be re-synced on the
    /// next tick — but empty-folder is only exposed for Trash/Spam
    /// where re-sync isn't disruptive).
    ///
    /// Returns (message_ids_for_imap, local_row_count_deleted).
    pub fn hard_delete_by_label(
        &self,
        account_id: i64,
        name: &str,
    ) -> Result<(Vec<String>, usize)> {
        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;

        // Collect first so we can return the Message-Ids after deletion.
        let mut stmt = tx.prepare(
            "SELECT m.id, m.message_id
             FROM messages m
             JOIN message_labels ml ON ml.message_id = m.id
             JOIN labels l ON l.id = ml.label_id
             WHERE m.account_id = ?1 AND l.name = ?2",
        )?;
        let rows: Vec<(i64, String)> = stmt
            .query_map(params![account_id, name], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        let mut msg_ids: Vec<String> = Vec::with_capacity(rows.len());
        let mut deleted = 0usize;
        for (id, mid) in rows {
            // FTS + label rows cascade via FK on the messages row.
            tx.execute("DELETE FROM messages_fts WHERE rowid = ?1", params![id])?;
            let n = tx.execute("DELETE FROM messages WHERE id = ?1", params![id])?;
            if n > 0 {
                deleted += 1;
                msg_ids.push(mid);
            }
        }
        tx.commit()?;
        Ok((msg_ids, deleted))
    }

    /// Count messages currently tagged with this label. Used as an
    /// emptiness guard before allowing folder deletion.
    pub fn count_messages_with_label(&self, account_id: i64, name: &str) -> Result<i64> {
        let conn = self.pool().get()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT ml.message_id)
             FROM message_labels ml
             JOIN labels l ON l.id = ml.label_id
             WHERE l.account_id = ?1 AND l.name = ?2",
            params![account_id, name],
            |r| r.get(0),
        )?;
        Ok(count)
    }

    /// Rename a local label. Both the label itself and all related
    /// subfolder labels (anything under `{from}/*`) get the prefix
    /// rewritten so our local tree mirrors the post-IMAP-RENAME shape.
    pub fn rename_label_tree(&self, account_id: i64, from: &str, to: &str) -> Result<usize> {
        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;
        let prefix = format!("{from}/");
        let mut renamed = 0usize;

        // Exact match first so the leaf row flips cleanly.
        renamed += tx.execute(
            "UPDATE labels SET name = ?1 WHERE account_id = ?2 AND name = ?3",
            params![to, account_id, from],
        )?;
        // Children: `{from}/...` → `{to}/...`. Collect first, rewrite
        // second — SQLite can't UPDATE with a SUBSTR that might collide
        // with an existing row inside a single statement across all rows.
        let mut collect =
            tx.prepare("SELECT id, name FROM labels WHERE account_id = ?1 AND name LIKE ?2")?;
        let kids: Vec<(i64, String)> = collect
            .query_map(params![account_id, format!("{prefix}%")], |r| {
                Ok((r.get(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(collect);
        for (id, old_name) in kids {
            let new_name = format!("{to}/{}", &old_name[prefix.len()..]);
            tx.execute(
                "UPDATE labels SET name = ?1 WHERE id = ?2",
                params![new_name, id],
            )?;
            renamed += 1;
        }
        tx.commit()?;
        Ok(renamed)
    }

    /// Drop a label and any subfolder labels beneath it. Cascades via
    /// ON DELETE on the message_labels FK — so messages that *only*
    /// had this label become unlabelled locally (still addressable by
    /// id/search, just not listed in any folder view).
    pub fn delete_label_tree(&self, account_id: i64, name: &str) -> Result<usize> {
        let conn = self.pool().get()?;
        let prefix_pattern = format!("{name}/%");
        let n = conn.execute(
            "DELETE FROM labels
             WHERE account_id = ?1 AND (name = ?2 OR name LIKE ?3)",
            params![account_id, name, prefix_pattern],
        )?;
        Ok(n as usize)
    }

    pub fn list_labels(&self, account_id: i64) -> Result<Vec<Label>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, account_id, name, kind FROM labels
             WHERE account_id = ?1 ORDER BY name",
        )?;
        let rows = stmt.query_map(params![account_id], |r| {
            Ok(Label {
                id: r.get(0)?,
                account_id: r.get(1)?,
                name: r.get(2)?,
                kind: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Total + unread counts per label for an account. A single query so
    /// big mailboxes don't fan out into N subqueries.
    pub fn folder_counts(&self, account_id: i64) -> Result<Vec<FolderCount>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT l.id, l.name, l.kind,
                    COUNT(m.id) as total,
                    COALESCE(SUM(CASE WHEN m.is_read = 0 THEN 1 ELSE 0 END), 0) as unread,
                    COALESCE(SUM(m.size_bytes), 0) as size_bytes
             FROM labels l
             LEFT JOIN message_labels ml ON ml.label_id = l.id
             LEFT JOIN messages m ON m.id = ml.message_id
             WHERE l.account_id = ?1
             GROUP BY l.id, l.name, l.kind
             ORDER BY l.name",
        )?;
        let rows = stmt.query_map(params![account_id], |r| {
            Ok(FolderCount {
                label_id: r.get(0)?,
                name: r.get(1)?,
                kind: r.get(2)?,
                total: r.get(3)?,
                unread: r.get(4)?,
                size_bytes: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Add labels onto a message identified by its Message-ID. Each
    /// label must already be registered via `upsert_label`; the
    /// `INSERT OR IGNORE` handles the dedup against existing junction
    /// rows.
    pub fn add_labels_by_message_id(
        &self,
        account_id: i64,
        message_id: &str,
        label_names: &[String],
    ) -> Result<bool> {
        let conn = self.pool().get()?;
        let local_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM messages WHERE account_id = ?1 AND message_id = ?2",
                params![account_id, message_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(id) = local_id else { return Ok(false) };
        for name in label_names {
            let label_id: Option<i64> = conn
                .query_row(
                    "SELECT id FROM labels WHERE account_id = ?1 AND name = ?2",
                    params![account_id, name],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(lid) = label_id {
                conn.execute(
                    "INSERT OR IGNORE INTO message_labels(message_id, label_id) VALUES (?1, ?2)",
                    params![id, lid],
                )?;
            }
        }
        Ok(true)
    }

    /// Remove a single label from many messages at once, looked up by
    /// Message-ID. Used by the streaming sync after a successful
    /// `UID MOVE → [Gmail]/Trash` to drop the source-folder label
    /// from the local rows — Gmail strips it server-side as part of
    /// the MOVE, so the local DB needs the same operation to stay
    /// in sync. Without this, a message synced from INBOX, MOVEd to
    /// Trash, then re-seen during the Trash sync, ends up with
    /// *both* INBOX and `[Gmail]/Trash` labels locally and shows up
    /// in two views in the Postern UI.
    pub fn remove_label_from_messages(
        &self,
        account_id: i64,
        message_ids: &[String],
        label_name: &str,
    ) -> Result<usize> {
        if message_ids.is_empty() {
            return Ok(0);
        }
        let conn = self.pool().get()?;
        let label_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM labels WHERE account_id = ?1 AND name = ?2",
                params![account_id, label_name],
                |r| r.get(0),
            )
            .optional()?;
        let Some(lid) = label_id else { return Ok(0) };
        let mut removed = 0usize;
        for chunk in message_ids.chunks(900) {
            let placeholders: String = (0..chunk.len())
                .map(|i| format!("?{}", i + 3))
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "DELETE FROM message_labels
                 WHERE label_id = ?1
                   AND message_id IN (
                       SELECT id FROM messages
                       WHERE account_id = ?2 AND message_id IN ({placeholders})
                   )"
            );
            let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(chunk.len() + 2);
            params.push(&lid);
            params.push(&account_id);
            for mid in chunk {
                params.push(mid);
            }
            removed += conn.execute(&sql, params_from_iter(params.iter().copied()))?;
        }
        Ok(removed)
    }
}
