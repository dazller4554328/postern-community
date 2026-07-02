-- Sender engagement table — backs the "Curated" view.
--
-- Tracks how often the user has corresponded WITH a given address —
-- either by replying to a thread that included them or by sending
-- a fresh message addressed to them. Both are strong signals that
-- mail FROM that person is worth surfacing in the Curated view.
--
-- We deliberately collapse "replied to" and "sent to fresh" into one
-- counter for v1. The decision-making the user wants ("would I read
-- this?") doesn't really care about the distinction; both confirm
-- two-way correspondence. We can split the counter later if a rule
-- needs to weight reply-context differently from cold sends.
--
-- `sender_addr` is stored lowercased and trimmed but otherwise as
-- the raw address string (no display-name parsing). The producer
-- side (`record_send`) and the join in `list_curated` both pass the
-- string through `lower(trim(...))` so an exact match works without
-- needing a stored-procedure or a generated column.

CREATE TABLE IF NOT EXISTS sender_engagement (
    sender_addr      TEXT PRIMARY KEY,
    engaged_count    INTEGER NOT NULL DEFAULT 0,
    last_engaged_utc INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sender_engagement_count
    ON sender_engagement(engaged_count DESC);
