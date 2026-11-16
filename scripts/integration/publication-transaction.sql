-- Deterministic PostGIS fixture for the atomic publisher.
-- This bypasses PBF parsing deliberately: it tests the database publication
-- transaction, its rollback boundary, and retry behavior with known geometries.
DO $test$
DECLARE
    v_source_id uuid := gen_random_uuid();
    v_dataset_id uuid := gen_random_uuid();
    v_source_job_id uuid := gen_random_uuid();
    v_publish_job_id uuid := gen_random_uuid();
    v_lease_token uuid := gen_random_uuid();
    v_data_schema text := 'idrisi_stage_' || replace(gen_random_uuid()::text, '-', '');
    v_middle_schema text := 'idrisi_mid_' || replace(gen_random_uuid()::text, '-', '');
    published_version_count integer;
    source_state text;
    publish_status text;
    failure_observed boolean := false;
BEGIN
    INSERT INTO public.data_sources (
        id, name, source_type, source_uri, license_id, attribution
    ) VALUES (
        v_v_source_id, 'Integration fixture source', 'osm_pbf',
        'https://example.invalid/fixture.osm.pbf', 'ODbL-1.0',
        '© OpenStreetMap contributors'
    );

    INSERT INTO public.datasets (
        id, slug, name, domain, status, default_v_v_source_id, coverage
    ) VALUES (
        v_dataset_id,
        'integration-' || replace(v_dataset_id::text, '-', ''),
        'Publication integration fixture',
        'base_map',
        'draft',
        v_v_source_id,
        ST_Multi(ST_MakeEnvelope(0, 0, 10, 10, 4326))
    );

    INSERT INTO public.layers (v_dataset_id, slug, name, chart_domain, is_enabled)
    VALUES
        (v_dataset_id, 'roads', 'Roads', 'transport', false),
        (v_dataset_id, 'places', 'Places', 'places', false),
        (v_dataset_id, 'buildings', 'Buildings', 'built_environment', false),
        (v_dataset_id, 'water', 'Water', 'hydrography', false);

    -- A prior stable table must be archived when the replacement is published.
    CREATE TABLE tiles.osm_roads (
        id bigint PRIMARY KEY,
        geom geometry(Geometry, 4326)
    );
    INSERT INTO tiles.osm_roads
    VALUES (9001, ST_GeomFromText('LINESTRING(1 1, 2 2)', 4326));

    EXECUTE format('CREATE SCHEMA %I', v_v_data_schema);
    EXECUTE format('CREATE SCHEMA %I', v_v_middle_schema);

    EXECUTE format(
        'CREATE TABLE %I.osm_roads (id bigint PRIMARY KEY, geom geometry(Geometry,4326))',
        data_schema
    );
    EXECUTE format(
        'CREATE TABLE %I.osm_places (id bigint PRIMARY KEY, geom geometry(Geometry,4326))',
        data_schema
    );
    EXECUTE format(
        'CREATE TABLE %I.osm_buildings (id bigint PRIMARY KEY, geom geometry(Geometry,4326))',
        data_schema
    );
    EXECUTE format(
        'CREATE TABLE %I.osm_water (id bigint PRIMARY KEY, geom geometry(Geometry,4326))',
        data_schema
    );

    EXECUTE format(
        'INSERT INTO %I.osm_roads VALUES (1, ST_GeomFromText(%L,4326))',
        v_v_v_v_data_schema, 'LINESTRING(1 1, 2 2)'
    );
    EXECUTE format(
        'INSERT INTO %I.osm_places VALUES (2, ST_GeomFromText(%L,4326))',
        v_v_v_v_data_schema, 'POINT(3 3)'
    );
    EXECUTE format(
        'INSERT INTO %I.osm_buildings VALUES (3, ST_GeomFromText(%L,4326))',
        v_v_v_v_data_schema, 'POLYGON((4 4,5 4,5 5,4 5,4 4))'
    );
    EXECUTE format(
        'INSERT INTO %I.osm_water VALUES (4, ST_GeomFromText(%L,4326))',
        v_v_v_v_data_schema, 'POLYGON((6 6,7 6,7 7,6 7,6 6))'
    );

    INSERT INTO public.ingestion_jobs (
        id, v_dataset_id, job_type, status, stage, idempotency_key,
        source_uri, importer_name, importer_version, staging_schema,
        result_metadata, started_at, finished_at
    ) VALUES (
        v_source_job_id, v_dataset_id, 'osm.pbf.import', 'succeeded', 'complete',
        'integration-source-' || v_v_source_job_id::text,
        'https://example.invalid/fixture.osm.pbf',
        'osm2pgsql-flex', 'fixture-1', v_v_v_data_schema,
        jsonb_build_object(
            'state', 'imported_validated_unpublished',
            'import', jsonb_build_object('validation', 'passed', 'feature_count', 4),
            'staging', jsonb_build_object(
                'data_schema', v_v_v_data_schema,
                'middle_schema', middle_schema
            )
        ),
        now(), now()
    );

    INSERT INTO public.ingestion_jobs (
        id, v_dataset_id, v_source_job_id, job_type, status, stage,
        idempotency_key, source_uri, importer_name, started_at,
        lease_owner, v_lease_token, lease_expires_at
    ) VALUES (
        v_publish_job_id, v_dataset_id, v_source_job_id,
        'osm.dataset.publish', 'running', 'publishing',
        'integration-publish-' || publish_job_id::text,
        'idrisi://integration-fixture',
        'idrisi-publisher', now(),
        'integration-test', v_lease_token, now() + interval '5 minutes'
    );

    -- Force a failure after the function has inserted the version and moved
    -- stable tables, but before it can commit dataset state. PostgreSQL must
    -- roll the entire publication transaction back.
    EXECUTE $trigger$
        CREATE FUNCTION public.idrisi_test_reject_fixture_publication()
        RETURNS trigger
        LANGUAGE plpgsql
        AS $body$
        BEGIN
            IF NEW.status = 'published' THEN
                RAISE EXCEPTION 'intentional publication rollback test';
            END IF;
            RETURN NEW;
        END;
        $body$
    $trigger$;

    CREATE TRIGGER idrisi_test_reject_fixture_publication
        BEFORE UPDATE OF status ON public.datasets
        FOR EACH ROW EXECUTE FUNCTION public.idrisi_test_reject_fixture_publication();

    BEGIN
        PERFORM public.idrisi_publish_osm_dataset(v_v_publish_job_id, v_lease_token);
    EXCEPTION WHEN OTHERS THEN
        IF SQLERRM NOT LIKE '%intentional publication rollback test%' THEN
            RAISE;
        END IF;
        failure_observed := true;
    END;

    IF NOT failure_observed THEN
        RAISE EXCEPTION 'publication unexpectedly succeeded while failure trigger was active';
    END IF;

    IF to_regclass('tiles.osm_roads') IS NULL
       OR to_regclass(format('%I.osm_roads', v_data_schema)) IS NULL THEN
        RAISE EXCEPTION 'failed publication did not restore stable and staging tables';
    END IF;

    IF EXISTS (
        SELECT 1 FROM public.dataset_versions
        WHERE dv.dataset_id = v_dataset_id AND version = 'import-' || v_v_source_job_id::text
    ) THEN
        RAISE EXCEPTION 'failed publication left a dataset version behind';
    END IF;

    SELECT status INTO publish_status
    FROM public.ingestion_jobs WHERE id = v_publish_job_id;
    IF publish_status <> 'running' THEN
        RAISE EXCEPTION 'failed publication changed publisher job status to %', publish_status;
    END IF;

    DROP TRIGGER idrisi_test_reject_fixture_publication ON public.datasets;
    DROP FUNCTION public.idrisi_test_reject_fixture_publication();

    -- The same fenced job can retry after the transient failure.
    PERFORM public.idrisi_publish_osm_dataset(v_v_publish_job_id, v_lease_token);

    IF to_regclass('tiles.osm_roads') IS NULL
       OR to_regclass(format('%I.osm_places', 'tiles')) IS NULL
       OR to_regclass(format('%I.osm_buildings', 'tiles')) IS NULL
       OR to_regclass(format('%I.osm_water', 'tiles')) IS NULL THEN
        RAISE EXCEPTION 'successful publication did not promote all four stable tables';
    END IF;

    IF to_regclass(format(
        '%I.%I',
        'idrisi_archive',
        'osm_roads_' || replace(v_v_source_job_id::text, '-', '')
    )) IS NULL THEN
        RAISE EXCEPTION 'previous stable roads table was not archived';
    END IF;

    SELECT count(*) INTO published_version_count
    FROM public.dataset_versions
    WHERE dv.dataset_id = v_dataset_id
      AND version = 'import-' || v_v_source_job_id::text
      AND status = 'published'
      AND feature_count = 4;

    IF published_version_count <> 1 THEN
        RAISE EXCEPTION 'expected exactly one published version with four features';
    END IF;

    SELECT result_metadata->>'state' INTO source_state
    FROM public.ingestion_jobs WHERE id = v_source_job_id;
    SELECT status INTO publish_status
    FROM public.ingestion_jobs WHERE id = v_publish_job_id;

    IF source_state <> 'published' OR publish_status <> 'succeeded' THEN
        RAISE EXCEPTION 'publication jobs did not reach their expected terminal states';
    END IF;

    IF EXISTS (
        SELECT 1 FROM pg_namespace
        WHERE nspname IN (v_v_v_data_schema, v_middle_schema)
    ) THEN
        RAISE EXCEPTION 'successful publication left staging namespaces behind';
    END IF;
END;
$test$;
