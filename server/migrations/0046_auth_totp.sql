-- Time-based one-time-password (TOTP) state for vault unlock.
-- Standard RFC 6238: HMAC-SHA1 of (secret, current 30-second window),
-- truncated to a 6-digit code. Compatible with any authenticator app
-- (Google Authenticator, 1Password, Bitwarden, Aegis, etc.) — no
-- third-party service involved.
--
-- Singleton row (id = 1). The actual base32 secret lives in the
-- existing `secrets` table (vault-encrypted) under ref='auth:totp';
-- this table just holds enabled/disabled state + a pointer to the
-- secrets row. Keeping the secret in the existing encrypted-at-rest
-- store means a DB-only compromise can't read TOTP secrets without
-- the vault KEK.
CREATE TABLE auth_totp (
  id          INTEGER PRIMARY KEY CHECK (id = 1),
  enabled     INTEGER NOT NULL DEFAULT 0,
  -- Pointer into the secrets table holding the base32-encoded TOTP
  -- secret. NULL when the user has not enrolled (or has disabled).
  secret_ref  TEXT,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
);
INSERT INTO auth_totp (id, enabled, secret_ref, created_at, updated_at)
VALUES (1, 0, NULL, strftime('%s', 'now'), strftime('%s', 'now'));

-- Recovery codes — single-use, generated at TOTP enrollment so the
-- user has a way back in if they lose their authenticator device.
-- Stored as SHA-256 hashes of the raw code (16 bytes random, base32-
-- encoded → 26-char string) so an offline DB read can't enumerate
-- the working values. used_at is set on consumption to preserve an
-- audit trail without us tracking the raw values.
CREATE TABLE auth_recovery_codes (
  id         INTEGER PRIMARY KEY,
  code_hash  TEXT    NOT NULL UNIQUE,
  created_at INTEGER NOT NULL,
  used_at    INTEGER
);
CREATE INDEX idx_recovery_codes_unused
  ON auth_recovery_codes(used_at) WHERE used_at IS NULL;
