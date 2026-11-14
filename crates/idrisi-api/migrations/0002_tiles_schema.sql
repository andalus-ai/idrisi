-- Dedicated read-only namespace for Martin-published vector tile tables.
-- The martin_tiles login is provisioned by the Postgres first-boot init script.
CREATE SCHEMA IF NOT EXISTS tiles;
GRANT USAGE ON SCHEMA tiles TO martin_tiles;
GRANT SELECT ON ALL TABLES IN SCHEMA tiles TO martin_tiles;
ALTER DEFAULT PRIVILEGES IN SCHEMA tiles
    GRANT SELECT ON TABLES TO martin_tiles;
