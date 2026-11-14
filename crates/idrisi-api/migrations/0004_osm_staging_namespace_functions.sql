-- Create and remove isolated OSM import namespaces under the active job lease.
-- The functions are SECURITY INVOKER: the ingestion DB role must have CREATE
-- privilege on the database. Names are derived exclusively from the fencing UUID.
CREATE OR REPLACE FUNCTION idrisi_create_osm_staging_schemas(p_lease_token uuid)
RETURNS TABLE(data_schema text, middle_schema text)
LANGUAGE plpgsql
AS $$
DECLARE
    v_job_id uuid;
    v_data_schema text;
    v_middle_schema text;
BEGIN
    SELECT id
      INTO v_job_id
      FROM ingestion_jobs
     WHERE lease_token = p_lease_token
       AND status = 'running'
       AND job_type = 'osm.pbf.import'
       AND lease_expires_at > now()
     FOR UPDATE;

    IF v_job_id IS NULL THEN
        RAISE EXCEPTION 'active import lease not found'
            USING ERRCODE = '55000';
    END IF;

    v_data_schema := 'idrisi_stage_' || replace(p_lease_token::text, '-', '');
    v_middle_schema := 'idrisi_mid_' || replace(p_lease_token::text, '-', '');

    -- Do not silently adopt pre-existing schemas: they may contain another
    -- attempt's data or have been created outside the worker's lease.
    IF EXISTS (SELECT 1 FROM pg_namespace WHERE nspname IN (v_data_schema, v_middle_schema)) THEN
        RAISE EXCEPTION 'staging namespace already exists'
            USING ERRCODE = '42P06';
    END IF;

    EXECUTE format('CREATE SCHEMA %I', v_data_schema);
    EXECUTE format('CREATE SCHEMA %I', v_middle_schema);

    RETURN QUERY SELECT v_data_schema, v_middle_schema;
END;
$$;

CREATE OR REPLACE FUNCTION idrisi_drop_osm_staging_schemas(p_lease_token uuid)
RETURNS void
LANGUAGE plpgsql
AS $$
DECLARE
    v_job_id uuid;
    v_data_schema text;
    v_middle_schema text;
BEGIN
    SELECT id
      INTO v_job_id
      FROM ingestion_jobs
     WHERE lease_token = p_lease_token
       AND status = 'running'
       AND job_type = 'osm.pbf.import'
       AND lease_expires_at > now()
     FOR UPDATE;

    IF v_job_id IS NULL THEN
        RAISE EXCEPTION 'active import lease not found'
            USING ERRCODE = '55000';
    END IF;

    v_data_schema := 'idrisi_stage_' || replace(p_lease_token::text, '-', '');
    v_middle_schema := 'idrisi_mid_' || replace(p_lease_token::text, '-', '');

    EXECUTE format('DROP SCHEMA IF EXISTS %I CASCADE', v_data_schema);
    EXECUTE format('DROP SCHEMA IF EXISTS %I CASCADE', v_middle_schema);
END;
$$;

REVOKE ALL ON FUNCTION idrisi_create_osm_staging_schemas(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION idrisi_drop_osm_staging_schemas(uuid) FROM PUBLIC;

COMMENT ON FUNCTION idrisi_create_osm_staging_schemas(uuid) IS
    'Creates UUID-derived OSM staging schemas only for the active, unexpired import lease.';
COMMENT ON FUNCTION idrisi_drop_osm_staging_schemas(uuid) IS
    'Drops UUID-derived OSM staging schemas only for the active, unexpired import lease.';
