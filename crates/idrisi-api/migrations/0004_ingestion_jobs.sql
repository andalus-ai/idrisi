-- Durable ingestion queue with lease-based worker claims.
CREATE TABLE ingestion_jobs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    job_type text NOT NULL,
    status text NOT NULL DEFAULT 'queued',
    payload jsonb NOT NULL,
    requested_by text,
    priority smallint NOT NULL DEFAULT 0,
    attempts integer NOT NULL DEFAULT 0,
    max_attempts integer NOT NULL DEFAULT 3,
    available_at timestamptz NOT NULL DEFAULT now(),
    locked_by text,
    lease_expires_at timestamptz,
    heartbeat_at timestamptz,
    started_at timestamptz,
    finished_at timestamptz,
    last_error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ingestion_jobs_type_nonempty CHECK (length(trim(job_type)) > 0),
    CONSTRAINT ingestion_jobs_status CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
    CONSTRAINT ingestion_jobs_attempts CHECK (attempts >= 0 AND max_attempts > 0 AND attempts <= max_attempts),
    CONSTRAINT ingestion_jobs_lease CHECK (
      (status = 'running' AND locked_by IS NOT NULL AND lease_expires_at IS NOT NULL)
      OR (status <> 'running' AND locked_by IS NULL AND lease_expires_at IS NULL)
    )
);
CREATE INDEX ingestion_jobs_claim_idx
    ON ingestion_jobs (priority DESC, available_at ASC, created_at ASC)
    WHERE status = 'queued';
CREATE INDEX ingestion_jobs_expired_lease_idx
    ON ingestion_jobs (lease_expires_at)
    WHERE status = 'running';

-- The existing ingestion principal is used by the first worker implementation.
GRANT SELECT, INSERT, UPDATE ON ingestion_jobs TO idrisi_ingest;
