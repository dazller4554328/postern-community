-- Calendar support — CalDAV accounts + synced events.
--
-- Scope:
--   * One row per user-configured CalDAV server (Nextcloud, iCloud,
--     Fastmail, Radicale, Baïkal, etc.). Credentials live in the
--     existing `secrets` table, keyed by `cal:<account_id>` once the
--     row exists; bootstrap path writes by a temporary ref first.
--   * Calendar collections (Personal, Work, Shared) are discovered
--     per account via PROPFIND and live in `cal_calendars`.
--   * Each VEVENT becomes one row in `cal_events`. Recurring events
--     store their base occurrence + raw RRULE; the query layer
--     expands RRULE into concrete occurrences at read time so we
--     don't bloat storage or drift out of sync with server-side
--     mutations.
--   * The full raw .ics is retained so a later edit/delete write-back
--     can submit a minimal patch instead of reconstructing iCalendar
--     from the decomposed columns.

CREATE TABLE IF NOT EXISTS cal_accounts (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    label             TEXT NOT NULL,
    server_url        TEXT NOT NULL,     -- user-entered root URL
    username          TEXT NOT NULL,
    credential_ref    TEXT NOT NULL,     -- foreign key into `secrets`
    -- Discovered on first sync and cached; NULL until the first
    -- successful PROPFIND. Saves a round-trip on every subsequent sync.
    principal_url     TEXT,
    calendar_home_url TEXT,
    last_sync_at      INTEGER,
    last_sync_error   TEXT,
    created_at        INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

CREATE TABLE IF NOT EXISTS cal_calendars (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id    INTEGER NOT NULL REFERENCES cal_accounts(id) ON DELETE CASCADE,
    dav_url       TEXT NOT NULL,         -- absolute URL to the collection
    name          TEXT NOT NULL,
    -- CalDAV collection etag-equivalent. Unchanged ctag = nothing
    -- changed in the collection, so we can skip the full REPORT.
    ctag          TEXT,
    color         TEXT,                  -- server-advertised hex, nullable
    read_only     INTEGER NOT NULL DEFAULT 0,
    hidden        INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    UNIQUE(account_id, dav_url)
);

CREATE TABLE IF NOT EXISTS cal_events (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    calendar_id   INTEGER NOT NULL REFERENCES cal_calendars(id) ON DELETE CASCADE,
    dav_href      TEXT NOT NULL,         -- path on the CalDAV server
    dav_etag      TEXT,                  -- for conditional writes later
    uid           TEXT NOT NULL,         -- iCal UID (stable across servers)
    summary       TEXT,
    description   TEXT,
    location      TEXT,
    dtstart_utc   INTEGER NOT NULL,      -- first occurrence start (seconds)
    dtend_utc     INTEGER,               -- end of first occurrence
    all_day       INTEGER NOT NULL DEFAULT 0,
    rrule         TEXT,                  -- raw RRULE string, expanded at read
    raw_ics       TEXT NOT NULL,         -- full .ics for later round-tripping
    created_at    INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at    INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    UNIQUE(calendar_id, dav_href)
);

-- Range scans: "events overlapping [from, to]" is the dominant query
-- path once we have recurring-event expansion at read time. Index the
-- start; recurring rows are fetched separately and expanded in-memory.
CREATE INDEX IF NOT EXISTS idx_cal_events_range
    ON cal_events (calendar_id, dtstart_utc);
CREATE INDEX IF NOT EXISTS idx_cal_events_uid
    ON cal_events (uid);
