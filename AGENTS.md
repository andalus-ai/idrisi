# AGENTS.md — Idrisi

This file defines working rules for humans and coding agents contributing to Idrisi.

## Mission

Build a self-hosted, API-first geospatial platform, Indonesia-first, supporting road, topographic/elevation, satellite/aerial, nautical, aviation, and custom map layers. Keep it reusable and provider-agnostic so applications such as Jezaku integrate through stable APIs rather than depending on internal engines.

## Non-negotiable architecture rules

1. **Rust-first platform, not rewrite-everything.** Use Rust for Idrisi-owned services, domain models, orchestration, adapters, and gateway code. Integrate mature systems such as PostgreSQL/PostGIS, Martin, GDAL, PROJ, Valhalla, Nominatim, and other engines as external processes/services.
2. **Iket owns the gateway layer.** Do not create a parallel Idrisi public gateway or duplicate Iket routing, TLS, authentication, rate-limiting, circuit-breaking, or administration features. Idrisi services own domain-level authorization and geospatial behavior.
3. **Stable provider contracts.** Domain services must depend on Idrisi interfaces, not vendor-specific SDKs or response formats. Keep provider-specific behavior inside adapters.
3. **Separate data classes.** Distinguish source datasets, normalized/derived datasets, operational indexes/graphs, rendered tiles, and user-authored data. Track provenance and versions.
4. **Do not put large data in Git.** No PBF files, raster imagery, tile archives, routing graphs, database dumps, or generated datasets in the repository. Use ignored local data directories, volumes, or object storage.
5. **No silent license assumptions.** Before ingesting, transforming, serving, or redistributing a dataset, record its license, attribution, restrictions, and redistribution conditions.
6. **API-first integration.** Publish versioned OpenAPI contracts. Keep internal implementation details out of public response schemas.
7. **Security by default.** Validate inputs, use parameterized SQL, enforce authorization at the gateway and service boundary where appropriate, avoid logging secrets or precise sensitive user locations, and document trust boundaries.
8. **Offline is a product capability.** Regional packages must be versioned, checksummed, resumable, and independently updateable.
9. **Reproducible operations.** Document configuration, persistent volumes, health checks, backups, restore procedures, and data update workflows.
10. **Measure before optimizing.** Establish correctness tests and baseline latency/resource measurements before introducing caches, custom indexes, or bespoke engines.

## Repository conventions

- Keep crates under `crates/` and deployable Idrisi domain services under `services/`.
- Keep Iket deployment/configuration examples under `infrastructure/iket/`; do not vendor Iket source into Idrisi.
- Put external engine adapters under `integrations/`.
- Put OpenAPI and protobuf definitions under `schemas/`.
- Put operational manifests and database bootstrap assets under `infrastructure/`.
- Architecture decisions belong in `docs/decisions/` as numbered ADRs.
- Roadmap and execution tracking belong in `docs/roadmap/`.
- Use descriptive names and small modules with explicit responsibilities.
- Prefer workspace-level dependency/version management in the root Cargo manifest once the Rust workspace is initialized.
- Keep example configuration free of real credentials. Provide `.env.example`, never commit populated secrets.

## Rust guidance

- Use stable Rust and pin the minimum supported version when the workspace is created.
- Use `Result` and typed errors; do not panic on expected runtime or input failures.
- Avoid `unwrap()` and `expect()` in production paths. They may be acceptable in tests or proven invariants with explanation.
- Use structured tracing with request/correlation IDs; never log access tokens, passwords, or secret keys.
- Validate coordinate ranges, geometry validity, coordinate reference systems, and units at system boundaries.
- Be explicit about coordinate order. GeoJSON uses longitude, latitude; document any engine-specific alternatives.
- Avoid floating-point equality for geometry/business decisions; use suitable tolerances or geospatial predicates.
- Keep async work bounded. Do not block Tokio executor threads with CPU-heavy raster, tile, or graph preprocessing; use dedicated workers/processes and queues.
- Add unit tests for domain logic and adapter contract tests for each provider.

## Geospatial correctness

- Use WGS 84 longitude/latitude (EPSG:4326) as the default interchange CRS unless a contract says otherwise.
- Preserve source CRS metadata and explicitly transform coordinates using PROJ/GDAL or an appropriate library; never relabel coordinates without transforming them.
- Document axis order, units, vertical datum, time validity, and accuracy where relevant.
- Treat elevation as datum-sensitive; do not combine elevation sources with incompatible vertical datums without an explicit transformation.
- Treat nautical and aviation data as safety-relevant. Never market a general-purpose map layer as an authoritative navigation chart or aeronautical source unless its authority, currency, and intended use are established.
- Distinguish map visualization from navigation-grade data and services.
- Use standard formats where appropriate: GeoJSON, GeoPackage, FlatGeobuf, GeoTIFF/COG, MBTiles, PMTiles, MVT, and OGC APIs/standards.
- Test antimeridian, islands, coastal geometry, dateline wrapping, polar/edge tile behavior, and Indonesia's multi-zone geography.

## Data ingestion requirements

Each dataset registration must capture:
- Stable dataset ID and human-readable name
- Provider/source URL or acquisition reference
- License and attribution
- Retrieval timestamp and source version/checksum
- Geographic and temporal coverage
- Source and target CRS, including vertical datum where applicable
- Processing pipeline/version and output format
- Update cadence and retirement policy
- Quality/accuracy notes and known limitations

Ingestion must be idempotent where possible, resumable, observable, and safe to rerun. Separate staging from publication; do not replace a working dataset until validation succeeds.

## API and compatibility

- Prefix Idrisi domain endpoints with `/v1`; Iket exposes a configured public prefix and routes to the private API.
- Iket owns edge routing, TLS, configured gateway authentication/API-key policies, and traffic controls.
- Idrisi must still enforce tenant/resource authorization. Never trust identity headers supplied directly by clients; identity propagation must use a documented trusted mechanism.
- Define request/response schemas and error formats before implementation.
- Use pagination, bounded result sizes, timeouts, and request-size limits.
- Do not expose backend engine URLs, internal IDs, or raw provider errors to untrusted clients.
- Add compatibility tests before changing public contracts.

## Testing and quality gates

For every meaningful change:
1. Run formatting and linting.
2. Run unit tests for changed crates.
3. Run integration/contract tests for affected services.
4. Add regression tests for bugs.
5. Update relevant docs and examples.
6. Check licenses and data attribution if datasets or dependencies change.

Expected Rust commands after the workspace exists:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace`

Do not claim a command passed unless it was actually run.

## Security and privacy

- Secrets come from environment/secret management, never source files.
- Apply least-privilege database roles and separate migration/runtime credentials.
- Use TLS at exposed boundaries and document reverse-proxy assumptions.
- Protect expensive operations such as routing matrices, large spatial queries, tile generation, and exports with quotas and limits.
- Consider location history, geofences, and user-generated places sensitive. Minimize retention and support deletion policies.
- Validate archive paths and uploaded file types; defend against zip-slip, decompression bombs, oversized geometries, SQL injection, and SSRF.
- Keep audit records for administrative changes without storing unnecessary personal data.

## Working approach

- Read the relevant architecture and ADRs before changing service boundaries.
- Prefer the smallest vertical slice that proves an end-to-end capability.
- Do not create placeholder modules that imply functionality exists. Mark planned components clearly.
- If a decision changes scope, licensing, public API, data model, or deployment topology, write/update an ADR.
- When uncertain, document assumptions and trade-offs rather than silently choosing an incompatible design.
