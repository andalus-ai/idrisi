# Phase 1 — Self-Hosted Infrastructure

## Scope

Phase 1 establishes a reproducible single-host deployment:

- PostgreSQL 16 + PostGIS 3.5
- Rust/Axum Idrisi domain API
- Martin 1.16.1 connected to PostGIS through a dedicated read-only role
- Static MapLibre reference viewer
- Persistent PostgreSQL volume and private Docker network
- SQLx migrations applied by the API at startup
- Iket remains the external ingress and gateway; Martin is not published on the host

This is a development and initial private-deployment baseline, not a claim of production hardening. Replace development credentials and put public ingress behind Iket before exposing the system.

## Canonical Compose file

The repository uses one Compose file: `compose.yaml`. Do not add parallel `docker-compose.yml` or `docker-compose.yaml` variants; divergent manifests make the default `docker compose` behavior ambiguous.

## Prerequisites

- Docker Engine with the Compose plugin
- Git
- Rust stable only when building or testing outside Docker
- At least 8 GB RAM recommended for the initial local stack

## Configure and start

```sh
cp .env.example .env
# Replace every *_PASSWORD value with a unique local secret.
docker compose config --quiet
docker compose up --build -d
docker compose ps
```

The Postgres entrypoint creates the database and least-privilege `martin_tiles` and `idrisi_ingest` roles on first initialization. The API currently runs migrations using the configured database owner connection; production deployments should separate migration credentials from runtime credentials before exposing write-capable APIs.

## Local endpoints

| Component | Local endpoint | Exposure |
|---|---|---|
| Idrisi API | `http://127.0.0.1:8080` | Loopback only |
| MapLibre viewer | `http://127.0.0.1:8090` | Loopback only |
| PostgreSQL | `127.0.0.1:5432` | Loopback only |
| Martin | `martin:3000` | Private Compose network only |

The API port, viewer port, and Postgres port can be changed with `IDRISI_API_PORT`, `MAP_VIEWER_PORT`, and `POSTGRES_PORT`.

## Verify the deployment

```sh
curl --fail http://127.0.0.1:8080/health/live
curl --fail http://127.0.0.1:8080/health/ready
curl --fail http://127.0.0.1:8080/v1/catalog/datasets
docker compose logs --tail=100 postgres idrisi-api martin
```

Readiness requires a working database connection. Liveness reports only that the API process is responding.

Martin has no published host port. Attach the Iket deployment to the `idrisi-internal` Docker network and route the configured tile prefix to `martin:3000`; route domain API paths to `idrisi-api:8080`. Follow [Iket integration](../integrations/iket.md).

## Data persistence and reset

The `idrisi-postgres` named volume persists across container rebuilds. Never run `docker compose down -v` against a deployment containing data unless the database volume is intentionally being destroyed.

For a disposable local database reset only:

```sh
docker compose down -v
docker compose up --build -d
```

Postgres initialization scripts execute only when the data directory is empty. Changing role passwords in `.env` does not rotate credentials inside an existing volume; use SQL `ALTER ROLE` and update dependent service configuration together.

## Rust quality gates

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

## Known boundaries

- The reference viewer is static and requires a reachable Iket tile route and imported data.
- Martin is a private upstream and should not be exposed directly.
- Routing, geocoding, raster serving, metrics aggregation, automated backups, and production secret management are outside Phase 1.
- A first successful container startup does not prove a non-empty vector tile is available; run the OSM pilot import and tile smoke test for that end-to-end validation.
