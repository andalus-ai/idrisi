# Component Boundaries and Technology Choices

## Platform-owned Rust components

| Component | Responsibility | Initial implementation |
|---|---|---|
| Gateway | Public API, auth integration, quotas, routing | Axum |
| Core | IDs, domain types, errors, shared contracts | Rust library crate |
| Config | Typed configuration and validation | Rust library crate |
| Catalog | Dataset/layer metadata and publication state | Axum service + PostGIS |
| Ingestion coordinator | Jobs, state transitions, provenance | Rust worker/service |
| Tile adapter | Authorization and normalized tile access | Rust adapter + Martin/PMTiles |
| Routing adapter | Stable route/matrix API | Rust adapter + Valhalla |
| Geocoding adapter | Search/reverse provider abstraction | Rust adapter + selected backend |
| Offline service | Package catalog, manifests, integrity checks | Rust service |

## External engines

| Capability | Initial candidate | Alternatives / notes |
|---|---|---|
| Spatial database | PostgreSQL + PostGIS | Keep SQL migrations and extensions explicit |
| Vector tiles | Martin | Evaluate Tegola or other MVT servers if needed |
| Static vector archives | PMTiles | MBTiles may be useful as an intermediate format |
| Raster processing | GDAL | Use COG and reproducible processing recipes |
| CRS transformations | PROJ | Preserve CRS and datum metadata |
| Routing | Valhalla | OSRM and GraphHopper behind adapter contract |
| Geocoding | Nominatim | Evaluate Pelias for multi-source needs |
| Object storage | S3-compatible API | Local filesystem for development only |
| Web map | MapLibre GL JS | Client is replaceable and consumes public contracts |

## Suggested repository structure

```text
idrisi/
├── AGENTS.md
├── README.md
├── Cargo.toml                  # add when workspace implementation starts
├── crates/
│   ├── idrisi-core/
│   ├── idrisi-config/
│   ├── idrisi-geo/
│   ├── idrisi-api/
│   ├── idrisi-auth/
│   └── idrisi-observability/
├── services/
│   ├── gateway/
│   ├── catalog/
│   ├── ingestion/
│   ├── tiles/
│   ├── routing/
│   ├── geocoding/
│   └── offline/
├── integrations/
│   ├── postgis/
│   ├── martin/
│   ├── valhalla/
│   ├── nominatim/
│   ├── gdal/
│   └── object-storage/
├── schemas/
│   ├── openapi/
│   └── examples/
├── infrastructure/
│   ├── compose/
│   ├── postgres/
│   ├── monitoring/
│   └── backup/
├── docs/
│   ├── architecture/
│   ├── decisions/
│   ├── data/
│   ├── operations/
│   └── roadmap/
├── examples/
└── tests/
```

This is a target structure, not a requirement to create every crate or service in the first commit. Start with the smallest vertical slice and split components when independent deployment or ownership justifies it.

## Deployment boundary

The first supported deployment is self-hosted Docker Compose on one Linux host. The gateway should address dependencies by internal service DNS names and environment-based configuration. Do not hard-code hostnames, paths, ports, or credentials in source code.

Kubernetes/Helm is a later packaging target after Compose has stable health checks, persistent-volume semantics, backup/restore, and upgrade behavior.
