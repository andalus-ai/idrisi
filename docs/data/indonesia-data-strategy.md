# Indonesia Geospatial Data Strategy

## 1. Scope

Indonesia is the first operational region. Data acquisition must be incremental and regionally bounded rather than requiring a planet-scale import at the start.

The initial pilot should select one manageable area that exercises dense urban roads, mixed land use, coastlines, and island geography. A Jakarta/Bandung/Yogyakarta pilot can validate different patterns; choose the first region based on available hardware, data licensing, and test goals.

## 2. Data domains and candidate sources

| Domain | Candidate sources/formats | Initial handling |
|---|---|---|
| Roads and places | OpenStreetMap regional PBF; GeoJSON/GeoPackage from approved sources | Import selected region; retain OSM attribution and ODbL compliance |
| Elevation/topography | Copernicus DEM, SRTM, or other sources subject to their terms; GeoTIFF/COG | Validate vertical datum, resolution, voids, and redistribution rights |
| Satellite/aerial | Licensed imagery providers or openly licensed imagery programs where permitted; GeoTIFF/COG | Treat as separate imagery provider; do not assume OSM includes imagery rights |
| Nautical/hydrographic | Official hydrographic authorities and datasets with explicit terms | Store authority, edition, update/correction status, scale, datum, and safety limitations |
| Aviation | Authoritative aeronautical information publications and datasets with explicit terms | Store issuing authority, effective dates, cycle, and usage restrictions |
| Custom overlays | Customer/operator data: GeoJSON, GeoPackage, FlatGeobuf, GeoTIFF | Validate schema, CRS, permissions, and tenant ownership |

These are candidate source families, not blanket approval to download, redistribute, or commercially use every dataset. Confirm the current license and access conditions for the exact source and product.

## 3. OpenStreetMap

- Acquire regional extracts from a reputable provider such as Geofabrik or use an approved OSM replication workflow.
- Record extract timestamp, source URL, checksum, and source region.
- Preserve required attribution and assess ODbL obligations for databases and derivative databases.
- Keep OSM-derived data lineage distinguishable from independent datasets.
- Build routing graphs and tile artifacts from pinned source versions.
- Plan updates as repeatable jobs; do not treat an old PBF as current data.

## 4. Ingestion pipeline

1. **Register:** create dataset record and verify license, attribution, source, coverage, and update policy.
2. **Acquire:** download to a staging area with resumability and checksum verification.
3. **Inspect:** validate file integrity, bounds, CRS, feature counts, and expected schema.
4. **Transform:** use GDAL/PROJ and domain-specific tools; store processing version and parameters.
5. **Load/build:** populate PostGIS, build PMTiles/COG, generate routing graph or geocoder index as applicable.
6. **Validate:** compare coverage, feature counts, geometry validity, sample routes, tile rendering, and metadata.
7. **Publish:** mark immutable version as published and update catalog pointer only after checks pass.
8. **Rollback:** retain previous release until the new release is accepted and backup/restore verified.

## 5. Dataset manifest

Each dataset version should include:
- Dataset ID and version
- Source/provider and source URL or acquisition reference
- License identifier and full attribution statement
- Retrieval time and checksum
- Geographic and temporal extent
- Source/target CRS and axis order
- Vertical datum and units where applicable
- Processing toolchain and recipe version
- Output artifact references and checksums
- Quality notes, known limitations, and validation report
- Update cadence and support/retirement status

## 6. Indonesia-specific quality checks

- Island separation, coastal boundaries, bridges, ferries, and water crossings
- Dense urban road connectivity and access restrictions
- Road classification and turn restrictions
- Antimeridian and longitude wrapping behavior
- Multiple UTM zones and projection transformations
- Tropical cloud cover, imagery acquisition date, and seasonal change
- Elevation resolution, vertical datum, and coastal voids
- Nautical chart datum, depth units, edition/correction status
- Aviation effective dates and cycle management

## 7. Data safety and authority

A visually useful layer is not automatically authoritative. Nautical and aviation data can affect safety-critical decisions. Idrisi must label source authority, currency, intended use, and known limitations. Until a formal validation and governance process exists, those layers are informational only and must not be presented as substitutes for official navigation charts, notices, aeronautical publications, or certified systems.

## 8. Storage layout

Keep source, staging, derived artifacts, and published artifacts separate. A suggested layout:

```text
/data/
  sources/{provider}/{dataset}/{version}/
  staging/{job_id}/
  derived/{dataset}/{version}/
  published/{dataset}/{version}/
  routing/{profile}/{graph_version}/
  imagery/{layer}/{version}/
  backups/{date}/
```

Use object storage for durable production artifacts where possible. Local paths are a development and single-host deployment convention, not a public API.
