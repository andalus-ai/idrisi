# Idrisi

**Idrisi** is a self-hosted, API-first geospatial platform for maps, spatial data, routing, geocoding, and offline regional map distribution.

The initial deployment target is **Indonesia**. The platform is designed to support multiple chart domains through pluggable data providers and rendering pipelines:

- Road and street maps
- Topographic and elevation maps
- Satellite and aerial imagery
- Nautical and hydrographic charts
- Aviation-related layers and aeronautical data
- Custom overlays and organization-owned spatial datasets

Idrisi is intended to be reusable by applications such as Jezaku without coupling those applications to a particular map vendor or rendering engine.

## Product principles

1. **Self-hosted first** — deployable on a developer workstation, a single server, or private infrastructure.
2. **Indonesia-first, globally extensible** — optimize the first operational dataset and workflows for Indonesia while keeping geographic coverage configurable.
3. **Provider-agnostic** — keep data providers, renderers, routers, and geocoders behind stable Idrisi contracts.
4. **Rust for platform services** — use Rust for Idrisi domain APIs, orchestration, ingestion coordination, and service adapters; reuse Iket for the API gateway.
5. **Reuse mature geospatial engines** — do not rewrite PostGIS, GDAL, PROJ, Martin, Valhalla, Nominatim, or other proven systems without a compelling, measured reason.
6. **Open standards and explicit licensing** — support standard geospatial formats and record provenance, attribution, and usage rights for every dataset.
7. **Offline-capable** — make versioned regional packages and cacheable map assets a first-class capability.
8. **Secure by default** — private deployments, authenticated APIs, tenant-aware authorization, and explicit controls for sensitive datasets.

## Initial architecture

- **API gateway:** [Iket](https://github.com/andalus-ai/iket), a standalone Go gateway for public ingress, TLS, authentication policies, rate limiting, and upstream routing.
- **Idrisi APIs:** Rust domain services behind Iket; Axum may be used internally, but Idrisi does not build a second public gateway.
- **Spatial database:** PostgreSQL + PostGIS for editable and queryable spatial data.
- **Vector tiles:** Martin 1.16.1 as a private PostGIS-backed tile service behind Iket; PMTiles for static, portable regional archives.
- **Raster and terrain:** GDAL/PROJ-based preprocessing and pluggable raster/terrain serving.
- **Routing:** Valhalla as the initial routing engine integration; keep OSRM and GraphHopper as possible alternative providers.
- **Geocoding:** provider interface with Nominatim as a candidate initial self-hosted backend; Pelias remains an alternative.
- **Client rendering:** MapLibre GL JS for the reference web client; mobile SDKs and other clients consume the same APIs.
- **Operations:** Docker Compose for the first reproducible self-hosted deployment, with documented persistent volumes, backups, health checks, and observability.

See [Architecture](docs/architecture/overview.md), [Iket integration](docs/integrations/iket.md), [Implementation Plan](docs/roadmap/implementation-plan.md), and [AGENTS.md](AGENTS.md).

## Repository status

The repository contains the Rust domain API, PostGIS catalog migrations, a private Martin vector-tile service, and Iket route examples. The tile schema is currently empty: no Indonesia OSM or other production map dataset has been imported yet. Routing, geocoding, raster serving, and ingestion orchestration remain planned.

## Development

The first implementation milestone will establish:

- A Rust workspace and shared configuration conventions
- A local Docker Compose environment with PostgreSQL/PostGIS
- A private Rust domain API skeleton and health/readiness endpoints
- OpenAPI contracts and integration test conventions
- A small Indonesia regional data-ingestion proof of concept

Do not download or commit large geographic datasets into Git. Store source files, generated tiles, routing graphs, and raster assets in configured data volumes or object storage.

## Data and attribution

Every imported dataset must have a recorded source, retrieval time, version or checksum, license, attribution text, geographic extent, processing recipe, and update policy. OpenStreetMap data requires appropriate attribution and compliance with the ODbL. Other datasets—including satellite, elevation, nautical, and aviation sources—must be reviewed independently before use or redistribution.

## License

A repository license has not yet been selected. Do not assume that Idrisi code or bundled data is licensed for unrestricted reuse until a license file and dataset-specific notices are added.
