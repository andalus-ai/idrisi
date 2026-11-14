-- Runtime support for the dedicated ingestion worker.
-- The existing catalog migration defines the durable job record; this migration
-- adds worker ownership/fencing and delayed retry scheduling.
ALTER TABLE ingestion_jobs
    ADD COLUMN lease_owner text,
    ADD COLUMN lease_token uuid,
    ADD COLUMN available_at timestamptz NOT NULL DEFAULT now(),
    ADD COLUMN priority integer NOT NULL DEFAULT 0;

CREATE INDEX ingestion_jobs_available_idx
    ON ingestion_jobs(priority DESC, available_at, queued_at)
    WHERE status IN ('queued', 'running');

GRANT SELECT, UPDATE ON TABLE ingestion_jobs TO idrisi_ingest;
