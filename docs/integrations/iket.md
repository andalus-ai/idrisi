# Iket Gateway Integration

## Purpose

Idrisi uses [Iket](https://github.com/andalus-ai/iket) as its only public API gateway. Iket is maintained and released independently. Idrisi owns the geospatial domain APIs and configures Iket to route those APIs and high-volume tile requests to private upstream services.

This integration avoids copying Iket source code or recreating gateway features inside Idrisi.

## Responsibility split

| Concern | Owner |
|---|---|
| Public listener, TLS, upstream proxying | Iket |
| Gateway route matching and path rewriting | Iket |
| Gateway authentication/API-key policies | Iket, when configured |
| Gateway rate limits and traffic controls | Iket, when configured |
| Gateway administration and configuration lifecycle | Iket |
| Geospatial API semantics and OpenAPI | Idrisi |
| Tenant/resource authorization | Idrisi domain services and configured Iket policies |
| Dataset catalog and provenance | Idrisi |
| Spatial data, tile serving, routing and geocoding adapters | Idrisi and its selected engines |
| Data ingestion and offline package lifecycle | Idrisi |

## Network topology

Clients connect to Iket. Iket routes control-plane and domain API requests to the private Idrisi Rust API, and vector-tile requests directly to the private Martin service. Idrisi API connects to PostGIS and other internal providers. Martin connects to PostGIS with a dedicated read-only login restricted to the `tiles` schema.

Neither the Idrisi API nor Martin should be exposed to the public internet. Keep Iket management/admin and enrollment surfaces private and protected by the controls supported by the deployed Iket version.

## Upstream route example

The file `infrastructure/iket/service.yaml.example` follows the service/route structure in Iket's repository examples. It is a template and must be validated with the exact Iket release deployed.

The example assumes:
- Internal Idrisi API DNS name: `idrisi-api`
- Internal Idrisi API port: `8080`
- Internal Martin DNS name: `martin`
- Internal Martin port: `3000`
- Public API prefix: `/api/idrisi/v1`
- Public tile prefix: `/api/idrisi/tiles`
- API upstream prefix: `/v1`
- Tile upstream prefix: `/`

Iket must preserve binary response bodies, `Content-Type`, cache headers, and query strings for tile requests. Configure authentication and rate policies for tile routes; do not assume that defining a route enables authentication automatically.

## Bootstrap and configuration

1. Select and pin a tested Iket release or immutable container image digest.
2. Follow Iket's official installation and first-run setup documentation.
3. Generate Iket's initial configuration and TLS assets using its supported CLI/bootstrap workflow.
4. Keep gateway main configuration, service route definitions, certificates, and logs in mounted paths.
5. Copy/adapt the example API and tile routes into the active Iket service configuration.
6. Attach the Iket container to the named `idrisi-internal` Docker network or declare it as an external network in the Iket Compose project.
7. Configure only the intended client-facing listener for client access.
8. Keep management and enrollment/admin ports private and protected using Iket's supported controls.
9. Validate configuration using the Iket version actually deployed.
10. Test API and tile paths from a client through Iket.

Official references:
- [Iket repository](https://github.com/andalus-ai/iket)
- [Iket documentation](https://iket-gateway.github.io/docs/)
- [Iket installation guide](https://iket-gateway.github.io/docs/install/)
- [Iket API-key management](https://iket-gateway.github.io/docs/api-key-management/)

## Identity and authorization

Gateway authentication is not a substitute for domain authorization.

- Idrisi must validate access to tenant-owned datasets, geofences, custom layers, and private package downloads.
- Martin is a tile engine, not an end-user authorization service. Do not publish private/tenant-specific data through a shared Martin source unless an authorization-aware design protects it.
- For the initial public tile proof of concept, publish only datasets approved for the route's access policy.
- If Iket forwards identity context, Idrisi must accept it only over a trusted network path and only in the documented format supported by the deployed configuration.
- Strip or overwrite client-supplied identity headers at the trusted edge.
- Never trust `X-User`, `X-Tenant`, or similar headers merely because they are present.
- Use separate service credentials for internal service-to-service calls where required.
- Fail closed for protected operations when identity context is absent or invalid.

## Request IDs and observability

Use one correlation ID across Iket and Idrisi where the deployed configuration supports safe propagation. Treat incoming client request IDs as untrusted input: validate length/characters or replace them. Do not log access tokens, API keys, or precise user coordinates by default.

## Operational requirements

- Pin Iket and Martin versions; avoid unbounded `latest` tags in production.
- Back up Iket configuration and administrative state using its documented storage mode.
- Keep certificates and private keys out of Git.
- Test certificate renewal/rotation and gateway restart behavior.
- Test route changes before publishing them.
- Verify timeout, body-size, streaming, range-request, and WebSocket behavior for the specific endpoints that need them.
- Verify PMTiles byte-range requests through the configured gateway path before relying on gateway proxying for offline packages.
- Ensure large downloads and tile requests have appropriate limits and caching behavior.
- Rotate the Martin read-only database password independently from the Idrisi catalog/migration credentials.

## Integration test checklist

- [ ] Iket starts with the pinned image and mounted configuration.
- [ ] Public health route reaches the Idrisi API.
- [ ] Public `/api/idrisi/v1` path maps to upstream `/v1`.
- [ ] Public `/api/idrisi/tiles` path maps to Martin without rewriting tile coordinates or source IDs.
- [ ] Tile responses preserve MVT content type and binary body.
- [ ] Martin is not reachable from the host or public internet.
- [ ] Martin database role cannot write to PostGIS or read outside the `tiles` schema.
- [ ] Protected API/tile routes reject unauthenticated requests.
- [ ] Tenant/resource authorization is enforced for private data.
- [ ] Request ID is correlated across gateway and API logs.
- [ ] Gateway management/admin surfaces are not public.
- [ ] PMTiles byte-range requests behave correctly if served through Iket.
