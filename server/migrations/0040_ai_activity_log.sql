-- Per-call AI activity log. Every chat / embed call dispatched
-- through an LlmProvider writes one row. Powers Settings → AI →
-- Activity (request inspector + cost summary).
--
-- Capped at 1,000 rows by the auto-trim trigger below — at ~50
-- embeds/min during initial indexing this is roughly 20 minutes
-- of live history, plus full chat history (which is sparse). Old
-- rows roll off the bottom; we keep this UI-debug-tier, not
-- forensic. The audit_log table remains the long-lived record
-- for security-relevant events (smtp_send, vault_unlock, etc).
--
-- Payload columns hold truncated JSON (first 4 KB each side) so
-- the user can see what was actually sent / received without the
-- table ballooning. Truncation happens in Rust before insert.
CREATE TABLE ai_activity_log (
  id                 INTEGER PRIMARY KEY AUTOINCREMENT,
  ts_utc             INTEGER NOT NULL,
  -- 'chat' | 'chat_stream' | 'embed' | 'health'
  kind               TEXT    NOT NULL,
  -- LlmProvider::id() — 'ollama' | 'openai' | 'anthropic' | 'openai_compat'.
  provider           TEXT    NOT NULL,
  -- Model id as the provider reported back (or the request model
  -- when no response made it).
  model              TEXT    NOT NULL DEFAULT '',
  -- 'ok' | 'error'
  status             TEXT    NOT NULL DEFAULT 'ok',
  -- Wall-clock latency from request build to response (or error).
  elapsed_ms         INTEGER NOT NULL DEFAULT 0,
  -- Token counts when the provider returned them (Ollama on
  -- embed reports 0; that's expected).
  prompt_tokens      INTEGER NOT NULL DEFAULT 0,
  completion_tokens  INTEGER NOT NULL DEFAULT 0,
  -- Approximate byte sizes of the request + response payloads
  -- BEFORE truncation. Lets the user see "you sent 47 KB" even
  -- when the stored sample is the first 4 KB.
  input_bytes        INTEGER NOT NULL DEFAULT 0,
  output_bytes       INTEGER NOT NULL DEFAULT 0,
  -- Truncated JSON payloads. Stored as TEXT so SQLCipher's
  -- whole-DB encryption protects them. NULL when the row was
  -- written without payload sampling (e.g. health probes).
  request_sample     TEXT,
  response_sample    TEXT,
  -- One-line error string when status='error'. Detailed enough
  -- to copy/paste into a bug report; no stack traces.
  error_message      TEXT
);

CREATE INDEX idx_ai_activity_ts ON ai_activity_log(ts_utc DESC);
CREATE INDEX idx_ai_activity_kind_provider ON ai_activity_log(kind, provider);
CREATE INDEX idx_ai_activity_status ON ai_activity_log(status);

-- Auto-trim: after each insert, drop the oldest rows so the table
-- stays at most 1,000 rows. Uses the AUTOINCREMENT id as the
-- monotonic ordering key so ties on ts_utc don't matter.
CREATE TRIGGER ai_activity_log_autotrim
AFTER INSERT ON ai_activity_log
BEGIN
  DELETE FROM ai_activity_log
   WHERE id <= (SELECT id FROM ai_activity_log
                ORDER BY id DESC
                LIMIT 1 OFFSET 1000);
END;
