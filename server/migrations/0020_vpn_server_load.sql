-- NordVPN returns a server load percentage (0-100) in its
-- recommendations response. We want to surface that in the status
-- badge so "degraded" means *the server is loaded*, not *a random IP
-- probe timed out*. Populated at install/refresh time; stale but
-- refreshed whenever the user hits "Find fastest server".

ALTER TABLE vpn_config ADD COLUMN server_load INTEGER;
