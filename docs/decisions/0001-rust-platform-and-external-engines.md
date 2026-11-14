# ADR 0001: Rust Platform Services with External Geospatial Engines

- Status: Accepted
- Date: 2026-11-14

## Context

Idrisi should be Rust-first and reusable, but the geospatial ecosystem already provides mature engines for spatial databases, tile serving, raster processing, coordinate transformations, routing, and geocoding. Reimplementing these systems would delay product value and introduce correctness and maintenance risks.

## Decision

Use Rust for Idrisi-owned APIs, orchestration, domain models, security integration, ingestion coordination, and provider adapters. Integrate proven engines as separately deployable components:

- PostgreSQL + PostGIS for spatial storage and querying
- Martin for initial vector tile serving
- GDAL and PROJ for raster processing and CRS transformations
- Valhalla for initial routing integration
- Nominatim as a candidate initial geocoding backend
- PMTiles for static/offline vector tile archives

All public APIs and internal service boundaries use Idrisi-owned contracts. Engine-specific request/response types remain inside adapters.

## Consequences

Positive:
- Faster delivery and reuse of proven geospatial capabilities.
- Independent replacement and scaling of engines.
- Rust remains the platform integration and domain layer.

Trade-offs:
- More deployment components and operational dependencies.
- Adapter and compatibility testing is required.
- Engine-specific behavior must be normalized carefully.

Revisit this decision only when measured limitations, licensing, or operational costs justify replacing a component.
