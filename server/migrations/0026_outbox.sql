-- Outbox queue that backs both undo-send and send-later.
--
-- Every POST /api/send enqueues into this table; a background worker
-- picks up rows where status='pending' and scheduled_at <= now() and
-- hands them to send_blocking. Undo = DELETE that flips status to
-- 'cancelled' before the worker dispatches. Send-later = the same row
-- with a scheduled_at further in the future.
--
-- Payload is the serialized SendRequest JSON (including attachments as
-- base64). We deliberately store everything needed to resend after
-- arbitrary delays, including app-password-dependent routing — the
-- worker re-fetches the password at dispatch time because the vault
-- may lock/unlock in between enqueue and dispatch.
CREATE TABLE IF NOT EXISTS outbox (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- JSON-serialized SendRequest. See crate::send::SendRequest.
    payload_json    TEXT NOT NULL,
    -- Unix epoch seconds. Dispatch when scheduled_at <= strftime('%s','now').
    scheduled_at    INTEGER NOT NULL,
    -- Lifecycle: pending → sending → (sent | failed). Or pending → cancelled.
    status          TEXT NOT NULL CHECK (status IN ('pending','sending','sent','failed','cancelled')),
    attempts        INTEGER NOT NULL DEFAULT 0,
    -- Set when the most recent dispatch errored. Cleared on success.
    last_error      TEXT,
    -- Short description (to / subject) cached for the outbox list UI
    -- so we don't need to re-parse payload_json for every list render.
    summary_to      TEXT NOT NULL,
    summary_subject TEXT NOT NULL,
    -- Sent message id once SMTP accepted. Null while pending/failed.
    sent_message_id TEXT,
    -- JSON-serialized SendForensics captured at dispatch time. Lets the
    -- compose success card + /outbox UI show VPN/SMTP/Autocrypt details
    -- even after the queue settled.
    forensics_json  TEXT,
    created_at      INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at      INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

-- Hot path: worker scan for "what's due now".
CREATE INDEX IF NOT EXISTS idx_outbox_pending_due
    ON outbox (scheduled_at) WHERE status = 'pending';

-- List view: user browsing their scheduled/failed sends.
CREATE INDEX IF NOT EXISTS idx_outbox_status_created
    ON outbox (status, created_at DESC);
