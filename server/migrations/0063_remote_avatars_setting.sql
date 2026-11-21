-- Remote sender-avatar lookups (Libravatar/Gravatar, sender-domain
-- icons, DuckDuckGo icons) are a privacy egress: each fetch tells those
-- hosts "this mailbox is viewing mail from this sender". For a
-- privacy-first client that has to be opt-in, not a silent default.
--
-- Default 0 (off): the avatar endpoint serves only locally-stored
-- contact photos and otherwise returns 404 (initials fallback) — no
-- third-party request leaves the box. The user can switch it on from
-- Settings → Privacy.
ALTER TABLE app_meta ADD COLUMN remote_avatars_enabled INTEGER NOT NULL DEFAULT 0;
