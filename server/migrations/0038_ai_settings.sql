-- Persistent AI configuration. Single-row table (id always = 1) so a
-- naive UPSERT just rewrites it; no per-user split because Postern
-- is a single-user app. API keys live in the `secrets` table — the
-- column here just stores the ref string. Defaults match the
-- pre-existing env-var path so an upgrade in place keeps working.
CREATE TABLE ai_settings (
  id            INTEGER PRIMARY KEY CHECK (id = 1),
  enabled       INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  -- One of: 'ollama' | 'anthropic' | 'openai' | 'openai_compat'
  provider_kind TEXT    NOT NULL DEFAULT 'ollama',
  -- Backend-specific model ids. Empty string means "use the
  -- backend's recommended default" (resolved at provider build time).
  chat_model    TEXT    NOT NULL DEFAULT '',
  embed_model   TEXT    NOT NULL DEFAULT '',
  -- Required for openai_compat (e.g. 'https://api.x.ai/v1' for
  -- Grok), and overrideable for ollama (defaults to localhost:11434).
  -- Anthropic + OpenAI ignore this — both have a fixed canonical URL.
  base_url      TEXT,
  -- Pointer into the `secrets` table for API-keyed providers
  -- (Anthropic / OpenAI / openai-compat). NULL for Ollama which is
  -- typically unauthenticated. Ciphertext lives in secrets.ciphertext
  -- under this ref, encrypted by the vault.
  api_key_ref   TEXT,
  -- True once the user has explicitly accepted that flipping to a
  -- third-party cloud provider sends email content off-box. Reset
  -- to 0 whenever provider_kind changes back to 'ollama' or the
  -- user clears it from Settings. Gates POST /api/ai/settings from
  -- saving a cloud provider until consent is recorded.
  cloud_consent INTEGER NOT NULL DEFAULT 0 CHECK (cloud_consent IN (0, 1)),
  updated_at    INTEGER NOT NULL DEFAULT 0
);

INSERT INTO ai_settings (id, enabled, provider_kind, chat_model, embed_model, updated_at)
VALUES (1, 1, 'ollama', '', '', 0);
