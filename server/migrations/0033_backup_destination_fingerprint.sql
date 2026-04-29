-- TOFU (trust-on-first-use) hostkey verification for SFTP destinations.
--
-- Stores the SHA-256 of the server's SSH public key as we saw it on
-- the first successful connect. Subsequent connects must present a
-- key that hashes to the same value or Postern refuses to authenticate.
--
-- Format matches `ssh-keygen -lf`: `SHA256:<base64-no-pad>`.
-- NULL = TOFU mode: capture and persist on next successful connect.

ALTER TABLE backup_destinations
    ADD COLUMN server_fingerprint TEXT;
