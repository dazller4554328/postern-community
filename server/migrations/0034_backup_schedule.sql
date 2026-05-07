-- Backup schedule. Single-row table (id always 1) — there's exactly
-- one schedule per install. A multi-schedule design would be more
-- flexible but harder to reason about retention against, and the v1
-- need is "weekly at 03:00 with 7 backups kept", not split policies.

CREATE TABLE backup_schedule (
    id              INTEGER PRIMARY KEY CHECK (id = 1),
    enabled         INTEGER NOT NULL DEFAULT 0,
    -- 'daily' | 'weekly'. Other values rejected at the storage layer.
    frequency       TEXT NOT NULL DEFAULT 'daily',
    -- Local-time hour and minute the backup should fire. Server's
    -- local timezone — operators usually pick a low-traffic hour and
    -- expect "03:00" to mean their wall-clock 03:00.
    hour            INTEGER NOT NULL DEFAULT 3 CHECK (hour BETWEEN 0 AND 23),
    minute          INTEGER NOT NULL DEFAULT 0 CHECK (minute BETWEEN 0 AND 59),
    -- 0 = Sunday … 6 = Saturday. Only honoured when frequency='weekly'.
    day_of_week     INTEGER NOT NULL DEFAULT 0 CHECK (day_of_week BETWEEN 0 AND 6),
    -- How many local tarballs to keep after a successful backup.
    -- Older ones are deleted. 0 = no automatic pruning.
    retention_count INTEGER NOT NULL DEFAULT 7 CHECK (retention_count >= 0),
    -- Unix seconds of the last successful auto-fire. Used to debounce
    -- — a 60s tick during the "scheduled minute" window must not
    -- fire twice.
    last_run_at     INTEGER
);

INSERT INTO backup_schedule (id, enabled) VALUES (1, 0);
