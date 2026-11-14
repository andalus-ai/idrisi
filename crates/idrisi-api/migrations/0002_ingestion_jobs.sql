-- Durable ingestion job control plane. Worker ownership/fencing is added by 0003.
CREATE TABLE ingestion_jobs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    dataset_id uuid NOT NULL REFERENCES datasets(id) ON DELETE RESTRICT,
    dataset_version_id uuid REFERENCES dataset_versions(id) ON DELETE RESTRICT,
    job_type text NOT NULL,
    status text NOT NULL DEFAULT 'queued',
    stage text NOT NULL DEFAULT 'queued',
    priority integer NOT NULL DEFAULT 0,
    idempotency_key text,
    source_uri text NOT NULL,
    source_checksum text,
    importer_name text NOT NULL,
    importer_version text,
    attempt_count integer NOT NULL DEFAULT 0,
    max_attempts integer NOT NULL DEFAULT 5,
    progress_percent smallint NOT NULL DEFAULT 0,
    staging_schema text,
    result_metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    error_code text,
    error_message text,
    available_at timestamptz NOT NULL DEFAULT now(),
    queued_at timestamptz NOT NULL DEFAULT now(),
    started_at timestamptz,
    heartbeat_at timestamptz,
    lease_expires_at timestamptz,
    finished_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ingestion_jobs_type_nonempty CHECK (length(trim(job_type)) > 0),
    CONSTRAINT ingestion_jobs_source_nonempty CHECK (length(trim(source_uri)) > 0),
    CONSTRAINT ingestion_jobs_importer_nonempty CHECK (length(trim(importer_name)) > 0),
    CONSTRAINT ingestion_jobs_status CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
    CONSTRAINT ingestion_jobs_stage CHECK (stage IN ('queued', 'acquiring', 'checksum', 'inspecting', 'importing', 'validating', 'publishing', 'complete')),
    CONSTRAINT ingestion_jobs_attempts CHECK (attempt_count >= 0 AND max_attempts > 0 AND attempt_count <= max_attempts),
    CONSTRAINT ingestion_jobs_progress CHECK (progress_percent BETWEEN 0 AND 100),
    CONSTRAINT ingestion_jobs_idempotency_nonempty CHECK (idempotency_key IS NULL OR length(trim(idempotency_key)) > 0)
);
CREATE UNIQUE INDEX ingestion_jobs_idempotency_idx
    ON ingestion_jobs(dataset_id, idempotency_key) WHERE idempotency_key IS NOT NULL;
CREATE INDEX ingestion_jobs_queue_idx
    ON ingestion_jobs(priority DESC, available_at, queued_at, id)
    WHERE status = 'queued';
CREATE INDEX ingestion_jobs_dataset_idx ON ingestion_jobs(dataset_id, created_at DESC);
CREATE INDEX ingestion_jobs_version_idx ON ingestion_jobs(dataset_version_id);
CREATE INDEX ingestion_jobs_lease_expiry_idx
    ON ingestion_jobs(lease_expires_at) WHERE status = 'running';
