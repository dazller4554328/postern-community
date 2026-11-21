-- Revision history for notes.
--
-- Notes auto-save destructively (the editor PATCHes title+body every
-- ~1s of idle). That's convenient, but an accidental edit wipes the
-- prior text with no recovery. This table snapshots the PRIOR state
-- on every update so the user can restore. Capped per-note in the
-- storage layer (default 50 rows / minimum 30s gap between snaps —
-- see `notes::update`) so a long typing session doesn't churn out
-- hundreds of micro-revisions.
--
-- Cascade on delete: removing the parent note removes its history.
-- That matches user intent ("I deleted the note") and keeps the
-- foreign key honest. A future "trash bin for notes" feature would
-- keep both the note row and its revisions during the grace period.

CREATE TABLE note_revisions (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    note_id    INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    title      TEXT NOT NULL DEFAULT '',
    body       TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

CREATE INDEX idx_note_revisions_note_created
    ON note_revisions (note_id, created_at DESC);
