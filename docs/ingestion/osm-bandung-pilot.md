# Bandung OSM pilot ingestion

## Purpose and boundaries

This is the first operational ingestion slice for Idrisi. It imports a bounded Bandung-area window from the Geofabrik Java OpenStreetMap PBF into PostGIS, registers provenance in the Idrisi catalog, and publishes four Martin-readable tables:

| Table | Contents | Geometry |
|---|---|---|
| `tiles.osm_roads` | Tagged highway ways | LineString |
| `tiles.osm_places` | Named/typed place and POI nodes | Point |
| `tiles.osm_buildings` | Building and parking polygons | Geometry |
| `tiles.osm_water` | Water bodies and reservoirs | Geometry |

This is a base-map proof of concept, not a complete OSM cartographic schema. It does not yet include a full landcover/landuse model, address interpolation, turn restrictions, route relations, coastline processing, or incremental replication.

## Source and license

- Source: Geofabrik Java extract, `https://download.geofabrik.de/asia/indonesia/java-latest.osm.pbf`
- Format: OSM PBF
- License: Open Data Commons Open Database License (ODbL) 1.0
- Required attribution: `© OpenStreetMap contributors`
- Attribution/copyright: `https://www.openstreetmap.org/copyright`

The script verifies the published Geofabrik MD5 sidecar and calculates a SHA-256 digest for the immutable Idrisi dataset version. Review the current OSM/Geofabrik terms before redistribution. OSM-derived databases and produced works have different ODbL obligations; this workflow does not replace legal review.

## Pilot bounds

Default bbox is `107.35,-7.15,107.85,-6.75` (min longitude, min latitude, max longitude, max latitude). It intentionally includes a buffer around the Bandung urban area. Override with `IDRISI_OSM_BBOX` when testing another extent.

The osm2pgsql bbox filter is a coarse filter and may cut features at the boundary. The import manual recommends creating a buffered extract for better boundary integrity. Treat the outer edge of this pilot as a processing boundary, not an authoritative administrative boundary.

## Prerequisites

- A running Idrisi Compose stack with migrations applied.
- A clean pilot database: the script refuses to overwrite existing pilot tables.
- `osm2pgsql` 2.x with flex output, `psql`, `curl`, `md5sum`, and `sha256sum`.
- Enough memory and SSD capacity for the Java PBF and PostGIS import. Measure actual use on the deployment host; do not infer production sizing from the PBF download size.

The first run of the Postgres volume must have both `MARTIN_DB_PASSWORD` and `IDRISI_INGEST_DB_PASSWORD` set before starting the stack. Existing volumes need the `idrisi_ingest` role created manually, just like the existing Martin role.

## Run

1. Copy `.env.example` to `.env`, set strong unique passwords, and start the stack:

   ```sh
   docker compose up --build -d
   ```

2. Export the ingestion connection URL. Use the host-mapped Postgres port and the same values configured in `.env`:

   ```sh
   export IDRISI_INGEST_DATABASE_URL='postgres://idrisi_ingest:<password>@127.0.0.1:5432/idrisi'
   ```

3. Run the import from the repository root:

   ```sh
   bash scripts/ingest/osm-bandung.sh
   ```

4. Inspect row counts and source geometry:

   ```sh
   docker compose exec postgres psql -U idrisi -d idrisi -c \
     "SELECT 'roads' AS layer, count(*) FROM tiles.osm_roads UNION ALL SELECT 'places', count(*) FROM tiles.osm_places UNION ALL SELECT 'buildings', count(*) FROM tiles.osm_buildings UNION ALL SELECT 'water', count(*) FROM tiles.osm_water;"
   ```

5. Martin auto-publishes the tables in the `tiles` schema. Through the Iket route, inspect TileJSON at:

   ```
   /api/idrisi/tiles/osm_roads
   /api/idrisi/tiles/osm_places
   /api/idrisi/tiles/osm_buildings
   /api/idrisi/tiles/osm_water
   ```

The script records a local `data/osm/bandung-manifest.json`, which is ignored by Git, and a catalog `dataset_versions` row with source checksum, retrieval time, coverage, and feature count.

## Safety and repeatability

- The importer is a dedicated database role, not the API or Postgres superuser.
- Martin has SELECT only; default privileges grant it read access to future tables created by the importer.
- The script checks for existing output tables and refuses to overwrite them.
- This initial import is a one-shot bootstrap. Incremental OSM replication and atomic blue/green dataset swaps are not implemented yet.
- If import succeeds but catalog publication fails, preserve the manifest and tables; repair the catalog record manually rather than blindly rerunning the import.
- Never commit PBF files, database dumps, or generated tile data to Git.

## Validation checklist

- [ ] Verify the Geofabrik MD5 sidecar and local SHA-256 manifest.
- [ ] Confirm all four output tables have rows and valid geometries.
- [ ] Confirm Martin can read the tables while `idrisi_ingest` cannot alter catalog definitions.
- [ ] Fetch TileJSON and at least one non-empty MVT through Iket.
- [ ] Render the sources in MapLibre and show OSM attribution.
- [ ] Record elapsed time, peak memory, and PostGIS volume growth before expanding the area.
