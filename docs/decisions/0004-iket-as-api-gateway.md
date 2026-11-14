# ADR 0004: Use Iket as the Idrisi API Gateway

- Status: Accepted
- Date: 2026-11-14

## Context

Idrisi requires a self-hosted API gateway for public ingress, TLS, upstream routing, authentication, rate limiting, traffic controls, and operator administration. The Andalus AI ecosystem already maintains Iket as a standalone Go API gateway with these capabilities.

Building a second gateway inside Idrisi would duplicate infrastructure, increase maintenance, and create inconsistent security and operational behavior across projects.

## Decision

Use [andalus-ai/iket](https://github.com/andalus-ai/iket) as the only public API gateway for Idrisi.

- Iket owns public ingress, TLS, gateway-level authentication/API-key policy, upstream routing, rate limits, traffic controls, and gateway administration.
- Idrisi owns Rust-based geospatial domain APIs, domain/resource authorization, provider adapters, data lifecycle, and geospatial contracts.
- Idrisi domain APIs remain private upstream services and may use Axum as an internal HTTP framework.
- Iket is deployed and versioned independently; Idrisi does not vendor or fork Iket.
- Gateway configuration is kept in Idrisi deployment examples and validated against the pinned Iket release.

## Consequences

Positive:
- Reuses a maintained gateway across Andalus AI projects.
- Avoids duplicating gateway routing, TLS, authentication, and traffic management.
- Keeps Idrisi focused on geospatial functionality.

Trade-offs:
- Idrisi deployment depends on a compatible Iket release and its configuration model.
- Gateway upgrades require compatibility and route smoke tests.
- Identity propagation must be explicitly configured and verified.
- Specialized geospatial delivery patterns (large range requests, tile caching, streaming) require end-to-end tests through Iket.

## Security constraints

- Do not expose Iket management/admin surfaces as public geospatial endpoints.
- Do not treat gateway authentication as sufficient for tenant/resource authorization.
- Do not trust client-supplied identity headers.
- Keep gateway certificates and secrets out of source control.
