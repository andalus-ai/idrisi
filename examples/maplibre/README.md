# Idrisi MapLibre reference viewer

A minimal browser client for the Bandung OSM pilot. It consumes Martin TileJSON/vector tiles through the configured Iket public route; it does not connect directly to PostGIS or Martin.

## Run

From the repository root:

```sh
docker compose up -d map-viewer
```

Open [http://localhost:8090](http://localhost:8090). Configure the Iket public base URL (default `/api/idrisi`) and, when required by the gateway, enter a bearer token in the page. The token is held only in JavaScript memory for the current tab; it is not written to local storage or cookies.

For a separately hosted Iket instance, enter its origin and public prefix, for example `https://maps.example.com/api/idrisi`. The browser then requests `/tiles/{source}` TileJSON and the tile URLs returned by Martin. Configure CORS at Iket for the viewer origin and allow the Authorization header if bearer authentication is enabled. Do not enable wildcard origins with credentials.

## Layer sources

| Source | Expected source-layer | Display |
|---|---|---|
| `osm_roads` | `osm_roads` | Road casing and line |
| `osm_places` | `osm_places` | POI/place circles |
| `osm_buildings` | `osm_buildings` | Building polygons |
| `osm_water` | `osm_water` | Water polygons/lines |

The style is intentionally schematic. It is a smoke-test client, not a production cartographic style. It expects the OSM import pipeline to have populated the corresponding Martin sources. A blank map background before ingestion is expected.

## Notes and limitations

- MapLibre GL JS 5.6.2 and its CSS are loaded from unpkg at runtime.
- The initial camera is centered on Bandung.
- The viewer shows OSM attribution and a non-navigation disclaimer.
- This viewer does not implement identity login, token refresh, user preferences, search, geolocation, routing, offline packs, or production error telemetry.
- For production, serve the static files over HTTPS from the same origin as Iket or a separately controlled origin with a restrictive CORS policy. Pin/vendor the MapLibre assets in a production build.
- Martin's table TileJSON must advertise a source-layer name matching the table name; confirm this against the live Martin version during integration testing.
