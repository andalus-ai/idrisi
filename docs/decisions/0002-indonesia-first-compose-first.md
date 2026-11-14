# ADR 0002: Indonesia-First and Self-Hosted Compose-First Delivery

- Status: Accepted
- Date: 2026-11-14

## Context

Idrisi needs a useful first deployment without attempting global data coverage or a complex multi-cluster platform. The initial product should be deployable privately and iterated on real regional data.

## Decision

- Indonesia is the first geographic coverage target.
- The platform architecture supports road, terrain/elevation, satellite/aerial, nautical, aviation, and custom chart domains.
- Docker Compose on a single Linux host is the first supported deployment.
- Start with a bounded pilot region and expand coverage after ingestion, rendering, and operational costs are measured.
- Kubernetes and multi-region deployment are later options, not MVP requirements.

## Consequences

- The initial deployment is simpler to reproduce and debug.
- Indonesia-specific data validation, island/coastline handling, and CRS considerations become early test requirements.
- All chart domains require separate source/license/quality validation; broad architecture scope does not imply all datasets are immediately available.
- Compose configuration and persistent-volume semantics must be designed so future migration remains possible.
