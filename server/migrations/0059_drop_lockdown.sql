-- Drop the orphan `lockdown_enabled` column from `app_meta`.
--
-- 0042 added it for a server-side kill-switch that was never wired
-- up. 0058 explicitly retained it as a no-op, but no reader / writer
-- / UI ever shipped, so the column is misleading dead schema. Drop
-- it via SQLite's standard rebuild dance to keep compatibility with
-- pre-3.35 builds and to make the resulting schema explicit.

CREATE TABLE app_meta_new (
    id                        INTEGER PRIMARY KEY CHECK (id = 1),
    install_id                TEXT NOT NULL,
    license_key               TEXT,
    license_status            TEXT NOT NULL DEFAULT 'unknown',
    license_tier              TEXT,
    license_verified_at_utc   INTEGER,
    last_update_check_at_utc  INTEGER,
    last_update_check_json    TEXT,
    created_at                INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at                INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

INSERT INTO app_meta_new (
    id, install_id, license_key, license_status, license_tier,
    license_verified_at_utc, last_update_check_at_utc,
    last_update_check_json, created_at, updated_at
)
SELECT
    id, install_id, license_key, license_status, license_tier,
    license_verified_at_utc, last_update_check_at_utc,
    last_update_check_json, created_at, updated_at
FROM app_meta;

DROP TABLE app_meta;
ALTER TABLE app_meta_new RENAME TO app_meta;
