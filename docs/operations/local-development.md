# Local Development Stack

This Compose stack is the first local infrastructure slice for Idrisi. It starts PostgreSQL/PostGIS, the Rust domain API, and Martin. It is intended for development and smoke testing, not as a production security baseline.

## Prerequisites

- Docker Engine with the Compose plugin
- Git
- Enough local disk for the PostgreSQL volume and Rust image build

## Start

From the repository root:

```sh
cp .env.example .env
docker compose up --build -d
docker compose ps
```

The example credentials are for a local workstation only. Replace them before using the stack on a shared host. Do not expose the database or Martin ports directly to the public internet.

## Local endpoints

| Component | URL | Purpose |
|---|---|---|
| Idrisi API liveness | http://localhost:8080/health/live | Process liveness |
| Idrisi API readiness | http://localhost:8080/health/ready | Database connectivity |
| Dataset catalog | http://localhost:8080/v1/catalog/datasets | Published catalog entries |
| Martin | http://localhost:3000 | Tile service landing/catalog endpoint |
| PostgreSQL | localhost:5432 | Local development database |

The API applies the SQLx migrations at startup. The catalog endpoint may return an empty list until datasets have been registered and imported.

Martin discovers eligible PostGIS tables and views. A usable tile endpoint will appear only after a spatial table/view with supported geometry and the required tile metadata exists. The Compose stack does not yet include the OSM import pipeline or a populated map dataset.

## Useful commands

```sh
# Follow service logs
docker compose logs -f idrisi-api postgis martin

# Rebuild only the API image
docker compose build idrisi-api

# Restart the API
docker compose restart idrisi-api

# Stop containers but preserve the database volume
docker compose down

# Stop containers and DELETE the local database volume
docker compose down -v
```

The final command is destructive and should only be used when the local database can be discarded.

## Configuration

Copy `.env.example` to `.env`. The Compose file reads the PostgreSQL database/user/password and host ports from that file. Never commit a populated `.env` file.

The database and service ports bind to loopback by default. For a deployed environment, put public ingress behind Iket or an appropriately secured reverse proxy, use managed secrets, provision backups, and restrict internal service networking.

## Current limitations

- No Iket container or gateway configuration is bundled; Iket remains a separately maintained project.
- The initial database credentials are shared by the API and Martin for local convenience. Production deployments must use separate least-privilege runtime roles, including read-only access for Martin.
- No OSM data is downloaded automatically.
- No raster, routing, geocoding, offline package, or specialist chart services are included yet.
- Image tags are pinned for the initial development baseline and should be upgraded through reviewed dependency changes.

## Smoke-test checklist

1. `docker compose ps` reports all three services running.
2. `curl -fsS http://localhost:8080/health/live` returns an alive response.
3. `curl -fsS http://localhost:8080/health/ready` returns a ready response.
4. `curl -fsS http://localhost:8080/v1/catalog/datasets` returns JSON.
5. Martin's landing/catalog endpoint is reachable at `http://localhost:3000`.
6. After a spatial table is loaded, verify its tile metadata and request a tile from Martin.

Do not mark the end-to-end map milestone complete until a real pilot dataset is imported and rendered in MapLibre.
