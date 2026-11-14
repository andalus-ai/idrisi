-- Register the first OSM pilot and isolate ingestion privileges from the API and tile reader.
CREATE SCHEMA IF NOT EXISTS tiles_import;
GRANT USAGE, CREATE ON SCHEMA tiles TO idrisi_ingest;
GRANT USAGE, CREATE ON SCHEMA tiles_import TO idrisi_ingest;

-- Martin remains read-only. These defaults apply to objects created by idrisi_ingest.
GRANT SELECT ON ALL TABLES IN SCHEMA tiles TO martin_tiles;
ALTER DEFAULT PRIVILEGES FOR ROLE idrisi_ingest IN SCHEMA tiles
    GRANT SELECT ON TABLES TO martin_tiles;

-- Allow the ingestion job to publish immutable version/provenance records.
GRANT SELECT ON data_sources, datasets, layers TO idrisi_ingest;
GRANT UPDATE (status, updated_at) ON datasets TO idrisi_ingest;
GRANT UPDATE (is_enabled, updated_at) ON layers TO idrisi_ingest;
GRANT SELECT, INSERT ON dataset_versions TO idrisi_ingest;

INSERT INTO data_sources (
    name, source_type, source_uri, license_id, license_url, attribution, terms_url, metadata
)
SELECT
    'OpenStreetMap via Geofabrik Java extract',
    'osm_pbf',
    'https://download.geofabrik.de/asia/indonesia/java-latest.osm.pbf',
    'ODbL-1.0',
    'https://opendatacommons.org/licenses/odbl/1-0/',
    '© OpenStreetMap contributors',
    'https://www.openstreetmap.org/copyright',
    '{"provider":"Geofabrik","region":"Java","format":"PBF","pilot_area":"Bandung"}'::jsonb
WHERE NOT EXISTS (
    SELECT 1 FROM data_sources WHERE name = 'OpenStreetMap via Geofabrik Java extract'
);

INSERT INTO datasets (
    slug, name, description, domain, status, default_source_id, coverage, metadata
)
SELECT
    'osm-bandung-pilot',
    'OpenStreetMap — Bandung pilot',
    'Bounded Bandung-area OSM import used to validate the Idrisi ingestion and vector-tile pipeline.',
    'base_map',
    'draft',
    source.id,
    ST_Multi(ST_MakeEnvelope(107.35, -7.15, 107.85, -6.75, 4326)),
    '{"region":"Bandung, West Java, Indonesia","source_region":"Java","initial_status":"not_imported"}'::jsonb
FROM data_sources AS source
WHERE source.name = 'OpenStreetMap via Geofabrik Java extract'
ON CONFLICT (slug) DO NOTHING;

INSERT INTO layers (dataset_id, slug, name, chart_domain, geometry_type, min_zoom, max_zoom, is_enabled, metadata)
SELECT dataset.id, layer.slug, layer.name, layer.chart_domain, layer.geometry_type, layer.min_zoom, layer.max_zoom, false, layer.metadata
FROM datasets AS dataset
CROSS JOIN (VALUES
    ('roads', 'Road network', 'transport', 'LINESTRING', 5, 18, '{"martin_source":"osm_roads"}'::jsonb),
    ('places', 'Places and points of interest', 'places', 'POINT', 8, 18, '{"martin_source":"osm_places"}'::jsonb),
    ('buildings', 'Buildings', 'built_environment', 'GEOMETRY', 13, 20, '{"martin_source":"osm_buildings"}'::jsonb),
    ('water', 'Water bodies', 'hydrography', 'GEOMETRY', 6, 18, '{"martin_source":"osm_water"}'::jsonb)
) AS layer(slug, name, chart_domain, geometry_type, min_zoom, max_zoom, metadata)
WHERE dataset.slug = 'osm-bandung-pilot'
ON CONFLICT (dataset_id, slug) DO NOTHING;
