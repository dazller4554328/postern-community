-- Trusted devices: a client that ticks "remember this device" at
-- unlock gets a cookie bearing a random opaque token. The hash of the
-- token is stored here — the server can validate incoming cookies but
-- cannot reconstruct them if the DB is ever read offline.
--
-- A valid, unrevoked, unexpired token shortcircuits the IP-change
-- auto-lock so roaming phones don't get booted every time wifi swaps
-- to cellular.
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
