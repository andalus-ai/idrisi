# ADR 0005: Martin for PostGIS Vector Tile Serving

- Status: Accepted
- Date: 2026-11-14

## Context

Idrisi needs standards-compatible vector tile delivery for MapLibre and other clients. The platform already uses PostGIS as its spatial database and Iket as its only public gateway. Implementing an MVT encoder and tile-serving engine in the Rust domain API would duplicate mature functionality and put high-volume binary traffic through the catalog service.

## Decision

Use the upstream Martin server, pinned to the stable `1.16.1` container release, as the initial PostGIS-backed vector tile engine.

- Martin runs as a private Compose service and is not published on a host port.
- Martin connects with a dedicated read-only `martin_tiles` database role.
- Automatic publication is restricted to the dedicated PostGIS `tiles` schema.
- Iket routes the public `/api/idrisi/tiles/{source}/...` prefix directly to Martin.
- The initial tile matrix is WebMercatorQuad / XYZ.
- The Rust API remains responsible for catalog, dataset lifecycle, resource authorization, and stable Idrisi metadata contracts.
- PMTiles remains the preferred immutable archive format for static/offline regional packages.

## Consequences

Positive:
- Reuses a mature Rust-based tile server.
- Avoids routing tile payloads through the catalog API.
- Limits Martin's database visibility to a dedicated read-only schema.
- Provides TileJSON and MVT endpoints for MapLibre-compatible clients.

Trade-offs:
- Tile source IDs and catalog layer IDs need an explicit mapping before production release.
- Iket must preserve binary response bodies, content types, caching headers, and query strings.
- Tile access authorization must be enforced by Iket policies or a trusted authorization layer; Martin's read-only database role is not an end-user authorization system.
- The current schema contains no production Indonesia tile data.

## Compatibility

Martin 1.16.1 requires PostgreSQL 12+ and PostGIS 3.5+ for PostgreSQL-backed sources. Idrisi therefore uses the PostGIS 3.5 image line on PostgreSQL 16. Re-evaluate the database minimums and configuration before upgrading Martin to 2.x.
