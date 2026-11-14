-- Contact photo storage — feeds the avatar lookup chain so the
-- user's stored photos beat any remote source.
--
-- Stored as raw bytes + the original Content-Type so the avatar
-- endpoint can serve them back without re-encoding. Postern
-- already stores other binary blobs in SQLCipher rows (attachments
-- spill to the blob store, but small per-row blobs like keys live
-- in the row itself), and a contact photo at ~30-100 KB fits the
-- same pattern. Larger uploads are clamped on the HTTP side.
--
-- Both columns are nullable — most contacts will never have a
-- photo set. The avatar endpoint treats NULL as "no contact-side
-- photo, fall through to remote sources".

ALTER TABLE contacts ADD COLUMN photo_blob BLOB;
ALTER TABLE contacts ADD COLUMN photo_mime TEXT;
