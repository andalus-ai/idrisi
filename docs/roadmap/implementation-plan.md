# Idrisi Implementation Plan

## Product decisions

- **Geographic coverage:** Indonesia first.
- **Chart scope:** all domains in the architecture—roads, topography/elevation, satellite/aerial, nautical, aviation, and custom overlays.
- **Deployment:** self-hosted first, with Docker Compose as the initial reproducible deployment target.
- **Implementation approach:** Rust-first platform services with mature external geospatial engines behind provider adapters.

“All chart domains” is the target capability scope, not a promise that every authoritative dataset is immediately available. Each domain is enabled only after source licensing, data quality, processing, and service integration are validated.

## Delivery strategy

Build a vertical slice early: start the private Idrisi API, PostGIS, and Martin; attach Iket as the independently deployed public gateway; register an Indonesia dataset, query PostGIS, serve a vector tile, render it in MapLibre, and expose it through Iket. Then add routing and geocoding, followed by raster/terrain, offline packaging, and specialist chart domains.

## Implementation status (2026-11-14)

- **Foundation:** Rust workspace, health endpoints, CI, PostGIS Compose service, SQLx migrations, and catalog read endpoints are implemented.
- **Catalog:** source, dataset, version, layer, licensing, attribution, and coverage schema is implemented; catalog writes and ingestion are not.
- **Vector tiles:** Martin 1.16.1 is configured as a private Compose service with a dedicated read-only database role; Iket route examples and the tile contract are committed.
- **Not yet demonstrated:** live Compose startup, real tile response, Indonesia OSM import, MapLibre rendering, and Iket end-to-end gateway test. The `tiles` schema is empty until ingestion publishes data.

## Phase 0 — Repository and engineering foundation

**Goal:** create a maintainable project skeleton and clear contribution rules.

Deliverables:
- Root Cargo workspace and shared dependency policy
- Rust formatting, linting, and test conventions
- Configuration and error-handling conventions
- CI for format, clippy, tests, and documentation checks
- Initial ADR set
- `.env.example` and data-volume ignore rules

Acceptance:
- A clean checkout builds the minimal Rust workspace.
- CI runs on pull requests.
- No secrets or large geospatial data are tracked.

## Phase 1 — Local self-hosted stack

**Goal:** boot Idrisi's core dependencies locally.

Deliverables:
- Docker Compose for PostGIS, Martin, and a private Rust domain API skeleton; Iket is deployed independently and attached to the private network
- Database migrations, catalog schema, and a dedicated read-only Martin tile role
- Liveness/readiness endpoints
- Structured logs, request IDs, and basic metrics
- Local developer guide

Acceptance:
- One documented command starts the baseline stack.
- Idrisi API reports PostGIS readiness; Iket routes to the API and Martin through configured private upstreams.
- PostGIS spatial query and a test vector tile work end-to-end.

## Phase 2 — Indonesia OSM ingestion and catalog

**Goal:** ingest and publish a bounded Indonesia pilot dataset.

Deliverables:
- Dataset/layer catalog schema
- Source registration and provenance manifest
- Regional PBF acquisition/staging workflow
- Idempotent import into PostGIS
- Geometry/CRS validation and quality report
- Dataset version publication and rollback metadata

Acceptance:
- Re-running an ingestion job is safe or explicitly rejected without corrupting a published version.
- Every published dataset has source, license, attribution, checksum, coverage, and version metadata.
- A pilot region renders through Martin and MapLibre.

## Phase 3 — Vector maps and styles

**Goal:** provide usable road and place maps.

Deliverables:
- Stable tile API contract and Martin adapter
- Vector tile schema and layer naming conventions
- MapLibre reference web client
- Style configuration and attribution UI
- PMTiles build/export proof of concept

Acceptance:
- Client renders the pilot region with attribution.
- Tile requests are bounded and observable.
- A versioned PMTiles artifact can be opened by a compatible client.

## Phase 4 — Routing and geocoding

**Goal:** deliver common navigation and place discovery capabilities.

Deliverables:
- Valhalla Compose service and graph build pipeline
- Idrisi route/matrix contracts and adapter
- Route profiles enabled only where data supports them
- Nominatim integration proof of concept or documented alternative selection
- Forward/reverse geocoding APIs
- Graph and geocoder version metadata

