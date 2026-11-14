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

The worker supports `osm.pbf.stage` and `osm.pbf.import`. Both download and inspect allowlisted PBF files. The import handler runs osm2pgsql flex in UUID-derived data and middle schemas, then validates required tables, geometry validity, SRID 4326, and layer extents against dataset coverage. Successful import output remains unpublished and is not visible to Martin.

See [worker operations](worker.md) for deployment, configuration and the operator enqueue example.

## Security rules

- Job rows contain declarative metadata only; never store credentials, SQL, shell fragments or executable paths in a job.
- Worker source hosts are an exact operator-configured allowlist.
- Redirects are disabled to prevent a trusted source URL from redirecting to an untrusted or internal endpoint.
- Each attempt writes to a unique path; a retry never overwrites an earlier artifact.
- Lease tokens fence stale workers from updating job state after another worker reclaims the lease.

## Next work

Dataset publication and Martin-visible table swaps remain a separate, explicitly transactional operation. The worker cannot access the published `tiles` schema; fixed security-definer functions provision and clean up only lease-derived staging schemas.
