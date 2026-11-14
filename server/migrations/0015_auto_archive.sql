-- Age-based auto-archive per account.
--   auto_archive_enabled   — master toggle
--   auto_archive_age_days  — messages older than N days are eligible
--   auto_archive_read_only — if true, only consider read messages (default)
ALTER TABLE accounts ADD COLUMN auto_archive_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE accounts ADD COLUMN auto_archive_age_days INTEGER NOT NULL DEFAULT 180;
ALTER TABLE accounts ADD COLUMN auto_archive_read_only INTEGER NOT NULL DEFAULT 1;
