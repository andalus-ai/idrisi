# idrisi-ingestion

Rust library for Idrisi-owned ingestion orchestration primitives and provider adapters.

## Current scope

The first adapter is a deterministic `osm2pgsql` invocation builder using flex output. It:

- Produces an argument vector; it never builds a shell command.
- Accepts only validated PostgreSQL identifiers for database, role, and staging schema.
- Keeps database passwords out of command-line arguments and this configuration type.
- Requires the caller to use a reviewed Lua flex style and a job-scoped staging schema.

The worker that acquires PBF files, claims database jobs, invokes the process, heartbeats leases, validates imported data, and publishes dataset versions is not implemented yet. Do not run this adapter directly from an HTTP request handler.

## Credential handling

The process supervisor must inject `PGPASSWORD` from a secret source or mount a protected pgpass file. Never put credentials in importer arguments, job metadata, logs, or this crate's configuration. Prefer a dedicated import role with privileges limited to the generated staging schema.

## Compatibility

Pin the osm2pgsql container/image version and validate the exact flags against that version in CI before enabling imports. Flex style scripts are versioned ingestion recipes and must be tested with representative PBF fixtures.
