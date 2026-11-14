-- Trusted senders: per-account allowlist of email addresses that should
-- never be filed as spam. Populated automatically when the user clicks
-- "Not spam" on a message, and editable from Settings → Trusted senders.
--
-- During IMAP sync, any message landing in the account's Spam/Junk
-- folder whose From: matches an allowlisted address is auto-rescued
-- back to INBOX (server-side IMAP move + local relabel).

CREATE TABLE IF NOT EXISTS trusted_senders (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id  INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  -- Lower-cased bare address (no display name, no angle brackets).
  -- The HTTP layer extracts this from the From: header before insert.
  email_lower TEXT    NOT NULL,
  created_at  INTEGER NOT NULL DEFAULT (strftime('%s','now')),
  UNIQUE (account_id, email_lower)
);

CREATE INDEX IF NOT EXISTS idx_trusted_senders_account
  ON trusted_senders (account_id, email_lower);
