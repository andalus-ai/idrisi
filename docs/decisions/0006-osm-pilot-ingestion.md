# ADR 0006: OSM pilot ingestion with osm2pgsql flex

- **Status:** Accepted
- **Date:** 2026-11-14

## Context

Idrisi needs a repeatable first import to validate the PostGIS → Martin → Iket path without attempting a nationwide production import. OSM source data must carry source, license, attribution, retrieval time, checksum, coverage, and version metadata into the dataset catalog.

The OSM import should use a mature upstream parser and geometry assembler rather than reimplementing PBF parsing and relation geometry handling in Rust. The initial pilot should be spatially bounded and easy to discard/rebuild while the data model is still evolving.

## Decision

- Use Geofabrik's Java regional PBF as the initial source and constrain the import to a buffered Bandung-area bbox.
- Use osm2pgsql 2.x flex output with a repository-owned Lua style.
- Publish a deliberately small set of normalized PostGIS tables in the tiles schema: roads, places, buildings, and water.
- Store osm2pgsql slim-mode middle tables in a separate tiles_import schema.
- Run ingestion with a dedicated idrisi_ingest role. Martin continues to use its separate read-only martin_tiles role.
- Record retrieval time, source checksum, coverage, feature count, and dataset version in the catalog.
- Refuse to overwrite existing pilot output tables. Incremental updates and atomic version swaps require a separate design.

## Consequences

- The pipeline is quick to test on one region and can later be expanded to other Java/Indonesia extents.
- The flex Lua file is trusted executable code and must be reviewed like application code.
- A bbox filter is only a coarse import boundary and can clip geometries; a buffered extract is preferable for production.
- The first schema is a rendering-oriented base-map slice, not a complete OSM schema or routing graph.
- OSM data is licensed under ODbL 1.0 and requires attribution. Data redistribution and derivative-database obligations must be reviewed before public deployment.
- A successful import still requires runtime validation of geometry quality, Martin TileJSON/MVT output, Iket proxying, and MapLibre rendering.
