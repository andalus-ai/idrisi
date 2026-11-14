-- Keep worker-generated OSM imports isolated from Martin's published namespace.
-- Publication will be implemented by a separate lease-fenced publisher in a later phase.
REVOKE ALL ON SCHEMA tiles FROM idrisi_ingest;
GRANT USAGE, CREATE ON SCHEMA tiles_import TO idrisi_ingest;

-- The ingestion worker only needs to read dataset metadata and write its queue state.
REVOKE UPDATE (status, updated_at) ON datasets FROM idrisi_ingest;
REVOKE UPDATE (is_enabled, updated_at) ON layers FROM idrisi_ingest;
REVOKE INSERT ON dataset_versions FROM idrisi_ingest;
GRANT SELECT ON data_sources, datasets, layers, dataset_versions TO idrisi_ingest;
