# Indonesia OSM Ingestion Pipeline

## Purpose and first vertical slice

This document defines the first operational data-ingestion path for Idrisi: import a pinned OpenStreetMap PBF extract into PostGIS through a controlled, repeatable job. It deliberately starts with OSM road/place/map data; imagery, elevation, nautical, aviation, and customer uploads have different licensing and validation requirements and are separate pipelines.

The first pilot should be a small, explicitly bounded Indonesian extract (for example, a city or province selected by the operator), not a country-wide import by default. Promote to a national extract only after storage, import duration, database growth, and recovery have been measured.

## Pipeline contract

`register → acquire → checksum → inspect → stage import → validate → publish`

1. **Register source and dataset.** Record provider, exact source URI, OSM/ODbL license identifier, attribution text, terms URL, region, and update policy in `data_sources` and `datasets`. Do not treat a provider's hosting terms as a replacement for ODbL obligations.
2. **Acquire immutable input.** Download to a job-specific staging directory. Use HTTPS, bounded redirects, a maximum expected file size, timeouts, and resumable transfer where supported. Never accept arbitrary server-side URLs from an unauthenticated caller; this creates an SSRF boundary. For the first release, configure approved provider URLs administratively.
3. **Verify bytes.** Calculate SHA-256 locally and compare it with a trusted published checksum when available. Store the calculated checksum even when the provider does not publish one. A checksum proves byte identity, not authenticity or freshness.
4. **Inspect before import.** Confirm the file is a readable OSM PBF, record replication/extract timestamp and bounds when available, reject empty or implausibly large inputs, and check that its bounds intersect the registered dataset coverage. Record tool versions and inspection output.
5. **Import to staging.** Invoke a pinned `osm2pgsql` build as a separate process/container, not on the Tokio request executor. Use a unique staging schema per job or dataset version. Use explicit connection credentials, process timeouts, CPU/memory/disk limits, and cancellation. Do not import directly into published tables.
6. **Validate.** Check expected tables and geometry columns, SRIDs, non-empty content, feature counts, geometry validity samples, bounds, and representative road/place coverage. Record warnings and validation results. A successful process exit is not sufficient.
7. **Publish atomically.** Only validated output can become a published `dataset_versions` record. Publication must be a short catalog transaction that switches the dataset's active version/reference; retain the previous published version for rollback. Never expose partially imported staging tables through Martin.
8. **Retain or clean up.** Keep source PBFs and published artifacts according to an explicit retention policy. Remove failed staging schemas only after diagnostic metadata is persisted. Do not put PBFs, database dumps, or generated artifacts in Git.

## Job lifecycle

The `ingestion_jobs` table is the durable control-plane record. Workers claim queued jobs, increment `attempt_count`, set `started_at` and a lease, and periodically update `heartbeat_at`/lease expiry. A worker that loses its lease must stop before publication. A recovery process may requeue an expired running job only after confirming that its previous worker is no longer active.

- `status`: queued, running, succeeded, failed, cancelled.
- `stage`: queued, acquiring, checksum, inspecting, importing, validating, publishing, complete.
- `idempotency_key`: caller/job identity; a retry with the same key must return the existing job, not create duplicate work.
- `result_metadata`: bounded structured facts such as counts, bounds, table names, and validation summary; never credentials or full environment dumps.
- `error_code` and `error_message`: sanitized operator-facing diagnostics; raw process output belongs in access-controlled logs with secrets redacted.

A job is not successful until publication commits. If publication fails, the prior published version remains active.

## Durable worker lease repository

The ingestion crate now includes a PostgreSQL `JobRepository` backed by SQLx. It claims queued jobs and reclaims expired leases using a transaction and `FOR UPDATE SKIP LOCKED`, increments the attempt counter, and records a worker owner plus a fresh UUID fencing token. Heartbeats, stage/progress updates, completion, and failure transitions require the active token and an unexpired lease. A stale worker therefore receives `LeaseLost` rather than mutating a job reclaimed by another worker. Jobs whose lease expires after exhausting `max_attempts` are marked failed during the next claim operation.

Migration `0003_ingestion_job_leases.sql` adds the ownership fields and constraints. Existing running jobs are requeued during migration because they cannot safely inherit a lease owned by a process from the old schema. The repository is a control-plane primitive; it is not yet a continuously running worker or a complete ingestion pipeline. Publication must remain a separate, transactionally guarded operation and must check lease ownership immediately before committing.

## OSM PBF structural inspection

