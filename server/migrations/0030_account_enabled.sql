-- Per-account master switches.
-- sync_enabled = 0 → scheduler skips this mailbox, no IMAP pulls, no
-- retention/auto-archive passes, no trigger-fire processing. The row
-- stays intact so the user can re-enable without re-entering creds.
-- send_enabled = 0 → SMTP send refuses with a clear error; outbox
-- items addressed from this account stall rather than blasting.
-- Existing rows default to enabled (backwards-compatible).
ALTER TABLE accounts ADD COLUMN sync_enabled INTEGER NOT NULL DEFAULT 1;
ALTER TABLE accounts ADD COLUMN send_enabled INTEGER NOT NULL DEFAULT 1;
