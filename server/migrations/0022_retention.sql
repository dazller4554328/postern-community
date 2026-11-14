-- Per-account retention policy: delete messages from the IMAP server
-- after they've been in Postern for X days. Postern's local copy is
-- preserved — this is purely about freeing provider-side quota (e.g.
-- stopping your Gmail inbox from growing without bound).
--
-- Scope is fixed in v1: the sweep only touches INBOX, user-created
-- labels, and Gmail `CATEGORY_*` folders. Sent, Drafts, Starred,
-- Important, Archive, Spam, and Trash are never auto-deleted — the
-- user hasn't opted into losing their own sent mail or drafts by
-- ticking a retention toggle.

ALTER TABLE accounts ADD COLUMN retention_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE accounts ADD COLUMN retention_days    INTEGER NOT NULL DEFAULT 90;
