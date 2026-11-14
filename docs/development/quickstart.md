# Development Quickstart

## Prerequisites

- Stable Rust toolchain (rustup)
- Docker Engine and Docker Compose plugin
- Git
- For OSM ingestion: osm2pgsql 2.x (flex output), psql, curl, md5sum, sha256sum

## Run the local stack

Copy `.env.example` to `.env`, replace all development passwords with unique values, and then run:

    docker compose up --build -d

The stack starts PostGIS, applies catalog migrations automatically, starts the private Rust domain API, Martin, and the static MapLibre reference viewer. The API and database development ports bind to loopback only. Martin has no published host port and is reachable only on the private Docker network.

Postgres first-boot initialization creates separate `martin_tiles` and `idrisi_ingest` roles. Martin is read-only. The ingestion role can write only to the tile/import schemas and publish dataset-version metadata; it is not the API catalog owner or Postgres superuser.

**Existing database volumes:** Docker's `/docker-entrypoint-initdb.d` scripts run only when the database volume is initialized for the first time. If you are upgrading an existing Idrisi volume, create both roles manually using the values of `MARTIN_DB_PASSWORD` and `IDRISI_INGEST_DB_PASSWORD`, then restart the API so pending migrations apply:

    docker compose exec postgres psql -U idrisi -d idrisi
    CREATE ROLE martin_tiles LOGIN PASSWORD 'replace-with-MARTIN_DB_PASSWORD';
    CREATE ROLE idrisi_ingest LOGIN PASSWORD 'replace-with-IDRISI_INGEST_DB_PASSWORD';
    \q

Do not reuse the catalog owner's credentials for Martin or ingestion.

## Catalog API

The initial catalog read API is:
- GET /v1/catalog/datasets
- GET /v1/catalog/datasets/{dataset_id}

For example:

    curl http://127.0.0.1:8080/v1/catalog/datasets

The catalog tracks source license/attribution metadata, datasets and geographic coverage, immutable dataset versions and source checksums, and chart layers.

## Bandung OSM pilot

The first ingestion pipeline is documented in [Bandung OSM pilot](../ingestion/osm-bandung-pilot.md). It uses Geofabrik's Java PBF, imports a buffered Bandung-area bounding box with osm2pgsql flex output, writes normalized tables to the Martin-published `tiles` schema, and records a source checksum/version in the catalog.

Export the ingestion connection string using the credentials in `.env`:

    export IDRISI_INGEST_DATABASE_URL='postgres://idrisi_ingest:<password>@127.0.0.1:5432/idrisi'

Then run:

    bash scripts/ingest/osm-bandung.sh

The script is deliberately one-shot and refuses to overwrite existing pilot tables. It does not yet implement incremental updates or blue/green publication.

## Vector tiles

After a successful import, Martin auto-publishes these sources from the `tiles` schema:

- `osm_roads`
- `osm_places`
- `osm_buildings`
- `osm_water`

The public route contract, once the Iket tile route is loaded, is:

    GET https://<iket-host>/api/idrisi/tiles/{source}
    GET https://<iket-host>/api/idrisi/tiles/{source}/{z}/{x}/{y}

The first endpoint returns TileJSON; the second returns Mapbox Vector Tile protobuf. Martin's internal listener is not exposed to the host. The example Iket route is in `infrastructure/iket/service.yaml.example`.

## MapLibre reference viewer

Open [http://localhost:8090](http://localhost:8090) after starting Compose. The viewer is a static development client; configure the Iket public base URL and bearer token if required. For a separately hosted Iket gateway, enter its full origin plus `/api/idrisi` and configure CORS to allow the viewer origin and the Authorization header. The viewer does not store tokens beyond the current page session. See [viewer documentation](../../examples/maplibre/README.md).

The viewer is expected to show an empty basemap until the Bandung OSM import has completed and Iket can reach Martin. It does not proxy Iket traffic itself.

## Health checks

    curl http://127.0.0.1:8080/health/live
    curl http://127.0.0.1:8080/health/ready

Liveness checks the HTTP process. Readiness checks the live PostGIS connection. The API exits on startup if the database is unavailable or migrations fail.

## Build and test

    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace
    docker compose config --quiet

## Iket gateway

Iket is maintained separately. Follow `docs/integrations/iket.md` to bootstrap a pinned Iket release and configure its API route to `idrisi-api:8080` and tile route to `martin:3000`. Attach the Iket container to the named Docker network `idrisi-internal` (for example, with `docker network connect idrisi-internal <iket-container>`) or declare it as an external network in the Iket Compose project. Do not expose Idrisi API or Martin publicly.

## Current limitations

The Bandung OSM ingestion pipeline and catalog provenance registration are implemented, but no data import has been executed from this development environment. A live non-empty TileJSON/MVT response, MapLibre rendering, Iket end-to-end test, incremental OSM replication, and atomic dataset swaps remain to be demonstrated. Routing, geocoding, raster serving, and other chart domains are still planned.
