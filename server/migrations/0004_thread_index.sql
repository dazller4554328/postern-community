-- thread_id itself doesn't need a schema change — we're changing *what*
-- we compute for it (root Message-ID instead of per-UID) and backfill
-- existing rows from their stored blobs. What we add here is an index so
-- /api/threads GROUP BY queries don't do full scans.

CREATE INDEX IF NOT EXISTS idx_messages_thread_id_date
  ON messages(thread_id, date_utc DESC);
