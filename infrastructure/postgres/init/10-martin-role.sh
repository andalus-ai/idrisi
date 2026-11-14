#!/bin/sh
set -eu

: "${MARTIN_DB_PASSWORD:?MARTIN_DB_PASSWORD must be set}"

psql --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" \
  --set=ON_ERROR_STOP=1 \
  --set=martin_password="$MARTIN_DB_PASSWORD" <<'SQL'
SELECT format('CREATE ROLE martin_tiles LOGIN PASSWORD %L', :'martin_password') \gexec
SQL
