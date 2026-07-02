-- Postern canonical schema — GENERATED FILE, DO NOT EDIT.
-- Regenerate via: server/scripts/generate-schema.sh
-- Sourced from server/migrations/000{1..N}_*.sql in order.
-- Last regenerated: 2026-05-10T15:34:47Z

-- NOTE: FTS5 virtual tables are skipped (CLI sqlite3 lacks the
-- module). See STORAGE_INVARIANTS.md for the messages_fts shape.

CREATE TABLE accounts (
  id              INTEGER PRIMARY KEY,
  kind            TEXT NOT NULL CHECK (kind IN ('gmail', 'imap')),
  email           TEXT NOT NULL UNIQUE,
  display_name    TEXT,
  imap_host       TEXT,
  imap_port       INTEGER,
  smtp_host       TEXT,
  smtp_port       INTEGER,
  credential_ref  TEXT NOT NULL,
  vpn_required    INTEGER NOT NULL DEFAULT 0 CHECK (vpn_required IN (0, 1)),
  created_at      INTEGER NOT NULL
, delete_after_sync INTEGER NOT NULL DEFAULT 0, archive_folder TEXT, archive_strategy TEXT NOT NULL DEFAULT 'single', archive_enabled INTEGER NOT NULL DEFAULT 1, auto_archive_enabled INTEGER NOT NULL DEFAULT 0, auto_archive_age_days INTEGER NOT NULL DEFAULT 180, auto_archive_read_only INTEGER NOT NULL DEFAULT 1, avatar_seed TEXT, avatar_set TEXT NOT NULL DEFAULT 'set1', retention_enabled INTEGER NOT NULL DEFAULT 0, retention_days    INTEGER NOT NULL DEFAULT 90, purge_gmail_categories INTEGER NOT NULL DEFAULT 0, skip_gmail_trash INTEGER NOT NULL DEFAULT 0, signature_html TEXT, signature_plain TEXT, sync_enabled INTEGER NOT NULL DEFAULT 1, send_enabled INTEGER NOT NULL DEFAULT 1, include_in_unified INTEGER NOT NULL DEFAULT 1, color TEXT);
CREATE TABLE secrets (
  ref             TEXT PRIMARY KEY,
  ciphertext      BLOB NOT NULL
);
CREATE TABLE messages (
  id              INTEGER PRIMARY KEY,
  account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  message_id      TEXT NOT NULL,
  thread_id       TEXT,
  subject         TEXT,
  from_addr       TEXT,
  to_addrs        TEXT,
  cc_addrs        TEXT,
  date_utc        INTEGER NOT NULL,
  blob_sha256     TEXT NOT NULL,
  size_bytes      INTEGER,
  snippet         TEXT,
  has_attachments INTEGER NOT NULL DEFAULT 0,
  is_read         INTEGER NOT NULL DEFAULT 0,
  is_starred      INTEGER NOT NULL DEFAULT 0, body_text TEXT, is_encrypted INTEGER NOT NULL DEFAULT 0, subject_key TEXT, receipt_to TEXT,
  UNIQUE(account_id, message_id)
);
CREATE INDEX idx_messages_account_date ON messages(account_id, date_utc DESC);
CREATE INDEX idx_messages_thread ON messages(account_id, thread_id);
CREATE TABLE labels (
  id              INTEGER PRIMARY KEY,
  account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  name            TEXT NOT NULL,
  kind            TEXT NOT NULL CHECK (kind IN ('system', 'user', 'gmail_category')),
  UNIQUE(account_id, name)
);
CREATE TABLE message_labels (
  message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  label_id   INTEGER NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
  PRIMARY KEY (message_id, label_id)
);
CREATE TABLE sync_state (
  account_id    INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  label_id      INTEGER NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
  uid_validity  INTEGER,
  uid_next      INTEGER,
  last_sync_utc INTEGER,
  PRIMARY KEY (account_id, label_id)
);
CREATE TABLE remote_fetch_cache (
  url_sha256     TEXT PRIMARY KEY,
  content_type   TEXT,
  body_path      TEXT NOT NULL,
  fetched_at     INTEGER NOT NULL,
  sender_domain  TEXT,
  via_vpn        INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE backup_config (
  id             INTEGER PRIMARY KEY CHECK (id = 1),
  repo_path      TEXT NOT NULL,
  credential_ref TEXT NOT NULL,
  schedule_cron  TEXT NOT NULL,
  last_run_utc   INTEGER,
  last_status    TEXT
);
CREATE TABLE vpn_config (
  id             INTEGER PRIMARY KEY CHECK (id = 1),
  provider       TEXT,
  enabled        INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
  region         TEXT,
  wg_config_ref  TEXT,
  last_check_utc INTEGER,
  last_status    TEXT
, country_id INTEGER, server_load INTEGER, server_country_code TEXT, server_number INTEGER, server_city TEXT);
CREATE TABLE pgp_keys (
  id              INTEGER PRIMARY KEY,
  fingerprint     TEXT NOT NULL UNIQUE,
  user_id         TEXT NOT NULL,
  primary_email   TEXT,
  is_secret       INTEGER NOT NULL DEFAULT 0 CHECK (is_secret IN (0, 1)),
  armored_public  TEXT NOT NULL,
  secret_ref      TEXT,
  created_at      INTEGER NOT NULL,
  expires_at      INTEGER,
  source          TEXT NOT NULL,
  last_used_at    INTEGER
);
CREATE INDEX idx_pgp_keys_email ON pgp_keys(primary_email);
CREATE INDEX idx_pgp_keys_is_secret ON pgp_keys(is_secret);
CREATE INDEX idx_messages_thread_id_date
  ON messages(thread_id, date_utc DESC);
CREATE TABLE kek_config (
  id         INTEGER PRIMARY KEY CHECK (id = 1),
  salt       BLOB NOT NULL,
  verifier   BLOB NOT NULL,  -- AEAD-encrypted known plaintext; decrypt succeeds iff password is right
  created_at INTEGER NOT NULL,
  params     TEXT NOT NULL   -- JSON: {"m_cost":19456,"t_cost":2,"p_cost":1,"algo":"argon2id"}
);
CREATE TABLE rules (
  id          INTEGER PRIMARY KEY,
  account_id  INTEGER REFERENCES accounts(id) ON DELETE CASCADE,
  name        TEXT NOT NULL,
  enabled     INTEGER NOT NULL DEFAULT 1,
  priority    INTEGER NOT NULL DEFAULT 0,
  condition_field   TEXT NOT NULL,
  condition_op      TEXT NOT NULL,
  condition_value   TEXT NOT NULL,
  action_type       TEXT NOT NULL,
  action_value      TEXT NOT NULL,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
);
CREATE INDEX idx_rules_account ON rules(account_id, enabled, priority);
CREATE TABLE audit_log (
  id         INTEGER PRIMARY KEY,
  ts_utc     INTEGER NOT NULL,
  event_type TEXT NOT NULL,
  detail     TEXT,
  ip         TEXT
, category TEXT NOT NULL DEFAULT 'security');
CREATE INDEX idx_audit_ts ON audit_log(ts_utc DESC);
CREATE INDEX idx_audit_category_ts ON audit_log(category, ts_utc DESC);
CREATE TABLE trusted_devices (
  id             INTEGER PRIMARY KEY,
  -- SHA-256 of the raw token (hex). Cookie value is the raw token.
  token_hash     TEXT    NOT NULL UNIQUE,
  -- Short UA summary for the settings list (browser/os/platform).
  user_agent     TEXT,
  -- IP where the device was first enrolled.
  first_seen_ip  TEXT,
  -- IP on the most recent request that accepted this token.
  last_seen_ip   TEXT,
  last_seen_at   INTEGER,
  created_at     INTEGER NOT NULL,
  expires_at     INTEGER NOT NULL
);
CREATE INDEX idx_trusted_devices_expiry ON trusted_devices(expires_at);
CREATE INDEX idx_messages_account_subject_key
  ON messages(account_id, subject_key);
CREATE TABLE outbox (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- JSON-serialized SendRequest. See crate::send::SendRequest.
    payload_json    TEXT NOT NULL,
    -- Unix epoch seconds. Dispatch when scheduled_at <= strftime('%s','now').
    scheduled_at    INTEGER NOT NULL,
    -- Lifecycle: pending → sending → (sent | failed). Or pending → cancelled.
    status          TEXT NOT NULL CHECK (status IN ('pending','sending','sent','failed','cancelled')),
    attempts        INTEGER NOT NULL DEFAULT 0,
    -- Set when the most recent dispatch errored. Cleared on success.
    last_error      TEXT,
    -- Short description (to / subject) cached for the outbox list UI
    -- so we don't need to re-parse payload_json for every list render.
    summary_to      TEXT NOT NULL,
    summary_subject TEXT NOT NULL,
    -- Sent message id once SMTP accepted. Null while pending/failed.
    sent_message_id TEXT,
    -- JSON-serialized SendForensics captured at dispatch time. Lets the
    -- compose success card + /outbox UI show VPN/SMTP/Autocrypt details
    -- even after the queue settled.
    forensics_json  TEXT,
    created_at      INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at      INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
CREATE INDEX idx_outbox_pending_due
    ON outbox (scheduled_at) WHERE status = 'pending';
CREATE INDEX idx_outbox_status_created
    ON outbox (status, created_at DESC);
CREATE TABLE cal_calendars (
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
CREATE TABLE cal_events (
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
CREATE INDEX idx_cal_events_range
    ON cal_events (calendar_id, dtstart_utc);
CREATE INDEX idx_cal_events_uid
    ON cal_events (uid);
CREATE TABLE reminders (
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
CREATE INDEX idx_reminders_due_pending
    ON reminders (due_at_utc) WHERE done = 0 AND notified = 0;
CREATE INDEX idx_reminders_due_at
    ON reminders (due_at_utc);
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
, server_fingerprint TEXT);
CREATE INDEX idx_backup_destinations_enabled
    ON backup_destinations(enabled);
CREATE TABLE backup_schedule (
    id              INTEGER PRIMARY KEY CHECK (id = 1),
    enabled         INTEGER NOT NULL DEFAULT 0,
    -- 'daily' | 'weekly'. Other values rejected at the storage layer.
    frequency       TEXT NOT NULL DEFAULT 'daily',
    -- Local-time hour and minute the backup should fire. Server's
    -- local timezone — operators usually pick a low-traffic hour and
    -- expect "03:00" to mean their wall-clock 03:00.
    hour            INTEGER NOT NULL DEFAULT 3 CHECK (hour BETWEEN 0 AND 23),
    minute          INTEGER NOT NULL DEFAULT 0 CHECK (minute BETWEEN 0 AND 59),
    -- 0 = Sunday … 6 = Saturday. Only honoured when frequency='weekly'.
    day_of_week     INTEGER NOT NULL DEFAULT 0 CHECK (day_of_week BETWEEN 0 AND 6),
    -- How many local tarballs to keep after a successful backup.
    -- Older ones are deleted. 0 = no automatic pruning.
    retention_count INTEGER NOT NULL DEFAULT 7 CHECK (retention_count >= 0),
    -- Unix seconds of the last successful auto-fire. Used to debounce
    -- — a 60s tick during the "scheduled minute" window must not
    -- fire twice.
    last_run_at     INTEGER
);
CREATE TABLE auth_totp (
  id          INTEGER PRIMARY KEY CHECK (id = 1),
  enabled     INTEGER NOT NULL DEFAULT 0,
  -- Pointer into the secrets table holding the base32-encoded TOTP
  -- secret. NULL when the user has not enrolled (or has disabled).
  secret_ref  TEXT,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
);
CREATE TABLE auth_recovery_codes (
  id         INTEGER PRIMARY KEY,
  code_hash  TEXT    NOT NULL UNIQUE,
  created_at INTEGER NOT NULL,
  used_at    INTEGER
);
CREATE INDEX idx_recovery_codes_unused
  ON auth_recovery_codes(used_at) WHERE used_at IS NULL;
CREATE TABLE contacts (
  id              INTEGER PRIMARY KEY,
  address         TEXT    NOT NULL UNIQUE COLLATE NOCASE,
  -- Display name extracted from "Name <addr>" headers. NULL when we
  -- only ever saw the bare address. Most-recent header wins on
  -- conflict (handled in the upsert path, not the schema).
  display_name    TEXT,
  -- First / last time this address showed up on any message we
  -- inserted. Updated on every upsert.
  first_seen_utc  INTEGER NOT NULL,
  last_seen_utc   INTEGER NOT NULL,
  -- Bumped on every upsert. Lets the UI rank "most-emailed" contacts
  -- without scanning the messages table.
  message_count   INTEGER NOT NULL DEFAULT 1,
  -- User-driven — toggled from the Contacts UI (phase 2).
  is_favorite     INTEGER NOT NULL DEFAULT 0,
  -- Free-text notes, manual entry only.
  notes           TEXT,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL
, photo_blob BLOB, photo_mime TEXT);
CREATE INDEX idx_contacts_last_seen
  ON contacts(last_seen_utc DESC);
CREATE INDEX idx_contacts_message_count
  ON contacts(message_count DESC);
CREATE INDEX idx_contacts_favorite
  ON contacts(last_seen_utc DESC) WHERE is_favorite = 1;
CREATE TABLE notes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    title       TEXT NOT NULL DEFAULT '',
    body        TEXT NOT NULL DEFAULT '',
    -- Pinned notes float to the top of the list. Plain bool stored
    -- as 0/1 so existing tooling stays happy.
    pinned      INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1)),
    created_at  INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at  INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
CREATE INDEX idx_notes_updated_at
    ON notes (updated_at DESC);
CREATE INDEX idx_notes_pinned
    ON notes (updated_at DESC) WHERE pinned = 1;
CREATE TABLE IF NOT EXISTS "cal_accounts" (
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
CREATE TABLE trusted_senders (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id  INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  -- Lower-cased bare address (no display name, no angle brackets).
  -- The HTTP layer extracts this from the From: header before insert.
  email_lower TEXT    NOT NULL,
  created_at  INTEGER NOT NULL DEFAULT (strftime('%s','now')),
  UNIQUE (account_id, email_lower)
);
CREATE INDEX idx_trusted_senders_account
  ON trusted_senders (account_id, email_lower);
CREATE TABLE IF NOT EXISTS "ai_settings" (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enabled INTEGER NOT NULL DEFAULT 1,
    provider_kind TEXT NOT NULL DEFAULT 'ollama',
    chat_model TEXT NOT NULL DEFAULT '',
    base_url TEXT,
    api_key_ref TEXT,
    cloud_consent INTEGER NOT NULL DEFAULT 0,
    auto_start INTEGER NOT NULL DEFAULT 1,
    polish_chat_model TEXT,
    updated_at INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS "app_meta" (
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
