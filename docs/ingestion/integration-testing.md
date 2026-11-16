# PostGIS integration gates

The CI integration gate starts an isolated Compose project containing PostGIS and the Idrisi API. API startup applies the complete SQLx migration chain; the test then exercises the migrated database rather than relying only on static SQL review.

Run locally with:

```bash
bash scripts/integration/postgis-publication-gates.sh
```

The script uses a unique Compose project name, separate host ports (15439 and 18080), and a dedicated temporary Postgres volume. Compose network names are project-scoped as well. It removes only that temporary project, network, and volume on exit; it does not touch the normal `idrisi` Compose project or its persistent data.

## Assertions

- API readiness succeeds after migrations.
- Exactly eight successful SQLx migrations are present.
- PostGIS and the publication function exist.
- `idrisi_ingest` has no access to the published `tiles` schema.
- `idrisi_publish` cannot create objects directly in `tiles`, has the required column-level queue update access, and cannot delete jobs.
- `martin_tiles` can use the `tiles` schema and cannot create objects in `public`.

## Atomic publication fixture

`scripts/integration/publication-transaction.sql` creates a deterministic four-layer PostGIS staging fixture without downloading a PBF. It then:

1. Seeds a previous stable roads table in `tiles`.
2. Creates staging roads, places, buildings, and water geometries and validated import/publish job records.
3. Installs a temporary failure trigger that aborts publication after the version insert and table moves have occurred.
4. Verifies PostgreSQL rolled back the version insert and table moves, leaving the previous stable table, staging table, and running publish job intact.
5. Removes the failure trigger and retries the same fenced publication job.
6. Verifies all four stable tables are promoted, the old roads table is archived, exactly one published dataset version exists, both job records reach the expected states, and staging schemas are removed.

This proves the database promotion transaction and retry boundary with known geometries. It deliberately does not test PBF parsing, osm2pgsql import behavior, or Martin's live TileJSON/vector-tile HTTP responses; those remain separate end-to-end checks.
