//! Secure notes storage. NordPass-style encrypted notepad backed by
//! the same SQLCipher DB as the rest of Postern, so unlocking the
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

impl Db {
    pub fn insert_note(&self, new: &NewNote) -> Result<Note> {
        let title = new.title.trim().to_string();
        let body = new.body.clone();
        let conn = self.pool().get()?;
        conn.execute(
            "INSERT INTO notes (title, body, pinned) VALUES (?1, ?2, ?3)",
            params![title, body, new.pinned as i64],
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
            None => current.title,
        };
        let body = match &patch.body {
            Some(b) => b.clone(),
            None => current.body,
        };
        let pinned = patch.pinned.unwrap_or(current.pinned);

        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE notes
                SET title = ?1, body = ?2, pinned = ?3,
                    updated_at = strftime('%s','now')
              WHERE id = ?4",
            params![title, body, pinned as i64, id],
        )?;
        drop(conn);
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
