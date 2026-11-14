# Dataset publication

Idrisi uses a separate publisher process and database role to promote validated OSM imports. The ingestion worker cannot publish data and has no access to the live `tiles` schema.

## Lifecycle

1. An `osm.pbf.import` job downloads, inspects and imports a source into lease-derived schemas.
2. The import job completes with `result_metadata.state = imported_validated_unpublished` and records its data schema in `ingestion_jobs.staging_schema`.
3. An operator enqueues an `osm.dataset.publish` job that references the successful import through `source_job_id`.
4. The publisher re-reads the source job and repeats PostGIS validation against the dataset's catalog coverage.
5. The publisher calls `idrisi_publish_osm_dataset`. This fixed `SECURITY DEFINER` function locks the publish job, source job and dataset; checks the lease token and validated import metadata; creates a dataset-version record; archives previous live tables; promotes the four stable layer names; updates catalog state; and completes the publish job.
6. All database changes occur in one PostgreSQL transaction. An exception rolls back table moves, archive operations, dataset-version writes and job completion together.

## Atomic promotion

The live layer names remain stable:

- `tiles.osm_roads`
- `tiles.osm_places`
- `tiles.osm_buildings`
- `tiles.osm_water`

Existing live tables are moved into the private `idrisi_archive` schema and renamed with the source import job UUID. The validated replacements are moved from the lease-derived data schema into `tiles`, ownership is transferred to the database owner, and SELECT is granted to `martin_tiles`. The old and new schemas are never simultaneously visible under the stable names because all DDL commits atomically.

PostgreSQL may briefly wait for active tile queries to release relation locks during promotion. The operation moves tables and indexes; it does not copy feature rows. Martin auto-publishes only the `tiles` schema.

## Enqueue a publication job

Replace the UUID with the ID of a successful `osm.pbf.import` job:

```sql
INSERT INTO ingestion_jobs (
    dataset_id, job_type, idempotency_key, source_uri,
    importer_name, source_job_id, max_attempts, requested_by
)
SELECT
    dataset_id,
    'osm.dataset.publish',
    'publish-osm-import-<source-job-uuid>',
    source_uri,
    'idrisi-publisher',
    id,
    3,
    'operator'
FROM ingestion_jobs
WHERE id = '<source-job-uuid>'::uuid
  AND job_type = 'osm.pbf.import'
  AND status = 'succeeded'
  AND result_metadata->>'state' = 'imported_validated_unpublished'
ON CONFLICT (idempotency_key) DO NOTHING;
```

The partial unique index prevents more than one queued/running/succeeded publication job for the same source import. Failed publication jobs can be retried with a new idempotency key.

## Database identities

- `idrisi_ingest`: downloads/imports PBFs and writes ingestion-job state. It cannot access `tiles` or `tiles_import`.
- `idrisi_publish`: claims publication jobs, reads the validated staging schema and invokes the fixed publication function. It has no direct DDL privileges on `tiles`.
- `idrisi`: owns the security-definer functions and live/archive tables.
- `martin_tiles`: read-only access to published tables in `tiles`.

Fresh Compose databases create `idrisi_publish` through `infrastructure/postgres/init/10-martin-role.sh`; set `IDRISI_PUBLISH_DB_PASSWORD` in `.env`. The init scripts run only when the Postgres data volume is first initialized.

For an existing database where migration 0008 created the role as NOLOGIN, set its login password once:

```sql
ALTER ROLE idrisi_publish LOGIN PASSWORD '<IDRISI_PUBLISH_DB_PASSWORD>';
```

Apply migrations by restarting the API after deploying the updated image, then start the publisher service. Do not enqueue publication until migration 0008 is applied and the publisher can connect.

## Retention and operational notes

- Previous published tables remain in `idrisi_archive`; archive retention/deletion is a separate maintenance policy.
- A failed publication transaction leaves the current live tables and catalog version unchanged.
- A successful publication updates the source import job to `published`, creates a `dataset_versions` row, marks prior published versions `superseded`, enables the dataset's registered layers and completes the publication job.
- This is the first transactional promotion implementation. A live Martin smoke test and a failure-injection test against PostGIS are required before production use.
