-- Lockdown mode — a hard server-side kill-switch that blocks every
-- outbound or destructive operation regardless of what the UI or
-- AI surface tries to do. When the flag is on, the user can read
-- mail and ask Datas questions, but cannot send / reply / archive /
-- trash / move / mark-spam / open remote content / dispatch the
-- outbox. AI cannot trigger any of those operations either, since
-- the server enforces the block at the route layer.
--
-- Singleton on app_meta because lockdown is install-wide, not
-- per-account.
ALTER TABLE app_meta ADD COLUMN lockdown_enabled INTEGER NOT NULL DEFAULT 0
  CHECK (lockdown_enabled IN (0, 1));
