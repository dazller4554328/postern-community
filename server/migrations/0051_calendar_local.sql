-- Local-first calendar.
--
-- Until now `cal_accounts` was implicitly CalDAV-flavoured: every row
-- assumed a remote server, credentials, principal/home discovery. This
-- migration introduces a `kind` discriminator so we can also model a
-- LOCAL account whose events live entirely in this DB and never touch
-- a remote server. Privacy-first email client → privacy-first calendar.
--
-- Schema deltas:
--   * cal_accounts.kind  TEXT  NOT NULL DEFAULT 'caldav'
--       'caldav' for the existing flavour, 'local' for purely-local.
--   * server_url / username / credential_ref relax from NOT NULL to
--     NULL-able. Local rows leave them NULL.
--
-- SQLite can't ALTER COLUMN to drop NOT NULL, so we rebuild
-- cal_accounts with the new constraints, copy data, swap names. The
-- two child tables (cal_calendars, cal_events) stay as-is — their FKs
-- target id, which we preserve.

PRAGMA foreign_keys = OFF;

CREATE TABLE cal_accounts_new (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    kind              TEXT    NOT NULL DEFAULT 'caldav'
                              CHECK (kind IN ('caldav', 'local')),
    label             TEXT    NOT NULL,
    server_url        TEXT,
    username          TEXT,
    credential_ref    TEXT,
    principal_url     TEXT,
    calendar_home_url TEXT,
    last_sync_at      INTEGER,
    last_sync_error   TEXT,
    created_at        INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

INSERT INTO cal_accounts_new
    (id, kind, label, server_url, username, credential_ref,
     principal_url, calendar_home_url, last_sync_at, last_sync_error,
     created_at)
SELECT id, 'caldav', label, server_url, username, credential_ref,
       principal_url, calendar_home_url, last_sync_at, last_sync_error,
       created_at
  FROM cal_accounts;

DROP TABLE cal_accounts;
ALTER TABLE cal_accounts_new RENAME TO cal_accounts;

PRAGMA foreign_keys = ON;
