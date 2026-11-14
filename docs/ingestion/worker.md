# Phase 2 — Ingestion worker (initial slice)

## What this increment does

The separate `idrisi-worker` process polls the durable PostgreSQL queue, claims only registered job types, renews fenced leases while work is running, and records retryable failures with bounded exponential backoff.

The first registered handler is `osm.pbf.stage`. It downloads an OSM PBF into a persistent, per-job/per-attempt directory, accepts HTTPS only, rejects redirects, enforces an operator-configured host allowlist and byte limit, computes SHA-256, and checks a queued checksum when one is supplied.

## Important boundary

This slice stages and verifies the source artifact only. It does **not** run osm2pgsql, create PostGIS staging tables, publish a dataset version, or update Martin-visible tables. A successful job means the source artifact is safely staged and checksum-verified, not that map data is live. The next worker increment will add isolated import and validation; publication remains a separate transaction.

## Compose

```sh
cp .env.example .env
docker compose up --build -d postgres idrisi-api idrisi-worker
docker compose logs -f idrisi-worker
```

The worker uses the dedicated `idrisi_ingest` role. Its data directory is stored in the `idrisi-ingestion-data` named volume and is not mounted into the API or Martin containers.

## Queue a source-staging job

Use a trusted operator database session. Resolve the dataset ID from its stable slug; never accept SQL or credentials from a job payload. Set `importer_name` to the registered value `geofabrik-pbf-download`.

```sql
INSERT INTO ingestion_jobs (
    dataset_id, job_type, idempotency_key, source_uri, importer_name,
    max_attempts, requested_by
)
SELECT
    id,
    'osm.pbf.stage',
    'osm-java-bandung-stage-2026-11-14',
    'https://download.geofabrik.de/asia/indonesia/java-261112.osm.pbf',
    'geofabrik-pbf-download',
    3,
    'operator'
FROM datasets
WHERE slug = 'osm-bandung-pilot'
ON CONFLICT (idempotency_key) DO NOTHING;
```

Optional `source_checksum` should contain the expected SHA-256 hex digest (or `sha256:<digest>`). The worker will reject a mismatch.

## Worker configuration

| Variable | Default | Purpose |
|---|---:|---|
| `IDRISI_WORKER_ID` | generated | Human-readable worker identity |
| `IDRISI_WORKER_DATA_DIR` | `/var/lib/idrisi/ingestion` | Persistent source artifact root |
| `IDRISI_WORKER_ALLOWED_HOSTS` | `download.geofabrik.de` | Comma-separated exact host allowlist |
| `IDRISI_WORKER_MAX_SOURCE_BYTES` | 20 GiB | Hard download ceiling |
| `IDRISI_WORKER_POLL_SECONDS` | 5 | Empty-queue poll interval |
| `IDRISI_WORKER_LEASE_SECONDS` | 300 | Database lease duration |
| `IDRISI_WORKER_HEARTBEAT_SECONDS` | 30 | Lease renewal cadence |
| `IDRISI_WORKER_REQUEST_TIMEOUT_SECONDS` | 14400 | Maximum source download duration |

## Operational notes

- Queue claims use `FOR UPDATE SKIP LOCKED` and a per-claim UUID fencing token.
- Heartbeat, stage, retry, and completion updates require the current unexpired fencing token.
- Unsupported job types are not claimed by this worker.
- Retry delays use bounded exponential backoff (30 seconds up to 16 minutes).
- A worker stops claiming new jobs on SIGINT/SIGTERM. An in-flight source download is allowed to finish; if the process is forcibly terminated, its lease expires and another worker can reclaim it.
- Each attempt uses a distinct artifact path so a retry never overwrites a previously downloaded file.
- Retention/garbage collection for failed and superseded attempt artifacts is a follow-up operational task.
