# Initial API Contracts

This document establishes proposed public API shapes. Exact OpenAPI schemas will be added before the first API release.

Iket is the public gateway. Clients reach the configured public prefix on Iket, which routes control-plane requests to the private Idrisi domain API and vector-tile requests directly to the private Martin service. Iket handles edge authentication and traffic policies when explicitly configured; Idrisi remains responsible for domain validation and tenant/resource authorization. Never trust identity headers supplied directly by clients.

## Principles

- Base path: `/v1` for Idrisi domain APIs.
- JSON for control-plane and feature APIs; standard binary/media responses for tiles and package downloads.
- WGS 84 longitude/latitude for GeoJSON-facing coordinate values.
- Explicit units, coordinate order, CRS, dataset version, and timestamps.
- Stable Idrisi response types independent of backend engines.
- Consistent errors with a machine-readable code, human-readable message, request ID, and optional validation details.
- Bound all list sizes, geometry sizes, matrix dimensions, and execution time.

## Implemented catalog endpoints

| Method | Path | Purpose |
|---|---|---|
| GET | `/v1/catalog/datasets` | List catalog datasets |
| GET | `/v1/catalog/datasets/{dataset_id}` | Retrieve a dataset |

## Vector tile delivery

Martin is the internal MVT engine. It is not a public gateway and its port must remain private. Iket routes the public tile prefix to Martin directly to avoid copying high-volume binary tile payloads through the Rust catalog API.

| Public method | Public path | Upstream |
|---|---|---|
| GET | `/api/idrisi/tiles/{source}` | Martin TileJSON `/{source}` |
| GET | `/api/idrisi/tiles/{source}/{z}/{x}/{y}` | Martin MVT `/{source}/{z}/{x}/{y}` |

- Tile matrix is WebMercatorQuad / XYZ unless a source explicitly documents otherwise.
- MVT responses are binary protobuf; clients should use TileJSON metadata to discover layer names, bounds, zoom range, and tile URL.
- Only PostGIS tables in the dedicated `tiles` schema are eligible for automatic publication.
- Martin uses a dedicated read-only database role. Never connect Martin with the catalog migration owner in production.
- Do not publish catalog tables or arbitrary application schemas as tile sources.
- Source identifiers are derived from Martin's configured table discovery; Idrisi will add stable catalog-to-tile source mapping before the first public data release.
- Tile routes require Iket authentication/policy consistent with the dataset's visibility. Martin itself does not provide Idrisi tenant authorization.

The tile endpoints are an engine integration contract, not a claim that production Indonesia tile data is already loaded. The `tiles` schema is initially empty.

## Planned endpoints

| Method | Path | Purpose |
|---|---|---|
| GET | `/v1/datasets` | Discover published datasets |
| GET | `/v1/layers` | Discover available layers |
| GET | `/v1/geocode/search` | Forward geocoding |
| GET | `/v1/geocode/reverse` | Reverse geocoding |
| POST | `/v1/routes` | Route calculation |
| POST | `/v1/matrix` | Bounded origin/destination matrix |
| GET | `/v1/places/nearby` | Nearby feature search |
| POST | `/v1/geofences` | Create/manage authorized geofences |
| GET | `/v1/elevation` | Elevation lookup/profile |
| GET | `/v1/offline/packages` | List regional packages |
| GET | `/v1/offline/packages/{id}` | Package manifest and metadata |
| GET | `/v1/offline/packages/{id}/download` | Package artifact delivery |

PMTiles may be served as a static archive with range requests rather than unpacked into individual XYZ endpoints.

## Route request (conceptual)

```json
{
  "coordinates": [[106.8272, -6.1754], [107.6098, -6.9147]],
  "profile": "car",
  "alternatives": 0,
  "units": "kilometers",
  "dataset_version": "optional-pinned-version"
}
```

A route response should expose normalized geometry, distance, duration, legs, maneuvers/instructions when requested, warnings, and the routing dataset/graph version. Do not leak raw Valhalla response objects as the public contract.

## Geocoding

Search supports query text, optional geographic bias/bounds, language, result limit, and dataset/provider preferences where authorized. Reverse lookup accepts a coordinate and optional zoom/detail hint. Results include a stable Idrisi result ID, label, coordinate, feature type, confidence where meaningful, source attribution, and data version.

## Dataset and layer discovery

Dataset records expose ID, name, domain, provider, version, extent, temporal coverage, CRS, license, attribution, status, and update timestamp. Layer records reference a dataset version and include type, style reference, zoom/scale limits, and access policy.

## Error format (conceptual)

```json
{
  "error": {
    "code": "INVALID_COORDINATE",
    "message": "Coordinate is outside the supported range.",
    "request_id": "req_...",
    "details": []
  }
}
```

Never return stack traces, database errors, credentials, or upstream provider errors to clients.

## API versioning

- Additive optional fields may be introduced within a version when compatibility is preserved.
- Breaking changes require a new version or an explicitly documented migration.
- Publish OpenAPI documents and representative examples.
- Maintain contract tests for gateway and adapters.
