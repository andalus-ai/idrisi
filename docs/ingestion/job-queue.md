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

The first worker binary supports only `osm.pbf.stage`. It downloads an allowlisted HTTPS PBF source, applies a byte limit, computes SHA-256 and stores the artifact under the persistent worker volume. A successful staging job does not mean that the data has been imported or published to Martin.

See [worker operations](worker.md) for deployment, configuration and the operator enqueue example.

## Security rules

- Job rows contain declarative metadata only; never store credentials, SQL, shell fragments or executable paths in a job.
- Worker source hosts are an exact operator-configured allowlist.
- Redirects are disabled to prevent a trusted source URL from redirecting to an untrusted or internal endpoint.
- Each attempt writes to a unique path; a retry never overwrites an earlier artifact.
- Lease tokens fence stale workers from updating job state after another worker reclaims the lease.

## Next work

The next increment adds a separate OSM import handler that writes to a per-job staging schema, followed by geometry and coverage validation. Dataset publication and Martin-visible table swaps remain a separate, explicitly transactional operation.
