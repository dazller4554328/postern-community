-- JWZ threading step 4: cluster reply-ish messages that lost their
-- References/In-Reply-To headers by normalized subject. We store the
-- key on the message row (cheaper than recomputing it for every
-- lookup) and index (account_id, subject_key) so the upsert-time
-- parent-thread lookup is a point read, not a scan.

ALTER TABLE messages ADD COLUMN subject_key TEXT;

CREATE INDEX IF NOT EXISTS idx_messages_account_subject_key
  ON messages(account_id, subject_key);