The ingestion crate includes an `inspector` adapter for the pinned `osmium-tool` executable. It runs `osmium fileinfo --extended --json` without a shell, applies a hard deadline, suppresses stderr, bounds accepted JSON output, and rejects non-PBF, empty, or incomplete reports. Extended mode reads the entire input, allowing malformed/truncated PBF data to fail before the database importer starts. The normalized result records file size, node/way/relation counts, available data bounds and timestamps, replication timestamp/sequence, and the bounded raw report for provenance.

The executable version must be pinned in the worker image and exercised against real PBF fixtures in CI. The parser intentionally treats bounding-box and timestamp details as optional because OSM extracts may omit them; coverage policy enforcement belongs to the next validation stage. This adapter is not yet called by a durable worker.

## Initial Rust importer adapter

The `idrisi-ingestion` crate now contains an `Osm2pgsqlConfig` and deterministic invocation builder. It creates an argument vector (never a shell string), validates PostgreSQL database/role/staging-schema identifiers, and deliberately excludes passwords from command-line arguments. The caller must inject credentials from a secret source when spawning the child process.

This is an adapter foundation, not a running worker. The job poller, reviewed Lua flex style, staging validation, and atomic publication still need implementation. Acquisition, process supervision, and PBF inspection adapters exist but are not yet wired into a worker. Pin and test the exact osm2pgsql image/CLI before real imports.

## Initial importer contract

Use `osm2pgsql` in a pinned worker image. The orchestration layer owns job lifecycle, provenance, limits, validation, and publication; the importer owns PBF decoding and OSM-to-PostGIS loading. Keep importer invocation behind an adapter so another importer can be evaluated without changing catalog semantics.

For the initial pilot, prefer a documented, explicit output schema and table naming convention. The exact schema must be agreed with Martin's source configuration before the first real import. Avoid exposing raw staging schemas to tile serving.

## Implemented acquisition and process primitives

The `idrisi-ingestion` crate now includes:

- HTTPS-only acquisition with an exact host allowlist, redirects disabled, request deadline, declared and streamed byte limits, create-new destination semantics, partial-file cleanup, and local SHA-256 calculation.
- A supervised `osm2pgsql` child-process runner with a hard deadline, kill-on-timeout behavior, argument-vector invocation, password injection through `PGPASSWORD`, and no stdout/stderr capture by default.

The downloader's SHA-256 is a local content fingerprint. It must still be compared with a trusted provider checksum when one is available. The caller must create and permission the job directory before download. The host allowlist must be operator-configured; never derive it from an untrusted API request.

These primitives are not yet wired to `ingestion_jobs`. A durable worker loop, importer recipe, validation, and publication remain outstanding. PBF structural inspection and database lease/heartbeat primitives exist but are not yet connected. The process runner's timeout and failure behavior still needs execution testing in CI and in the worker container.

## OSM provenance and attribution

At minimum capture:

- Provider and exact extract URL, retrieval time, extract timestamp/replication sequence if supplied.
- SHA-256 checksum, byte size, and PBF bounds.
- License: Open Database License (ODbL) 1.0, with the applicable license URL.
- Attribution statement: `© OpenStreetMap contributors` and a link to the OSM copyright page in user-facing map credits.
- Importer name/version, import options, schema/recipe version, and Idrisi build/commit.
- Dataset region, source coverage, output coverage, feature counts, validation report, and known limitations.

Review the ODbL obligations for the specific database, derivative database, and distribution model before publishing or distributing extracts. This document is an engineering checklist, not legal advice. Provider availability, attribution requirements, and service terms can change; verify them when configuring each source.

## Security and resource controls

- Only trusted administrators may register or change source endpoints in the initial release.
- Do not allow caller-supplied arbitrary URLs, local paths, shell fragments, schema names, or importer flags.
- Pass process arguments as an argument vector; never interpolate user input into a shell command.
- Use a dedicated database role limited to the staging schema. Runtime API and Martin roles must not be able to mutate staging or catalog publication state.
- Enforce file-size, wall-clock, memory, CPU, disk, and concurrent-job limits.
- Use generated schema identifiers; validate identifiers before SQL composition and quote them safely.
- Keep secrets out of job metadata, traces, and importer stdout/stderr.
- Back up the catalog and published data before destructive cleanup or schema promotion.

## Acceptance criteria for the first pilot

- A small Indonesia extract can be acquired and checksum-recorded reproducibly.
- Re-running the same idempotency key does not create a second job.
- Invalid checksum, corrupt PBF, out-of-coverage input, importer failure, and validation failure all leave the active published version unchanged.
- Import writes only to a job-specific staging schema.
- A successful job records dataset version, source provenance, importer version, counts, bounds, and validation summary.
- Martin can read only published tables; staging content is not visible through the tile endpoint.
- The previous version can be restored without re-downloading or re-importing the source.

## Not implemented by the catalog migration alone

