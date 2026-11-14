# PostGIS integration gates

The CI integration gate starts an isolated Compose project containing PostGIS and the Idrisi API. API startup applies the complete SQLx migration chain; the test then checks the migrated database rather than relying only on static SQL review.

Run locally with:

```bash
bash scripts/integration/postgis-publication-gates.sh
```

The script uses a unique Compose project name, separate host ports (15439 and 18080), and a dedicated temporary Postgres volume. It removes only that temporary project and volume on exit. It does not touch the normal `idrisi` Compose project or its persistent data.

## Assertions

- API readiness succeeds after migrations.
- Exactly eight successful, unique SQLx migrations are present.
- PostGIS and the fixed publication function exist.
- `idrisi_ingest` has no access to the published `tiles` schema.
- `idrisi_publish` cannot create objects directly in `tiles`, has only the required column-level queue update access, and cannot delete jobs.
- `martin_tiles` can use the `tiles` schema and cannot create objects in `public`.

## Deliberate scope boundary

These checks validate migration execution and baseline role isolation on a real PostGIS server. They do not yet validate a real PBF import, a successful table promotion, rollback under injected failure, archive restoration, or Martin TileJSON/vector-tile responses. Those require a deterministic miniature OSM fixture and are the next integration-test increment. The test does not claim that the publication pipeline is production-verified.
