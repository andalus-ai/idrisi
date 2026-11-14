-- Durable orchestration metadata for asynchronous dataset ingestion.
-- Source files and imported geospatial rows remain outside this catalog schema.
CREATE TABLE ingestion_jobs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    dataset_id uuid NOT NULL REFERENCES datasets(id) ON DELETE RESTRICT,
    dataset_version_id uuid REFERENCES dataset_versions(id) ON DELETE RESTRICT,
    job_type text NOT NULL,
    status text NOT NULL DEFAULT 'queued',
    stage text NOT NULL DEFAULT 'queued',
    idempotency_key text NOT NULL UNIQUE,
    source_uri text NOT NULL,
    source_checksum text,
    source_checksum_algorithm text NOT NULL DEFAULT 'sha256',
    importer_name text NOT NULL,
    importer_version text,
    attempt_count integer NOT NULL DEFAULT 0,
    max_attempts integer NOT NULL DEFAULT 3,
    progress_percent smallint NOT NULL DEFAULT 0,
    staging_schema text,
    result_metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    error_code text,
    error_message text,
    requested_by text,
    queued_at timestamptz NOT NULL DEFAULT now(),
    started_at timestamptz,
    finished_at timestamptz,
    heartbeat_at timestamptz,
    lease_expires_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ingestion_jobs_type_nonempty CHECK (length(trim(job_type)) > 0),
    CONSTRAINT ingestion_jobs_source_uri_nonempty CHECK (length(trim(source_uri)) > 0),
    CONSTRAINT ingestion_jobs_importer_nonempty CHECK (length(trim(importer_name)) > 0),
    CONSTRAINT ingestion_jobs_status CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
    CONSTRAINT ingestion_jobs_stage CHECK (stage IN ('queued', 'acquiring', 'checksum', 'inspecting', 'importing', 'validating', 'publishing', 'complete')),
    CONSTRAINT ingestion_jobs_attempts CHECK (attempt_count >= 0 AND max_attempts > 0 AND attempt_count <= max_attempts),
    CONSTRAINT ingestion_jobs_progress CHECK (progress_percent BETWEEN 0 AND 100),
    CONSTRAINT ingestion_jobs_checksum_algorithm CHECK (source_checksum_algorithm IN ('sha256')),
    CONSTRAINT ingestion_jobs_terminal_time CHECK (
        (status IN ('succeeded', 'failed', 'cancelled') AND finished_at IS NOT NULL)
        OR (status IN ('queued', 'running') AND finished_at IS NULL)
    )
);

CREATE INDEX ingestion_jobs_queue_idx ON ingestion_jobs(queued_at)
    WHERE status = 'queued';
CREATE INDEX ingestion_jobs_dataset_idx ON ingestion_jobs(dataset_id, created_at DESC);
CREATE INDEX ingestion_jobs_lease_idx ON ingestion_jobs(lease_expires_at)
    WHERE status = 'running';
CREATE INDEX ingestion_jobs_version_idx ON ingestion_jobs(dataset_version_id)
    WHERE dataset_version_id IS NOT NULL;
