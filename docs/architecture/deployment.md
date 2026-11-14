# Self-Hosted Deployment Baseline

## Initial target

The first supported deployment is a single Linux host using Docker Compose. This is the fastest path to a reproducible developer environment and a private regional deployment. It is not a claim that one host is sufficient for every production workload.

## Logical services

- `idrisi-gateway`: Rust/Axum public API.
- `postgres`: PostgreSQL with PostGIS extension.
- `martin`: vector tile server connected to PostGIS.
- `valhalla`: routing engine and versioned graph data.
- `geocoder`: optional Nominatim initially, subject to import/resource requirements.
- `object-storage`: optional S3-compatible storage for artifacts and packages.
- `worker`: ingestion and publication jobs.
- `metrics/logs`: optional observability stack for development; recommended for production.

Do not expose internal engine/database ports publicly. Use private Compose networks and publish only the gateway and intentionally public artifact endpoints.

## Persistent storage

Use distinct persistent volumes or mounted directories for:
- PostgreSQL data
- OSM/source staging data
- Derived tiles and PMTiles
- Raster/COG imagery
- Routing graphs
- Geocoder indexes
- Backups and manifests

Data volumes must not be deleted by routine container rebuilds. Document ownership, permissions, expected size, and backup behavior.

## Configuration

Provide a safe `.env.example` with non-secret defaults only. Validate required production secrets at startup. Configuration should cover:
- Public base URL and bind address
- Database URL and pool limits
- Backend service URLs
- Data root and object storage settings
- Authentication mode and token issuer/audience
- Rate limits and request limits
- Logging level and tracing exporter
- Dataset/provider enablement

## Health and readiness

Each service should expose health checks. Gateway liveness must not depend on all optional providers. Readiness should report required dependencies and distinguish degraded optional capabilities from total failure.

## Backups and restore

- Use PostgreSQL-native backups with tested restore.
- Version and checksum immutable artifacts.
- Back up dataset manifests, configuration, and required secrets through an appropriate secret-management process.
- Keep backups outside the primary host.
- Document RPO/RTO targets before production use.
- Test restore and rollback after schema and dataset upgrades.

## Upgrade process

1. Review release notes and migrations.
2. Back up database and relevant manifests/artifacts.
3. Pull pinned images.
4. Apply backward-compatible migrations.
5. Start services and verify readiness.
6. Run smoke tests for tiles, routing, geocoding, and catalog.
7. Retain previous image and dataset versions for rollback.

## Scaling path

When a single host is no longer sufficient:
- Scale stateless gateway replicas.
- Separate database from compute and use managed or dedicated PostgreSQL if desired.
- Scale tile/routing/geocoding services independently.
- Move large artifacts to object storage/CDN while retaining authorization controls.
- Run ingestion as queued workers.
- Consider Kubernetes only after operational behavior is proven in Compose.
