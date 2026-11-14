# Development Quickstart

## Prerequisites

- Stable Rust toolchain (rustup)
- Docker Engine and Docker Compose plugin
- Git

## Run the local stack

Copy .env.example to .env and replace the development password before using the stack beyond a local workstation.

    docker compose up --build

The stack starts PostGIS, applies catalog migrations automatically, and starts the private Rust domain API. The API and database development ports bind to loopback only. Do not expose these ports publicly.

## Catalog API

The initial catalog read API is:
- GET /v1/catalog/datasets
- GET /v1/catalog/datasets/{dataset_id}

For example:

    curl http://127.0.0.1:8080/v1/catalog/datasets

The catalog schema tracks data sources and license/attribution metadata, datasets and geographic coverage, immutable dataset versions and source checksums, and chart layers. The initial API is read-only; ingestion and publication workflows will be added in a later iteration.

## Health checks

    curl http://127.0.0.1:8080/health/live
    curl http://127.0.0.1:8080/health/ready

Liveness checks the HTTP process. Readiness checks the live PostGIS connection. The API exits on startup if the database is unavailable or migrations fail.

## Build and test

    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace

## Iket gateway

Iket is maintained separately. Follow docs/integrations/iket.md to bootstrap a pinned Iket release and configure its upstream route to idrisi-api:8080. The Compose stack creates the named Docker network idrisi-internal. Attach the Iket container to that network (for example, with docker network connect idrisi-internal <iket-container>) or declare it as an external network in the Iket Compose project. Do not expose the Idrisi API publicly in a production deployment.

## Current limitations

This increment does not yet implement dataset creation, ingestion jobs, Martin vector tiles, Valhalla routing, geocoding, or Iket deployment in this Compose file. The catalog records metadata and provenance but does not yet contain imported Indonesia OSM features.
