-- Postern PGP keyring. Unified table for both our own keypairs (is_secret=1)
-- and harvested/imported public-only keys (is_secret=0).
--
-- Sources:
--   'generated' — created locally via UI, secret is ours
--   'imported'  — pasted armored key (may or may not have secret)
--   'autocrypt' — harvested from an incoming Autocrypt: header
--   'wkd'       — fetched from the recipient's Web Key Directory
--   'keyserver' — fetched from keys.openpgp.org or similar
--
-- Secret key material, when present, is stored encrypted via the existing
-- `secrets` table (the `secret_ref` column points into it). Sprint 4
-- base64-wraps it; Sprint 5 wires KEK-derived encryption.

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
