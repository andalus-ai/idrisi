# ADR 0003: Versioned Datasets and Provider Contracts

- Status: Accepted
- Date: 2026-11-14

## Context

Map tiles, routing graphs, geocoder indexes, raster layers, and specialist chart data are produced by different tools and update at different rates. Clients need stable behavior and operators need reproducible publication and rollback.

## Decision

- Every published dataset and derived artifact has an immutable version and provenance manifest.
- Provider implementations are accessed through Idrisi-owned contracts.
- Map, routing, geocoder, and imagery versions are tracked independently but can be associated in a release manifest.
- New artifacts are staged and validated before publication.
- Publication updates catalog references atomically; the previous known-good version remains available for rollback.
- Public responses identify relevant dataset/provider versions when meaningful.

## Consequences

- More metadata and release orchestration are required.
- Clients and operators can reason about freshness and compatibility.
- Provider changes are isolated behind adapters and contract tests.
