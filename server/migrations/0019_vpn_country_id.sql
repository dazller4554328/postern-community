-- The "find fastest server" button on the VPN page re-runs the NordVPN
-- recommendations endpoint against the same country the user originally
-- chose. That means we have to remember which country — wg_config_ref
-- alone doesn't tell us (the WG config just has a hostname + key).

ALTER TABLE vpn_config ADD COLUMN country_id INTEGER;
