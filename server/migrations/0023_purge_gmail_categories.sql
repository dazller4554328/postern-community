-- Opt-in "nuclear" purge for Gmail accounts. When both
-- `delete_after_sync` and `purge_gmail_categories` are on, the sync
-- cycle walks the five Gmail categories (Updates / Promotions /
-- Social / Forums / Purchases) via X-GM-RAW, downloads any message
-- not already local, and MOVEs every matched UID to [Gmail]/Trash
-- so the provider-side copy leaves every label and drops into
-- Gmail's 30-day trash lifecycle.
--
-- Hidden from the UI for non-Gmail accounts; the scheduler also
-- guards on account.kind == Gmail before running the pass.

ALTER TABLE accounts ADD COLUMN purge_gmail_categories INTEGER NOT NULL DEFAULT 0;
