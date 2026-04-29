use rusqlite::params;
use serde::Serialize;

use super::Db;
use crate::error::Result;

#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    pub id: i64,
    pub ts_utc: i64,
    pub event_type: String,
    pub detail: Option<String>,
    pub ip: Option<String>,
    pub category: String,
}

impl Db {
    /// Log a security-category event. Kept as a thin wrapper so existing
    /// call sites don't need to spell the category out.
    pub fn log_event(
        &self,
        event_type: &str,
        detail: Option<&str>,
        ip: Option<&str>,
    ) -> Result<()> {
        self.log_event_cat("security", event_type, detail, ip)
    }

    /// Log an activity-category event — sync cycles, SMTP send, IMAP errors.
    pub fn log_activity(&self, event_type: &str, detail: Option<&str>) -> Result<()> {
        self.log_event_cat("activity", event_type, detail, None)
    }

    pub fn log_event_cat(
        &self,
        category: &str,
        event_type: &str,
        detail: Option<&str>,
        ip: Option<&str>,
    ) -> Result<()> {
        let conn = self.pool().get()?;
        conn.execute(
            "INSERT INTO audit_log (ts_utc, event_type, detail, ip, category)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                chrono::Utc::now().timestamp(),
                event_type,
                detail,
                ip,
                category
            ],
        )?;
        Ok(())
    }

    pub fn list_audit(
        &self,
        category: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AuditEntry>> {
        let conn = self.pool().get()?;
        match category {
            Some(cat) => {
                let mut stmt = conn.prepare(
                    "SELECT id, ts_utc, event_type, detail, ip, category
                     FROM audit_log WHERE category = ?1
                     ORDER BY ts_utc DESC LIMIT ?2 OFFSET ?3",
                )?;
                let rows = stmt.query_map(params![cat, limit, offset], map_row)?;
                Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT id, ts_utc, event_type, detail, ip, category
                     FROM audit_log ORDER BY ts_utc DESC LIMIT ?1 OFFSET ?2",
                )?;
                let rows = stmt.query_map(params![limit, offset], map_row)?;
                Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
            }
        }
    }
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AuditEntry> {
    Ok(AuditEntry {
        id: r.get(0)?,
        ts_utc: r.get(1)?,
        event_type: r.get(2)?,
        detail: r.get(3)?,
        ip: r.get(4)?,
        category: r.get(5)?,
    })
}
