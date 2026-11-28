-- Email templates: reusable subject + body snippets the user can drop
-- into the compose form instead of re-typing (or copy/pasting) the
-- same message every time. Global rather than per-account — a
-- template is about the message, not the mailbox it goes out from.
--
-- Body is plain text, same as the compose textarea. The signature is
-- NOT part of a template; compose inserts the template above the
-- account signature so the two never collide.

CREATE TABLE IF NOT EXISTS email_templates (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Short label shown in the compose picker ("Invoice reminder").
    name        TEXT NOT NULL,
    -- Optional. Empty string means "leave the subject alone".
    subject     TEXT NOT NULL DEFAULT '',
    body        TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at  INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

CREATE INDEX IF NOT EXISTS idx_email_templates_name
    ON email_templates (name COLLATE NOCASE);
