-- A publication job explicitly references a previously validated import job.
DO $do$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'idrisi_publish') THEN
        CREATE ROLE idrisi_publish NOLOGIN;
    END IF;
END;
$do$;

ALTER TABLE ingestion_jobs
    ADD COLUMN source_job_id uuid REFERENCES ingestion_jobs(id) ON DELETE RESTRICT;

CREATE INDEX ingestion_jobs_source_job_idx
    ON ingestion_jobs(source_job_id)
    WHERE source_job_id IS NOT NULL;

CREATE UNIQUE INDEX ingestion_jobs_publish_source_unique
    ON ingestion_jobs(source_job_id)
    WHERE job_type = 'osm.dataset.publish'
      AND source_job_id IS NOT NULL
      AND status IN ('queued', 'running', 'succeeded');

CREATE SCHEMA IF NOT EXISTS idrisi_archive AUTHORIZATION CURRENT_USER;
ALTER SCHEMA idrisi_archive OWNER TO CURRENT_USER;
GRANT USAGE ON SCHEMA tiles TO martin_tiles;
GRANT USAGE ON SCHEMA public TO idrisi_publish;

GRANT SELECT ON data_sources, datasets, layers, dataset_versions, ingestion_jobs
    TO idrisi_publish;
GRANT UPDATE (
    status, stage, progress_percent, attempt_count, started_at, heartbeat_at,
    lease_expires_at, lease_owner, lease_token, updated_at, available_at,
    finished_at, error_code, error_message, result_metadata
) ON ingestion_jobs TO idrisi_publish;

-- Replace the staging-schema provisioner so the publisher can read validated
-- import tables. New tables created by idrisi_ingest inherit SELECT for publisher.
CREATE OR REPLACE FUNCTION idrisi_create_osm_staging_schemas(p_lease_token uuid)
RETURNS TABLE(data_schema text, middle_schema text)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $fn$
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
    EXECUTE format('GRANT USAGE ON SCHEMA %I TO idrisi_publish', data_schema);
    EXECUTE format(
        'ALTER DEFAULT PRIVILEGES FOR ROLE idrisi_ingest IN SCHEMA %I GRANT SELECT ON TABLES TO idrisi_publish',
        data_schema
    );
    RETURN NEXT;
END;
$fn$;

-- Existing schemas created before this migration also become readable by publisher.
DO $do$
DECLARE
    schema_name text;
BEGIN
    FOR schema_name IN
        SELECT nspname
        FROM pg_namespace
        WHERE nspname ~ '^idrisi_stage_[0-9a-f]{32}$'
    LOOP
        EXECUTE format('GRANT USAGE ON SCHEMA %I TO idrisi_publish', schema_name);
        EXECUTE format('GRANT SELECT ON ALL TABLES IN SCHEMA %I TO idrisi_publish', schema_name);
    END LOOP;
END;
$do$;

CREATE OR REPLACE FUNCTION idrisi_publish_osm_dataset(
    p_publish_job_id uuid,
    p_lease_token uuid
)
RETURNS jsonb
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $fn$
DECLARE
    publish_job public.ingestion_jobs%ROWTYPE;
    source_job public.ingestion_jobs%ROWTYPE;
    dataset_row public.datasets%ROWTYPE;
    data_schema text;
    middle_schema text;
    version_id uuid;
    total_features bigint := 0;
    layer_count bigint;
    empty_count bigint;
    wrong_srid_count bigint;
    invalid_geometry_count bigint;
    west double precision;
    south double precision;
    east double precision;
    north double precision;
    coverage_west double precision;
    coverage_south double precision;
    coverage_east double precision;
    coverage_north double precision;
    layer_name text;
    archive_name text;
    result jsonb;
