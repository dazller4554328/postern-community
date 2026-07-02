-- Per-account display colour. Drives the unread-indicator pill in the
-- inbox row + any other place we want to surface "this is a Gmail
-- message vs Work message" at a glance. NULL = use a deterministic
-- default derived client-side from account.id, so existing rows keep
-- working without a backfill.
ALTER TABLE accounts ADD COLUMN color TEXT;
