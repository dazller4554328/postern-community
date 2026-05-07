-- Postern canonical schema — GENERATED FILE, DO NOT EDIT.
-- Regenerate via: server/scripts/generate-schema.sh
-- Sourced from server/migrations/000{1..N}_*.sql in order.
-- Last regenerated: 2026-04-28T09:27:38Z

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
, delete_after_sync INTEGER NOT NULL DEFAULT 0, archive_folder TEXT, archive_strategy TEXT NOT NULL DEFAULT 'single', archive_enabled INTEGER NOT NULL DEFAULT 1, auto_archive_enabled INTEGER NOT NULL DEFAULT 0, auto_archive_age_days INTEGER NOT NULL DEFAULT 180, auto_archive_read_only INTEGER NOT NULL DEFAULT 1, avatar_seed TEXT, avatar_set TEXT NOT NULL DEFAULT 'set1', retention_enabled INTEGER NOT NULL DEFAULT 0, retention_days    INTEGER NOT NULL DEFAULT 90, purge_gmail_categories INTEGER NOT NULL DEFAULT 0, skip_gmail_trash INTEGER NOT NULL DEFAULT 0, signature_html TEXT, signature_plain TEXT, sync_enabled INTEGER NOT NULL DEFAULT 1, send_enabled INTEGER NOT NULL DEFAULT 1, include_in_unified INTEGER NOT NULL DEFAULT 1);
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
CREATE TABLE cal_accounts (
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
CREATE TABLE app_meta (
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
, lockdown_enabled INTEGER NOT NULL DEFAULT 0
  CHECK (lockdown_enabled IN (0, 1)));
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
CREATE TABLE ai_embeddings (
    message_id  INTEGER PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    -- Embedding model identifier (e.g. 'nomic-embed-text:latest').
    model       TEXT    NOT NULL,
    -- Vector dimensionality. Stored explicitly so the decoder can
    -- validate blob length without trusting the model name lookup.
    dim         INTEGER NOT NULL,
    -- Packed f32 little-endian. byte_length(vector) MUST equal dim*4.
    vector      BLOB    NOT NULL,
    created_at  INTEGER NOT NULL
);
CREATE INDEX idx_ai_embeddings_model ON ai_embeddings(model);
CREATE TABLE ai_chat_log (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at          INTEGER NOT NULL,
    -- NULL = unified scope; otherwise FK to accounts.id with
    -- ON DELETE SET NULL so deleting a mailbox doesn't orphan
    -- the historical conversations referencing it.
    account_scope       INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    question            TEXT    NOT NULL,
    answer              TEXT    NOT NULL,
    -- Stable provider id (the LlmProvider::id() string), e.g.
    -- 'ollama' or 'anthropic'. Used by the audit UI to render
    -- "this answer came from <provider>".
    provider            TEXT    NOT NULL,
    -- Exact model identifiers used at chat-time and retrieval-time.
    -- Kept so a future "replay this question" feature can pin them.
    chat_model          TEXT    NOT NULL,
    embed_model         TEXT    NOT NULL,
    -- 'local_only' | 'user_controlled_remote' | 'third_party_cloud'
    -- — captured at the moment of the call so a posture change
    -- after the fact doesn't rewrite history.
    privacy_posture     TEXT    NOT NULL,
    -- JSON array of message_ids the model was given as context.
    -- Powers the clickable "sources" footer under each answer.
    cited_message_ids   TEXT    NOT NULL,
    prompt_tokens       INTEGER NOT NULL DEFAULT 0,
    completion_tokens   INTEGER NOT NULL DEFAULT 0,
    elapsed_ms          INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_ai_chat_log_created_at ON ai_chat_log(created_at DESC);
CREATE TABLE ai_settings (
  id            INTEGER PRIMARY KEY CHECK (id = 1),
  enabled       INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  -- One of: 'ollama' | 'anthropic' | 'openai' | 'openai_compat'
  provider_kind TEXT    NOT NULL DEFAULT 'ollama',
  -- Backend-specific model ids. Empty string means "use the
  -- backend's recommended default" (resolved at provider build time).
  chat_model    TEXT    NOT NULL DEFAULT '',
  embed_model   TEXT    NOT NULL DEFAULT '',
  -- Required for openai_compat (e.g. 'https://api.x.ai/v1' for
  -- Grok), and overrideable for ollama (defaults to localhost:11434).
  -- Anthropic + OpenAI ignore this — both have a fixed canonical URL.
  base_url      TEXT,
  -- Pointer into the `secrets` table for API-keyed providers
  -- (Anthropic / OpenAI / openai-compat). NULL for Ollama which is
  -- typically unauthenticated. Ciphertext lives in secrets.ciphertext
  -- under this ref, encrypted by the vault.
  api_key_ref   TEXT,
  -- True once the user has explicitly accepted that flipping to a
  -- third-party cloud provider sends email content off-box. Reset
  -- to 0 whenever provider_kind changes back to 'ollama' or the
  -- user clears it from Settings. Gates POST /api/ai/settings from
  -- saving a cloud provider until consent is recorded.
  cloud_consent INTEGER NOT NULL DEFAULT 0 CHECK (cloud_consent IN (0, 1)),
  updated_at    INTEGER NOT NULL DEFAULT 0
, embed_provider_kind TEXT NOT NULL DEFAULT 'ollama', embed_base_url TEXT, embed_api_key_ref TEXT, auto_start INTEGER NOT NULL DEFAULT 1
  CHECK (auto_start IN (0, 1)), user_rules TEXT, excluded_senders TEXT, excluded_labels TEXT);
CREATE TABLE ai_activity_log (
  id                 INTEGER PRIMARY KEY AUTOINCREMENT,
  ts_utc             INTEGER NOT NULL,
  -- 'chat' | 'chat_stream' | 'embed' | 'health'
  kind               TEXT    NOT NULL,
  -- LlmProvider::id() — 'ollama' | 'openai' | 'anthropic' | 'openai_compat'.
  provider           TEXT    NOT NULL,
  -- Model id as the provider reported back (or the request model
  -- when no response made it).
  model              TEXT    NOT NULL DEFAULT '',
  -- 'ok' | 'error'
  status             TEXT    NOT NULL DEFAULT 'ok',
  -- Wall-clock latency from request build to response (or error).
  elapsed_ms         INTEGER NOT NULL DEFAULT 0,
  -- Token counts when the provider returned them (Ollama on
  -- embed reports 0; that's expected).
  prompt_tokens      INTEGER NOT NULL DEFAULT 0,
  completion_tokens  INTEGER NOT NULL DEFAULT 0,
  -- Approximate byte sizes of the request + response payloads
  -- BEFORE truncation. Lets the user see "you sent 47 KB" even
  -- when the stored sample is the first 4 KB.
  input_bytes        INTEGER NOT NULL DEFAULT 0,
  output_bytes       INTEGER NOT NULL DEFAULT 0,
  -- Truncated JSON payloads. Stored as TEXT so SQLCipher's
  -- whole-DB encryption protects them. NULL when the row was
  -- written without payload sampling (e.g. health probes).
  request_sample     TEXT,
  response_sample    TEXT,
  -- One-line error string when status='error'. Detailed enough
  -- to copy/paste into a bug report; no stack traces.
  error_message      TEXT
);
CREATE INDEX idx_ai_activity_ts ON ai_activity_log(ts_utc DESC);
CREATE INDEX idx_ai_activity_kind_provider ON ai_activity_log(kind, provider);
CREATE INDEX idx_ai_activity_status ON ai_activity_log(status);
CREATE TRIGGER ai_activity_log_autotrim
AFTER INSERT ON ai_activity_log
BEGIN
  DELETE FROM ai_activity_log
   WHERE id <= (SELECT id FROM ai_activity_log
                ORDER BY id DESC
                LIMIT 1 OFFSET 1000);
END;
