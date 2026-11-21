//! Secure notes storage. NordPass-style encrypted notepad backed by
//! the same `SQLCipher` DB as the rest of Postern, so unlocking the
//! vault unlocks every note in one shot.
//!
//! The HTTP layer must have already passed `require_unlocked()` —
//! the queries here read/write the encrypted DB directly.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::Db;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize)]
pub struct Note {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub pinned: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewNote {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateNote {
    pub title: Option<String>,
    pub body: Option<String>,
    pub pinned: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NoteRevision {
    pub id: i64,
    pub note_id: i64,
    pub title: String,
    pub body: String,
    pub created_at: i64,
}

/// Newest revisions stay; older ones get pruned beyond this.
const REVISION_RETENTION_PER_NOTE: i64 = 50;
/// Minimum gap between adjacent revisions for the SAME note. Without
/// this a long typing session burns through `REVISION_RETENTION_PER_NOTE`
/// in a couple of minutes and loses the actually-interesting history
/// (the version from before today).
const REVISION_MIN_GAP_SECONDS: i64 = 30;

impl Db {
    pub fn insert_note(&self, new: &NewNote) -> Result<Note> {
        let title = new.title.trim().to_string();
        let body = new.body.clone();
        let conn = self.pool().get()?;
        conn.execute(
            "INSERT INTO notes (title, body, pinned) VALUES (?1, ?2, ?3)",
            params![title, body, i64::from(new.pinned)],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        self.get_note(id)
    }

    pub fn list_notes(&self) -> Result<Vec<Note>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, title, body, pinned, created_at, updated_at
               FROM notes
               ORDER BY pinned DESC, updated_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([], row_to_note)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_note(&self, id: i64) -> Result<Note> {
        let conn = self.pool().get()?;
        conn.query_row(
            "SELECT id, title, body, pinned, created_at, updated_at
               FROM notes WHERE id = ?1",
            params![id],
            row_to_note,
        )
        .optional()?
        .ok_or(Error::NotFound)
    }

    pub fn update_note(&self, id: i64, patch: &UpdateNote) -> Result<Note> {
        let current = self.get_note(id)?;
        let title = match &patch.title {
            Some(t) => t.trim().to_string(),
            None => current.title.clone(),
        };
        let body = match &patch.body {
            Some(b) => b.clone(),
            None => current.body.clone(),
        };
        let pinned = patch.pinned.unwrap_or(current.pinned);

        let text_changed = title != current.title || body != current.body;

        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;

        // Snapshot the OLD title+body to note_revisions BEFORE we
        // overwrite — but only when the content actually changed and
        // the previous snapshot is at least REVISION_MIN_GAP_SECONDS
        // old. The gap matters: during continuous typing the auto-
        // save fires every ~1s, and snapshotting every save would
        // burn through the retention cap in under a minute.
        if text_changed {
            let last_revision_at: Option<i64> = tx
                .query_row(
                    "SELECT MAX(created_at) FROM note_revisions WHERE note_id = ?1",
                    params![id],
                    |r| r.get::<_, Option<i64>>(0),
                )
                .optional()?
                .flatten();
            let now: i64 = tx.query_row("SELECT strftime('%s','now')", [], |r| {
                r.get::<_, String>(0).map(|s| s.parse::<i64>().unwrap_or(0))
            })?;
            let take_snapshot = match last_revision_at {
                None => true,
                Some(prev) => now - prev >= REVISION_MIN_GAP_SECONDS,
            };
            if take_snapshot {
                tx.execute(
                    "INSERT INTO note_revisions (note_id, title, body, created_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![id, current.title, current.body, now],
                )?;
                // Prune anything beyond the retention cap, oldest first.
                tx.execute(
                    "DELETE FROM note_revisions
                       WHERE note_id = ?1
                         AND id NOT IN (
                             SELECT id FROM note_revisions
                              WHERE note_id = ?1
                              ORDER BY created_at DESC, id DESC
                              LIMIT ?2
                         )",
                    params![id, REVISION_RETENTION_PER_NOTE],
                )?;
            }
        }

        tx.execute(
            "UPDATE notes
                SET title = ?1, body = ?2, pinned = ?3,
                    updated_at = strftime('%s','now')
              WHERE id = ?4",
            params![title, body, i64::from(pinned), id],
        )?;
        tx.commit()?;
        self.get_note(id)
    }

    pub fn delete_note(&self, id: i64) -> Result<()> {
        let conn = self.pool().get()?;
        let n = conn.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    pub fn list_note_revisions(&self, note_id: i64) -> Result<Vec<NoteRevision>> {
        // Bail with NotFound if the parent note is gone, so the HTTP
        // layer returns 404 instead of an empty list (which would look
        // like "no history yet" — different meaning).
        self.get_note(note_id)?;
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, note_id, title, body, created_at
               FROM note_revisions
              WHERE note_id = ?1
              ORDER BY created_at DESC, id DESC",
        )?;
        let rows = stmt.query_map(params![note_id], row_to_revision)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_note_revision(&self, note_id: i64, revision_id: i64) -> Result<NoteRevision> {
        let conn = self.pool().get()?;
        conn.query_row(
            "SELECT id, note_id, title, body, created_at
               FROM note_revisions
              WHERE id = ?1 AND note_id = ?2",
            params![revision_id, note_id],
            row_to_revision,
        )
        .optional()?
        .ok_or(Error::NotFound)
    }

    /// Replace the note's current title+body with the named revision.
    /// The pre-restore state goes through the normal `update_note` path
    /// so it itself gets snapshotted into history — undoing a restore
    /// is a regular Restore on the most-recent revision.
    pub fn restore_note_revision(&self, note_id: i64, revision_id: i64) -> Result<Note> {
        let rev = self.get_note_revision(note_id, revision_id)?;
        let patch = UpdateNote {
            title: Some(rev.title),
            body: Some(rev.body),
            pinned: None,
        };
        self.update_note(note_id, &patch)
    }
}

fn row_to_note(r: &rusqlite::Row) -> rusqlite::Result<Note> {
    Ok(Note {
        id: r.get(0)?,
        title: r.get(1)?,
        body: r.get(2)?,
        pinned: r.get::<_, i64>(3)? != 0,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

fn row_to_revision(r: &rusqlite::Row) -> rusqlite::Result<NoteRevision> {
    Ok(NoteRevision {
        id: r.get(0)?,
        note_id: r.get(1)?,
        title: r.get(2)?,
        body: r.get(3)?,
        created_at: r.get(4)?,
    })
}
