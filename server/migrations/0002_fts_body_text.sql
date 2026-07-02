-- Fix the Sprint 1 FTS5 design. Original `messages_fts` used content='',
-- which stores only the tokenized index — SELECT returns NULL and
-- snippet()/highlight() can't reconstruct context. We replace it with a
-- regular (content-storing) FTS5 so search results can show real
-- context snippets. Also moves body_text onto the `messages` row so
-- rendering + search-metadata paths don't re-parse the blob.

ALTER TABLE messages ADD COLUMN body_text TEXT;

DROP TABLE IF EXISTS messages_fts;

CREATE VIRTUAL TABLE messages_fts USING fts5(
  subject, from_addr, to_addrs, body_text,
  tokenize='porter unicode61'
);

-- Existing rows have NULL body_text — a startup backfill re-parses
-- their RFC822 blobs from the blob store and repopulates this column
-- and the FTS index. See `storage::Db::backfill_body_text`.
