# Phase 2 — Ingestion worker (initial slice)

## What this increment does

The separate `idrisi-worker` process polls the durable PostgreSQL queue, claims only registered job types, renews fenced leases while work is running, and records retryable failures with bounded exponential backoff.

The first registered handler is `osm.pbf.stage`. It downloads an OSM PBF into a persistent, per-job/per-attempt directory, accepts HTTPS only, rejects redirects, enforces an operator-configured host allowlist and byte limit, computes SHA-256, checks a queued checksum when one is supplied, then runs `osmium fileinfo --extended --json` against the complete PBF. The `osm.pbf.import` handler repeats source acquisition and inspection, runs the reviewed flex style with unique per-attempt table names in `tiles_import`, and validates the resulting geometries and extents against the dataset's catalog coverage.

## Important boundary

A successful `osm.pbf.stage` job means the PBF was downloaded, checksum-verified (when a checksum was supplied), and parsed by osmium. A successful `osm.pbf.import` job means the file was imported into uniquely named tables in `tiles_import` and passed structural PostGIS validation. Neither job publishes data. The worker has no access to the `tiles` schema; publication remains a separate transaction and role.

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


## Queue an isolated OSM import

The import job derives its bounding box from the registered dataset coverage. It accepts no SQL, Lua, executable path, schema name, or arbitrary command from the job payload. It re-downloads the PBF using the configured source allowlist rather than trusting an operator-supplied local path.

```sql
INSERT INTO ingestion_jobs (
    dataset_id, job_type, idempotency_key, source_uri, importer_name,
    importer_version, staging_schema, max_attempts, requested_by
)
SELECT
    dataset.id,
    'osm.pbf.import',
    'osm-bandung-import-v1',
    source.source_uri,
    'osm2pgsql-flex',
    'debian-bookworm',
    'tiles_import',
    3,
    'operator'
FROM datasets AS dataset
JOIN data_sources AS source ON source.id = dataset.default_source_id
WHERE dataset.slug = 'osm-bandung-pilot'
ON CONFLICT (idempotency_key) DO NOTHING;
```

Each lease attempt generates a distinct table prefix under `tiles_import`. Validation checks required tables, non-empty roads, null/empty/invalid geometry, SRID 4326, and bounds against dataset coverage. The resulting tables remain unpublished and are not visible to Martin.

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
| `IDRISI_OSMIUM_PATH` | `/usr/bin/osmium` | Pinned osmium-tool executable path |
| `IDRISI_WORKER_INSPECTION_TIMEOUT_SECONDS` | 1800 | Maximum full-file PBF inspection duration |
| `IDRISI_OSM2PGSQL_PATH` | `/usr/bin/osm2pgsql` | osm2pgsql flex executable |
| `IDRISI_WORKER_IMPORT_TIMEOUT_SECONDS` | 21600 | Maximum import duration |

## Operational notes

- Queue claims use `FOR UPDATE SKIP LOCKED` and a per-claim UUID fencing token.
- Heartbeat, stage, retry, and completion updates require the current unexpired fencing token.
- Unsupported job types are not claimed by this worker.
- Retry delays use bounded exponential backoff (30 seconds up to 16 minutes).
- A worker stops claiming new jobs on SIGINT/SIGTERM. If a source download is in flight, it cancels the handler, removes that attempt's partial artifact, and releases the job back to the queue without consuming a retry attempt.
- Each attempt uses a distinct artifact path so a retry never overwrites a previously downloaded file.
- PBF inspection reads the complete file, has a configurable deadline, and caps the JSON report at 4 MiB. The runtime image installs `osmium-tool`.
- Retention/garbage collection for failed and superseded attempt artifacts is a follow-up operational task.
