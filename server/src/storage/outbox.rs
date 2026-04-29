//! Outbox queue — durable store for pending/scheduled sends.
//!
//! Every POST /api/send enqueues into this table and a background
//! worker drains it. Undo-send is a row with `scheduled_at` a few
//! seconds in the future that gets cancelled before the worker sees
//! it. Send-later is the same row with a larger delay.
//!
//! Status transitions:
//!   pending   → sending   (worker claims for dispatch)
//!   sending   → sent      (SMTP accepted)
//!   sending   → failed    (SMTP refused / network error / etc)
//!   pending   → cancelled (user hit undo or cancelled a scheduled send)

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::Db;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxStatus {
    Pending,
    Sending,
    Sent,
    Failed,
    Cancelled,
}

impl OutboxStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutboxStatus::Pending => "pending",
            OutboxStatus::Sending => "sending",
            OutboxStatus::Sent => "sent",
            OutboxStatus::Failed => "failed",
            OutboxStatus::Cancelled => "cancelled",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "pending" => OutboxStatus::Pending,
            "sending" => OutboxStatus::Sending,
            "sent" => OutboxStatus::Sent,
            "failed" => OutboxStatus::Failed,
            "cancelled" => OutboxStatus::Cancelled,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OutboxEntry {
    pub id: i64,
    pub account_id: i64,
    /// Serialized SendRequest. Callers deserialize when they need it —
    /// the list UI only wants the summary fields.
    pub payload_json: String,
    pub scheduled_at: i64,
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub summary_to: String,
    pub summary_subject: String,
    pub sent_message_id: Option<String>,
    /// JSON-encoded SendForensics captured at dispatch time. The schema
    /// is whatever `crate::send::SendForensics` serializes to; consumers
    /// that care about individual fields re-parse.
    pub forensics_json: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Db {
    /// Enqueue a new outbox entry. Returns the new row id.
    ///
    /// `payload_json` is the already-serialized SendRequest. We don't
    /// take the typed struct here so this module can stay independent
    /// of `crate::send` and avoid the circular dep.
    pub fn outbox_enqueue(
        &self,
        account_id: i64,
        payload_json: &str,
        scheduled_at: i64,
        summary_to: &str,
        summary_subject: &str,
    ) -> Result<i64> {
        let conn = self.pool().get()?;
        conn.execute(
            "INSERT INTO outbox (account_id, payload_json, scheduled_at, status,
                                 summary_to, summary_subject)
             VALUES (?1, ?2, ?3, 'pending', ?4, ?5)",
            params![account_id, payload_json, scheduled_at, summary_to, summary_subject],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Cancel a pending row. Returns true when the row transitioned,
    /// false when it was already past 'pending' (too late to undo).
    pub fn outbox_cancel(&self, id: i64) -> Result<bool> {
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE outbox
                SET status = 'cancelled', updated_at = strftime('%s','now')
              WHERE id = ?1 AND status = 'pending'",
            params![id],
        )?;
        Ok(n > 0)
    }

    /// Reschedule a pending row (e.g. user adjusts a scheduled send).
    /// Rejected if the row is no longer pending.
    pub fn outbox_reschedule(&self, id: i64, scheduled_at: i64) -> Result<bool> {
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE outbox
                SET scheduled_at = ?1, updated_at = strftime('%s','now')
              WHERE id = ?2 AND status = 'pending'",
            params![scheduled_at, id],
        )?;
        Ok(n > 0)
    }

    /// Atomically claim the next batch of due rows. Each returned entry
    /// is transitioned from 'pending' to 'sending' as part of the
    /// selection so concurrent workers can't double-dispatch.
    ///
    /// The SQLite CTE keeps the ids visible to the UPDATE; we do a
    /// second SELECT by id to return the full rows.
    pub fn outbox_claim_due(&self, now: i64, limit: usize) -> Result<Vec<OutboxEntry>> {
        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;
        let ids: Vec<i64> = {
            let mut stmt = tx.prepare(
                "SELECT id FROM outbox
                  WHERE status = 'pending' AND scheduled_at <= ?1
                  ORDER BY scheduled_at ASC
                  LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![now, limit as i64], |r| r.get::<_, i64>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        if ids.is_empty() {
            tx.commit()?;
            return Ok(Vec::new());
        }
        // Flip to 'sending' in bulk. attempts is bumped so the UI can
        // show a retry count on eventual failure.
        let placeholders: Vec<String> = (0..ids.len()).map(|i| format!("?{}", i + 1)).collect();
        let sql = format!(
            "UPDATE outbox
                SET status = 'sending', attempts = attempts + 1,
                    updated_at = strftime('%s','now')
              WHERE id IN ({})",
            placeholders.join(",")
        );
        tx.execute(
            &sql,
            rusqlite::params_from_iter(ids.iter().copied()),
        )?;
        // Fetch the now-claimed rows.
        let select_sql = format!(
            "SELECT id, account_id, payload_json, scheduled_at, status, attempts,
                    last_error, summary_to, summary_subject, sent_message_id,
                    forensics_json, created_at, updated_at
               FROM outbox WHERE id IN ({}) ORDER BY scheduled_at ASC",
            placeholders.join(",")
        );
        let entries: Vec<OutboxEntry> = {
            let mut stmt = tx.prepare(&select_sql)?;
            let rows = stmt.query_map(
                rusqlite::params_from_iter(ids.iter().copied()),
                row_to_outbox,
            )?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        tx.commit()?;
        Ok(entries)
    }

    pub fn outbox_mark_sent(
        &self,
        id: i64,
        message_id: &str,
        forensics_json: &str,
    ) -> Result<()> {
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE outbox
                SET status = 'sent', sent_message_id = ?1, forensics_json = ?2,
                    last_error = NULL, updated_at = strftime('%s','now')
              WHERE id = ?3",
            params![message_id, forensics_json, id],
        )?;
        Ok(())
    }

    pub fn outbox_mark_failed(&self, id: i64, err: &str) -> Result<()> {
        let conn = self.pool().get()?;
        conn.execute(
            "UPDATE outbox
                SET status = 'failed', last_error = ?1,
                    updated_at = strftime('%s','now')
              WHERE id = ?2",
            params![err, id],
        )?;
        Ok(())
    }

    /// If the worker crashes mid-dispatch (process restart, panic), rows
    /// can stay stuck in 'sending' forever. Called at startup — any row
    /// left in 'sending' older than `max_age_secs` gets reverted to
    /// 'pending' so the next worker tick retries it.
    pub fn outbox_requeue_stale_sending(&self, max_age_secs: i64) -> Result<usize> {
        let conn = self.pool().get()?;
        let n = conn.execute(
            "UPDATE outbox
                SET status = 'pending',
                    updated_at = strftime('%s','now')
              WHERE status = 'sending'
                AND updated_at < strftime('%s','now') - ?1",
            params![max_age_secs],
        )?;
        Ok(n)
    }

    pub fn outbox_get(&self, id: i64) -> Result<OutboxEntry> {
        let conn = self.pool().get()?;
        conn.query_row(
            "SELECT id, account_id, payload_json, scheduled_at, status, attempts,
                    last_error, summary_to, summary_subject, sent_message_id,
                    forensics_json, created_at, updated_at
               FROM outbox WHERE id = ?1",
            params![id],
            row_to_outbox,
        )
        .optional()?
        .ok_or(Error::NotFound)
    }

    /// Active = pending or sending. Used by the UI for the "scheduled"
    /// view and also for the main app badge/indicator.
    pub fn outbox_list_active(&self) -> Result<Vec<OutboxEntry>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, account_id, payload_json, scheduled_at, status, attempts,
                    last_error, summary_to, summary_subject, sent_message_id,
                    forensics_json, created_at, updated_at
               FROM outbox WHERE status IN ('pending','sending')
               ORDER BY scheduled_at ASC",
        )?;
        let rows = stmt.query_map([], row_to_outbox)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Recent failures — shown in the UI so users can diagnose.
    pub fn outbox_list_recent_failures(&self, limit: usize) -> Result<Vec<OutboxEntry>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, account_id, payload_json, scheduled_at, status, attempts,
                    last_error, summary_to, summary_subject, sent_message_id,
                    forensics_json, created_at, updated_at
               FROM outbox WHERE status = 'failed'
               ORDER BY updated_at DESC
               LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], row_to_outbox)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

fn row_to_outbox(r: &rusqlite::Row) -> rusqlite::Result<OutboxEntry> {
    Ok(OutboxEntry {
        id: r.get(0)?,
        account_id: r.get(1)?,
        payload_json: r.get(2)?,
        scheduled_at: r.get(3)?,
        status: r.get(4)?,
        attempts: r.get(5)?,
        last_error: r.get(6)?,
        summary_to: r.get(7)?,
        summary_subject: r.get(8)?,
        sent_message_id: r.get(9)?,
        forensics_json: r.get(10)?,
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
    })
}
