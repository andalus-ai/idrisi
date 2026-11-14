-- Idrisi catalog foundation. All timestamps are UTC; geometry payloads live in PostGIS
-- or external object storage, while this catalog records their provenance and lifecycle.
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS postgis;

CREATE TABLE data_sources (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name text NOT NULL,
    source_type text NOT NULL,
    source_uri text,
    license_id text NOT NULL,
    license_url text,
    attribution text NOT NULL,
    terms_url text,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT data_sources_name_nonempty CHECK (length(trim(name)) > 0),
    CONSTRAINT data_sources_type_nonempty CHECK (length(trim(source_type)) > 0),
    CONSTRAINT data_sources_license_nonempty CHECK (length(trim(license_id)) > 0),
    CONSTRAINT data_sources_attribution_nonempty CHECK (length(trim(attribution)) > 0)
);

CREATE TABLE datasets (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    slug text NOT NULL UNIQUE,
    name text NOT NULL,
    description text,
    domain text NOT NULL,
    status text NOT NULL DEFAULT 'draft',
    default_source_id uuid REFERENCES data_sources(id) ON DELETE RESTRICT,
    coverage geometry(MultiPolygon, 4326),
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT datasets_slug_format CHECK (slug ~ '^[a-z0-9][a-z0-9._-]*$'),
    CONSTRAINT datasets_name_nonempty CHECK (length(trim(name)) > 0),
    CONSTRAINT datasets_domain_nonempty CHECK (length(trim(domain)) > 0),
    CONSTRAINT datasets_status CHECK (status IN ('draft', 'processing', 'published', 'deprecated', 'archived'))
);
CREATE INDEX datasets_status_idx ON datasets(status);
CREATE INDEX datasets_domain_idx ON datasets(domain);
CREATE INDEX datasets_coverage_gix ON datasets USING gist(coverage);

CREATE TABLE dataset_versions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    dataset_id uuid NOT NULL REFERENCES datasets(id) ON DELETE CASCADE,
    version text NOT NULL,
    status text NOT NULL DEFAULT 'staged',
    source_id uuid NOT NULL REFERENCES data_sources(id) ON DELETE RESTRICT,
    source_version text,
    source_checksum text,
    coverage geometry(MultiPolygon, 4326),
    feature_count bigint,
    storage_uri text,
    published_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    UNIQUE(dataset_id, version),
    CONSTRAINT dataset_versions_status CHECK (status IN ('staged', 'validated', 'published', 'superseded', 'failed')),
    CONSTRAINT dataset_versions_feature_count CHECK (feature_count IS NULL OR feature_count >= 0)
);
CREATE INDEX dataset_versions_dataset_idx ON dataset_versions(dataset_id, created_at DESC);
CREATE INDEX dataset_versions_coverage_gix ON dataset_versions USING gist(coverage);

CREATE TABLE layers (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    dataset_id uuid NOT NULL REFERENCES datasets(id) ON DELETE CASCADE,
    slug text NOT NULL,
    name text NOT NULL,
    chart_domain text NOT NULL,
    geometry_type text,
    min_zoom smallint NOT NULL DEFAULT 0,
    max_zoom smallint NOT NULL DEFAULT 14,
    is_enabled boolean NOT NULL DEFAULT true,
    attribution_override text,
    style jsonb NOT NULL DEFAULT '{}'::jsonb,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(dataset_id, slug),
    CONSTRAINT layers_slug_format CHECK (slug ~ '^[a-z0-9][a-z0-9._-]*$'),
    CONSTRAINT layers_name_nonempty CHECK (length(trim(name)) > 0),
    CONSTRAINT layers_zoom_range CHECK (min_zoom >= 0 AND max_zoom <= 30 AND min_zoom <= max_zoom)
);
CREATE INDEX layers_dataset_idx ON layers(dataset_id);
CREATE INDEX layers_domain_idx ON layers(chart_domain);
