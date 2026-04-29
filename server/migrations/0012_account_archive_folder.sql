-- Per-account archive folder. NULL means "use the default for this
-- account's kind" (Gmail → [Gmail]/All Mail; generic IMAP → Archive).
ALTER TABLE accounts ADD COLUMN archive_folder TEXT;
