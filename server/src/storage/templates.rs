//! Email templates: reusable subject + body snippets for the compose
//! form. Global (not per-account). The HTTP layer must already have
//! passed `require_unlocked()` — these read/write the encrypted DB.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::Db;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize)]
pub struct EmailTemplate {
    pub id: i64,
    pub name: String,
    pub subject: String,
    pub body: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewTemplate {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateTemplate {
    pub name: Option<String>,
    pub subject: Option<String>,
    pub body: Option<String>,
}

/// Templates are picked from a dropdown; a name longer than this is
/// unreadable there and almost certainly a paste mistake.
const MAX_NAME_LEN: usize = 120;

fn validate_name(raw: &str) -> Result<String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(Error::BadRequest("template name is required".into()));
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(Error::BadRequest(format!(
            "template name must be {MAX_NAME_LEN} characters or fewer"
        )));
    }
    Ok(name.to_string())
}

impl Db {
    pub fn insert_template(&self, new: &NewTemplate) -> Result<EmailTemplate> {
        let name = validate_name(&new.name)?;
        let conn = self.pool().get()?;
        conn.execute(
            "INSERT INTO email_templates (name, subject, body) VALUES (?1, ?2, ?3)",
            params![name, new.subject.trim(), new.body],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        self.get_template(id)
    }

    pub fn list_templates(&self) -> Result<Vec<EmailTemplate>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, subject, body, created_at, updated_at
               FROM email_templates
               ORDER BY name COLLATE NOCASE ASC, id ASC",
        )?;
        let rows = stmt.query_map([], row_to_template)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_template(&self, id: i64) -> Result<EmailTemplate> {
        let conn = self.pool().get()?;
        conn.query_row(
            "SELECT id, name, subject, body, created_at, updated_at
               FROM email_templates WHERE id = ?1",
            params![id],
            row_to_template,
        )
        .optional()?
        .ok_or(Error::NotFound)
    }

    pub fn update_template(&self, id: i64, patch: &UpdateTemplate) -> Result<EmailTemplate> {
        let current = self.get_template(id)?;
        let name = match &patch.name {
            Some(n) => validate_name(n)?,
            None => current.name,
        };
        let subject = patch
            .subject
            .as_deref()
            .map_or(current.subject, |s| s.trim().to_string());
        let body = patch.body.clone().unwrap_or(current.body);
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE email_templates
                SET name = ?2, subject = ?3, body = ?4,
                    updated_at = strftime('%s','now')
              WHERE id = ?1",
            params![id, name, subject, body],
        )?;
        drop(conn);
        self.get_template(id)
    }

    pub fn delete_template(&self, id: i64) -> Result<()> {
        let conn = self.pool().get()?;
        let n = conn.execute("DELETE FROM email_templates WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(Error::NotFound);
        }
        Ok(())
    }
}

fn row_to_template(r: &rusqlite::Row<'_>) -> rusqlite::Result<EmailTemplate> {
    Ok(EmailTemplate {
        id: r.get(0)?,
        name: r.get(1)?,
        subject: r.get(2)?,
        body: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("t.db")).unwrap();
        db.migrate().unwrap();
        (dir, db)
    }

    #[test]
    fn insert_list_update_delete_roundtrip() {
        let (_dir, db) = db();
        let t = db
            .insert_template(&NewTemplate {
                name: "  Invoice reminder ".into(),
                subject: "Invoice overdue".into(),
                body: "Hi,\n\nJust a reminder.".into(),
            })
            .unwrap();
        assert_eq!(t.name, "Invoice reminder");
        assert_eq!(db.list_templates().unwrap().len(), 1);

        let t2 = db
            .update_template(
                t.id,
                &UpdateTemplate {
                    body: Some("New body".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(t2.name, "Invoice reminder");
        assert_eq!(t2.subject, "Invoice overdue");
        assert_eq!(t2.body, "New body");

        db.delete_template(t.id).unwrap();
        assert!(db.list_templates().unwrap().is_empty());
        assert!(matches!(db.delete_template(t.id), Err(Error::NotFound)));
    }

    #[test]
    fn list_is_sorted_by_name_case_insensitively() {
        let (_dir, db) = db();
        for n in ["zeta", "Alpha", "beta"] {
            db.insert_template(&NewTemplate { name: n.into(), ..Default::default() })
                .unwrap();
        }
        let names: Vec<String> = db.list_templates().unwrap().into_iter().map(|t| t.name).collect();
        assert_eq!(names, vec!["Alpha", "beta", "zeta"]);
    }

    #[test]
    fn blank_name_is_rejected() {
        let (_dir, db) = db();
        let r = db.insert_template(&NewTemplate { name: "   ".into(), ..Default::default() });
        assert!(matches!(r, Err(Error::BadRequest(_))));
    }
}
