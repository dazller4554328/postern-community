-- One-shot cleanup: plain IMAP accounts shouldn't carry "[Gmail]/Xxx"
-- labels. Those are Gmail's bracketed namespace; mean nothing to a
-- regular IMAP server. Stray copies on IMAP accounts came from a
-- prior `resolve_smart_move` bug that ignored account kind, and from
-- user-defined rules whose `move_to` action_value carried a literal
-- "[Gmail]/Trash" written while looking at the Gmail account.
--
-- The visible symptoms post-2026-05-17:
--   1. The new unified-Empty-Trash dispatch picked up the stale row
--      via the folders listing and tried SELECT "[Gmail]/Trash" on a
--      box that uses '.' as a hierarchy delimiter → IMAP error
--      "Name must not have '/' characters".
--   2. The sidebar double-listed Trash / Spam — the canonical
--      "Trash" entry and a phantom "[Gmail]/Trash" entry.
--
-- This migration:
--   * Ensures the conventional IMAP-side label row exists for each
--     foreign Gmail-namespaced row on IMAP accounts (Trash / Spam /
--     Sent / Drafts).
--   * Re-tags any message that would be orphaned by the strip — if
--     the message's only folder label was a Gmail-namespaced one, it
--     gains the IMAP-side equivalent so it doesn't vanish from the
--     UI.
--   * Drops the foreign label rows. ON DELETE CASCADE on
--     message_labels and sync_state cleans up the join + sync state
--     rows automatically.
--
-- Foreign labels with no IMAP equivalent ([Gmail]/Important,
-- [Gmail]/Starred, [Gmail]/All Mail) are just dropped. Starred state
-- is preserved on the messages.is_starred column; Important / All
-- Mail are Gmail concepts that don't translate.

-- Step 1: Create the native IMAP-side label rows that the re-tag in
-- Step 2 will reference. INSERT OR IGNORE keeps it idempotent if the
-- account already has the row.
INSERT OR IGNORE INTO labels (account_id, name, kind)
SELECT DISTINCT
    a.id,
    CASE l.name
        WHEN '[Gmail]/Trash'     THEN 'Trash'
        WHEN '[Gmail]/Spam'      THEN 'Spam'
        WHEN '[Gmail]/Sent Mail' THEN 'Sent'
        WHEN '[Gmail]/Drafts'    THEN 'Drafts'
    END,
    'system'
FROM accounts a
JOIN labels l ON l.account_id = a.id
WHERE a.kind = 'imap'
  AND l.name IN ('[Gmail]/Trash', '[Gmail]/Spam', '[Gmail]/Sent Mail', '[Gmail]/Drafts');

-- Step 2: Tag the affected messages with the native equivalent so
-- the strip in Step 3 doesn't leave any of them folder-less.
-- INSERT OR IGNORE keeps it safe if the native label was already
-- attached.
INSERT OR IGNORE INTO message_labels (message_id, label_id)
SELECT m.id, l_native.id
FROM messages m
JOIN message_labels ml ON ml.message_id = m.id
JOIN labels l_foreign ON l_foreign.id = ml.label_id
JOIN accounts a ON a.id = m.account_id
JOIN labels l_native ON l_native.account_id = a.id
                     AND l_native.name = CASE l_foreign.name
                         WHEN '[Gmail]/Trash'     THEN 'Trash'
                         WHEN '[Gmail]/Spam'      THEN 'Spam'
                         WHEN '[Gmail]/Sent Mail' THEN 'Sent'
                         WHEN '[Gmail]/Drafts'    THEN 'Drafts'
                     END
WHERE a.kind = 'imap'
  AND l_foreign.name IN ('[Gmail]/Trash', '[Gmail]/Spam', '[Gmail]/Sent Mail', '[Gmail]/Drafts');

-- Step 3: Drop the foreign labels. CASCADE on message_labels and
-- sync_state takes care of the join / sync rows referencing them.
DELETE FROM labels
WHERE id IN (
    SELECT l.id
    FROM labels l
    JOIN accounts a ON a.id = l.account_id
    WHERE a.kind = 'imap' AND l.name LIKE '[Gmail]/%'
);
