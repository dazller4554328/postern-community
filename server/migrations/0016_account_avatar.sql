-- Per-account avatar config. avatar_seed drives which robohash image is
-- rendered; NULL falls back to the account's email at the client.
-- avatar_set picks the robohash collection — set1 is classic robots.
ALTER TABLE accounts ADD COLUMN avatar_seed TEXT;
ALTER TABLE accounts ADD COLUMN avatar_set TEXT NOT NULL DEFAULT 'set1';
