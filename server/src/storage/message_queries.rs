//! Read-side queries over the messages table: list, get, full-text
//! search. Pulled out of `storage::messages` so that file can stay
//! focused on the write/upsert side.
//!
//! All three entry points share two helpers — `build_label_clause`
//! and the row mappers — which is why they live here together rather
//! than scattered across the read modules.

use rusqlite::{params, params_from_iter, OptionalExtension};

use super::messages::{Message, MessageDetail, MessageListItem, SearchHit};
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
            "SELECT m.id, m.account_id, m.message_id, m.subject,
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
            params_from_iter(binds.iter().map(std::convert::AsRef::as_ref)),
            row_to_message_list_item,
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_message_detail(&self, id: i64) -> Result<MessageDetail> {
        let conn = self.pool().get()?;

        let message = conn
            .query_row(
                "SELECT id, account_id, message_id, subject,
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

        Ok(MessageDetail {
            message,
            labels,
            in_reply_to: None,
            references: Vec::new(),
        })
    }

    pub fn search(
        &self,
        query: &str,
        account_id: Option<i64>,
        limit: i64,
        offset: i64,
        sort: &str,
        include_trash_spam: bool,
    ) -> Result<Vec<SearchHit>> {
        let parsed = super::search_query::parse(query, chrono::Utc::now().timestamp());
        self.search_parsed(&parsed, account_id, limit, offset, sort, include_trash_spam)
    }

    /// Run a pre-parsed query. Split out so callers constructing
    /// filters programmatically (saved searches, preset refinements)
    /// don't have to round-trip through the text parser.
    ///
    /// When `include_trash_spam` is false (the usual case), messages
    /// labelled with any Trash- or Spam-flavoured folder are excluded
    /// — matching Gmail's default search behaviour. The caller flips
    /// it on when the user is explicitly inside unified Trash / Spam.
    pub fn search_parsed(
        &self,
        parsed: &super::search_query::ParsedQuery,
        account_id: Option<i64>,
        limit: i64,
        offset: i64,
        sort: &str,
        include_trash_spam: bool,
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
                "SELECT m.id, m.account_id, m.message_id, m.subject,
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
                "SELECT m.id, m.account_id, m.message_id, m.subject,
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

        // Default: hide messages that live in Trash or Spam folders so
        // a global search doesn't keep returning rows the user just
        // deleted. Toggled off when the caller is explicitly viewing
        // unified Trash / Spam (the only place those messages should
        // surface in search), or when the parsed query already has an
        // explicit `label:` / `in:` filter — there the user has named
        // a specific bucket and shouldn't be silently filtered.
        // Case-insensitive to match the mixed capitalisation real-world
        // servers expose.
        if !include_trash_spam && parsed.labels.is_empty() {
            sql.push_str(
                " AND NOT EXISTS (
                    SELECT 1 FROM message_labels ml_ts
                    JOIN labels l_ts ON l_ts.id = ml_ts.label_id
                    WHERE ml_ts.message_id = m.id
                      AND l_ts.account_id = m.account_id
                      AND LOWER(l_ts.name) IN (
                        'trash', 'bin', 'deleted', 'deleted items',
                        'deleted messages', '[gmail]/trash',
                        'spam', 'junk', 'junk e-mail', 'junk mail',
                        '[gmail]/spam'
                      )
                )",
            );
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
        let rows = stmt.query_map(
            params_from_iter(binds.iter().map(std::convert::AsRef::as_ref)),
            |r| {
                Ok(SearchHit {
                    item: row_to_message_list_item(r)?,
                    match_snippet: r.get(15)?,
                })
            },
        )?;
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
    let placeholders: String = std::iter::repeat_n("?", labels.len())
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
        subject: r.get(3)?,
        from_addr: r.get(4)?,
        to_addrs: r.get(5)?,
        cc_addrs: r.get(6)?,
        date_utc: r.get(7)?,
        snippet: r.get(8)?,
        has_attachments: r.get::<_, i64>(9)? != 0,
        is_read: r.get::<_, i64>(10)? != 0,
        is_starred: r.get::<_, i64>(11)? != 0,
        is_encrypted: r.get::<_, i64>(12)? != 0,
        receipt_to: r.get(13)?,
    })
}

fn row_to_message_list_item(r: &rusqlite::Row) -> rusqlite::Result<MessageListItem> {
    Ok(MessageListItem {
        message: row_to_message(r)?,
        account_email: r.get(14)?,
    })
}