The migration supplies durable job metadata only. It does not yet provide a worker, download client, `osm2pgsql` image, job submission API, validation implementation, or atomic schema publication. Those are the next implementation slices and must be tested against a running PostGIS instance before calling the pipeline operational.


## Lease-aware stage supervision

The ingestion crate now provides `worker::run_with_heartbeat`. It wraps a long-running stage future, renews the job lease at a configurable interval, and drops the stage future immediately when the heartbeat fails or the fencing token is rejected. The heartbeat interval must be positive and shorter than the lease duration. Child-process stages must retain `kill_on_drop` so cancellation also stops the external process.

This is a concurrency primitive, not yet the worker poller or end-to-end pipeline. It prevents a common failure mode in which a download, PBF scan, or importer keeps running after another worker has reclaimed the job. The next worker slice should compose claim → acquire → checksum → inspect → import → validate, then add a transactionally fenced handoff to publication. Do not mark a job successful before publication commits.


## Lease-fenced staging namespace functions

Migration `0004_osm_staging_namespace_functions.sql` supplies the two database functions used by the executable worker:

- `idrisi_create_osm_staging_schemas(lease_token)` creates one data schema and one osm2pgsql middle schema, both derived from the active import fencing token.
- `idrisi_drop_osm_staging_schemas(lease_token)` removes only those derived schemas and only while that same token still owns an unexpired `osm.pbf.import` lease.

Both functions are `SECURITY INVOKER`; they do not elevate privileges. The ingestion database role therefore needs database-level `CREATE` privilege to create schemas and ownership/privileges sufficient to drop the schemas it created. Do not grant these functions to the API or tile-serving roles. The worker role must not be a superuser.

The functions lock and verify the job row before performing DDL. A stale worker cannot create or drop a namespace after its lease has been reclaimed. Schema identifiers are generated from the UUID token and passed through PostgreSQL identifier quoting; no job-supplied schema string is executed.

Before deploying this migration, configure a dedicated ingestion role and verify its database/schema privileges in an integration test. The worker's table validator expects the four reviewed flex tables (`osm_roads`, `osm_places`, `osm_buildings`, `osm_water`) in the generated data schema. This migration creates namespaces only; it does not publish them or make them visible to Martin.


## Atomic dataset publication

Migration `0005_atomic_dataset_publication.sql` adds the dataset's `active_version_id`, records the staging schema and source import job on each dataset version, and creates a publication event ledger.

The publisher calls `idrisi_publish_osm_dataset(publication_job_id, lease_token)`. The function locks the publication job, source import job, and dataset; verifies the current unexpired fencing token; verifies that the source job succeeded as an `osm.pbf.import` job with the `imported_validated_unpublished` result; checks the UUID-derived staging schema and the four expected tables; then creates the dataset-version record, supersedes the prior active version, switches the dataset's active-version pointer, writes an audit event, and marks the publication job succeeded in one database transaction.

The active-version pointer is the catalog's authoritative version selector. Readers must resolve the dataset through this pointer, not by selecting the newest row by timestamp. A transaction rollback leaves the previous pointer and version status unchanged. The publication function is idempotent with respect to the source import/version identity; a failed transaction leaves no partial version or audit event.

### Important serving boundary

This migration does **not** expose the staging schema to Martin and does not create public tile sources. The current Martin contract only discovers tables in the dedicated `tiles` schema. The publisher records which immutable staging schema is active; a separate, reviewed tile-serving adapter must resolve that active pointer and expose versioned tables/views before the new data is visible to map clients. Do not grant Martin access to `idrisi_stage_*` schemas or point it at the catalog migration role.

### Required database ownership and grants

The publication function is `SECURITY DEFINER` because it performs a tightly-scoped multi-table catalog transition. Its owner must be the dedicated migration/catalog owner, not the runtime worker. Keep the function's fixed search path, revoke PUBLIC execution, and grant execution only to the publisher role after reviewing the deployment role names. The publisher must not receive direct UPDATE privileges on `datasets`, `dataset_versions`, or publication events. The worker must not receive execute permission on the publication function.

### Acceptance tests before production

1. Publish a valid source import and assert exactly one published version, one audit event, a completed publication job, and an updated active pointer.
2. Publish a replacement and assert the previous version becomes `superseded` while the new one becomes active.
3. Force a failure after version insertion but before job completion and assert the entire transaction rolls back.
4. Attempt publication with an expired, replaced, or unrelated lease token and assert no catalog state changes.
5. Attempt publication for a source job from another dataset, a failed job, a non-import job, or a source without the expected validation marker and assert rejection.
6. Verify Martin's database role cannot read `idrisi_stage_*` schemas and cannot execute the publication function.
7. Verify catalog readers resolve only `datasets.active_version_id`, never the most recent created version.
