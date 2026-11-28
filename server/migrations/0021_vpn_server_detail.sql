-- Structured server metadata so the UI can show a confidence-inspiring
-- "Nord · [flag] UK #2347" badge instead of a single generic pill.
-- We parse these out of the NordVPN server record at install time:
--   hostname "uk2347.nordvpn.com" -> code "uk", number 2347
--   station "London" -> city "London"
-- ManualWireGuard leaves them NULL since it has no concept of a
-- standardized server directory.

ALTER TABLE vpn_config ADD COLUMN server_country_code TEXT;
ALTER TABLE vpn_config ADD COLUMN server_number INTEGER;
ALTER TABLE vpn_config ADD COLUMN server_city TEXT;
