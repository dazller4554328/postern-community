-- Split the audit_log feed into two categories so the UI can show
-- security events (vault, IP, account changes) separately from
-- server activity (sync cycles, SMTP send, IMAP errors).
ALTER TABLE audit_log ADD COLUMN category TEXT NOT NULL DEFAULT 'security';

CREATE INDEX idx_audit_category_ts ON audit_log(category, ts_utc DESC);
