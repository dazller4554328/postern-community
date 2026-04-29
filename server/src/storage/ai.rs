//! Storage layer for AI features — embedding upserts + nearest-
//! neighbour retrieval, plus chat-log inserts.
//!
//! Vector encoding: f32 little-endian packed into a BLOB. Decoded on
//! read via a single `bytemuck`-style cast. SQLite has no native
//! vector type and no native dot-product, so cosine similarity runs
//! in Rust over the candidate set. For mailboxes up to ~100k
//! messages on a modern CPU this stays comfortably under 100ms even
//! without an index — the embeddings live in an OS page cache and
//! the inner loop is auto-vectorised by LLVM. If profiling later
//! shows it mattering we can drop in `sqlite-vec` (extension) or
//! HNSW; the `top_k_similar` signature stays the same, only the
//! inner search strategy changes.

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::Db;
use crate::error::{Error, Result};

/// Result of a similarity search. `score` is cosine similarity in
/// [-1.0, 1.0] — higher = more similar. Stable order is descending
/// score, ties broken by descending date_utc so newer mail wins.
#[derive(Debug, Clone, Serialize)]
pub struct SimilarMessage {
    pub message_id: i64,
    pub account_id: i64,
    pub date_utc: i64,
    pub score: f32,
}

/// Single Q&A row for the chat history pane.
#[derive(Debug, Clone, Serialize)]
pub struct ChatLogRow {
    pub id: i64,
    pub created_at: i64,
    pub account_scope: Option<i64>,
    pub question: String,
    pub answer: String,
    pub provider: String,
    pub chat_model: String,
    pub embed_model: String,
    pub privacy_posture: String,
    /// JSON-encoded array of message_ids. Decoded by callers.
    pub cited_message_ids: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub elapsed_ms: i64,
}

/// New chat-log entry. Caller fills it before calling
/// `insert_chat_log`; the storage layer just persists.
pub struct NewChatLog<'a> {
    pub account_scope: Option<i64>,
    pub question: &'a str,
    pub answer: &'a str,
    pub provider: &'a str,
    pub chat_model: &'a str,
    pub embed_model: &'a str,
    pub privacy_posture: &'a str,
    pub cited_message_ids: &'a [i64],
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub elapsed_ms: u64,
}

/// One row from `messages_missing_embedding`. Carries everything
/// the indexer needs to build a semantically-rich embedding input
/// — sender + recipients + subject + body — so retrieval queries
/// like "emails from Joe" or "emails I sent to the support team"
/// can match on header content even when the body doesn't echo
/// the names.
#[derive(Debug, Clone)]
pub struct MissingEmbedRow {
    pub id: i64,
    pub from_addr: Option<String>,
    pub to_addrs: Option<String>,
    pub cc_addrs: Option<String>,
    pub subject: Option<String>,
    pub snippet: Option<String>,
    pub body_text: Option<String>,
}

