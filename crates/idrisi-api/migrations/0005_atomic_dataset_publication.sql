-- Atomic catalog publication for validated OSM staging schemas.
-- Requires ingestion job/lease migrations and the lease-fenced staging functions.
ALTER TABLE datasets
    ADD COLUMN IF NOT EXISTS active_version_id uuid;

ALTER TABLE dataset_versions
    ADD COLUMN IF NOT EXISTS published_schema text;

ALTER TABLE dataset_versions
    ADD COLUMN IF NOT EXISTS source_job_id uuid;

ALTER TABLE dataset_versions
    ADD CONSTRAINT dataset_versions_source_job_fk
    FOREIGN KEY (source_job_id) REFERENCES ingestion_jobs(id) ON DELETE RESTRICT;

ALTER TABLE datasets
    ADD CONSTRAINT datasets_active_version_fk
    FOREIGN KEY (active_version_id) REFERENCES dataset_versions(id)
    DEFERRABLE INITIALLY DEFERRED;

CREATE UNIQUE INDEX dataset_versions_source_job_unique
    ON dataset_versions(source_job_id)
    WHERE source_job_id IS NOT NULL;

CREATE TABLE dataset_publication_events (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    dataset_id uuid NOT NULL REFERENCES datasets(id) ON DELETE RESTRICT,
    dataset_version_id uuid NOT NULL REFERENCES dataset_versions(id) ON DELETE RESTRICT,
    publication_job_id uuid NOT NULL UNIQUE REFERENCES ingestion_jobs(id) ON DELETE RESTRICT,
    source_job_id uuid NOT NULL REFERENCES ingestion_jobs(id) ON DELETE RESTRICT,
    previous_version_id uuid REFERENCES dataset_versions(id) ON DELETE RESTRICT,
    published_schema text NOT NULL,
    published_at timestamptz NOT NULL DEFAULT now(),
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT publication_schema_format CHECK (
        published_schema ~ '^idrisi_stage_[0-9a-f]{32}$'
    )
);
CREATE INDEX dataset_publication_events_dataset_idx
    ON dataset_publication_events(dataset_id, published_at DESC);

-- The function is the sole supported promotion path. Its transaction includes
-- version creation, active-pointer swap, supersession, audit event, and job
-- completion. It deliberately leaves physical staging tables untouched.
CREATE OR REPLACE FUNCTION idrisi_publish_osm_dataset(
    p_publication_job_id uuid,
    p_lease_token uuid
) RETURNS jsonb
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public
AS $$
DECLARE
    v_job ingestion_jobs%ROWTYPE;
    v_source ingestion_jobs%ROWTYPE;
    v_dataset datasets%ROWTYPE;
    v_schema text;
    v_version_id uuid;
    v_version text;
    v_previous_version_id uuid;
    v_feature_count bigint;
    v_import jsonb;
    v_event_id uuid;
