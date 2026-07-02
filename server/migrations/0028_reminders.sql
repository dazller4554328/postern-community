-- Local reminders. Independent of CalDAV — these never leave the
-- device. Surfaced on the calendar grid alongside synced events and
-- delivered to the UI via /api/reminders/due once their due time
-- passes.
--
-- Recurrence is intentionally minimal for v1: none/daily/weekly/monthly.
-- A reminder that's marked done resets to undone + advances due_at_utc
-- by one period when `repeat` is non-none, so the next occurrence
-- appears on the calendar immediately after the user dismisses the
-- current one. This keeps the data model flat (one row per series, no
-- per-occurrence rows) and is enough until users ask for more.
--
-- Snooze stores an explicit override timestamp rather than mutating
-- due_at_utc so the reminder still renders on the calendar at its
-- "real" due time. The poller picks `MAX(due_at_utc, snoozed_until_utc)`
-- when deciding whether a reminder is currently due.

CREATE TABLE IF NOT EXISTS reminders (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    title               TEXT NOT NULL,
    notes               TEXT,
    due_at_utc          INTEGER NOT NULL,        -- unix seconds
    repeat              TEXT NOT NULL DEFAULT 'none'
                          CHECK (repeat IN ('none','daily','weekly','monthly')),
    done                INTEGER NOT NULL DEFAULT 0,
    -- True once we've fired a notification for the current occurrence.
    -- Cleared whenever the row's due_at_utc advances (recurrence) or
    -- the user snoozes — both mean "we owe the user another ping".
    notified            INTEGER NOT NULL DEFAULT 0,
    -- When non-NULL and in the future, suppresses notification until
    -- this timestamp passes. Calendar still shows the reminder at
    -- due_at_utc.
    snoozed_until_utc   INTEGER,
    created_at          INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at          INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

-- Hot path: poller's "anything due right now?" query. Filters on
-- not-done + not-notified + due-time-passed; the partial index keeps
-- it tiny even when the reminders table grows.
CREATE INDEX IF NOT EXISTS idx_reminders_due_pending
    ON reminders (due_at_utc) WHERE done = 0 AND notified = 0;

-- Calendar range scans.
CREATE INDEX IF NOT EXISTS idx_reminders_due_at
    ON reminders (due_at_utc);
