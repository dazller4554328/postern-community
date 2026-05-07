-- Secure notes. NordPass-style encrypted notepad: title + markdown
-- body. The whole DB is SQLCipher-encrypted, so "secure" here means
-- "lives behind the same vault unlock as the rest of the user's mail
-- — no extra key per note." Available in both pro and free builds.
--
-- v1 schema is intentionally flat. Tags / folders / sharing /
-- attachments are deferred until users ask for them.

CREATE TABLE IF NOT EXISTS notes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    title       TEXT NOT NULL DEFAULT '',
    body        TEXT NOT NULL DEFAULT '',
    -- Pinned notes float to the top of the list. Plain bool stored
    -- as 0/1 so existing tooling stays happy.
    pinned      INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1)),
    created_at  INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at  INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

-- Default list ordering: pinned first, then most-recently-edited.
-- Two indexes rather than a compound one because (a) the table is
-- expected to stay small (hundreds, not millions of rows) and
-- (b) the `pinned` partial index keeps the cost zero for users
-- who never pin anything.
CREATE INDEX IF NOT EXISTS idx_notes_updated_at
    ON notes (updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_notes_pinned
    ON notes (updated_at DESC) WHERE pinned = 1;
