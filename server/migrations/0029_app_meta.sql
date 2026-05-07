-- App-wide singleton table for metadata that doesn't belong on any
-- specific mail account: install ID (used to identify this Postern
-- install to the update server), license key + cached verification
-- status, and last update-check timestamp.
--
-- Always exactly one row. The install_id is generated + inserted
-- the first time the app boots (see storage::app_meta::get_or_init).
CREATE TABLE IF NOT EXISTS app_meta (
    id                        INTEGER PRIMARY KEY CHECK (id = 1),
    install_id                TEXT NOT NULL,
    license_key               TEXT,
    -- One of: unknown | active | malformed | expired | revoked | not_found.
    -- Populated by the most recent /api/license/verify call.
    license_status            TEXT NOT NULL DEFAULT 'unknown',
    license_tier              TEXT,
    license_verified_at_utc   INTEGER,
    -- Last time we successfully talked to the update server. NULL
    -- means "never checked". The UI shows "Last checked: N minutes
    -- ago" off this.
    last_update_check_at_utc  INTEGER,
    -- Last response from /check, stored verbatim for the UI so we
    -- don't have to keep the update server reachable to render
    -- available-update metadata.
    last_update_check_json    TEXT,
    created_at                INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at                INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
