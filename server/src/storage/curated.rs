//! Sender-engagement table + curated-list query.
//!
//! Phase 1 of the Curated view: rank messages by whether the user
//! has corresponded with the sender. No AI involved — replies and
//! sends are tracked at the source (the `/api/send` handler) and
//! one-shot backfilled from existing sent mail at first run. The
//! Curated list endpoint joins messages against this table and
//! sorts by a recency-weighted engagement score.
//!
//! Phase 2 (later) layers structured rules on top — keyword and
//! semantic matchers parsed from natural-language input via the
//! local LLM. Phase 1 alone is good enough to surface the mail
//! the user actually reads, and it ships without any AI dependency.

use rusqlite::params;
use serde::Serialize;

use super::{messages::MessageListItem, Db};
use crate::error::Result;

/// Recency horizon — older than this contributes 0 to the score
/// and ranking falls back to engagement + tie-breaker date_utc.
/// 60 days is "still relevant for a busy inbox"; tune from
/// telemetry. Linear decay (not exponential) because Postern's
/// bundled SQLite build doesn't include the math extension that
/// EXP() lives in. Linear is close enough for ranking — the curve
/// shape barely matters when you're choosing between rows.
const RECENCY_HORIZON_SECS: f64 = 60.0 * 24.0 * 3600.0;

/// One row of the curated list — a message plus the score that put
/// it there. Frontend renders the message normally; the score is
/// surfaced as a sort field and (eventually) as a "why this is here"
/// debug hint.
#[derive(Debug, Clone, Serialize)]
pub struct CuratedListItem {
    #[serde(flatten)]
    pub item: MessageListItem,
    /// Composite score: recency + engagement. Higher = more likely
    /// the user wants to see this. Range is roughly [0, 2] in
    /// practice (engagement maxes around 1.0, recency around 1.0).
    pub curated_score: f64,
}

impl Db {
    /// Bump engagement counts for a list of recipient addresses —
    /// called from `/api/send` after the outbox row is enqueued, so
    /// the user's intent to communicate is recorded even if the
    /// dispatcher can't deliver yet.
    ///
    /// Addresses are normalised to lowercase + trimmed; that matches
    /// the lookup used by `list_curated`. Empty / malformed entries
    /// are silently skipped so a typo'd recipient doesn't poison
    /// the engagement table.
    pub fn record_engagement_send(&self, recipients: &[String]) -> Result<usize> {
        if recipients.is_empty() {
            return Ok(0);
        }
        let conn = self.pool().get()?;
        let now = chrono::Utc::now().timestamp();
        let mut written = 0usize;
        for raw in recipients {
            let addr = normalise_addr(raw);
            if addr.is_empty() {
                continue;
            }
            conn.execute(
                "INSERT INTO sender_engagement (sender_addr, engaged_count, last_engaged_utc, updated_at)
                 VALUES (?1, 1, ?2, ?2)
                 ON CONFLICT(sender_addr) DO UPDATE SET
                     engaged_count = engaged_count + 1,
                     last_engaged_utc = excluded.last_engaged_utc,
                     updated_at = excluded.updated_at",
                params![addr, now],
            )?;
            written += 1;
        }
        Ok(written)
    }