BEGIN
    SELECT * INTO publish_job
    FROM public.ingestion_jobs
    WHERE id = p_publish_job_id
    FOR UPDATE;

    IF NOT FOUND
       OR publish_job.job_type <> 'osm.dataset.publish'
       OR publish_job.importer_name <> 'idrisi-publisher'
       OR publish_job.status <> 'running'
       OR publish_job.lease_token IS DISTINCT FROM p_lease_token
       OR publish_job.lease_expires_at IS NULL
       OR publish_job.lease_expires_at <= now()
       OR publish_job.source_job_id IS NULL THEN
        RAISE EXCEPTION 'publish job lease or source reference is invalid';
    END IF;

    SELECT * INTO source_job
    FROM public.ingestion_jobs
    WHERE id = publish_job.source_job_id
    FOR UPDATE;

    IF NOT FOUND
       OR source_job.job_type <> 'osm.pbf.import'
       OR source_job.importer_name <> 'osm2pgsql-flex'
       OR source_job.status <> 'succeeded'
       OR source_job.dataset_id <> publish_job.dataset_id
       OR source_job.result_metadata->>'state' IS DISTINCT FROM 'imported_validated_unpublished'
       OR source_job.result_metadata #>> '{import,validation}' IS DISTINCT FROM 'passed' THEN
        RAISE EXCEPTION 'source import job is not a validated unpublished import';
    END IF;

    data_schema := source_job.staging_schema;
    IF data_schema IS NULL
       OR data_schema !~ '^idrisi_stage_[0-9a-f]{32}$'
       OR source_job.result_metadata #>> '{staging,data_schema}' IS DISTINCT FROM data_schema THEN
        RAISE EXCEPTION 'source import staging schema is invalid';
    END IF;

    middle_schema := 'idrisi_mid_' || substring(data_schema FROM length('idrisi_stage_') + 1);
    IF source_job.result_metadata #>> '{staging,middle_schema}' IS DISTINCT FROM middle_schema THEN
        RAISE EXCEPTION 'source import middle schema is invalid';
    END IF;

    SELECT * INTO dataset_row
    FROM public.datasets
    WHERE id = publish_job.dataset_id
    FOR UPDATE;

    IF NOT FOUND OR dataset_row.default_source_id IS NULL OR dataset_row.coverage IS NULL THEN
        RAISE EXCEPTION 'dataset source and coverage must be registered before publication';
    END IF;

    coverage_west := public.ST_XMin(public.Box3D(dataset_row.coverage));
    coverage_south := public.ST_YMin(public.Box3D(dataset_row.coverage));
    coverage_east := public.ST_XMax(public.Box3D(dataset_row.coverage));
    coverage_north := public.ST_YMax(public.Box3D(dataset_row.coverage));

    FOREACH layer_name IN ARRAY ARRAY['osm_roads', 'osm_places', 'osm_buildings', 'osm_water'] LOOP
        IF to_regclass(format('%I.%I', data_schema, layer_name)) IS NULL THEN
            RAISE EXCEPTION 'validated import is missing layer %', layer_name;
        END IF;

        EXECUTE format(
            'SELECT COUNT(*)::bigint, COUNT(*) FILTER (WHERE geom IS NULL OR public.ST_IsEmpty(geom))::bigint, COUNT(*) FILTER (WHERE geom IS NOT NULL AND public.ST_SRID(geom) <> 4326)::bigint, COUNT(*) FILTER (WHERE geom IS NOT NULL AND NOT public.ST_IsValid(geom))::bigint, MIN(public.ST_XMin(public.Box3D(geom))), MIN(public.ST_YMin(public.Box3D(geom))), MAX(public.ST_XMax(public.Box3D(geom))), MAX(public.ST_YMax(public.Box3D(geom))) FROM %I.%I',
            data_schema,
            layer_name
        )
        INTO layer_count, empty_count, wrong_srid_count, invalid_geometry_count,
             west, south, east, north;

        IF empty_count + wrong_srid_count + invalid_geometry_count > 0 THEN
            RAISE EXCEPTION 'imported layer % failed geometry validation', layer_name;
        END IF;
        IF layer_name = 'osm_roads' AND layer_count = 0 THEN
            RAISE EXCEPTION 'validated import has no road features';
        END IF;
        IF west IS NOT NULL AND (
            west < coverage_west - 0.0001
            OR south < coverage_south - 0.0001
            OR east > coverage_east + 0.0001
            OR north > coverage_north + 0.0001
        ) THEN
            RAISE EXCEPTION 'imported layer % lies outside dataset coverage', layer_name;
        END IF;
        total_features := total_features + layer_count;
    END LOOP;

    IF total_features <= 0
       OR source_job.result_metadata #>> '{import,feature_count}' IS DISTINCT FROM total_features::text THEN
        RAISE EXCEPTION 'import feature count does not match validated PostGIS tables';
    END IF;

    UPDATE public.dataset_versions
    SET status = 'superseded'
    WHERE dataset_id = dataset_row.id AND status = 'published';

    INSERT INTO public.dataset_versions (
        dataset_id, version, status, source_id, source_version, source_checksum,
        coverage, feature_count, published_at, metadata
    )
    VALUES (
        dataset_row.id,
        'import-' || source_job.id::text,
        'published',
        dataset_row.default_source_id,
        source_job.importer_version,
        COALESCE(source_job.source_checksum, source_job.result_metadata->>'sha256'),
        dataset_row.coverage,
        total_features,
        now(),
        jsonb_build_object(
            'source_job_id', source_job.id,
            'staging_schema', data_schema,
            'import', source_job.result_metadata->'import',
            'pbf', source_job.result_metadata->'pbf'
        )
    )
    RETURNING id INTO version_id;

    -- Move the previous stable table names out of Martin's schema, then promote
    -- the validated replacement tables in the same transaction.
    FOREACH layer_name IN ARRAY ARRAY['osm_roads', 'osm_places', 'osm_buildings', 'osm_water'] LOOP
        IF to_regclass(format('%I.%I', 'tiles', layer_name)) IS NOT NULL THEN
            EXECUTE format('ALTER TABLE %I.%I SET SCHEMA %I', 'tiles', layer_name, 'idrisi_archive');
            archive_name := layer_name || '_' || replace(source_job.id::text, '-', '');
            EXECUTE format('ALTER TABLE %I.%I RENAME TO %I', 'idrisi_archive', layer_name, archive_name);
        END IF;

        EXECUTE format('ALTER TABLE %I.%I SET SCHEMA %I', data_schema, layer_name, 'tiles');
        EXECUTE format('ALTER TABLE %I.%I OWNER TO CURRENT_USER', 'tiles', layer_name);
        EXECUTE format('GRANT SELECT ON TABLE %I.%I TO martin_tiles', 'tiles', layer_name);
    END LOOP;

    EXECUTE format('DROP SCHEMA IF EXISTS %I CASCADE', middle_schema);
    EXECUTE format('DROP SCHEMA IF EXISTS %I CASCADE', data_schema);

    UPDATE public.datasets
    SET status = 'published', updated_at = now()
    WHERE id = dataset_row.id;

    UPDATE public.layers
    SET is_enabled = true, updated_at = now()
    WHERE dataset_id = dataset_row.id;

    UPDATE public.ingestion_jobs
    SET dataset_version_id = version_id,
        result_metadata = jsonb_set(
            jsonb_set(result_metadata, '{state}', '"published"'::jsonb),
            '{publication}',
            jsonb_build_object('dataset_version_id', version_id, 'published_at', now())
        ),
        updated_at = now()
    WHERE id = source_job.id;

    result := jsonb_build_object(
        'state', 'published',
        'dataset_id', dataset_row.id,
        'dataset_version_id', version_id,
        'source_job_id', source_job.id,
        'published_layers', ARRAY['osm_roads', 'osm_places', 'osm_buildings', 'osm_water'],
        'feature_count', total_features
    );

    UPDATE public.ingestion_jobs
    SET status = 'succeeded', stage = 'complete', progress_percent = 100,
        dataset_version_id = version_id, result_metadata = result,
        finished_at = now(), heartbeat_at = now(),
        lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL,
        updated_at = now()
    WHERE id = publish_job.id;

    RETURN result;
END;
$fn$;

REVOKE ALL ON FUNCTION idrisi_create_osm_staging_schemas(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION idrisi_create_osm_staging_schemas(uuid) TO idrisi_ingest;
REVOKE ALL ON FUNCTION idrisi_publish_osm_dataset(uuid, uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION idrisi_publish_osm_dataset(uuid, uuid) TO idrisi_publish;
