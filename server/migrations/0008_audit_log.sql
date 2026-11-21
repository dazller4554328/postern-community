CREATE TABLE audit_log (
  id         INTEGER PRIMARY KEY,
  ts_utc     INTEGER NOT NULL,
  event_type TEXT NOT NULL,
  detail     TEXT,
  ip         TEXT
);

CREATE INDEX idx_audit_ts ON audit_log(ts_utc DESC);
