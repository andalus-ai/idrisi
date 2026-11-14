-- A lease token fences stale workers from updating a job after it has been reclaimed.
-- Requeue any pre-lease workers before enforcing the new ownership invariant.
UPDATE ingestion_jobs
SET status = 'queued', stage = 'queued', heartbeat_at = NULL,
    lease_expires_at = NULL, started_at = NULL, updated_at = now()
WHERE status = 'running';

ALTER TABLE ingestion_jobs
    ADD COLUMN lease_owner text,
    ADD COLUMN lease_token uuid;

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

CREATE INDEX ingestion_jobs_claim_idx
    ON ingestion_jobs(status, queued_at, lease_expires_at)
    WHERE status IN ('queued', 'running');
