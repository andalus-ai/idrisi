#!/usr/bin/env bash
set -euo pipefail

compose=(docker compose -f compose.yaml)
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$project_root"

cleanup() {
  "${compose[@]}" down --volumes --remove-orphans
}
trap cleanup EXIT

if [[ ! -f .env ]]; then
  cp .env.example .env
fi

"${compose[@]}" up --build -d postgres idrisi-api

echo "Waiting for API migrations and readiness..."
ready=0
for _ in $(seq 1 60); do
  if curl --fail --silent http://127.0.0.1:8080/health/ready >/dev/null; then
    ready=1
    break
  fi
  sleep 5
done
if [[ "$ready" != 1 ]]; then
  "${compose[@]}" logs --no-color idrisi-api postgres
  echo "API did not become ready; migrations may have failed." >&2
  exit 1
fi

"${compose[@]}" exec -T postgres sh -ec '
  psql --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" --set=ON_ERROR_STOP=1 <<'"'"'SQL'"'"'
DO $check$
DECLARE
    migration_count integer;
    duplicate_count integer;
BEGIN
    SELECT count(*) INTO migration_count FROM _sqlx_migrations WHERE success;
    SELECT count(*) INTO duplicate_count
    FROM (
        SELECT version FROM _sqlx_migrations GROUP BY version HAVING count(*) > 1
    ) duplicates;
    IF migration_count <> 8 OR duplicate_count <> 0 THEN
        RAISE EXCEPTION '"'"'expected 8 unique successful migrations; found %, duplicate versions %',
            migration_count, duplicate_count;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_extension WHERE extname = '"'"'postgis'"'"') THEN
        RAISE EXCEPTION '"'"'PostGIS extension is missing'"'"';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_proc
        WHERE proname = '"'"'idrisi_publish_osm_dataset'"'"'
    ) THEN
        RAISE EXCEPTION '"'"'publication function is missing'"'"';
    END IF;

    IF has_schema_privilege('"'"'idrisi_ingest'"'"', '"'"'tiles'"'"', '"'"'USAGE'"'"')
       OR has_schema_privilege('"'"'idrisi_ingest'"'"', '"'"'tiles'"'"', '"'"'CREATE'"'"') THEN
        RAISE EXCEPTION '"'"'ingestion role must not access the published tiles schema'"'"';
    END IF;

    IF has_schema_privilege('"'"'idrisi_publish'"'"', '"'"'tiles'"'"', '"'"'CREATE'"'"') THEN
        RAISE EXCEPTION '"'"'publisher role must not create objects in tiles directly'"'"';
    END IF;

    IF NOT has_column_privilege('idrisi_publish', '"'"'ingestion_jobs'"'"', '"'"'status'"'"', '"'"'UPDATE'"'"')
       OR has_table_privilege('"'"'idrisi_publish', '"'"'ingestion_jobs'"'"', '"'"'DELETE'"'"') THEN
        RAISE EXCEPTION '"'"'publisher queue privileges are not least-privilege'"'"';
    END IF;

    IF NOT has_schema_privilege('"'"'martin_tiles'"'"', '"'"'tiles'"'"', '"'"'USAGE'"'"')
       OR has_schema_privilege('"'"'martin_tiles'"'"', '"'"'public'"'"', '"'"'CREATE'"'"') THEN
        RAISE EXCEPTION '"'"'Martin role privileges are not correctly isolated'"'"';
    END IF;
END;
$check$;
SQL
'

echo "PostGIS migration and role-boundary integration checks passed."
