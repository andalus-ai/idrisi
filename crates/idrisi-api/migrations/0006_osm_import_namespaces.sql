-- Give the worker no access to the published Martin namespace.
REVOKE ALL ON SCHEMA tiles FROM idrisi_ingest;
REVOKE ALL ON SCHEMA tiles_import FROM idrisi_ingest;

-- Import workers may inspect catalog metadata, but publication writes belong to
-- a separate future publisher with its own fenced transaction.
REVOKE UPDATE (status, updated_at) ON datasets FROM idrisi_ingest;
REVOKE UPDATE (is_enabled, updated_at) ON layers FROM idrisi_ingest;
REVOKE INSERT ON dataset_versions FROM idrisi_ingest;
GRANT SELECT ON data_sources, datasets, layers, dataset_versions TO idrisi_ingest;

-- Create/drop only UUID-derived schemas. The worker cannot submit schema names,
-- SQL fragments, or arbitrary DDL. These SECURITY DEFINER functions run with
-- the migration owner privileges and fix their search_path to pg_catalog.
CREATE OR REPLACE FUNCTION idrisi_create_osm_staging_schemas(p_lease_token uuid)
RETURNS TABLE(data_schema text, middle_schema text)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $$
DECLARE
    token text;
BEGIN
    IF p_lease_token IS NULL THEN
        RAISE EXCEPTION 'lease token is required';
    END IF;
    token := replace(p_lease_token::text, '-', '');
    data_schema := 'idrisi_stage_' || token;
    middle_schema := 'idrisi_mid_' || token;

    EXECUTE format('CREATE SCHEMA %I AUTHORIZATION idrisi_ingest', data_schema);
    EXECUTE format('CREATE SCHEMA %I AUTHORIZATION idrisi_ingest', middle_schema);
    RETURN NEXT;
END;
$$;

CREATE OR REPLACE FUNCTION idrisi_drop_osm_staging_schemas(p_lease_token uuid)
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $$
DECLARE
    token text;
BEGIN
    IF p_lease_token IS NULL THEN
        RAISE EXCEPTION 'lease token is required';
    END IF;
    token := replace(p_lease_token::text, '-', '');
    EXECUTE format('DROP SCHEMA IF EXISTS %I CASCADE', 'idrisi_stage_' || token);
    EXECUTE format('DROP SCHEMA IF EXISTS %I CASCADE', 'idrisi_mid_' || token);
END;
$$;

REVOKE ALL ON FUNCTION idrisi_create_osm_staging_schemas(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION idrisi_drop_osm_staging_schemas(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION idrisi_create_osm_staging_schemas(uuid) TO idrisi_ingest;
GRANT EXECUTE ON FUNCTION idrisi_drop_osm_staging_schemas(uuid) TO idrisi_ingest;
