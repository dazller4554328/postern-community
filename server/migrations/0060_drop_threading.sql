-- Remove conversation threading from the schema.
--
-- ThreadView and the threads codepath were ripped out 2026-05-14 —
-- click-on-sender already covers "see everything from this person",
-- and the JWZ-style grouping never converged cleanly enough to be
-- worth maintaining. This drops both columns that fed it (`thread_id`
-- and `subject_key`) and the two indexes that lived on them.
--
-- DROP COLUMN requires SQLite >= 3.35 (March 2021). The bundled
-- SQLCipher (rusqlite "bundled-sqlcipher-vendored-openssl") ships
-- with a recent SQLite, so this is safe in production. We don't do
-- the table-rebuild dance because `messages` has a FK from
-- `message_labels` with ON DELETE CASCADE — dropping the table would
-- wipe every label association.

DROP INDEX IF EXISTS idx_messages_thread;
DROP INDEX IF EXISTS idx_messages_thread_id_date;
DROP INDEX IF EXISTS idx_messages_account_subject_key;

ALTER TABLE messages DROP COLUMN thread_id;
ALTER TABLE messages DROP COLUMN subject_key;
