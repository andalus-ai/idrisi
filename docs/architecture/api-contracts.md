# Initial API Contracts

This document establishes proposed public API shapes. It is a design contract, not a statement that endpoints are implemented. Exact OpenAPI schemas will be added before the first API release.

Iket is the public gateway. Clients reach the configured public prefix on Iket, which routes to private Idrisi domain API endpoints under /v1. Iket handles edge authentication and traffic policies when explicitly configured; Idrisi remains responsible for domain validation and tenant/resource authorization. Never trust identity headers supplied directly by clients.

## Principles

- Base path: `/v1`.
- JSON for control-plane and feature APIs; standard binary/media responses for tiles and package downloads.
- WGS 84 longitude/latitude for GeoJSON-facing coordinate values.
- Explicit units, coordinate order, CRS, dataset version, and timestamps.
- Stable Idrisi response types independent of backend engines.
- Consistent errors with a machine-readable code, human-readable message, request ID, and optional validation details.
- Bound all list sizes, geometry sizes, matrix dimensions, and execution time.

## Proposed endpoints

| Method | Path | Purpose |
|---|---|---|
| GET | `/v1/health/live` | Process liveness |
| GET | `/v1/health/ready` | Dependency readiness |
| GET | `/v1/datasets` | Discover published datasets |
| GET | `/v1/layers` | Discover available layers |
| GET | `/v1/tiles/{z}/{x}/{y}` | Tile access (format negotiated or explicit) |
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

Tile URL compatibility and MVT content negotiation must be tested against MapLibre. PMTiles may be served as a static archive with range requests rather than unpacked into individual XYZ endpoints.

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

Supported profiles should be capability-discovered. Do not promise a mode (for example, cycling or pedestrian) until the selected provider and source graph support it.

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

Never return stack traces, database errors, credentials, or upstream provider internals to clients.

## API versioning

- Additive optional fields may be introduced within a version when compatibility is preserved.
- Breaking changes require a new version or an explicitly documented migration.
- Publish OpenAPI documents and representative examples.
- Maintain contract tests for gateway and adapters.
