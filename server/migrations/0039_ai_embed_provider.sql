-- Decouple the embedding provider from the chat provider. Lets a
-- user run chat on a cloud LLM (OpenAI/Claude/Grok) while keeping
-- embeddings on local Ollama — the bulk-cost item AND the part
-- where every email body would otherwise be uploaded for indexing.
--
-- Default: embed_provider_kind = 'ollama'. On upgrade, anyone who
-- was implicitly routing embeddings to OpenAI (because chat=OpenAI)
-- gets switched to local Ollama. They can opt back in via Settings
-- → AI if they want.
ALTER TABLE ai_settings ADD COLUMN embed_provider_kind TEXT NOT NULL DEFAULT 'ollama';
-- Optional override — required when embed_provider_kind = 'openai_compat'
-- (e.g. self-hosted vLLM at http://vllm.lan:8000/v1), ignored otherwise.
ALTER TABLE ai_settings ADD COLUMN embed_base_url TEXT;
-- Pointer into the secrets table for an embed-specific key. Only
-- needed when the embed provider is a cloud vendor AND distinct
-- from the chat provider — otherwise the chat key (api_key_ref)
-- is reused. NULL by default.
ALTER TABLE ai_settings ADD COLUMN embed_api_key_ref TEXT;
