# Iket Gateway Integration

## Purpose

Idrisi uses [Iket](https://github.com/andalus-ai/iket) as its only public API gateway. Iket is maintained and released independently. Idrisi owns the geospatial domain APIs and configures Iket to route traffic to those private upstream services.

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
| Tenant/resource authorization | Idrisi domain services |
| Dataset catalog and provenance | Idrisi |
| Spatial data, tiles, routing and geocoding adapters | Idrisi |
| Data ingestion and offline package lifecycle | Idrisi |

## Network topology

Clients connect to Iket. Iket routes requests over a private Compose network to the Idrisi domain API. That API connects to PostGIS, Martin, Valhalla, geocoding providers, and artifact storage.

The Idrisi API should not publish a host port in the default self-hosted topology. Only Iket's intended client-facing listener should be reachable by clients. Keep Iket management/admin and enrollment surfaces private and protected by the controls supported by the deployed Iket version.

## Upstream route example

The file infrastructure/iket/service.yaml.example follows the service/route structure in Iket's repository examples. It is a template and must be validated with the exact Iket release deployed.

The example assumes:
- Internal Idrisi API DNS name: idrisi-api
- Internal API port: 8080
- Public prefix: /api/idrisi/v1
- Upstream prefix: /v1

Do not assume route configuration automatically enables authentication. Configure the relevant Iket auth/API-key policies explicitly, then test both allowed and denied requests.

## Bootstrap and configuration

1. Select and pin a tested Iket release or immutable container image digest.
2. Follow Iket's official installation and first-run setup documentation.
3. Generate Iket's initial configuration and TLS assets using its supported CLI/bootstrap workflow.
4. Keep gateway main configuration, service route definitions, certificates, and logs in mounted paths.
5. Copy/adapt the example route file into the active Iket service configuration.
6. Configure only the intended client-facing listener for client access.
7. Keep management and enrollment/admin ports private and protected using Iket's supported controls.
8. Validate configuration using the Iket version actually deployed.
9. Start Idrisi upstreams and test routes from a client through Iket.

Official references:
- [Iket repository](https://github.com/andalus-ai/iket)
- [Iket documentation](https://iket-gateway.github.io/docs/)
- [Iket installation guide](https://iket-gateway.github.io/docs/install/)
- [Iket API-key management](https://iket-gateway.github.io/docs/api-key-management/)

## Identity and authorization

Gateway authentication is not a substitute for domain authorization.

- Idrisi must validate access to tenant-owned datasets, geofences, custom layers, and private package downloads.
- If Iket forwards identity context, Idrisi must accept it only over a trusted network path and only in the documented format supported by the deployed configuration.
- Strip or overwrite client-supplied identity headers at the trusted edge.
- Never trust X-User, X-Tenant, or similar headers merely because they are present.
- Use separate service credentials for internal service-to-service calls where required.
- Fail closed for protected operations when identity context is absent or invalid.

## Request IDs and observability

Use one correlation ID across Iket and Idrisi where the deployed configuration supports safe propagation. Treat incoming client request IDs as untrusted input: validate length/characters or replace them. Do not log access tokens, API keys, or precise user coordinates by default.

## Operational requirements

- Pin Iket version; avoid relying on an unbounded latest tag in production.
- Back up Iket configuration and administrative state using its documented storage mode.
- Keep certificates and private keys out of Git.
- Test certificate renewal/rotation and gateway restart behavior.
- Test route changes before publishing them.
- Verify timeout, body-size, streaming, range-request, and WebSocket behavior for the specific endpoints that need them.
- Verify PMTiles range requests through the configured gateway path before relying on gateway proxying for offline packages.
- Ensure large downloads and tile requests have appropriate limits and caching behavior.

## Integration test checklist

- [ ] Iket starts with the pinned image and mounted configuration.
- [ ] Public health route reaches the Idrisi API.
- [ ] Public /api/idrisi/v1 path maps to upstream /v1.
- [ ] The Idrisi API is not reachable directly from the host.
- [ ] Protected route rejects unauthenticated requests.
- [ ] Authorized request reaches the correct upstream.
- [ ] Tenant/resource authorization is enforced by Idrisi.
- [ ] Request ID is correlated across gateway and API logs.
- [ ] Gateway management/admin surfaces are not public.
- [ ] PMTiles byte-range requests behave correctly if served through Iket.
