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

#[allow(dead_code)] // status string round-trip kept for completeness
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
    /// Serialized `SendRequest`. Callers deserialize when they need it —
    /// the list UI only wants the summary fields.
    pub payload_json: String,
    pub scheduled_at: i64,
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub summary_to: String,
    pub summary_subject: String,
    pub sent_message_id: Option<String>,
    /// JSON-encoded `SendForensics` captured at dispatch time. The schema
    /// is whatever `crate::send::SendForensics` serializes to; consumers
    /// that care about individual fields re-parse.
    pub forensics_json: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Db {
    /// Enqueue a new outbox entry. Returns the new row id.
    ///
    /// `payload_json` is the already-serialized `SendRequest`. We don't
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
            params![
                account_id,
                payload_json,
                scheduled_at,
                summary_to,
                summary_subject
            ],
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
    /// The `SQLite` CTE keeps the ids visible to the UPDATE; we do a
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
        tx.execute(&sql, rusqlite::params_from_iter(ids.iter().copied()))?;
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

    pub fn outbox_mark_sent(&self, id: i64, message_id: &str, forensics_json: &str) -> Result<()> {
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

    /// Permanently delete every failed row. Returns the number of
    /// rows removed so the UI can confirm the action. Scope is
    /// deliberately narrow — only `status='failed'` so a stray click
    /// can't nuke pending sends that haven't dispatched yet.
    pub fn outbox_clear_failed(&self) -> Result<usize> {
        let conn = self.pool().get()?;
        let n = conn.execute("DELETE FROM outbox WHERE status = 'failed'", [])?;
        Ok(n)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Spin up an empty migrated DB plus a single test account row.
    /// Outbox rows reference accounts via FK, so every test needs at
    /// least one. Bypasses the `insert_account` path because that
    /// requires a Vault to encrypt the password — these tests are
    /// about the outbox state machine, not credential storage.
    fn db_with_account() -> (tempfile::TempDir, Db, i64) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        let db = Db::open(&path).unwrap();
        db.migrate().unwrap();
        let conn = db.pool().get().unwrap();
        conn.execute(
            "INSERT INTO accounts (kind, email, imap_host, imap_port, credential_ref, created_at)
             VALUES ('imap', 'test@example.com', 'imap.example.com', 993, 'test-cred', strftime('%s','now'))",
            [],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        (dir, db, id)
    }

    fn enqueue(db: &Db, account_id: i64, scheduled_at: i64) -> i64 {
        db.outbox_enqueue(
            account_id,
            "{}",
            scheduled_at,
            "to@example.com",
            "test subject",
        )
        .unwrap()
    }

    // ── Enqueue → Pending ────────────────────────────────────────────

    #[test]
    fn enqueue_creates_pending_row() {
        let (_dir, db, acc) = db_with_account();
        let id = enqueue(&db, acc, 100);
        let row = db.outbox_get(id).unwrap();
        assert_eq!(row.status, "pending");
        assert_eq!(row.attempts, 0);
        assert_eq!(row.scheduled_at, 100);
        assert!(row.last_error.is_none());
    }

    // ── Cancel — only valid from pending ─────────────────────────────

    #[test]
    fn cancel_pending_succeeds() {
        let (_dir, db, acc) = db_with_account();
        let id = enqueue(&db, acc, 100);
        assert!(db.outbox_cancel(id).unwrap());
        assert_eq!(db.outbox_get(id).unwrap().status, "cancelled");
    }

    /// Cancelling a row that's already `sending` must be a no-op —
    /// otherwise an undo race could cancel a row that's already
    /// being delivered, leaving SMTP-side state inconsistent with
    /// the DB.
    #[test]
    fn cancel_sending_row_is_rejected() {
        let (_dir, db, acc) = db_with_account();
        let id = enqueue(&db, acc, 0);
        let claimed = db.outbox_claim_due(0, 10).unwrap();
        assert_eq!(claimed.len(), 1);
        assert!(!db.outbox_cancel(id).unwrap());
        assert_eq!(db.outbox_get(id).unwrap().status, "sending");
    }

    // ── Reschedule — only valid from pending ─────────────────────────

    #[test]
    fn reschedule_pending_updates_time() {
        let (_dir, db, acc) = db_with_account();
        let id = enqueue(&db, acc, 100);
        assert!(db.outbox_reschedule(id, 999).unwrap());
        assert_eq!(db.outbox_get(id).unwrap().scheduled_at, 999);
    }

    #[test]
    fn reschedule_sending_row_is_rejected() {
        let (_dir, db, acc) = db_with_account();
        let id = enqueue(&db, acc, 0);
        db.outbox_claim_due(0, 10).unwrap();
        assert!(!db.outbox_reschedule(id, 9999).unwrap());
    }

    // ── Claim — atomic pending → sending, due-time gating ────────────

    /// The atomicity property: a row claimed once cannot be claimed
    /// again. This is the no-double-dispatch guarantee — getting it
    /// wrong means the same email might be SMTP-sent twice.
    #[test]
    fn claim_due_transitions_pending_to_sending_atomically() {
        let (_dir, db, acc) = db_with_account();
        let id = enqueue(&db, acc, 50);
        let first = db.outbox_claim_due(100, 10).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].id, id);
        assert_eq!(first[0].status, "sending");
        assert_eq!(first[0].attempts, 1);
        // A second claim sees nothing — the row is no longer pending.
        let second = db.outbox_claim_due(100, 10).unwrap();
        assert!(second.is_empty(), "row was double-claimed");
    }

    #[test]
    fn claim_due_skips_rows_scheduled_in_future() {
        let (_dir, db, acc) = db_with_account();
        let _id = enqueue(&db, acc, 1_000);
        let claimed = db.outbox_claim_due(500, 10).unwrap();
        assert!(claimed.is_empty());
    }

    #[test]
    fn claim_due_orders_by_scheduled_at_ascending() {
        let (_dir, db, acc) = db_with_account();
        let later = enqueue(&db, acc, 200);
        let earlier = enqueue(&db, acc, 100);
        let claimed = db.outbox_claim_due(500, 10).unwrap();
        assert_eq!(claimed.len(), 2);
        assert_eq!(claimed[0].id, earlier);
        assert_eq!(claimed[1].id, later);
    }

    #[test]
    fn claim_due_respects_limit() {
        let (_dir, db, acc) = db_with_account();
        for i in 0..5 {
            enqueue(&db, acc, i);
        }
        let claimed = db.outbox_claim_due(500, 2).unwrap();
        assert_eq!(claimed.len(), 2);
    }

    /// Cancelled rows must never be claimed for dispatch — getting
    /// this wrong sends an email the user already retracted.
    #[test]
    fn claim_due_skips_cancelled_rows() {
        let (_dir, db, acc) = db_with_account();
        let id = enqueue(&db, acc, 0);
        assert!(db.outbox_cancel(id).unwrap());
        let claimed = db.outbox_claim_due(500, 10).unwrap();
        assert!(claimed.is_empty());
    }

    // ── mark_sent / mark_failed terminal transitions ────────────────

    #[test]
    fn mark_sent_records_message_id_and_clears_error() {
        let (_dir, db, acc) = db_with_account();
        let id = enqueue(&db, acc, 0);
        db.outbox_claim_due(0, 10).unwrap();
        db.outbox_mark_sent(id, "<msg@example.com>", "{}").unwrap();
        let row = db.outbox_get(id).unwrap();
        assert_eq!(row.status, "sent");
        assert_eq!(row.sent_message_id.as_deref(), Some("<msg@example.com>"));
        assert!(row.last_error.is_none());
    }

    #[test]
    fn mark_failed_records_error() {
        let (_dir, db, acc) = db_with_account();
        let id = enqueue(&db, acc, 0);
        db.outbox_claim_due(0, 10).unwrap();
        db.outbox_mark_failed(id, "smtp 550 rejected").unwrap();
        let row = db.outbox_get(id).unwrap();
        assert_eq!(row.status, "failed");
        assert_eq!(row.last_error.as_deref(), Some("smtp 550 rejected"));
    }

    // ── Stale-sending rescue — recover from worker crash ─────────────

    /// If the worker crashes while a row is in `sending`, the
    /// startup rescue must flip it back to `pending`. Otherwise the
    /// email never gets retried — silent loss.
    #[test]
    fn requeue_stale_sending_recovers_old_rows() {
        let (_dir, db, acc) = db_with_account();
        let id = enqueue(&db, acc, 0);
        db.outbox_claim_due(0, 10).unwrap();

        // Backdate updated_at to simulate a crash 10 minutes ago.
        let conn = db.pool().get().unwrap();
        conn.execute(
            "UPDATE outbox SET updated_at = strftime('%s','now') - 600 WHERE id = ?1",
            params![id],
        )
        .unwrap();
        drop(conn);

        let n = db.outbox_requeue_stale_sending(60).unwrap();
        assert_eq!(n, 1);
        assert_eq!(db.outbox_get(id).unwrap().status, "pending");
    }

    #[test]
    fn requeue_stale_sending_leaves_recent_sending_alone() {
        let (_dir, db, acc) = db_with_account();
        let id = enqueue(&db, acc, 0);
        db.outbox_claim_due(0, 10).unwrap();
        let n = db.outbox_requeue_stale_sending(60).unwrap();
        assert_eq!(n, 0);
        assert_eq!(db.outbox_get(id).unwrap().status, "sending");
    }

    // ── Listing + clearing failures ─────────────────────────────────

    #[test]
    fn list_active_excludes_terminal_states() {
        let (_dir, db, acc) = db_with_account();
        let pending = enqueue(&db, acc, 100);
        let to_send = enqueue(&db, acc, 0);
        let to_fail = enqueue(&db, acc, 0);
        let to_cancel = enqueue(&db, acc, 200);

        db.outbox_cancel(to_cancel).unwrap();
        db.outbox_claim_due(0, 10).unwrap();
        db.outbox_mark_sent(to_send, "<id@x>", "{}").unwrap();
        db.outbox_mark_failed(to_fail, "boom").unwrap();

        let active: Vec<i64> = db
            .outbox_list_active()
            .unwrap()
            .iter()
            .map(|e| e.id)
            .collect();
        assert!(active.contains(&pending));
        assert!(!active.contains(&to_send));
        assert!(!active.contains(&to_fail));
        assert!(!active.contains(&to_cancel));
    }

    #[test]
    fn clear_failed_only_removes_failed_rows() {
        let (_dir, db, acc) = db_with_account();
        let pending = enqueue(&db, acc, 100);
        let to_fail = enqueue(&db, acc, 0);
        db.outbox_claim_due(0, 10).unwrap();
        db.outbox_mark_failed(to_fail, "x").unwrap();

        let removed = db.outbox_clear_failed().unwrap();
        assert_eq!(removed, 1);
        // Pending row untouched.
        assert!(db.outbox_get(pending).is_ok());
    }
}
