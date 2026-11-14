#!/usr/bin/env bash
set -euo pipefail

# Run from the repository root. Requires curl, osm2pgsql >= 2.0, psql, md5sum, sha256sum.
# Geofabrik's Java PBF is the source extract; --bbox keeps the initial database pilot bounded.
# Keep a generous bbox buffer around the intended urban area to reduce clipped geometries.
: "${IDRISI_INGEST_DATABASE_URL:?Set IDRISI_INGEST_DATABASE_URL for the idrisi_ingest role}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DATA_DIR="${IDRISI_DATA_DIR:-$ROOT_DIR/data/osm}"
PBF_URL="${IDRISI_OSM_PBF_URL:-https://download.geofabrik.de/asia/indonesia/java-261112.osm.pbf}"
BBOX="${IDRISI_OSM_BBOX:-107.35,-7.15,107.85,-6.75}"
PBF_FILENAME="$(basename "$PBF_URL")"
PBF_PATH="$DATA_DIR/$PBF_FILENAME"
MD5_PATH="$PBF_PATH.md5"
MANIFEST_PATH="$DATA_DIR/bandung-manifest.json"
RETRIEVED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
VERSION="bandung-$(date -u +%Y%m%dT%H%M%SZ)"

mkdir -p "$DATA_DIR"

if ! command -v osm2pgsql >/dev/null 2>&1; then
  echo "osm2pgsql is required; see https://osm2pgsql.org/doc/install.html" >&2
  exit 1
fi
if ! osm2pgsql --version | grep -Eq '2\.[0-9]'; then
  echo "This profile requires osm2pgsql 2.x (flex output)." >&2
  exit 1
fi

# Avoid accidental destructive/repeat imports. Publish a fresh dataset version via a
# separately designed staging/swap workflow before supporting subsequent full imports.
EXISTING="$(psql "$IDRISI_INGEST_DATABASE_URL" -Atqc \
  "SELECT count(*) FROM pg_tables WHERE schemaname = 'tiles' AND tablename IN ('osm_roads','osm_places','osm_buildings','osm_water')")"
if [[ "$EXISTING" != "0" ]]; then
  echo "OSM pilot tables already exist. Refusing to overwrite; use a clean pilot DB or the future versioned swap workflow." >&2
  exit 1
fi

echo "Downloading Java extract: $PBF_URL"
curl --fail --location --retry 3 --output "$PBF_PATH" "$PBF_URL"
curl --fail --location --retry 3 --output "$MD5_PATH" "$PBF_URL.md5"

EXPECTED_MD5="$(awk '{print $1}' "$MD5_PATH")"
ACTUAL_MD5="$(md5sum "$PBF_PATH" | awk '{print $1}')"
if [[ -z "$EXPECTED_MD5" || "$EXPECTED_MD5" != "$ACTUAL_MD5" ]]; then
  echo "Geofabrik MD5 verification failed." >&2
  exit 1
fi
SHA256="$(sha256sum "$PBF_PATH" | awk '{print $1}')"

echo "Importing Bandung pilot (bbox: $BBOX)"
osm2pgsql \
  --create \
  --slim \
  --output=flex \
  --style="$ROOT_DIR/infrastructure/osm2pgsql/bandung.lua" \
  --schema=tiles \
  --middle-schema=tiles_import \
  --bbox="$BBOX" \
  --database="$IDRISI_INGEST_DATABASE_URL" \
  "$PBF_PATH"

FEATURE_COUNT="$(psql "$IDRISI_INGEST_DATABASE_URL" -Atqc \
  "SELECT (SELECT count(*) FROM tiles.osm_roads) + (SELECT count(*) FROM tiles.osm_places) + (SELECT count(*) FROM tiles.osm_buildings) + (SELECT count(*) FROM tiles.osm_water)")"

cat > "$MANIFEST_PATH" <<EOF
{
  "dataset": "osm-bandung-pilot",
  "source": "$PBF_URL",
  "retrieved_at": "$RETRIEVED_AT",
  "version": "$VERSION",
  "source_md5": "$ACTUAL_MD5",
  "source_sha256": "$SHA256",
  "bbox": "$BBOX",
  "feature_count": $FEATURE_COUNT,
  "license": "ODbL-1.0",
  "attribution": "© OpenStreetMap contributors"
}
EOF

psql "$IDRISI_INGEST_DATABASE_URL" \
  --set=ON_ERROR_STOP=1 \
  --set=version="$VERSION" \
  --set=checksum="$SHA256" \
  --set=source_uri="$PBF_URL" \
  --set=source_version="$PBF_FILENAME" \
  --set=retrieved_at="$RETRIEVED_AT" \
  --set=feature_count="$FEATURE_COUNT" \
  --set=bbox="$BBOX" <<'SQL'
WITH source AS (
    SELECT id FROM data_sources
    WHERE name = 'OpenStreetMap via Geofabrik Java extract'
), dataset AS (
    SELECT id FROM datasets WHERE slug = 'osm-bandung-pilot'
)
INSERT INTO dataset_versions (
    dataset_id, version, status, source_id, source_version, source_checksum,
    coverage, feature_count, storage_uri, published_at, metadata
)
SELECT
    dataset.id, :'version', 'published', source.id, :'source_version', :'checksum',
    ST_Multi(ST_MakeEnvelope(
        split_part(:'bbox', ',', 1)::double precision,
        split_part(:'bbox', ',', 2)::double precision,
        split_part(:'bbox', ',', 3)::double precision,
        split_part(:'bbox', ',', 4)::double precision,
        4326
    )),
    :'feature_count'::bigint, :'source_uri', now(),
    jsonb_build_object('retrieved_at', :'retrieved_at', 'bbox', :'bbox', 'manifest', 'data/osm/bandung-manifest.json')
FROM source, dataset
ON CONFLICT (dataset_id, version) DO NOTHING;

UPDATE datasets SET status = 'published', updated_at = now()
WHERE slug = 'osm-bandung-pilot';

UPDATE layers SET is_enabled = true, updated_at = now()
WHERE dataset_id = (SELECT id FROM datasets WHERE slug = 'osm-bandung-pilot');
SQL

echo "Bandung OSM pilot import complete."
echo "Manifest: $MANIFEST_PATH"
echo "Feature rows: $FEATURE_COUNT"
echo "Martin TileJSON sources are expected at /osm_roads, /osm_places, /osm_buildings, and /osm_water."
