-- Per-account flag for whether Archive is surfaced in the UI and
-- eligible for auto-archive. Defaults to enabled — accounts that
-- don't want archive semantics can turn this off explicitly.
ALTER TABLE accounts ADD COLUMN archive_enabled INTEGER NOT NULL DEFAULT 1;
