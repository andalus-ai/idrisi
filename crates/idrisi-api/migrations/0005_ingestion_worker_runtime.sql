-- Runtime support for the dedicated ingestion worker.
-- The canonical durable job record is created by 0003_ingestion_jobs.sql.
-- Requeue any pre-fencing running jobs before enforcing the lease invariant.
UPDATE ingestion_jobs
SET status = 'queued', stage = 'queued', heartbeat_at = NULL,
    lease_expires_at = NULL, started_at = NULL, updated_at = now()
WHERE status = 'running';

ALTER TABLE ingestion_jobs
    ADD COLUMN lease_owner text,
    ADD COLUMN lease_token uuid,
    ADD COLUMN available_at timestamptz NOT NULL DEFAULT now(),
    ADD COLUMN priority integer NOT NULL DEFAULT 0;

ALTER TABLE ingestion_jobs
    ADD CONSTRAINT ingestion_jobs_lease_owner_nonempty
        CHECK (lease_owner IS NULL OR length(trim(lease_owner)) > 0),
    ADD CONSTRAINT ingestion_jobs_running_lease
        CHECK (
            (status = 'running' AND lease_owner IS NOT NULL AND lease_token IS NOT NULL
                AND lease_expires_at IS NOT NULL)
            OR
            (status <> 'running' AND lease_owner IS NULL AND lease_token IS NULL)
        );

CREATE INDEX ingestion_jobs_available_idx
    ON ingestion_jobs(priority DESC, available_at, queued_at)
    WHERE status IN ('queued', 'running');

GRANT SELECT, UPDATE ON TABLE ingestion_jobs TO idrisi_ingest;
