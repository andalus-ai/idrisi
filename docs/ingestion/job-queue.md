# Ingestion job queue

## Purpose

Migration `0003_ingestion_jobs.sql` defines the durable catalog queue. Migration `0005_ingestion_worker_runtime.sql` adds lease fencing, priority and delayed retry scheduling. The queue is private control-plane state; jobs are currently enqueued through trusted operator database sessions, not a public API.

## Lifecycle

```text
queued -> running -> succeeded
                  -> queued (retry delay; attempts remain)
                  -> failed (attempts exhausted)
running -> running (expired lease reclaimed with a new fencing token)
```

Each claim increments `attempt_count`, assigns a worker identity and a fresh UUID lease token, and sets the lease expiry. Heartbeat, stage, retry and completion updates require the current unexpired token. A worker that loses its lease must stop work and must not publish output.

Claims use `FOR UPDATE SKIP LOCKED`; workers pass an explicit allowlist of job types so an unsupported worker never claims unrelated work.

## Supported job types

The worker supports `osm.pbf.stage` and `osm.pbf.import`. Both use HTTPS host allowlisting, streamed byte ceilings, SHA-256 verification, and bounded full-file `osmium fileinfo` inspection. The import handler additionally runs osm2pgsql flex into uniquely named tables in the fixed `tiles_import` schema, then validates row counts, geometry validity, SRID 4326, and that layer extents stay inside the dataset's registered coverage. Import jobs re-download the source instead of trusting an arbitrary filesystem path in queue metadata. A successful import is still unpublished and is not visible to Martin.

See [worker operations](worker.md) for deployment, configuration and the operator enqueue example.

## Security rules

- Job rows contain declarative metadata only; never store credentials, SQL, shell fragments or executable paths in a job.
- Worker source hosts are an exact operator-configured allowlist.
- Redirects are disabled to prevent a trusted source URL from redirecting to an untrusted or internal endpoint.
- Each attempt writes to a unique path; a retry never overwrites an earlier artifact.
- Lease tokens fence stale workers from updating job state after another worker reclaims the lease.

## Next work

Dataset publication and Martin-visible table swaps remain a separate, explicitly transactional operation. The worker role has no `USAGE` or `CREATE` privilege on the published `tiles` schema and cannot update dataset/layer publication state.
