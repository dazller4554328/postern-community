-- Optional user-defined extension to Datas's prompt. Appended
-- AFTER the seven Commandments so it can extend behaviour
-- ("always answer in German", "be more terse", "highlight invoice
-- amounts in bold") without overriding the security floor. The
-- Commandments still take precedence by both ordering and explicit
-- "non-negotiable" framing in the prompt.
ALTER TABLE ai_settings ADD COLUMN user_rules TEXT;
