-- Phase 1 of the "Curated" view (sender_engagement table, added in
-- 0054) was removed before phase 2 ever shipped — the user judged
-- the surface area didn't earn its keep. This drops the leftover
-- table from existing installs so the DB doesn't carry empty
-- bloat. New installs effectively run 0054 then 0056 back to back;
-- the net effect is the same as if 0054 had never run.
--
-- IF EXISTS is defensive: a fresh install that never had 0054
-- applied (e.g. a brand-new vault that gets both migrations on
-- first boot) won't fail.

DROP INDEX IF EXISTS idx_sender_engagement_count;
DROP TABLE IF EXISTS sender_engagement;
