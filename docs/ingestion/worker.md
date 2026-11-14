# Phase 2 — Ingestion worker (initial slice)

## What this increment does

The separate `idrisi-worker` process polls the durable PostgreSQL queue, claims only registered job types, renews fenced leases while work is running, and records retryable failures with bounded exponential backoff.

The worker supports `osm.pbf.stage` and `osm.pbf.import`. Both download an allowlisted HTTPS PBF, enforce a streamed byte ceiling, calculate SHA-256, optionally verify a queued checksum, and run bounded full-file `osmium fileinfo` inspection. The import handler additionally invokes osm2pgsql flex using the reviewed Bandung profile, rendered into a data schema derived from the current lease token, and validates the resulting PostGIS tables.

## Important boundary

A successful `osm.pbf.stage` job means the PBF was downloaded, checksum-verified when requested, and parsed by osmium. A successful `osm.pbf.import` job means it was imported into lease-derived data and middle schemas and passed PostGIS validation. Neither job publishes data or updates Martin-visible tables. The worker cannot access `tiles` or `tiles_import`; publication remains a separate transaction and role.

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
    importer_version, max_attempts, requested_by
)
SELECT
    dataset.id,
    'osm.pbf.import',
    'osm-bandung-import-v1',
    source.source_uri,
    'osm2pgsql-flex',
    'debian-bookworm',
    3,
    'operator'
FROM datasets AS dataset
JOIN data_sources AS source ON source.id = dataset.default_source_id
WHERE dataset.slug = 'osm-bandung-pilot'
ON CONFLICT (idempotency_key) DO NOTHING;
```

Each lease attempt generates `idrisi_stage_<lease-uuid>` for flex output and `idrisi_mid_<lease-uuid>` for osm2pgsql slim state. Fixed `SECURITY DEFINER` functions create/drop only these UUID-derived names; job payloads cannot supply schema names or DDL. Validation checks all four expected layers, non-empty roads, null/empty/invalid geometry, SRID 4326, and extents inside dataset coverage. Successful staging schemas remain unpublished and invisible to Martin. Failed jobs drop their attempt schemas; hard process loss may leave orphan schemas for retention cleanup.

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
| `IDRISI_OSM_STYLE_PATH` | `/opt/idrisi/osm2pgsql/bandung.lua` | Reviewed flex style template |
| `IDRISI_WORKER_IMPORT_TIMEOUT_SECONDS` | 21600 | Maximum import duration |
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


## Per-job staging namespace preparation

The ingestion crate now exposes `staging::StagingNamespace`. It derives distinct,
PostgreSQL-safe data and osm2pgsql middle-schema identifiers from the job UUID.
Its flex-style renderer redirects the four reviewed `schema = 'tiles'` declarations
in the committed Bandung Lua profile into the job's data schema. It fails closed
if the template no longer contains exactly four declarations.

This namespace primitive is now used by the import handler. The worker can only create/drop its lease-derived schemas through fixed database functions; it cannot create arbitrary schemas or access the published `tiles` namespace.
