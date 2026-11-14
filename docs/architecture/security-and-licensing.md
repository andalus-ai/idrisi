# Security, Privacy, Licensing, and Attribution

## Security boundaries

- Public clients connect only to the gateway or explicitly published static artifact endpoints.
- Databases, routing engines, geocoders, and tile backends remain on private networks by default.
- Use separate credentials for migrations, runtime services, ingestion workers, and backup jobs.
- Scope API keys and tokens to tenant, service, layer, and permitted operations.
- Rate-limit and bound expensive operations including route matrices, large geometry queries, geocoding, exports, and tile generation.
- Apply request size, geometry vertex count, coordinate range, timeout, and concurrency limits.
- Use TLS at exposed boundaries and document trusted reverse-proxy configuration.

## Privacy

Location traces, search history, geofences, and user-created places may reveal sensitive behavior. Collect only what is needed, define retention periods, restrict access, and support deletion where appropriate. Avoid storing precise request coordinates in routine logs. Provide configurable retention and audit policies for private deployments.

## Data licensing

Every dataset must be independently reviewed. Record:
- License and version (or source terms)
- Required attribution
- Redistribution and derivative-work conditions
- Commercial-use restrictions, if any
- Share-alike or database obligations
- Access restrictions and expiry
- Applicable jurisdiction and source authority

OpenStreetMap data is licensed under the ODbL and requires attribution. The team must assess whether a particular use creates a derivative database or other obligations; this document is not legal advice. Do not assume that combining OSM with another dataset removes or changes the obligations.

Satellite imagery, elevation, hydrographic, and aeronautical data often have distinct access, display, caching, and redistribution conditions. Review the exact product terms before ingestion or publication. Keep source-specific attribution available in map UI and API metadata.

## Supply chain

- Pin container image versions; use digests for production deployments where practical.
- Track Rust dependencies and licenses.
- Scan images and dependencies in CI when implementation begins.
- Do not commit secrets, tokens, private data, or licensed source datasets.
- Maintain SBOMs and document upgrade/patch cadence for exposed services.

## Operational controls

- Encrypt backups and restrict backup access.
- Test restore, not just backup creation.
- Audit dataset publication, permission changes, and administrative operations.
- Separate development/test data from production user data.
- Define incident response and credential rotation procedures before public exposure.