Acceptance:
- Integration tests calculate known routes in the pilot area.
- Responses identify graph/data version and return normalized geometry/instructions.
- Geocoding results carry required attribution.
- Matrix requests enforce limits and quotas.

## Phase 5 — Raster, terrain, and elevation

**Goal:** add topographic and imagery foundations.

Deliverables:
- GDAL/PROJ processing pipeline
- COG storage and metadata catalog
- Elevation lookup/profile API
- Raster tile provider interface and initial implementation
- Terrain layer style and MapLibre demo

Acceptance:
- A legally usable elevation dataset is processed and served for the pilot region.
- CRS, units, vertical datum, resolution, and source attribution are exposed.
- Raster/terrain rendering is tested at multiple zoom levels.

## Phase 6 — Offline regional packages

**Goal:** make maps portable for disconnected or constrained networks.

Deliverables:
- Regional package manifests and catalog API
- PMTiles package export
- Optional routing graph package export
- Checksums, resumable download, and range-request support
- Package version compatibility and update metadata

Acceptance:
- A client can inspect and download a package.
- Integrity checks detect a corrupt package.
- The reference client can open the offline map without calling the tile service.
- Routing works offline only when a compatible graph package is installed.

## Phase 7 — Nautical, aviation, and custom layers

**Goal:** add specialist chart domains without compromising data governance.

Deliverables:
- Domain-specific provider contracts and metadata
- Source authority, edition/cycle, effective dates, and correction status
- License/attribution review workflow
- Custom layer ingestion with tenant isolation and validation
- Layer access policy and client presentation metadata

Acceptance:
- No layer is published without source and usage metadata.
- Nautical and aviation layers are clearly labeled with authority, currency, and limitations.
- Custom tenant layers cannot be accessed by unauthorized tenants.
- Safety-critical claims are not made without formal validation and governance.

## Phase 8 — Production hardening

**Goal:** make private deployments supportable.

Deliverables:
- Authentication/authorization integration and API key lifecycle
- Rate limits, quotas, abuse protection, and audit events
- Backup/restore automation and dataset rollback
- Metrics, dashboards, tracing, alerts
- Load and resilience testing
- Security and dependency scanning
- Upgrade and recovery runbooks

Acceptance:
- Restore is demonstrated from backup.
- Critical API paths have documented SLOs and measured baselines.
- Failure of an optional provider does not crash unrelated services.
- Security review findings are triaged before production exposure.

## First 2–3 implementation iterations

1. **Iteration A — foundation completed:** Rust workspace, CI, configuration, health endpoints, PostGIS catalog schema, Compose Martin service, read-only tile role, and Iket route examples.
2. **Iteration B — next:** Indonesia pilot OSM acquisition/import, provenance manifest, normalized `tiles` tables, first real TileJSON/MVT response, and MapLibre reference view.
3. **Iteration C:** PMTiles export, Valhalla graph build/integration, route API contract, and end-to-end integration tests.

## Initial pilot recommendation

Start with one urban region rather than loading all Indonesia immediately. Use the pilot to validate dense street networks, POIs, coastline/island handling, routing, tile rendering, and the operational cost of imports. Expand to nationwide OSM coverage only after the pipeline is repeatable and resource requirements are measured.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Dataset license or redistribution incompatibility | Dataset registry, legal review, explicit attribution, source isolation |
| Large imports exceed available memory/storage | Regional pilot, streaming/chunked jobs, capacity benchmarks |
| Backend lock-in | Stable Idrisi provider contracts and adapter contract tests |
| Stale routing graph | Independent graph versioning and atomic publication |
| Incorrect CRS/axis order | Boundary validation and test fixtures |
| Unreliable satellite/nautical/aviation sources | Domain-specific source governance and metadata |
| Offline package drift | Immutable versions, checksums, compatibility manifests |
| Sensitive location data exposure | Data minimization, retention policy, access controls |

## Definition of MVP

The first usable MVP is complete when an operator can self-host Idrisi, import a licensed Indonesia pilot OSM extract, publish and render vector tiles through the gateway, query the dataset catalog, and export a PMTiles region. Routing and geocoding are the next high-value increments, not prerequisites for proving the initial map stack.
