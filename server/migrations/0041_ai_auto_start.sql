-- "Always on" toggle for AI features. When true, the AI provider
-- holder is rebuilt automatically the first time the vault unlocks
-- after a restart — so the user doesn't have to re-save Settings →
-- AI or flip the toolbar toggle every time the container reboots
-- (e.g. after an in-app update).
--
-- Defaults to ON so that the post-update experience is "AI keeps
-- working" rather than "AI silently dropped". Users who want
-- finer-grained control can flip it off in Settings → AI.
ALTER TABLE ai_settings ADD COLUMN auto_start INTEGER NOT NULL DEFAULT 1
  CHECK (auto_start IN (0, 1));
