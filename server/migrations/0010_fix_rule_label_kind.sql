-- Rule-created labels were incorrectly inserted as kind=system.
-- Reclassify any label that isn't a known IMAP system folder.
UPDATE labels SET kind = 'user'
WHERE kind = 'system'
  AND name NOT IN (
    'INBOX',
    'Sent', 'Sent Mail', 'Sent Messages', 'Sent Items',
    'Drafts',
    'Spam', 'Junk',
    'Trash', 'Bin', 'Deleted',
    'Archive',
    'All Mail'
  )
  AND name NOT LIKE '[Gmail]/%'
  AND name NOT LIKE 'CATEGORY_%';