BEGIN
    SELECT * INTO v_job
      FROM ingestion_jobs
     WHERE id = p_publication_job_id
       AND status = 'running'
       AND job_type = 'osm.dataset.publish'
       AND importer_name = 'idrisi-publisher'
       AND lease_token = p_lease_token
       AND lease_expires_at > clock_timestamp()
     FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'active publication lease not found'
            USING ERRCODE = '55000';
    END IF;

    IF v_job.source_job_id IS NULL THEN
        RAISE EXCEPTION 'publication job has no source import job'
            USING ERRCODE = '23514';
    END IF;

    SELECT * INTO v_source
      FROM ingestion_jobs
     WHERE id = v_job.source_job_id
     FOR UPDATE;

    IF NOT FOUND
       OR v_source.dataset_id <> v_job.dataset_id
       OR v_source.status <> 'succeeded'
       OR v_source.job_type <> 'osm.pbf.import'
       OR v_source.result_metadata->>'state' <> 'imported_validated_unpublished'
    THEN
        RAISE EXCEPTION 'source import is not validated and ready'
            USING ERRCODE = '23514';
    END IF;

    v_schema := v_source.staging_schema;
    IF v_schema IS NULL
       OR v_schema !~ '^idrisi_stage_[0-9a-f]{32}$'
       OR v_source.result_metadata #>> '{staging,data_schema}' <> v_schema
    THEN
        RAISE EXCEPTION 'source staging schema metadata is invalid'
            USING ERRCODE = '23514';
    END IF;

    -- Require the exact reviewed flex tables to still exist. Revalidation of
    -- geometry/coverage is performed by the publisher before this transaction.
    IF to_regclass(format('%I.%I', v_schema, 'osm_roads')) IS NULL
       OR to_regclass(format('%I.%I', v_schema, 'osm_places')) IS NULL
       OR to_regclass(format('%I.%I', v_schema, 'osm_buildings')) IS NULL
       OR to_regclass(format('%I.%I', v_schema, 'osm_water')) IS NULL
    THEN
        RAISE EXCEPTION 'one or more validated staging tables are missing'
            USING ERRCODE = '23514';
    END IF;

    SELECT * INTO v_dataset
      FROM datasets
     WHERE id = v_job.dataset_id
     FOR UPDATE;

    IF NOT FOUND OR v_dataset.default_source_id IS NULL THEN
        RAISE EXCEPTION 'dataset or its default source is missing'
            USING ERRCODE = '23514';
    END IF;

    v_previous_version_id := v_dataset.active_version_id;
    v_import := COALESCE(v_source.result_metadata->'import', '{}'::jsonb);
    v_feature_count := NULLIF(v_import->>'feature_count', '')::bigint;
    v_version := 'ingest-' || replace(v_source.id::text, '-', '');

    INSERT INTO dataset_versions (
        dataset_id, version, status, source_id, source_version,
        source_checksum, coverage, feature_count, published_at,
        metadata, published_schema, source_job_id
    )
    VALUES (
        v_dataset.id, v_version, 'published', v_dataset.default_source_id,
        v_source.importer_version, v_source.source_checksum,
        v_dataset.coverage, v_feature_count, clock_timestamp(),
        jsonb_build_object(
            'publication_job_id', v_job.id,
            'source_job_id', v_source.id,
            'import_validation', v_import,
            'pbf', v_source.result_metadata->'pbf'
        ),
        v_schema, v_source.id
    )
    ON CONFLICT (dataset_id, version) DO NOTHING
    RETURNING id INTO v_version_id;

    IF v_version_id IS NULL THEN
        SELECT id INTO v_version_id
          FROM dataset_versions
         WHERE dataset_id = v_dataset.id
           AND version = v_version
           AND source_job_id = v_source.id;
        IF v_version_id IS NULL THEN
            RAISE EXCEPTION 'version identifier collision'
                USING ERRCODE = '23505';
        END IF;
    END IF;

    IF v_previous_version_id IS NOT NULL
       AND v_previous_version_id <> v_version_id
    THEN
        UPDATE dataset_versions
           SET status = 'superseded'
         WHERE id = v_previous_version_id
           AND dataset_id = v_dataset.id
           AND status = 'published';
    END IF;

    UPDATE datasets
       SET active_version_id = v_version_id,
           status = 'published',
           updated_at = clock_timestamp()
     WHERE id = v_dataset.id;

    INSERT INTO dataset_publication_events (
        dataset_id, dataset_version_id, publication_job_id,
        source_job_id, previous_version_id, published_schema, metadata
    )
    VALUES (
        v_dataset.id, v_version_id, v_job.id,
        v_source.id, v_previous_version_id, v_schema,
        jsonb_build_object('worker_id', v_job.lease_owner)
    )
    RETURNING id INTO v_event_id;

    UPDATE ingestion_jobs
       SET status = 'succeeded',
           stage = 'complete',
           progress_percent = 100,
           result_metadata = jsonb_build_object(
               'dataset_version_id', v_version_id,
               'version', v_version,
               'published_schema', v_schema,
               'publication_event_id', v_event_id,
               'previous_version_id', v_previous_version_id
           ),
           finished_at = clock_timestamp(),
           heartbeat_at = clock_timestamp(),
           lease_owner = NULL,
           lease_token = NULL,
           lease_expires_at = NULL,
           updated_at = clock_timestamp()
     WHERE id = v_job.id
       AND status = 'running'
       AND lease_token = p_lease_token
       AND lease_expires_at > clock_timestamp();

    IF NOT FOUND THEN
        RAISE EXCEPTION 'publication lease expired before commit'
            USING ERRCODE = '55000';
    END IF;

    RETURN jsonb_build_object(
        'dataset_id', v_dataset.id,
        'dataset_version_id', v_version_id,
        'version', v_version,
        'published_schema', v_schema,
        'previous_version_id', v_previous_version_id,
        'publication_event_id', v_event_id,
        'published_at', clock_timestamp()
    );
END;
$$;

REVOKE ALL ON FUNCTION idrisi_publish_osm_dataset(uuid, uuid) FROM PUBLIC;
COMMENT ON FUNCTION idrisi_publish_osm_dataset(uuid, uuid) IS
    'Atomically activates a validated OSM dataset version and completes its publication job under the active fencing lease.';
