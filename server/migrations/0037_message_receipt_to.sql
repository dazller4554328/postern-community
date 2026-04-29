-- Read-receipt support. When an incoming message carries
-- `Disposition-Notification-To: <addr>`, the parser stashes the
-- requested recipient here so the read view can show a "sender
-- requested a read receipt" banner. Sending the MDN is always a
-- manual user action — Postern never auto-sends receipts (privacy
-- stance).
ALTER TABLE messages ADD COLUMN receipt_to TEXT;
