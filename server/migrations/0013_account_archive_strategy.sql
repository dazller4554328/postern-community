-- Per-account archive organization strategy:
--   'single'  — everything in the base archive folder (e.g. Archive/)
--   'yearly'  — Archive/2026/
--   'monthly' — Archive/2026/03/
ALTER TABLE accounts ADD COLUMN archive_strategy TEXT NOT NULL DEFAULT 'single';