impl Db {
    /// Insert or replace the embedding for a message. The vector
    /// length must equal `dim` — caller's responsibility to keep
    /// those in sync. Stored verbatim; we do not normalise here so
    /// retrieval can decide on cosine vs dot-product semantics.
    pub fn upsert_embedding(
        &self,
        message_id: i64,
        model: &str,
        vector: &[f32],
    ) -> Result<()> {
        let dim = vector.len();
        if dim == 0 {
            return Err(Error::BadRequest("empty embedding vector".into()));
        }
        let bytes = pack_f32(vector);
        let conn = self.pool().get()?;
        conn.execute(
            "INSERT OR REPLACE INTO ai_embeddings(
                message_id, model, dim, vector, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                message_id,
                model,
                dim as i64,
                bytes,
                chrono::Utc::now().timestamp(),
            ],
        )?;
        Ok(())
    }

    /// Return the count of indexed messages for a model. Drives the
    /// "indexing 12,431/50,000" progress UI.
    pub fn embedding_coverage(&self, model: &str) -> Result<i64> {
        let conn = self.pool().get()?;
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ai_embeddings WHERE model = ?1",
            params![model],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    /// Total message count, ignoring account scope. Combined with
    /// `embedding_coverage` to render the "indexed N/M" progress
    /// in Settings → AI.
    pub fn total_message_count(&self) -> Result<i64> {
        let conn = self.pool().get()?;
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
        Ok(n)
    }

    /// Total chat-log row count for the "stored conversations: N"
    /// stat in the settings panel.
    pub fn chat_log_count(&self) -> Result<i64> {
        let conn = self.pool().get()?;
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM ai_chat_log", [], |r| r.get(0))?;
        Ok(n)
    }

    /// Find the next batch of messages that haven't been embedded
    /// against `model` yet, honouring the user's index-exclusion
    /// rules. The structured tuple lets the indexer prefix the
    /// embed input with `From: …\nTo: …\nCc: …\nSubject: …\n\n`
    /// so retrieval queries like "emails from Joe" actually match
    /// emails Joe sent (otherwise the only signal is whether the
    /// body happens to mention Joe by name, which it usually
    /// doesn't on a transactional email).
    ///
    /// `sender_patterns` are SQL `LIKE` patterns (already
    /// translated from `*` to `%`); `labels` are exact label
    /// names. A message is excluded if its `from_addr` matches
    /// any sender pattern OR if it carries any excluded label.
    pub fn messages_missing_embedding(
        &self,
        model: &str,
        batch: usize,
        sender_patterns: &[String],
        labels: &[String],
    ) -> Result<Vec<MissingEmbedRow>> {
        let conn = self.pool().get()?;
        let (sender_clause, label_clause) =
            build_exclusion_clauses(sender_patterns, labels, "m");
        let sql = format!(
            "SELECT m.id, m.from_addr, m.to_addrs, m.cc_addrs,
                    m.subject, m.snippet, m.body_text
             FROM messages m
             LEFT JOIN ai_embeddings e
               ON e.message_id = m.id AND e.model = ?
             WHERE e.message_id IS NULL
             {sender_clause}
             {label_clause}
             ORDER BY m.date_utc DESC
             LIMIT ?"
        );
        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        binds.push(Box::new(model.to_owned()));
        for p in sender_patterns {
            binds.push(Box::new(p.clone()));
        }
        for l in labels {
            binds.push(Box::new(l.clone()));
        }
        binds.push(Box::new(batch as i64));
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(binds.iter().map(|b| b.as_ref())),
            |r| {
                Ok(MissingEmbedRow {
                    id: r.get::<_, i64>(0)?,
                    from_addr: r.get::<_, Option<String>>(1)?,
                    to_addrs: r.get::<_, Option<String>>(2)?,
                    cc_addrs: r.get::<_, Option<String>>(3)?,
                    subject: r.get::<_, Option<String>>(4)?,
                    snippet: r.get::<_, Option<String>>(5)?,
                    body_text: r.get::<_, Option<String>>(6)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Cosine-similarity nearest-neighbour search. Returns the top
    /// `k` messages most similar to `query`. Optional `account_id`
    /// scopes to one mailbox; otherwise honours the per-account
    /// `include_in_unified` flag like the rest of the unified-view
    /// queries. Optional `before_utc` clips results to messages
    /// older than a cutoff.
    pub fn top_k_similar(
        &self,
        model: &str,
        query: &[f32],
        k: usize,
        account_id: Option<i64>,
        sender_patterns: &[String],
        labels: &[String],
    ) -> Result<Vec<SimilarMessage>> {
        if query.is_empty() {
            return Ok(vec![]);
        }
        let q_norm = norm(query);
        if q_norm == 0.0 {
            return Ok(vec![]);
        }

        let conn = self.pool().get()?;
        // Pull (message_id, account_id, date_utc, vector) for every
        // candidate. Filter the candidate set in SQL — we'd much
        // rather skip a vector decode than do the math and discard.
        let mut sql = String::from(
            "SELECT e.message_id, m.account_id, m.date_utc, e.vector
             FROM ai_embeddings e
             JOIN messages m ON m.id = e.message_id
             JOIN accounts a ON a.id = m.account_id
             WHERE e.model = ?1",
        );
        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(model.to_owned())];
        if let Some(aid) = account_id {
            sql.push_str(" AND m.account_id = ?");
            binds.push(Box::new(aid));
        } else {
            sql.push_str(" AND a.include_in_unified = 1");
        }
        // Apply the same exclusion list the indexer uses, so a
        // newly-added rule pulls "Trash" / "*@cpanel.example.com"
        // out of retrieval immediately, without waiting for the
        // prune pass to finish.
        let (sender_clause, label_clause) =
            build_exclusion_clauses(sender_patterns, labels, "m");
        if !sender_clause.is_empty() {
            sql.push(' ');
            sql.push_str(&sender_clause);
            for p in sender_patterns {
                binds.push(Box::new(p.clone()));
            }
        }
        if !label_clause.is_empty() {
            sql.push(' ');
            sql.push_str(&label_clause);
            for l in labels {
                binds.push(Box::new(l.clone()));
            }
        }

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(binds.iter().map(|b| b.as_ref())),
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Vec<u8>>(3)?,
                ))
            },
        )?;

        // Top-K selection. For typical k (<= 50) on tens of
        // thousands of candidates a binary-heap-of-k beats a full
        // sort, but std::collections::BinaryHeap is max-heap and we
        // want a min-heap-of-k for "keep the K largest". A small
        // hand-rolled approach: collect all (score, id), partial-
        // sort by score descending, take k. Simpler than a heap
        // wrapper, fast enough at the volumes we care about.
        let mut scored: Vec<SimilarMessage> = Vec::new();
        for row in rows {
            let (id, account_id, date_utc, blob) = row?;
            let vec = unpack_f32(&blob)?;
            if vec.len() != query.len() {
                // Mismatched dimensionality — model rotation in
                // progress. Skip gracefully; the next index pass
                // will re-embed.
                continue;
            }
            let v_norm = norm(&vec);
            if v_norm == 0.0 {
                continue;
            }
            let score = dot(query, &vec) / (q_norm * v_norm);
            scored.push(SimilarMessage {
                message_id: id,
                account_id,
                date_utc,
                score,
            });
        }
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.date_utc.cmp(&a.date_utc))
        });
        scored.truncate(k);
        Ok(scored)
    }

    pub fn insert_chat_log(&self, entry: &NewChatLog<'_>) -> Result<i64> {
        let conn = self.pool().get()?;
        let cited = serde_json::to_string(entry.cited_message_ids)
            .map_err(|e| Error::Other(anyhow::anyhow!("encode citations: {e}")))?;
        conn.execute(
            "INSERT INTO ai_chat_log(
                created_at, account_scope, question, answer, provider,
                chat_model, embed_model, privacy_posture,
                cited_message_ids, prompt_tokens, completion_tokens,
                elapsed_ms
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![
                chrono::Utc::now().timestamp(),
                entry.account_scope,
                entry.question,
                entry.answer,
                entry.provider,
                entry.chat_model,
                entry.embed_model,
                entry.privacy_posture,
                cited,
                entry.prompt_tokens as i64,
                entry.completion_tokens as i64,
                entry.elapsed_ms as i64,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list_chat_log(&self, limit: i64, offset: i64) -> Result<Vec<ChatLogRow>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT id, created_at, account_scope, question, answer,
                    provider, chat_model, embed_model, privacy_posture,
                    cited_message_ids, prompt_tokens, completion_tokens,
                    elapsed_ms
             FROM ai_chat_log
             ORDER BY created_at DESC
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], |r| {
            Ok(ChatLogRow {
                id: r.get(0)?,
                created_at: r.get(1)?,
                account_scope: r.get(2)?,
                question: r.get(3)?,
                answer: r.get(4)?,
                provider: r.get(5)?,
                chat_model: r.get(6)?,
                embed_model: r.get(7)?,
                privacy_posture: r.get(8)?,
                cited_message_ids: r.get(9)?,
                prompt_tokens: r.get(10)?,
                completion_tokens: r.get(11)?,
                elapsed_ms: r.get(12)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Dropping a single chat-log entry. Used by a future "forget
    /// this conversation" UI. Returns whether a row was deleted —
    /// false means the id was already gone.
    pub fn delete_chat_log(&self, id: i64) -> Result<bool> {
        let conn = self.pool().get()?;
        let n = conn.execute("DELETE FROM ai_chat_log WHERE id = ?1", params![id])?;
        Ok(n > 0)
    }

    /// "Forget everything". Wipes the whole chat history in one
    /// transaction. Used by Settings → AI → Privacy → Forget all.
    pub fn clear_chat_log(&self) -> Result<usize> {
        let conn = self.pool().get()?;
        Ok(conn.execute("DELETE FROM ai_chat_log", [])?)
    }

    /// Wipe every row from `ai_embeddings`. The indexer naturally
    /// picks up where it left off — its `messages_missing_embedding`
    /// query joins LEFT against this table, so any message that no
    /// longer has a row gets re-embedded on the next tick. Used by
    /// Settings → AI → "Re-index from scratch" when the user wants
    /// uniform vector quality after a format change (e.g. the new
    /// indexer that includes From / To / Cc headers in the input).
    pub fn clear_embeddings(&self) -> Result<usize> {
        let conn = self.pool().get()?;
        Ok(conn.execute("DELETE FROM ai_embeddings", [])?)
    }

    /// Insert a single AI activity row. The auto-trim trigger on
    /// the table keeps the row count bounded; this method just
    /// writes and returns. Errors propagate so the decorator can
    /// at least log them — but failure to write activity must NOT
    /// break the actual chat/embed call, callers should
    /// `let _ =` this.
    pub fn insert_ai_activity(&self, a: &NewAiActivity) -> Result<()> {
        let conn = self.pool().get()?;
        conn.execute(
            "INSERT INTO ai_activity_log
              (ts_utc, kind, provider, model, status, elapsed_ms,
               prompt_tokens, completion_tokens, input_bytes, output_bytes,
               request_sample, response_sample, error_message)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                a.ts_utc,
                a.kind,
                a.provider,
                a.model,
                a.status,
                a.elapsed_ms as i64,
                a.prompt_tokens as i64,
                a.completion_tokens as i64,
                a.input_bytes as i64,
                a.output_bytes as i64,
                a.request_sample,
                a.response_sample,
                a.error_message,
            ],
        )?;
        Ok(())
    }

    /// Filtered list. `kind_filter` and `provider_filter` accept
    /// `None` for "any". `errors_only` keeps only status='error'
    /// rows. Newest-first via the ts_utc index.
    pub fn list_ai_activity(
        &self,
        kind_filter: Option<&str>,
        provider_filter: Option<&str>,
        errors_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AiActivityRow>> {
        let conn = self.pool().get()?;
        let mut sql = String::from(
            "SELECT id, ts_utc, kind, provider, model, status, elapsed_ms,
                    prompt_tokens, completion_tokens, input_bytes, output_bytes,
                    error_message
             FROM ai_activity_log WHERE 1=1",
        );
        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(k) = kind_filter {
            sql.push_str(" AND kind = ?");
            binds.push(Box::new(k.to_owned()));
        }
        if let Some(p) = provider_filter {
            sql.push_str(" AND provider = ?");
            binds.push(Box::new(p.to_owned()));
        }
        if errors_only {
            sql.push_str(" AND status = 'error'");
        }
        sql.push_str(" ORDER BY ts_utc DESC, id DESC LIMIT ? OFFSET ?");
        binds.push(Box::new(limit));
        binds.push(Box::new(offset));
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(binds.iter().map(|b| b.as_ref())),
            |r| {
                Ok(AiActivityRow {
                    id: r.get(0)?,
                    ts_utc: r.get(1)?,
                    kind: r.get(2)?,
                    provider: r.get(3)?,
                    model: r.get(4)?,
                    status: r.get(5)?,
                    elapsed_ms: r.get::<_, i64>(6)? as u64,
                    prompt_tokens: r.get::<_, i64>(7)? as u32,
                    completion_tokens: r.get::<_, i64>(8)? as u32,
                    input_bytes: r.get::<_, i64>(9)? as u64,
                    output_bytes: r.get::<_, i64>(10)? as u64,
                    error_message: r.get(11)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Single-row detail view including the truncated payload
    /// samples. Returns `Ok(None)` when the id has rolled off the
    /// trim window.
    pub fn get_ai_activity(&self, id: i64) -> Result<Option<AiActivityDetail>> {
        let conn = self.pool().get()?;
        Ok(conn
            .query_row(
                "SELECT id, ts_utc, kind, provider, model, status, elapsed_ms,
                        prompt_tokens, completion_tokens, input_bytes, output_bytes,
                        error_message, request_sample, response_sample
                 FROM ai_activity_log WHERE id = ?1",
                params![id],
                |r| {
                    Ok(AiActivityDetail {
                        row: AiActivityRow {
                            id: r.get(0)?,
                            ts_utc: r.get(1)?,
                            kind: r.get(2)?,
                            provider: r.get(3)?,
                            model: r.get(4)?,
                            status: r.get(5)?,
                            elapsed_ms: r.get::<_, i64>(6)? as u64,
                            prompt_tokens: r.get::<_, i64>(7)? as u32,
                            completion_tokens: r.get::<_, i64>(8)? as u32,
                            input_bytes: r.get::<_, i64>(9)? as u64,
                            output_bytes: r.get::<_, i64>(10)? as u64,
                            error_message: r.get(11)?,
                        },
                        request_sample: r.get(12)?,
                        response_sample: r.get(13)?,
                    })
                },
            )
            .optional()?)
    }

    /// Aggregated counts + estimated USD cost over a time window.
    /// `since_ts_utc` is the lower bound; we use a window rather
    /// than a duration so the caller decides "today", "last hour",
    /// "this month" without us needing a clock.
    pub fn ai_activity_summary(&self, since_ts_utc: i64) -> Result<Vec<AiActivityBucket>> {
        let conn = self.pool().get()?;
        let mut stmt = conn.prepare(
            "SELECT provider, kind, model,
                    COUNT(*)                    AS calls,
                    SUM(elapsed_ms)             AS sum_elapsed_ms,
                    SUM(prompt_tokens)          AS sum_prompt,
                    SUM(completion_tokens)      AS sum_completion,
                    SUM(CASE WHEN status='error' THEN 1 ELSE 0 END) AS errors
             FROM ai_activity_log
             WHERE ts_utc >= ?1
             GROUP BY provider, kind, model
             ORDER BY calls DESC",
        )?;
        let rows = stmt.query_map(params![since_ts_utc], |r| {
            Ok(AiActivityBucket {
                provider: r.get(0)?,
                kind: r.get(1)?,
                model: r.get(2)?,
                calls: r.get::<_, i64>(3)? as u64,
                sum_elapsed_ms: r.get::<_, i64>(4)? as u64,
                sum_prompt_tokens: r.get::<_, i64>(5)? as u64,
                sum_completion_tokens: r.get::<_, i64>(6)? as u64,
                errors: r.get::<_, i64>(7)? as u64,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Wipe the activity log entirely. Settings → AI → Activity
    /// → Clear button.
    pub fn clear_ai_activity(&self) -> Result<usize> {
        let conn = self.pool().get()?;
        Ok(conn.execute("DELETE FROM ai_activity_log", [])?)
    }

    /// Read the singleton AI settings row (id = 1). Always returns
    /// `Ok` once migrations have run — the migration seeds the row.
    pub fn get_ai_settings(&self) -> Result<AiSettings> {
        let conn = self.pool().get()?;
        let row = conn.query_row(
            "SELECT enabled, provider_kind, chat_model, embed_model,
                    base_url, api_key_ref, cloud_consent, updated_at,
                    embed_provider_kind, embed_base_url, embed_api_key_ref,
                    auto_start, user_rules,
                    excluded_senders, excluded_labels,
                    polish_chat_model, freedom_mode, chat_max_tokens
             FROM ai_settings WHERE id = 1",
            [],
            |r| {
                Ok(AiSettings {
                    enabled: r.get::<_, i64>(0)? != 0,
                    provider_kind: r.get(1)?,
                    chat_model: r.get(2)?,
                    embed_model: r.get(3)?,
                    base_url: r.get(4)?,
                    api_key_ref: r.get(5)?,
                    cloud_consent: r.get::<_, i64>(6)? != 0,
                    updated_at: r.get(7)?,
                    embed_provider_kind: r.get(8)?,
                    embed_base_url: r.get(9)?,
                    embed_api_key_ref: r.get(10)?,
                    auto_start: r.get::<_, i64>(11)? != 0,
                    user_rules: r.get(12)?,
                    excluded_senders: r.get(13)?,
                    excluded_labels: r.get(14)?,
                    polish_chat_model: r.get(15)?,
                    freedom_mode: r.get(16)?,
                    chat_max_tokens: r.get(17)?,
                })
            },
        )?;
        Ok(row)
    }

    /// Save the singleton AI settings row + (optionally) rotate the
    /// API keys in the secrets table.
    ///
    /// `api_key` (chat) and `embed_api_key` semantics each:
    ///   * `None` — leave the existing key alone (UI sends None when
    ///     the user hasn't touched the field).
    ///   * `Some("")` — clear the key.
    ///   * `Some(value)` — encrypt with the vault and upsert.
    ///
    /// Returns the new settings row.
    pub fn set_ai_settings(
        &self,
        update: &UpdateAiSettings,
        api_key: Option<&str>,
        embed_api_key: Option<&str>,
        vault: &crate::vault::Vault,
    ) -> Result<AiSettings> {
        let mut conn = self.pool().get()?;
        let tx = conn.transaction()?;

        // Resolve api_key_ref (chat key) + secret-table state.
        let key_ref = upsert_ai_key(
            &tx,
            api_key,
            "ai:api_key",
            "api_key_ref",
            vault,
        )?;

        // Same dance for the optional embed-side key. Stored under a
        // distinct secrets ref so a chat-key clear doesn't take the
        // embed key with it.
        let embed_key_ref = upsert_ai_key(
            &tx,
            embed_api_key,
            "ai:embed_api_key",
            "embed_api_key_ref",
            vault,
        )?;

        let now = chrono::Utc::now().timestamp();
        tx.execute(
            "UPDATE ai_settings
                SET enabled = ?1,
                    provider_kind = ?2,
                    chat_model = ?3,
                    embed_model = ?4,
                    base_url = ?5,
                    api_key_ref = ?6,
                    cloud_consent = ?7,
                    updated_at = ?8,
                    embed_provider_kind = ?9,
                    embed_base_url = ?10,
                    embed_api_key_ref = ?11,
                    auto_start = ?12,
                    user_rules = ?13,
                    excluded_senders = ?14,
                    excluded_labels = ?15,
                    polish_chat_model = ?16,
                    freedom_mode = ?17,
                    chat_max_tokens = ?18
              WHERE id = 1",
            params![
                i32::from(update.enabled),
                update.provider_kind,
                update.chat_model,
                update.embed_model,
                update.base_url,
                key_ref,
                i32::from(update.cloud_consent),
                now,
                update.embed_provider_kind,
                update.embed_base_url,
                embed_key_ref,
                i32::from(update.auto_start),
                update.user_rules,
                update.excluded_senders,
                update.excluded_labels,
                update.polish_chat_model,
                update.freedom_mode,
                update.chat_max_tokens,
            ],
        )?;
        tx.commit()?;
        self.get_ai_settings()
    }

    /// Decrypt the chat-side API key. None when no key has been
    /// configured yet (e.g. chat=Ollama).
    pub fn ai_api_key(&self, vault: &crate::vault::Vault) -> Result<Option<String>> {
        decrypt_ai_secret(self, vault, |s| s.api_key_ref)
    }

    /// Decrypt the embed-side API key. Falls back to the chat key
    /// when:
    ///   * embed_provider_kind == provider_kind (same vendor → same
    ///     key works), AND
    ///   * embed_api_key_ref is None (user didn't override).
    /// Otherwise returns the embed key (or None if neither is set).
    pub fn ai_embed_api_key(&self, vault: &crate::vault::Vault) -> Result<Option<String>> {
        let settings = self.get_ai_settings()?;
        if settings.embed_api_key_ref.is_none()
            && settings.embed_provider_kind == settings.provider_kind
        {
            return self.ai_api_key(vault);
        }
        decrypt_ai_secret(self, vault, |s| s.embed_api_key_ref)
    }
}

/// Internal: rotate / clear / keep an AI-side key. Returns the new
/// `*_key_ref` value to write into `ai_settings`. Reads the existing
/// ref via the supplied column name when `api_key` is None so the
/// caller doesn't have to thread that through.
fn upsert_ai_key(
    tx: &rusqlite::Transaction<'_>,
    api_key: Option<&str>,
    secrets_ref: &str,
    column: &str,
    vault: &crate::vault::Vault,
) -> Result<Option<String>> {
    match api_key {
        None => {
            let sql = format!("SELECT {column} FROM ai_settings WHERE id = 1");
            let existing: Option<String> = tx
                .query_row(&sql, [], |r| r.get(0))
                .optional()?
                .flatten();
            Ok(existing)
        }
        Some("") => {
            let sql = format!("SELECT {column} FROM ai_settings WHERE id = 1");
            let prior: Option<String> = tx
                .query_row(&sql, [], |r| r.get(0))
                .optional()?
                .flatten();
            if let Some(r) = prior {
                tx.execute("DELETE FROM secrets WHERE ref = ?1", params![r])?;
            }
            Ok(None)
        }
        Some(plaintext) => {
            let r = secrets_ref.to_owned();
            let wrapped = vault.encrypt(plaintext.as_bytes())?;
            tx.execute(
                "INSERT INTO secrets(ref, ciphertext) VALUES (?1, ?2)
                 ON CONFLICT(ref) DO UPDATE SET ciphertext = excluded.ciphertext",
                params![r, wrapped],
            )?;
            Ok(Some(r))
        }
    }
}

/// Internal: decrypt a secret pointed at by either api_key_ref or
/// embed_api_key_ref. Returns None when the column is null OR when
/// the secrets row is missing.
fn decrypt_ai_secret(
    db: &Db,
    vault: &crate::vault::Vault,
    pick_ref: impl Fn(AiSettings) -> Option<String>,
) -> Result<Option<String>> {
    let settings = db.get_ai_settings()?;
    let Some(key_ref) = pick_ref(settings) else {
        return Ok(None);
    };
    let conn = db.pool().get()?;
    let cipher: Option<Vec<u8>> = conn
        .query_row(
            "SELECT ciphertext FROM secrets WHERE ref = ?1",
            params![key_ref],
            |r| r.get(0),
        )
        .optional()?;
    let Some(c) = cipher else {
        return Ok(None);
    };
    let plain = vault.decrypt(&c)?;
    Ok(Some(
        String::from_utf8(plain)
            .map_err(|e| Error::Other(anyhow::anyhow!("ai key not utf-8: {e}")))?,
    ))
}

/// Persisted form of Settings → AI. Returned from `get_ai_settings`
/// and surfaced verbatim to the UI; API keys never travel in this
/// struct (only their presence does, via `*_set` flags in the HTTP DTO).
///
/// Chat and embed providers are decoupled — a common setup is
/// chat=openai (best-quality answers) + embed=ollama (local, free,
/// keeps every email body off the cloud during indexing).
#[derive(Debug, Clone, Serialize)]
pub struct AiSettings {
    pub enabled: bool,
    pub provider_kind: String,
    pub chat_model: String,
    pub embed_model: String,
    pub base_url: Option<String>,
    pub api_key_ref: Option<String>,
    /// Embedding provider — independent of chat. Defaults to
    /// 'ollama' so the bulk-cost / bulk-content item stays local.
    pub embed_provider_kind: String,
    pub embed_base_url: Option<String>,
    /// Separate API-key ref for the embed provider. Only used when
    /// embed is a cloud vendor AND distinct from chat — otherwise
    /// the chat key is reused.
    pub embed_api_key_ref: Option<String>,
    pub cloud_consent: bool,
    /// "Always on" — when true, the AI provider holder is rebuilt
    /// automatically the first time the vault unlocks after a
    /// restart. Defaults to true so post-update experience is
    /// continuous. False means the user has to manually flip the
    /// toolbar toggle (or re-save Settings) to bring AI back up.
    pub auto_start: bool,
    /// User-defined additional rules appended to the system prompt
    /// after the seven Commandments. Extends behaviour, can't
    /// override the security floor. NULL or empty = none.
    pub user_rules: Option<String>,
    /// Newline-delimited sender exclusion list. Each line is a
    /// pattern: literal text matches a substring, `*` becomes a
    /// SQL `%` wildcard so e.g. `*@cpanel.example.com` excludes
    /// every cPanel-server email. NULL = no exclusions.
    pub excluded_senders: Option<String>,
    /// Newline-delimited label exclusion list. Each line is an
    /// exact label name (e.g. `Trash`, `[Gmail]/Promotions`).
    pub excluded_labels: Option<String>,
    /// Optional chat-model override used by the compose-pane Polish
    /// rewrite. NULL or empty = inherit `chat_model`. Provider stays
    /// the configured chat provider — same API key, just a different
    /// model id (e.g. gpt-4o for Ask, gpt-4o-mini for Polish).
    pub polish_chat_model: Option<String>,
    /// "strict" | "balanced" | "open" — drives how restrictive
    /// Datas's prompt is. NULL is treated as "balanced" by callers.
    /// Action floor (Commandments) is unchanged across modes.
    pub freedom_mode: Option<String>,
    /// Per-request output-token cap for Ask Datas. NULL = in-code
    /// default. Caller clamps to a sane range.
    pub chat_max_tokens: Option<i64>,
    pub updated_at: i64,
}

/// Mutable shape sent to `set_ai_settings`. Mirrors `AiSettings` minus
/// the immutable / derived fields.
#[derive(Debug, Clone)]
pub struct UpdateAiSettings<'a> {
    pub enabled: bool,
    pub provider_kind: &'a str,
    pub chat_model: &'a str,
    pub embed_model: &'a str,
    pub base_url: Option<&'a str>,
    pub embed_provider_kind: &'a str,
    pub embed_base_url: Option<&'a str>,
    pub cloud_consent: bool,
    pub auto_start: bool,
    pub user_rules: Option<&'a str>,
    pub excluded_senders: Option<&'a str>,
    pub excluded_labels: Option<&'a str>,
    pub polish_chat_model: Option<&'a str>,
    pub freedom_mode: Option<&'a str>,
    pub chat_max_tokens: Option<i64>,
}

/// Insert payload for `insert_ai_activity`. The decorator builds
/// one of these per chat / embed call.
#[derive(Debug, Clone)]
pub struct NewAiActivity<'a> {
    pub ts_utc: i64,
    pub kind: &'a str,
    pub provider: &'a str,
    pub model: &'a str,
    pub status: &'a str,
    pub elapsed_ms: u64,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub request_sample: Option<&'a str>,
    pub response_sample: Option<&'a str>,
    pub error_message: Option<&'a str>,
}

/// Listing row — does not include the (potentially-large) payload
/// samples. The detail endpoint returns those separately.
#[derive(Debug, Clone, Serialize)]
pub struct AiActivityRow {
    pub id: i64,
    pub ts_utc: i64,
    pub kind: String,
    pub provider: String,
    pub model: String,
    pub status: String,
    pub elapsed_ms: u64,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub error_message: Option<String>,
}

/// Detail view — adds the truncated payload samples. Used by the
/// click-through inspector.
#[derive(Debug, Clone, Serialize)]
pub struct AiActivityDetail {
    #[serde(flatten)]
    pub row: AiActivityRow,
    pub request_sample: Option<String>,
    pub response_sample: Option<String>,
}

/// One bucket of `ai_activity_summary`. Drives the cost / count
/// strip at the top of the Activity tab. The frontend multiplies
/// token counts by the cost-per-1M-token rate (returned via
/// `/api/ai/activity/summary`) so the table here doesn't need
/// to know prices.
#[derive(Debug, Clone, Serialize)]
pub struct AiActivityBucket {
    pub provider: String,
    pub kind: String,
    pub model: String,
    pub calls: u64,
    pub sum_elapsed_ms: u64,
    pub sum_prompt_tokens: u64,
    pub sum_completion_tokens: u64,
    pub errors: u64,
}

// ---------- exclusion helpers ---------------------------------------

/// Parse a newline-delimited config string into a clean list of
/// non-empty trimmed entries. Used for both senders and labels.
/// Lines starting with `#` are treated as comments.
pub fn parse_exclusion_list(raw: Option<&str>) -> Vec<String> {
    let Some(text) = raw else { return Vec::new() };
    text.lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// Translate user-typed sender patterns (`*@cpanel.example.com`,
/// literal addresses, substrings) into SQL `LIKE` patterns. `*`
/// becomes `%`. A pattern with no `*` gets `%` wrapped on both
/// sides so it matches as a substring (because users often type
/// just `cpanel` and expect that to catch everything containing
/// it). Literal `%` and `_` are escaped via the `ESCAPE '\\'`
/// clause that the WHERE-builder uses.
pub fn sender_patterns_to_like(patterns: &[String]) -> Vec<String> {
    patterns
        .iter()
        .map(|p| {
            // Escape SQL LIKE metacharacters that the user did NOT
            // type as wildcards.
            let mut s = String::with_capacity(p.len());
            for ch in p.chars() {
                match ch {
                    '*' => s.push('%'),
                    '%' | '_' | '\\' => {
                        s.push('\\');
                        s.push(ch);
                    }
                    c => s.push(c),
                }
            }
            // No-wildcard pattern → substring match by default.
            if !s.contains('%') {
                s = format!("%{s}%");
            }
            s
        })
        .collect()
}

/// Build (sender_clause, label_clause) WHERE fragments suitable
/// for inserting into a SELECT against `messages`. The returned
/// strings are EITHER empty (nothing to filter) OR begin with
/// ` AND ...`. Bind parameters for senders come first, then for
/// labels — caller binds in that order.
fn build_exclusion_clauses(
    sender_patterns: &[String],
    labels: &[String],
    table_alias: &str,
) -> (String, String) {
    let sender_clause = if sender_patterns.is_empty() {
        String::new()
    } else {
        let placeholders: Vec<String> = (0..sender_patterns.len())
            .map(|_| format!("{table_alias}.from_addr LIKE ? ESCAPE '\\'"))
            .collect();
        format!(
            "AND ({alias}.from_addr IS NULL OR NOT ({preds}))",
            alias = table_alias,
            preds = placeholders.join(" OR ")
        )
    };
    let label_clause = if labels.is_empty() {
        String::new()
    } else {
        let placeholders: Vec<String> = (0..labels.len()).map(|_| "?".to_owned()).collect();
        format!(
            "AND {alias}.id NOT IN (
                SELECT ml.message_id FROM message_labels ml
                JOIN labels l ON l.id = ml.label_id
                WHERE l.name IN ({names})
            )",
            alias = table_alias,
            names = placeholders.join(", ")
        )
    };
    (sender_clause, label_clause)
}

impl Db {
    /// Delete every `ai_embeddings` row whose message currently
    /// matches one of the exclusion patterns. Called when the
    /// user saves a new exclusion config so the noise leaves
    /// retrieval immediately, not just for future indexing.
    /// Returns the number of rows deleted.
    pub fn prune_excluded_embeddings(
        &self,
        sender_patterns: &[String],
        labels: &[String],
    ) -> Result<usize> {
        if sender_patterns.is_empty() && labels.is_empty() {
            return Ok(0);
        }
        let conn = self.pool().get()?;
        // Two passes — one keyed on senders, one keyed on labels.
        // Sum rows. Avoids juggling AND/OR around variable-
        // cardinality clause sets, and the duplicate-id case is
        // a no-op on the second pass (the row is gone).
        let mut total = 0usize;
        if !sender_patterns.is_empty() {
            let preds: Vec<String> = (0..sender_patterns.len())
                .map(|_| "m.from_addr LIKE ? ESCAPE '\\'".to_owned())
                .collect();
            let q = format!(
                "DELETE FROM ai_embeddings
                 WHERE message_id IN (
                     SELECT m.id FROM messages m
                     WHERE m.from_addr IS NOT NULL AND ({})
                 )",
                preds.join(" OR ")
            );
            let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
            for p in sender_patterns {
                binds.push(Box::new(p.clone()));
            }
            total += conn.execute(
                &q,
                rusqlite::params_from_iter(binds.iter().map(|b| b.as_ref())),
            )?;
        }
        if !labels.is_empty() {
            let placeholders: Vec<String> = (0..labels.len()).map(|_| "?".to_owned()).collect();
            let q = format!(
                "DELETE FROM ai_embeddings
                 WHERE message_id IN (
                     SELECT ml.message_id FROM message_labels ml
                     JOIN labels l ON l.id = ml.label_id
                     WHERE l.name IN ({})
                 )",
                placeholders.join(", ")
            );
            let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
            for l in labels {
                binds.push(Box::new(l.clone()));
            }
            total += conn.execute(
                &q,
                rusqlite::params_from_iter(binds.iter().map(|b| b.as_ref())),
            )?;
        }
        Ok(total)
    }
}

// ---------- vector packing helpers ----------------------------------

fn pack_f32(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

fn unpack_f32(bytes: &[u8]) -> Result<Vec<f32>> {
    if bytes.len() % 4 != 0 {
        return Err(Error::Other(anyhow::anyhow!(
            "embedding blob length {} not a multiple of 4",
            bytes.len()
        )));
    }
    let mut out = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        let arr: [u8; 4] = chunk.try_into().expect("chunks_exact(4) yields 4 bytes");
        out.push(f32::from_le_bytes(arr));
    }
    Ok(out)
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    let mut s = 0.0f32;
    for i in 0..a.len() {
        s += a[i] * b[i];
    }
    s
}

fn norm(v: &[f32]) -> f32 {
    let mut s = 0.0f32;
    for f in v {
        s += f * f;
    }
    s.sqrt()
}

// Suppress "this row also has a hit" lint that the unused
// SimilarMessage warns about until a caller reads `score`.
#[cfg(test)]
fn _ensure_similar_message_used(s: SimilarMessage) -> f32 {
    s.score
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db_with_message() -> (tempfile::TempDir, Db, i64) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("t.db")).unwrap();
        db.migrate().unwrap();
        let conn = db.pool().get().unwrap();
        conn.execute(
            "INSERT INTO accounts(kind, email, imap_host, imap_port, credential_ref, created_at)
             VALUES ('imap', 'a@b.test', 'imap.example.com', 993, 'acct:a@b.test', 0)",
            [],
        )
        .unwrap();
        let acct_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO messages(account_id, message_id, thread_id, date_utc,
                                  blob_sha256, size_bytes, body_text, has_attachments,
                                  is_read, is_encrypted)
             VALUES (?1, '<a@local>', NULL, 0, '', 0, NULL, 0, 0, 0)",
            params![acct_id],
        )
        .unwrap();
        let msg_id = conn.last_insert_rowid();
        (dir, db, msg_id)
    }

    #[test]
    fn pack_unpack_roundtrip() {
        let v = vec![1.0_f32, -2.5, 0.0, 3.14, f32::NEG_INFINITY];
        let bytes = pack_f32(&v);
        assert_eq!(bytes.len(), v.len() * 4);
        let back = unpack_f32(&bytes).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn unpack_rejects_non_multiple_of_four() {
        let bytes = vec![0u8, 1, 2];
        assert!(unpack_f32(&bytes).is_err());
    }

    #[test]
    fn upsert_then_recall() {
        let (_t, db, msg_id) = db_with_message();
        let v = vec![1.0_f32, 0.0, 0.0];
        db.upsert_embedding(msg_id, "test-model", &v).unwrap();
        assert_eq!(db.embedding_coverage("test-model").unwrap(), 1);
        assert_eq!(db.embedding_coverage("other-model").unwrap(), 0);
    }

    #[test]
    fn top_k_returns_most_similar_first() {
        let (_t, db, msg_a) = db_with_message();
        // Add a second message on the same account so we have two
        // candidates to rank.
        let conn = db.pool().get().unwrap();
        conn.execute(
            "INSERT INTO messages(account_id, message_id, thread_id, date_utc,
                                  blob_sha256, size_bytes, body_text, has_attachments,
                                  is_read, is_encrypted)
             VALUES (1, '<b@local>', NULL, 100, '', 0, NULL, 0, 0, 0)",
            [],
        )
        .unwrap();
        let msg_b = conn.last_insert_rowid();
        drop(conn);

        // a aligned with the query, b orthogonal
        db.upsert_embedding(msg_a, "m", &[1.0, 0.0, 0.0]).unwrap();
        db.upsert_embedding(msg_b, "m", &[0.0, 1.0, 0.0]).unwrap();

        let hits = db.top_k_similar("m", &[1.0, 0.0, 0.0], 2, None, &[], &[]).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].message_id, msg_a);
        assert!((hits[0].score - 1.0).abs() < 1e-6);
        assert_eq!(hits[1].message_id, msg_b);
        assert!(hits[1].score.abs() < 1e-6);
    }

    #[test]
    fn top_k_skips_dim_mismatched_rows() {
        // Simulates a model rotation in progress: existing rows are
        // 3-dim, the new query is 4-dim. Stale rows must be skipped
        // gracefully, not crash the query.
        let (_t, db, msg_id) = db_with_message();
        db.upsert_embedding(msg_id, "m", &[1.0, 0.0, 0.0]).unwrap();
        let hits = db
            .top_k_similar("m", &[1.0, 0.0, 0.0, 0.0], 5, None, &[], &[])
            .unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn missing_embedding_lists_unindexed_only() {
        let (_t, db, msg_id) = db_with_message();
        let pending = db.messages_missing_embedding("m", 10, &[], &[]).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, msg_id);

        db.upsert_embedding(msg_id, "m", &[1.0, 0.0]).unwrap();
        let pending = db.messages_missing_embedding("m", 10, &[], &[]).unwrap();
        assert!(pending.is_empty());

        // A different model is still pending — supports the
        // re-embed-on-upgrade flow.
        let pending = db.messages_missing_embedding("other-model", 10, &[], &[]).unwrap();
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn chat_log_insert_then_list() {
        let (_t, db, msg_id) = db_with_message();
        let entry = NewChatLog {
            account_scope: None,
            question: "when did I get the receipt?",
            answer: "Yesterday at 14:00.",
            provider: "ollama",
            chat_model: "llama3.1:8b",
            embed_model: "nomic-embed-text",
            privacy_posture: "local_only",
            cited_message_ids: &[msg_id],
            prompt_tokens: 100,
            completion_tokens: 20,
            elapsed_ms: 8700,
        };
        let id = db.insert_chat_log(&entry).unwrap();
        assert!(id > 0);

        let rows = db.list_chat_log(10, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].question, "when did I get the receipt?");
        // Citations stored as JSON array of i64s.
        let cited: Vec<i64> = serde_json::from_str(&rows[0].cited_message_ids).unwrap();
        assert_eq!(cited, vec![msg_id]);
    }

    #[test]
    fn clear_chat_log_wipes_all_rows() {
        let (_t, db, msg_id) = db_with_message();
        for q in ["a", "b", "c"] {
            db.insert_chat_log(&NewChatLog {
                account_scope: None,
                question: q,
                answer: "ans",
                provider: "ollama",
                chat_model: "x",
                embed_model: "y",
                privacy_posture: "local_only",
                cited_message_ids: &[msg_id],
                prompt_tokens: 0,
                completion_tokens: 0,
                elapsed_ms: 0,
            })
            .unwrap();
        }
        assert_eq!(db.list_chat_log(10, 0).unwrap().len(), 3);
        let cleared = db.clear_chat_log().unwrap();
        assert_eq!(cleared, 3);
        assert!(db.list_chat_log(10, 0).unwrap().is_empty());
    }
}
