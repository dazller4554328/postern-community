-- Optional follow-up to the nuclear category purge: after the purge
-- has moved every matched UID into [Gmail]/Trash, also permanently
-- delete everything currently sitting in Trash (STORE +FLAGS \Deleted
-- + EXPUNGE on the Trash mailbox). The practical effect is "skip the
-- 30-day trash timer and free Gmail quota now".
--
-- Caveat we surface in the UI: this wipes the *entire* Trash, not just
-- the messages we just moved in, so any manually-trashed mail from
-- Gmail's web UI also goes permanently. Users opt in knowing that.

ALTER TABLE accounts ADD COLUMN skip_gmail_trash INTEGER NOT NULL DEFAULT 0;
