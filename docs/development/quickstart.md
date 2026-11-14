# Development Quickstart

## Prerequisites

- Stable Rust toolchain (rustup)
- Docker Engine and Docker Compose plugin
- Git

## Run the local stack

Copy `.env.example` to `.env`, change both development passwords, and then run:

    docker compose up --build

The stack starts PostGIS, applies catalog migrations automatically, starts the private Rust domain API, and starts Martin. The API and database development ports bind to loopback only. Martin has no published host port and is reachable only on the private Docker network.

PostGIS first-boot initialization creates a dedicated `martin_tiles` login. The SQL migration creates the `tiles` schema and grants that role read-only access. The role is intentionally separate from the catalog migration owner.

**Existing database volumes:** Docker's `/docker-entrypoint-initdb.d` scripts run only when the database volume is initialized for the first time. If you are upgrading an existing Idrisi volume, create the `martin_tiles` login manually using the value of `MARTIN_DB_PASSWORD`, then apply the migration:

    docker compose exec postgres psql -U idrisi -d idrisi
    CREATE ROLE martin_tiles LOGIN PASSWORD 'replace-with-MARTIN_DB_PASSWORD';
    \q

Then restart the API so SQLx applies pending migrations. Do not reuse the catalog owner's credentials for Martin.

## Catalog API

The initial catalog read API is:
- GET /v1/catalog/datasets
- GET /v1/catalog/datasets/{dataset_id}

For example:

    curl http://127.0.0.1:8080/v1/catalog/datasets

The catalog tracks source license/attribution metadata, datasets and geographic coverage, immutable dataset versions and source checksums, and chart layers. The API is read-only; ingestion and publication workflows are upcoming.

## Vector tiles

Martin discovers spatial tables only in the dedicated `tiles` schema. The schema is initially empty, so there are no data TileJSON or MVT sources until an ingestion pipeline publishes a geometry table there.

The public route contract, once the Iket tile route is loaded, is:

    GET https://<iket-host>/api/idrisi/tiles/{source}
    GET https://<iket-host>/api/idrisi/tiles/{source}/{z}/{x}/{y}

The first endpoint returns TileJSON; the second returns Mapbox Vector Tile protobuf. Martin's internal listener is not exposed to the host. The example Iket route is in `infrastructure/iket/service.yaml.example`.

## Health checks

    curl http://127.0.0.1:8080/health/live
    curl http://127.0.0.1:8080/health/ready

Liveness checks the HTTP process. Readiness checks the live PostGIS connection. The API exits on startup if the database is unavailable or migrations fail.

## Build and test

    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace

## Iket gateway

Iket is maintained separately. Follow `docs/integrations/iket.md` to bootstrap a pinned Iket release and configure its API route to `idrisi-api:8080` and tile route to `martin:3000`. Attach the Iket container to the named Docker network `idrisi-internal` (for example, with `docker network connect idrisi-internal <iket-container>`) or declare it as an external network in the Iket Compose project. Do not expose Idrisi API or Martin publicly.

## Current limitations

This increment adds the Martin service and private tile route contract, but does not yet load Indonesia OSM data, create production tile tables, map catalog layer IDs to Martin source IDs, generate a MapLibre style, or deploy Iket in this Compose file. Routing, geocoding, raster serving, and ingestion orchestration remain upcoming.
