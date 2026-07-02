-- Contacts: durable address book that powers recipient autocomplete
-- and (in phase 2) a Settings → Contacts UI.
--
-- One row per unique email address (case-insensitive, RFC be damned —
-- nobody actually uses case-sensitive local parts). The table is fed
-- by the upsert_message hook on every new message synced or sent,
-- and seeded once at boot from the existing messages corpus.
--
-- Keeps it cheap to query for autocomplete: indexed on address +
-- last_seen + message_count so 'most-recently-used first' and
-- 'most-frequently-used first' rankings are both fast.
CREATE TABLE contacts (
  id              INTEGER PRIMARY KEY,
  address         TEXT    NOT NULL UNIQUE COLLATE NOCASE,
  -- Display name extracted from "Name <addr>" headers. NULL when we
  -- only ever saw the bare address. Most-recent header wins on
  -- conflict (handled in the upsert path, not the schema).
  display_name    TEXT,
  -- First / last time this address showed up on any message we
  -- inserted. Updated on every upsert.
  first_seen_utc  INTEGER NOT NULL,
  last_seen_utc   INTEGER NOT NULL,
  -- Bumped on every upsert. Lets the UI rank "most-emailed" contacts
  -- without scanning the messages table.
  message_count   INTEGER NOT NULL DEFAULT 1,
  -- User-driven — toggled from the Contacts UI (phase 2).
  is_favorite     INTEGER NOT NULL DEFAULT 0,
  -- Free-text notes, manual entry only.
  notes           TEXT,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL
);

CREATE INDEX idx_contacts_last_seen
  ON contacts(last_seen_utc DESC);

CREATE INDEX idx_contacts_message_count
  ON contacts(message_count DESC);

-- Favorites get their own index so the UI can pin them without
-- scanning. Partial index keeps the cost ~zero on installs that
-- never favorite anyone.
CREATE INDEX idx_contacts_favorite
  ON contacts(last_seen_utc DESC) WHERE is_favorite = 1;
