-- AI Datas / chat-history / activity-log feature removed
-- 2026-05-08. AI is now used exclusively by the compose-pane Polish
-- and Dictate features. The chat-history and per-call activity-log
-- tables are dropped here, alongside the embed/user_rules/exclusions
-- /freedom_mode/chat_max_tokens columns on ai_settings that were only
-- meaningful for Datas.
--
-- The lockdown_enabled column on app_meta is retained as a no-op
-- (dropping a column on SQLite requires a table rebuild and the
-- column is harmless when unused).

DROP TRIGGER IF EXISTS ai_activity_log_trim;
DROP INDEX IF EXISTS idx_ai_activity_log_ts;
DROP INDEX IF EXISTS idx_ai_activity_log_kind;
DROP INDEX IF EXISTS idx_ai_activity_log_provider;
DROP TABLE IF EXISTS ai_activity_log;

DROP INDEX IF EXISTS idx_ai_chat_log_created_at;
DROP TABLE IF EXISTS ai_chat_log;

-- Rebuild ai_settings without the Datas-only columns. SQLite has
-- no DROP COLUMN support before 3.35; we recreate the table to stay
-- compatible with older builds and to verify the schema explicitly.
CREATE TABLE ai_settings_new (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enabled INTEGER NOT NULL DEFAULT 1,
    provider_kind TEXT NOT NULL DEFAULT 'ollama',
    chat_model TEXT NOT NULL DEFAULT '',
    base_url TEXT,
    api_key_ref TEXT,
    cloud_consent INTEGER NOT NULL DEFAULT 0,
    auto_start INTEGER NOT NULL DEFAULT 1,
    polish_chat_model TEXT,
    updated_at INTEGER NOT NULL DEFAULT 0
);

INSERT INTO ai_settings_new (
    id, enabled, provider_kind, chat_model,
    base_url, api_key_ref, cloud_consent, auto_start,
    polish_chat_model, updated_at
)
SELECT
    id, enabled, provider_kind, chat_model,
    base_url, api_key_ref, cloud_consent, auto_start,
    polish_chat_model, updated_at
FROM ai_settings;

DROP TABLE ai_settings;
ALTER TABLE ai_settings_new RENAME TO ai_settings;

-- Drop any orphaned embed-side secret rows. Chat-side keys
-- remain in place — those still drive Polish + Dictate.
DELETE FROM secrets WHERE ref = 'ai:embed_api_key';
