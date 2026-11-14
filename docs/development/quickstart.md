# Development Quickstart

## Prerequisites

- Stable Rust toolchain (rustup)
- Docker Engine and Docker Compose plugin
- Git

## Run the Rust API

From the repository root:

    cargo run -p idrisi-api

The initial domain API exposes:
- GET /health/live
- GET /health/ready

Check it with:

    curl http://127.0.0.1:8080/health/live
    curl http://127.0.0.1:8080/health/ready

The readiness endpoint currently confirms that the HTTP process has initialized. It will be extended with required dependency checks when PostGIS and provider clients are connected.

## Run local dependencies

Copy .env.example to .env and replace the development password before using the stack beyond a local workstation.

    docker compose up --build

The API is exposed on loopback for local development. PostGIS is exposed on loopback for database tooling. Do not bind these development ports to a public interface.

## Build and test

    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace

## Iket gateway

Iket is maintained separately. Follow docs/integrations/iket.md to bootstrap a pinned Iket release and configure its upstream route to idrisi-api:8080. For a separate Compose project, attach Iket to the Idrisi network or use an explicitly configured private network; do not expose the Idrisi API publicly in a production deployment.

## Current limitations

This is the initial service skeleton. It does not yet connect to PostGIS, Martin, Valhalla, or a geocoder, and the health endpoints do not claim those dependencies are ready.
