# Ingestion job queue

## Purpose

Migration `0004_ingestion_jobs.sql` adds a durable PostgreSQL queue for long-running ingestion work. The queue is private control-plane state: it is not exposed through the public API yet. The first worker integration reuses the dedicated `idrisi_ingest` database principal.

## Job lifecycle

```text
queued -> running -> succeeded
                  -> queued (retry delay, attempts remain)
                  -> failed (attempts exhausted)
queued -> cancelled (operator action; worker cancellation is not yet wired)
running -> running (expired lease reclaimed by another worker)
```

Each claim increments `attempts` and assigns a worker ID plus lease expiry. Workers must heartbeat during long operations. Completion, heartbeat, and failure updates are fenced by both job ID and worker ID and require an unexpired lease. A worker that loses its lease must stop mutating outputs; it must not publish.

The claim query uses `FOR UPDATE SKIP LOCKED`, so multiple workers can poll concurrently without claiming the same queued row. Expired jobs are eligible for reclaim while attempts remain. Expired jobs whose final attempt was already consumed are moved to terminal `failed` state during the next claim poll.

## Enqueue a pilot job (operator-only)

Until an authenticated administrative API exists, enqueue through a trusted database session only:

```sql
INSERT INTO ingestion_jobs (job_type, payload, requested_by, priority, max_attempts)
VALUES (
  'osm.pbf.import',
  jsonb_build_object(
    'dataset_slug', 'osm-bandung-pilot',
    'source_url', 'https://download.geofabrik.de/asia/indonesia/java-261112.osm.pbf',
    'bbox', '107.35,-7.15,107.85,-6.75'
  ),
  'operator',
  10,
  3
)
RETURNING id;
```

The queue stores declarative job metadata, not credentials or arbitrary shell commands. A worker must allowlist supported `job_type` values, validate payload schemas, enforce source-host and byte limits, and resolve credentials from its environment/secret store. Never accept executable paths, SQL, shell fragments, or database passwords from job payloads.

## Rust queue primitives

The `idrisi-ingestion::queue` module provides:

- `claim_next(pool, worker_id, lease)`
- `heartbeat(pool, job_id, worker_id, lease)`
- `complete(pool, job_id, worker_id)`
- `fail_or_retry(pool, job_id, worker_id, error, retry_delay)`

Boolean mutation results indicate whether the caller still owned a valid lease. A `false` result means the lease expired or another worker reclaimed the job; the caller must treat itself as fenced out.

## Current boundary and next work

This increment provides the durable schema and concurrency-safe queue primitives. It does **not** yet include an always-on worker binary, API submission/status endpoints, cancellation signaling, per-stage progress events, or a transactional publication/swap implementation. The existing Bandung one-shot script remains the only end-to-end import runner; do not enqueue jobs until a worker handler for `osm.pbf.import` is deployed.
