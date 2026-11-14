-- Off-site destinations for the backup tarball.
--
-- One row per destination the operator has configured. `kind` is
-- 'sftp' today; 'gdrive' will land in a follow-up. Auth credentials
-- (passwords / private keys) live behind a `secrets.ref` alias —
-- never inline in this table — so a backup of this DB stays no more
-- sensitive than the rest of the install.

CREATE TABLE backup_destinations (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    kind                  TEXT NOT NULL,                 -- 'sftp'
    label                 TEXT NOT NULL,                 -- "Hetzner box"
    enabled               INTEGER NOT NULL DEFAULT 1,
    -- Public, non-secret config in JSON: host/port/user/remote_dir
    -- for SFTP; matching shape for future kinds.
    public_config_json    TEXT NOT NULL,
    -- Reference into `secrets` for the credential blob (vault-encrypted).
    -- The blob is JSON like { "auth": "password", "password": "..." }
    -- or { "auth": "key", "key_pem": "...", "passphrase": null }.
    credential_ref        TEXT NOT NULL,
    last_push_at          INTEGER,
    last_push_filename    TEXT,
    last_push_status      TEXT,                          -- 'ok' | 'error'
    last_push_error       TEXT,
    created_at            INTEGER NOT NULL
);

CREATE INDEX idx_backup_destinations_enabled
    ON backup_destinations(enabled);
