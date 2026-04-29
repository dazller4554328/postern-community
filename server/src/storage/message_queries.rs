//! Read-side queries over the messages table: list, get, full-text
//! search, and thread aggregation. Pulled out of `storage::messages`
//! so that file can stay focused on the write/upsert side.
//!
//! All four entry points share two helpers — `build_label_clause`
//! and the row mappers — which is why they live here together rather
//! than scattered across the read modules.

use rusqlite::{params, params_from_iter, OptionalExtension};

use super::messages::{Message, MessageDetail, MessageListItem, SearchHit, ThreadSummary};
use super::Db;
use crate::error::Result;

impl Db {
    pub fn list_messages(
        &self,
        account_id: Option<i64>,
        labels: &[String],
        limit: i64,
        offset: i64,
        sort: &str,
    ) -> Result<Vec<MessageListItem>> {
        let conn = self.pool().get()?;

        let mut sql = String::from(
            "SELECT m.id, m.account_id, m.message_id, m.thread_id, m.subject,
                    m.from_addr, m.to_addrs, m.cc_addrs, m.date_utc, m.snippet,
                    m.has_attachments, m.is_read, m.is_starred, m.is_encrypted,
                    m.receipt_to, a.email
             FROM messages m
             JOIN accounts a ON a.id = m.account_id",
        );
        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let label_clause = build_label_clause(labels);
        let mut clauses: Vec<String> = Vec::new();

        if let Some(aid) = account_id {
            clauses.push("m.account_id = ?".to_string());
            binds.push(Box::new(aid));
        } else {
            // Unified-scope listing: respect the per-account
            // include_in_unified toggle. A mailbox with the toggle
            // off still syncs but is hidden from cross-account views.
            clauses.push("a.include_in_unified = 1".to_string());
        }
        if let Some(clause) = label_clause {
            clauses.push(clause);
            for lbl in labels {
                binds.push(Box::new(lbl.clone()));
            }
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        let order = match sort {
            "date_asc" => "m.date_utc ASC",
            "sender_asc" => "LOWER(m.from_addr) ASC, m.date_utc DESC",
            "sender_desc" => "LOWER(m.from_addr) DESC, m.date_utc DESC",
            "subject_asc" => "LOWER(m.subject) ASC, m.date_utc DESC",
            "subject_desc" => "LOWER(m.subject) DESC, m.date_utc DESC",
            _ => "m.date_utc DESC",
        };
        sql.push_str(&format!(" ORDER BY {order} LIMIT ? OFFSET ?"));
        binds.push(Box::new(limit));
        binds.push(Box::new(offset));

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params_from_iter(binds.iter().map(|b| b.as_ref())),
            row_to_message_list_item,
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_message_detail(&self, id: i64) -> Result<MessageDetail> {
        let conn = self.pool().get()?;

        let message = conn
            .query_row(
                "SELECT id, account_id, message_id, thread_id, subject,
                        from_addr, to_addrs, cc_addrs, date_utc, snippet,
                        has_attachments, is_read, is_starred, is_encrypted,
                        receipt_to
                 FROM messages WHERE id = ?1",
                params![id],
                row_to_message,
            )
            .optional()?
            .ok_or(crate::error::Error::NotFound)?;

        let mut stmt = conn.prepare(
            "SELECT l.name FROM labels l
             JOIN message_labels ml ON ml.label_id = l.id
             WHERE ml.message_id = ?1 ORDER BY l.name",
        )?;
        let labels: Vec<String> = stmt
            .query_map(params![id], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(MessageDetail { message, labels })
    }

    pub fn search(
        &self,
        query: &str,
        account_id: Option<i64>,
        limit: i64,
        offset: i64,
        sort: &str,
    ) -> Result<Vec<SearchHit>> {
        let parsed = super::search_query::parse(query, chrono::Utc::now().timestamp());
        self.search_parsed(&parsed, account_id, limit, offset, sort)
    }

    /// Run a pre-parsed query. Split out so callers constructing
    /// filters programmatically (saved searches, preset refinements)
    /// don't have to round-trip through the text parser.
    pub fn search_parsed(
        &self,
        parsed: &super::search_query::ParsedQuery,
        account_id: Option<i64>,
        limit: i64,
        offset: i64,
        sort: &str,
    ) -> Result<Vec<SearchHit>> {
        let use_fts = !parsed.fts_expr.is_empty();
        // If the user typed literally nothing *and* set no structural
        // filters, there's no query to run — return empty rather than
        // the whole inbox.
        if !use_fts && !parsed.has_structural_filters() {
            return Ok(vec![]);
        }

        let conn = self.pool().get()?;
        let mut sql = String::new();
        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // Always project the same 14 columns + the snippet slot so the
        // row-mapper at the bottom doesn't have to branch. When FTS
        // isn't used, the snippet slot gets the stored message.snippet
        // so the UI still renders the preview line.
        if use_fts {
            sql.push_str(
                "SELECT m.id, m.account_id, m.message_id, m.thread_id, m.subject,
                        m.from_addr, m.to_addrs, m.cc_addrs, m.date_utc, m.snippet,
                        m.has_attachments, m.is_read, m.is_starred, m.is_encrypted,
                        m.receipt_to, a.email,
                        snippet(messages_fts, 3, '<mark>', '</mark>', '…', 24) AS match_snippet
                 FROM messages_fts
                 JOIN messages m ON m.id = messages_fts.rowid
                 JOIN accounts a ON a.id = m.account_id
                 WHERE messages_fts MATCH ?",
            );
            binds.push(Box::new(parsed.fts_expr.clone()));
        } else {
            sql.push_str(
                "SELECT m.id, m.account_id, m.message_id, m.thread_id, m.subject,
                        m.from_addr, m.to_addrs, m.cc_addrs, m.date_utc, m.snippet,
                        m.has_attachments, m.is_read, m.is_starred, m.is_encrypted,
                        m.receipt_to, a.email,
                        COALESCE(m.snippet, '') AS match_snippet
                 FROM messages m
                 JOIN accounts a ON a.id = m.account_id
                 WHERE 1 = 1",
            );
        }

        // Account scope. `account:email` in the query beats the URL
        // account_id so power users can jump across accounts from a
        // single search box.
        if let Some(ref email) = parsed.account_email {
            sql.push_str(" AND LOWER(a.email) = ?");
            binds.push(Box::new(email.clone()));
        } else if let Some(aid) = account_id {
            sql.push_str(" AND m.account_id = ?");
            binds.push(Box::new(aid));
        } else {
            sql.push_str(" AND a.include_in_unified = 1");
        }

        // Structural filters.
        if let Some(b) = parsed.has_attachment {
            sql.push_str(if b {
                " AND m.has_attachments = 1"
            } else {
                " AND m.has_attachments = 0"
            });
        }
        if let Some(b) = parsed.is_read {
            sql.push_str(if b {
                " AND m.is_read = 1"
            } else {
                " AND m.is_read = 0"
            });
        }
        if let Some(b) = parsed.is_starred {
            sql.push_str(if b {
                " AND m.is_starred = 1"
            } else {
                " AND m.is_starred = 0"
            });
        }
        if let Some(b) = parsed.is_encrypted {
            sql.push_str(if b {
                " AND m.is_encrypted = 1"
            } else {
                " AND m.is_encrypted = 0"
            });
        }
        if let Some(ts) = parsed.before_utc {
            sql.push_str(" AND m.date_utc < ?");
            binds.push(Box::new(ts));
        }
        if let Some(ts) = parsed.after_utc {
            sql.push_str(" AND m.date_utc >= ?");
            binds.push(Box::new(ts));
        }
        // Labels: AND together — `label:Work label:Important` means
        // "in both". Implemented as an EXISTS subquery per label so
        // we don't need DISTINCT over a row-exploding JOIN.
        for lbl in &parsed.labels {
            sql.push_str(
                " AND EXISTS (
                    SELECT 1 FROM message_labels ml
                    JOIN labels l ON l.id = ml.label_id
                    WHERE ml.message_id = m.id
                      AND l.account_id = m.account_id
                      AND l.name = ?
                )",
            );
            binds.push(Box::new(lbl.clone()));
        }

        // Sort. "relevance" only makes sense when FTS is in play;
        // fall back to date on the non-FTS path so the user doesn't
        // get a stable-but-confusing rowid order.
        let order = match (sort, use_fts) {
            ("relevance", true) => "rank, m.date_utc DESC",
            ("date_asc", _) => "m.date_utc ASC",
            ("sender_asc", _) => "LOWER(m.from_addr) ASC, m.date_utc DESC",
            ("sender_desc", _) => "LOWER(m.from_addr) DESC, m.date_utc DESC",
            ("subject_asc", _) => "LOWER(m.subject) ASC, m.date_utc DESC",
            ("subject_desc", _) => "LOWER(m.subject) DESC, m.date_utc DESC",
            _ => "m.date_utc DESC",
        };
        sql.push_str(&format!(" ORDER BY {order} LIMIT ? OFFSET ?"));
        binds.push(Box::new(limit));
        binds.push(Box::new(offset));

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(binds.iter().map(|b| b.as_ref())), |r| {
            Ok(SearchHit {
                item: row_to_message_list_item(r)?,
                match_snippet: r.get(16)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn list_threads(
        &self,
        account_id: Option<i64>,
        labels: &[String],
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ThreadSummary>> {
        let conn = self.pool().get()?;

        // Two-step: select thread keys + summary aggregates, then decorate
        // with participants + per-thread latest snippet/from. Cheaper than
        // stuffing everything into a single correlated query.
        let mut sql = String::from(
            "SELECT m.thread_id,
                    MAX(m.date_utc) AS latest_date,
                    COUNT(*) AS msg_count,
                    COALESCE(SUM(CASE WHEN m.is_read = 0 THEN 1 ELSE 0 END), 0) AS unread_count,
                    MAX(m.has_attachments) AS has_attach
             FROM messages m
             JOIN accounts a ON a.id = m.account_id",
        );
        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let label_clause = build_label_clause(labels);
        let mut clauses: Vec<String> = Vec::new();
        if let Some(aid) = account_id {
            clauses.push("m.account_id = ?".to_string());
            binds.push(Box::new(aid));
        } else {
            // Unified-scope listing: respect the per-account
            // include_in_unified toggle. A mailbox with the toggle
            // off still syncs but is hidden from cross-account views.
            clauses.push("a.include_in_unified = 1".to_string());
        }
        if let Some(clause) = label_clause {
            clauses.push(clause);
            for lbl in labels {
                binds.push(Box::new(lbl.clone()));
            }
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" GROUP BY m.thread_id ORDER BY latest_date DESC LIMIT ? OFFSET ?");
        binds.push(Box::new(limit));
        binds.push(Box::new(offset));

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(binds.iter().map(|b| b.as_ref())), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)? != 0,
            ))
        })?;
        let thread_meta: Vec<(String, i64, i64, i64, bool)> =
            rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        // Per-thread decoration. For N threads this is 2N queries — fine
        // for typical page sizes (<= 50) and dead simple. We can replace
        // it with a single CTE if profiling shows it mattering.
        let mut out = Vec::with_capacity(thread_meta.len());
        for (thread_id, latest_date, message_count, unread_count, has_attachments) in thread_meta {
            let (subject, latest_snippet, latest_from) = conn.query_row(
                "SELECT subject, snippet, from_addr
                 FROM messages WHERE thread_id = ?1
                 ORDER BY date_utc DESC LIMIT 1",
                params![thread_id],
                |r| {
                    Ok((
                        r.get::<_, Option<String>>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                },
            )?;

            let mut part_stmt = conn.prepare(
                "SELECT DISTINCT from_addr FROM messages
                 WHERE thread_id = ?1 AND from_addr IS NOT NULL
                 ORDER BY date_utc DESC LIMIT 8",
            )?;
            let participants: Vec<String> = part_stmt
                .query_map(params![thread_id], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            let mut acct_stmt = conn.prepare(
                "SELECT DISTINCT a.email FROM messages m
                 JOIN accounts a ON a.id = m.account_id
                 WHERE m.thread_id = ?1",
            )?;
            let account_emails: Vec<String> = acct_stmt
                .query_map(params![thread_id], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            out.push(ThreadSummary {
                thread_id,
                subject,
                participants,
                message_count,
                unread_count,
                has_attachments,
                latest_date_utc: latest_date,
                latest_snippet,
                latest_from,
                account_emails,
            });
        }
        Ok(out)
    }

    pub fn thread_messages(&self, thread_id: &str) -> Result<Vec<MessageListItem>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT m.id, m.account_id, m.message_id, m.thread_id, m.subject,
                    m.from_addr, m.to_addrs, m.cc_addrs, m.date_utc, m.snippet,
                    m.has_attachments, m.is_read, m.is_starred, m.is_encrypted,
                    m.receipt_to, a.email
             FROM messages m
             JOIN accounts a ON a.id = m.account_id
             WHERE m.thread_id = ?1
             ORDER BY m.date_utc ASC",
        )?;
        let rows = stmt.query_map(params![thread_id], row_to_message_list_item)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

/// Produces a `m.id IN (... l.name IN (?, ?, ?))` fragment sized to the
/// number of labels, or `None` when no filter is requested. Returns None
/// for empty slice so the caller simply omits the clause.
fn build_label_clause(labels: &[String]) -> Option<String> {
    if labels.is_empty() {
        return None;
    }
    let placeholders: String = std::iter::repeat("?")
        .take(labels.len())
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "m.id IN (SELECT ml.message_id FROM message_labels ml
                  JOIN labels l ON l.id = ml.label_id
                  WHERE l.name IN ({placeholders}))"
    ))
}

fn row_to_message(r: &rusqlite::Row) -> rusqlite::Result<Message> {
    Ok(Message {
        id: r.get(0)?,
        account_id: r.get(1)?,
        message_id: r.get(2)?,
        thread_id: r.get(3)?,
        subject: r.get(4)?,
        from_addr: r.get(5)?,
        to_addrs: r.get(6)?,
        cc_addrs: r.get(7)?,
        date_utc: r.get(8)?,
        snippet: r.get(9)?,
        has_attachments: r.get::<_, i64>(10)? != 0,
        is_read: r.get::<_, i64>(11)? != 0,
        is_starred: r.get::<_, i64>(12)? != 0,
        is_encrypted: r.get::<_, i64>(13)? != 0,
        receipt_to: r.get(14)?,
    })
}

fn row_to_message_list_item(r: &rusqlite::Row) -> rusqlite::Result<MessageListItem> {
    Ok(MessageListItem {
        message: row_to_message(r)?,
        account_email: r.get(15)?,
    })
}
