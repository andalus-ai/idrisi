#!/usr/bin/env bash
set -euo pipefail

# Verify the public Iket -> Martin path after the Bandung OSM import.
# Requires curl, jq, and wc. Run from any directory.
: "${IDRISI_IKET_BASE_URL:?Set IDRISI_IKET_BASE_URL to the public Iket prefix, e.g. https://maps.example.com/api/idrisi}"

BASE_URL="${IDRISI_IKET_BASE_URL%/}"
TOKEN="${IDRISI_IKET_TOKEN:-}"
TILE_Z="${IDRISI_SMOKE_TILE_Z:-12}"
# A representative tile around central Bandung at zoom 12.
TILE_X="${IDRISI_SMOKE_TILE_X:-3272}"
TILE_Y="${IDRISI_SMOKE_TILE_Y:-2126}"
SOURCES=(osm_roads osm_places osm_buildings osm_water)
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

for command in curl jq wc; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "Required command not found: $command" >&2
    exit 1
  fi
done

CURL_ARGS=(--fail --silent --show-error --location --retry 2)
if [[ -n "$TOKEN" ]]; then
  CURL_ARGS+=(--header "Authorization: Bearer $TOKEN")
fi

for source in "${SOURCES[@]}"; do
  echo "Checking TileJSON: $source"
  tilejson="$TMP_DIR/$source.json"
  curl "${CURL_ARGS[@]}" "$BASE_URL/tiles/$source" --output "$tilejson"

  jq -e --arg layer "$source" '
    (.tiles | type == "array" and length > 0) and
    (.vector_layers | type == "array" and any(.[]; .id == $layer))
  ' "$tilejson" >/dev/null || {
    echo "Invalid TileJSON or missing source-layer '$source'." >&2
    exit 1
  }

  template="$(jq -r '.tiles[0]' "$tilejson")"
  tile_url="${template//\{z\}/$TILE_Z}"
  tile_url="${tile_url//\{x\}/$TILE_X}"
  tile_url="${tile_url//\{y\}/$TILE_Y}"

  # Martin may advertise relative tile paths. Resolve those under the Iket
  # public prefix so the request remains on the gateway route.
  if [[ "$tile_url" == /* ]]; then
    tile_url="$BASE_URL/${tile_url#/}"
  elif [[ "$tile_url" != http://* && "$tile_url" != https://* ]]; then
    tile_url="$BASE_URL/$tile_url"
  fi

  echo "Checking MVT: $source z=$TILE_Z x=$TILE_X y=$TILE_Y"
  headers="$TMP_DIR/$source.headers"
  body="$TMP_DIR/$source.pbf"
  curl "${CURL_ARGS[@]}" --dump-header "$headers" "$tile_url" --output "$body"

  content_type="$(awk 'BEGIN{IGNORECASE=1} /^content-type:/ {gsub("\r",""); print tolower($2)}' "$headers" | tail -n 1)"
  case "$content_type" in
    application/vnd.mapbox-vector-tile*|application/x-protobuf*|application/octet-stream*) ;;
    *)
      echo "Unexpected content-type for $source: ${content_type:-missing}" >&2
      exit 1
      ;;
  esac

  bytes="$(wc -c < "$body" | tr -d ' ')"
  if [[ "$bytes" -le 0 ]]; then
    echo "Empty vector tile response for $source." >&2
    exit 1
  fi
  echo "PASS $source ($bytes bytes, $content_type)"
done

echo "All Bandung vector-tile smoke checks passed through Iket."
