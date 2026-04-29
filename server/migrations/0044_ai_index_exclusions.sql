-- Per-install AI indexing exclusions. Two newline-delimited
-- pattern lists: senders (matched against messages.from_addr,
-- with `*` translated to SQL `%` wildcards) and labels (matched
-- against the `labels` table by exact name).
--
-- Use cases:
--   * Skip cPanel / server-monitor / shipping-tracker noise that
--     dominates the corpus but adds nothing to retrieval quality.
--   * Skip Trash / Spam / Gmail Promotions tabs by default.
--
-- Stored as TEXT (newline-separated) so editing is one paste in
-- the Settings UI; alternative would have been a separate table
-- with row-level CRUD which is overkill for a single-user app.
ALTER TABLE ai_settings ADD COLUMN excluded_senders TEXT;
ALTER TABLE ai_settings ADD COLUMN excluded_labels TEXT;
