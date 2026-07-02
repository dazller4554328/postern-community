-- Postern initial schema.
-- Maps to §6 "Data Model" in PROPOSAL.md. Keep this file append-only:
-- new migrations go in new 000N_*.sql files, never edit this one after it ships.

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
);

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
  is_starred      INTEGER NOT NULL DEFAULT 0,
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

CREATE VIRTUAL TABLE messages_fts USING fts5(
  subject, from_addr, to_addrs, body_text, snippet,
  content='', tokenize='porter unicode61'
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
);