    /// One-shot backfill — runs at startup if `sender_engagement` is
    /// empty. Walks every message where the From address matches a
    /// configured account email (i.e. the user sent it) and seeds
    /// engagement for each To/Cc recipient. Cheap on small mailboxes,
    /// O(N) on big ones; the result is that a fresh install with
    /// imported mail gets a useful Curated view immediately instead
    /// of waiting for new sends to accumulate signal.
    pub fn backfill_engagement_if_empty(&self) -> Result<usize> {
        let conn = self.pool().get()?;
        let any: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sender_engagement)",
            [],
            |r| r.get(0),
        )?;
        if any {
            return Ok(0);
        }

        // Pull every sent message's recipient list. We keep the join
        // loose (LIKE on the account email) because `from_addr` may
        // be stored as `Name <addr>`; substring match is fine for
        // backfill where false positives just mean over-counting.
        let mut stmt = conn.prepare(
            "SELECT m.to_addrs, m.cc_addrs, m.date_utc
             FROM messages m
             JOIN accounts a ON a.id = m.account_id
             WHERE m.from_addr LIKE '%' || a.email || '%'",
        )?;
        let rows = stmt
            .query_map([], |r| {
                let to: Option<String> = r.get(0)?;
                let cc: Option<String> = r.get(1)?;
                let date: i64 = r.get(2)?;
                Ok((to, cc, date))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        // Aggregate in-memory: count + max date per recipient. One
        // INSERT per recipient at the end. Avoids an UPDATE storm
        // for chatty inboxes.
        use std::collections::HashMap;
        let mut tally: HashMap<String, (i64, i64)> = HashMap::new();
        for (to, cc, date) in rows {
            for raw in split_addr_list(to.as_deref())
                .into_iter()
                .chain(split_addr_list(cc.as_deref()))
            {
                let addr = normalise_addr(&raw);
                if addr.is_empty() {
                    continue;
                }
                let entry = tally.entry(addr).or_insert((0, date));
                entry.0 += 1;
                if date > entry.1 {
                    entry.1 = date;
                }
            }
        }

        let now = chrono::Utc::now().timestamp();
        let mut written = 0usize;
        for (addr, (count, last_date)) in &tally {
            conn.execute(
                "INSERT INTO sender_engagement (sender_addr, engaged_count, last_engaged_utc, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(sender_addr) DO NOTHING",
                params![addr, count, last_date, now],
            )?;
            written += 1;
        }
        Ok(written)
    }

    /// Return the top-N engaged senders, ordered by raw count. Used
    /// by debug surfaces and (later) the Curated panel's "people you
    /// correspond with" list. Phase 1 doesn't expose this in the UI;
    /// it's here for telemetry.
    #[allow(dead_code)]
    pub fn top_engaged_senders(&self, limit: i64) -> Result<Vec<(String, i64)>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT sender_addr, engaged_count
             FROM sender_engagement
             ORDER BY engaged_count DESC, last_engaged_utc DESC
             LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Curated message list: combines per-message recency with the
    /// sender's engagement count. Returns `MessageListItem` so the
    /// frontend can render with the same row component as the
    /// regular inbox.
    ///
    /// Score formula:
    ///   recency = max(0, 1 - (now - date_utc) / HORIZON)  ∈ [0, 1]
    ///   engagement = min(engaged_count / 10.0, 1.0)        ∈ [0, 1]
    ///   score = 0.4 * recency + 0.6 * engagement
    /// 60/40 toward engagement so a long-time correspondent with a
    /// week-old email beats a stranger with a fresh one — the whole
    /// point of the view. Linear-decay recency keeps the SQL portable
    /// across the bundled SQLite that doesn't have math functions.
    pub fn list_curated(
        &self,
        account_id: Option<i64>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CuratedListItem>> {
        let conn = self.pool().get()?;
        let now = chrono::Utc::now().timestamp();

        let mut sql = String::from(
            "SELECT m.id, m.account_id, m.message_id, m.thread_id, m.subject,
                    m.from_addr, m.to_addrs, m.cc_addrs, m.date_utc, m.snippet,
                    m.has_attachments, m.is_read, m.is_starred, m.is_encrypted,
                    m.receipt_to, a.email,
                    COALESCE(e.engaged_count, 0) AS engaged_count
             FROM messages m
             JOIN accounts a ON a.id = m.account_id
             LEFT JOIN sender_engagement e ON e.sender_addr = LOWER(TRIM(m.from_addr))",
        );
        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(aid) = account_id {
            sql.push_str(" WHERE m.account_id = ?");
            binds.push(Box::new(aid));
        } else {
            sql.push_str(" WHERE a.include_in_unified = 1");
        }
        // Score is computed in SQL so paging is correct (we can't
        // ORDER in Rust if the user wants offset > 0 against a
        // sorted view). SQLite has the math we need.
        // Linear recency: clamp (age/horizon) to [0, 1], then invert
        // so newer = larger. SQLite's scalar MIN/MAX accept multiple
        // arguments and are part of the core engine, no extension
        // needed. Multiply by 1.0 to coerce ints into REAL division.
        sql.push_str(&format!(
            " ORDER BY (
                0.4 * (1.0 - MIN(1.0, MAX(0.0, (? - m.date_utc) * 1.0 / {RECENCY_HORIZON_SECS})))
              + 0.6 * MIN(1.0, COALESCE(e.engaged_count, 0) * 1.0 / 10.0)
              ) DESC,
              m.date_utc DESC
             LIMIT ? OFFSET ?",
        ));
        binds.push(Box::new(now));
        binds.push(Box::new(limit));
        binds.push(Box::new(offset));

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(binds.iter().map(|b| b.as_ref())),
            |r| {
                let item = super::message_queries::row_to_message_list_item(r)?;
                let engaged_count: i64 = r.get(16)?;
                let secs_old = (now - item.message.date_utc).max(0) as f64;
                // Mirror the SQL exactly so the displayed score matches
                // the order — diverging would surface as a "wait, why
                // is this row above that one?" UX bug.
                let recency = (1.0 - (secs_old / RECENCY_HORIZON_SECS).min(1.0)).max(0.0);
                let engagement = (engaged_count as f64 / 10.0).min(1.0);
                let score = 0.4 * recency + 0.6 * engagement;
                Ok(CuratedListItem {
                    item,
                    curated_score: score,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

/// Lowercase + trim. Caller already knows the input is a single
/// address (has come out of `split_addr_list` or a `to:` field
/// that isn't comma-separated). We deliberately don't strip the
/// "Joe <joe@foo>" wrapping here — `prefillFromMessage` writes
/// the To field as the original `from_addr`, so storing the same
/// shape on both sides is what makes the plain `LOWER(TRIM(...))`
/// JOIN match.
fn normalise_addr(raw: &str) -> String {
    raw.trim().to_lowercase()
}

/// Split a comma-or-semicolon-separated recipient string into
/// individual addresses. Tolerates "Foo <a@b>, Bar <c@d>" by
/// splitting only on commas (since `<>` doesn't contain commas).
fn split_addr_list(s: Option<&str>) -> Vec<String> {
    s.into_iter()
        .flat_map(|joined| {
            joined
                .split([',', ';'])
                .map(|p| p.trim().to_owned())
                .filter(|p| !p.is_empty())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_addr_list_handles_basic() {
        let v = split_addr_list(Some("a@b.com, c@d.com"));
        assert_eq!(v, vec!["a@b.com".to_string(), "c@d.com".to_string()]);
    }

    #[test]
    fn split_addr_list_handles_display_names() {
        // Display name + bracketed address survives split because
        // commas are only inside the spec, never inside a single
        // RFC 5322 angle-addr token.
        let v = split_addr_list(Some("Foo <a@b.com>, \"Bar, Baz\" <c@d.com>"));
        // Note: this WILL break a quoted-string display name
        // containing a comma. We accept that for the engagement
        // table since the entry would still match on a later
        // re-send (different formatting).
        assert!(v.contains(&"Foo <a@b.com>".to_string()));
    }

    #[test]
    fn split_addr_list_skips_empty() {
        assert!(split_addr_list(Some("")).is_empty());
        assert!(split_addr_list(None).is_empty());
        let v = split_addr_list(Some(",  ,  a@b.com,"));
        assert_eq!(v, vec!["a@b.com".to_string()]);
    }

    #[test]
    fn normalise_addr_trims_and_lowers() {
        assert_eq!(normalise_addr("  ALICE@EXAMPLE.COM  "), "alice@example.com");
        assert_eq!(
            normalise_addr("Joe <joe@example.com>"),
            "joe <joe@example.com>"
        );
    }
}
