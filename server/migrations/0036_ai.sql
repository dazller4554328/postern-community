-- AI integration tables. Both live entirely inside SQLCipher (no
-- separate blob store) — embeddings are small (~3 KB per message at
-- 768 dims × f32) and chat logs are pure text. SQLCipher's
-- whole-database AES-CBC + HMAC handles the at-rest encryption.

-- ─────────────────────────────────────────────────────────────────
-- One embedding per message. Vectors stored as packed little-endian
-- f32 blobs — 4 bytes × `dim` floats. Postern decodes via
-- bytemuck-style cast in storage::ai.
--
-- Why per-message rather than per-thread or per-blob:
--   • Search granularity: "find Joe's PayPal receipt" should hit
--     the specific email, not the whole thread.
--   • Re-embedding is cheap when a model upgrade lands — drop rows
--     where model differs from the configured model and re-walk.
--   • CASCADE-on-message-delete keeps the index honest without a
--     sweeper task.
--
-- The `model` column captures which embedding model produced this
-- vector. If the operator upgrades from nomic-embed-text to bge-m3,
-- the dimensions change (768 → 1024) and old rows become useless;
-- having the model name lets the retrieval layer ignore stale rows
-- and the indexer re-embed lazily as messages are queried.
-- ─────────────────────────────────────────────────────────────────
CREATE TABLE ai_embeddings (
    message_id  INTEGER PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    -- Embedding model identifier (e.g. 'nomic-embed-text:latest').
    model       TEXT    NOT NULL,
    -- Vector dimensionality. Stored explicitly so the decoder can
    -- validate blob length without trusting the model name lookup.
    dim         INTEGER NOT NULL,
    -- Packed f32 little-endian. byte_length(vector) MUST equal dim*4.
    vector      BLOB    NOT NULL,
    created_at  INTEGER NOT NULL
);

-- Index on `model` so the "re-embed everything that's not on the
-- current model" query is fast even on large mailboxes. The PK
-- already covers per-message lookups.
CREATE INDEX idx_ai_embeddings_model ON ai_embeddings(model);

-- ─────────────────────────────────────────────────────────────────
-- Chat history. Every Q&A round trip is logged here — drives the
-- audit story ("what did the AI see and say?") and a future
-- conversation-history pane in the UI.
--
-- account_scope mirrors the inbox's notion of scope at query time:
--   • specific account_id → query was filtered to that mailbox
--   • NULL → unified-inbox query, all accounts in scope
--
-- Tokens + elapsed_ms support a per-feature usage view ("AI used 4M
-- tokens this month, of which 80% were summaries") without needing
-- to plumb in a separate metrics store.
-- ─────────────────────────────────────────────────────────────────
CREATE TABLE ai_chat_log (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at          INTEGER NOT NULL,
    -- NULL = unified scope; otherwise FK to accounts.id with
    -- ON DELETE SET NULL so deleting a mailbox doesn't orphan
    -- the historical conversations referencing it.
    account_scope       INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    question            TEXT    NOT NULL,
    answer              TEXT    NOT NULL,
    -- Stable provider id (the LlmProvider::id() string), e.g.
    -- 'ollama' or 'anthropic'. Used by the audit UI to render
    -- "this answer came from <provider>".
    provider            TEXT    NOT NULL,
    -- Exact model identifiers used at chat-time and retrieval-time.
    -- Kept so a future "replay this question" feature can pin them.
    chat_model          TEXT    NOT NULL,
    embed_model         TEXT    NOT NULL,
    -- 'local_only' | 'user_controlled_remote' | 'third_party_cloud'
    -- — captured at the moment of the call so a posture change
    -- after the fact doesn't rewrite history.
    privacy_posture     TEXT    NOT NULL,
    -- JSON array of message_ids the model was given as context.
    -- Powers the clickable "sources" footer under each answer.
    cited_message_ids   TEXT    NOT NULL,
    prompt_tokens       INTEGER NOT NULL DEFAULT 0,
    completion_tokens   INTEGER NOT NULL DEFAULT 0,
    elapsed_ms          INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_ai_chat_log_created_at ON ai_chat_log(created_at DESC);
