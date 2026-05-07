-- One-shot data cleanup for messages affected by the
-- delete_after_sync label-mirror bug: when a Gmail message had been
-- MOVE'd to [Gmail]/Trash by the streaming sync, the local DB
-- accumulated *both* the source-folder label (e.g. INBOX) and
-- [Gmail]/Trash because the next sync over Trash re-tagged the
-- message instead of replacing labels. Result: messages showed
-- under both Inbox and Trash views in the Postern UI even though
-- Gmail had only the Trash label.
--
-- Fix forward landed in the same release; this migration cleans up
-- the historical state. Drops only system folder labels, never user
-- labels — a real user-applied label survives Trash on Gmail's side
-- and we don't want to nuke it locally.

DELETE FROM message_labels
WHERE message_id IN (
    SELECT ml.message_id
    FROM message_labels ml
    JOIN labels lt ON lt.id = ml.label_id AND lt.name = '[Gmail]/Trash'
    JOIN messages m ON m.id = ml.message_id
    JOIN accounts a ON a.id = m.account_id
    WHERE a.kind = 'gmail'
)
AND label_id IN (
    SELECT id FROM labels
    WHERE name IN (
        'INBOX',
        '[Gmail]/Sent Mail',
        '[Gmail]/Drafts',
        '[Gmail]/Spam',
        '[Gmail]/All Mail',
        'CATEGORY_PERSONAL',
        'CATEGORY_SOCIAL',
        'CATEGORY_PROMOTIONS',
        'CATEGORY_UPDATES',
        'CATEGORY_FORUMS'
    )
);
