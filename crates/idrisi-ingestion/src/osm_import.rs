//! Isolated OSM flex import rendering and PostGIS validation.
use serde_json::{json, Value};
use sqlx::PgPool;

const STYLE_TEMPLATE: &str = include_str!("../../../../infrastructure/osm2pgsql/bandung.lua");
const IMPORT_SCHEMA: &str = "tiles_import";

#[derive(Debug, thiserror::Error)]
pub enum OSMImportError {
    #[error("invalid import schema or table prefix")]
    InvalidIdentifier,
    #[error("dataset coverage is missing")]
    MissingCoverage,
    #[error("imported layer {0} is missing")]
    MissingLayer(&'static str),
    #[error("imported layer {0} contains invalid geometries")]
    InvalidGeometry(&'static str),
    #[error("imported roads layer is empty")]
    EmptyRoads,
    #[error("imported layer {0} lies outside dataset coverage")]
    OutsideCoverage(&'static str),
    #[error("could not validate imported data: {0}")]
    Database(#[from] sqlx::Error),
    #[error("invalid PostGIS extent")]
    InvalidExtent,
}

#[derive(Debug, Clone)]
pub struct DatasetBounds {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

impl DatasetBounds {
    pub fn as_bbox(&self) -> String {
        format!("{},{},{},{}", self.west, self.south, self.east, self.north)
    }
}

pub fn validate_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c == '_' || c.is_ascii_lowercase())
        && chars.all(|c| c == '_' || c.is_ascii_lowercase() || c.is_ascii_digit())
        && value.len() <= 63
}

/// Render the reviewed Lua recipe with unique per-attempt table names.
/// Only fixed table-name literals are replaced; no job-provided Lua is accepted.
pub fn render_style(schema: &str, prefix: &str) -> Result<String, OSMImportError> {
    if schema != IMPORT_SCHEMA || !validate_identifier(prefix) {
        return Err(OSMImportError::InvalidIdentifier);
    }
    let mut style = STYLE_TEMPLATE.replace("{ schema = 'tiles' }", "{ schema = 'tiles_import' }");
    for layer in ["osm_roads", "osm_places", "osm_buildings", "osm_water"] {
        let from = format!("('{layer}'");
        let to = format!("('{prefix}_{layer}'");
        style = style.replace(&from, &to);
    }
    if style.matches("{ schema = 'tiles_import' }").count() != 4 {
        return Err(OSMImportError::InvalidIdentifier);
    }
    Ok(style)
}

pub async fn dataset_bounds(pool: &PgPool, dataset_id: uuid::Uuid) -> Result<DatasetBounds, OSMImportError> {
    let row = sqlx::query_as::<_, (Option<f64>, Option<f64>, Option<f64>, Option<f64>)>(
        "SELECT ST_XMin(Box3D(coverage)), ST_YMin(Box3D(coverage)), ST_XMax(Box3D(coverage)), ST_YMax(Box3D(coverage)) FROM datasets WHERE id = $1 AND coverage IS NOT NULL",
    )
    .bind(dataset_id)
    .fetch_optional(pool)
    .await?
    .ok_or(OSMImportError::MissingCoverage)?;
    match row {
        (Some(west), Some(south), Some(east), Some(north))
            if west < east && south < north =>
        {
            Ok(DatasetBounds { west, south, east, north })
        }
        _ => Err(OSMImportError::MissingCoverage),
    }
}

fn parse_extent(value: &str) -> Result<(f64, f64, f64, f64), OSMImportError> {
    let inner = value
        .strip_prefix("BOX(")
        .and_then(|value| value.strip_suffix(')'))
        .ok_or(OSMImportError::InvalidExtent)?;
    let mut corners = inner.split(',');
    let lower = corners.next().ok_or(OSMImportError::InvalidExtent)?;
    let upper = corners.next().ok_or(OSMImportError::InvalidExtent)?;
    if corners.next().is_some() {
        return Err(OSMImportError::InvalidExtent);
    }
    let parse_pair = |pair: &str| -> Result<(f64, f64), OSMImportError> {
        let mut values = pair.split_whitespace().map(str::parse::<f64>);
        let x = values
            .next()
            .ok_or(OSMImportError::InvalidExtent)?
            .map_err(|_| OSMImportError::InvalidExtent)?;
        let y = values
            .next()
            .ok_or(OSMImportError::InvalidExtent)?
            .map_err(|_| OSMImportError::InvalidExtent)?;
        if values.next().is_some() || !x.is_finite() || !y.is_finite() {
            return Err(OSMImportError::InvalidExtent);
        }
        Ok((x, y))
    };
    let (west, south) = parse_pair(lower)?;
    let (east, north) = parse_pair(upper)?;
    Ok((west, south, east, north))
}

pub async fn validate_import(
    pool: &PgPool,
    schema: &str,
    prefix: &str,
    bounds: &DatasetBounds,
) -> Result<Value, OSMImportError> {
    if schema != IMPORT_SCHEMA || !validate_identifier(prefix) {
        return Err(OSMImportError::InvalidIdentifier);
    }
    let mut layers = serde_json::Map::new();
    let mut total_features = 0_i64;
    for layer in ["osm_roads", "osm_places", "osm_buildings", "osm_water"] {
        let table = format!("{prefix}_{layer}");
        let query = format!(
            "SELECT COUNT(*)::bigint, \
             COUNT(*) FILTER (WHERE geom IS NULL OR ST_IsEmpty(geom))::bigint, \
             COUNT(*) FILTER (WHERE geom IS NOT NULL AND ST_SRID(geom) <> 4326)::bigint, \
             COUNT(*) FILTER (WHERE geom IS NOT NULL AND NOT ST_IsValid(geom))::bigint, \
             ST_Extent(geom)::text \
             FROM {schema}.{table}"
        );
        let (count, empty, wrong_srid, invalid, extent): (i64, i64, i64, i64, Option<String>) =
            sqlx::query_as(&query).fetch_one(pool).await
                .map_err(|error| match error {
                    sqlx::Error::Database(_) => OSMImportError::MissingLayer(layer),
                    other => OSMImportError::Database(other),
                })?;
        if empty + wrong_srid + invalid > 0 {
            return Err(OSMImportError::InvalidGeometry(layer));
        }
        if layer == "osm_roads" && count == 0 {
            return Err(OSMImportError::EmptyRoads);
        }
        if let Some(extent) = extent {
            let (west, south, east, north) = parse_extent(&extent)?;
            let tolerance = 0.0001_f64;
            if west < bounds.west - tolerance
                || south < bounds.south - tolerance
                || east > bounds.east + tolerance
                || north > bounds.north + tolerance
            {
                return Err(OSMImportError::OutsideCoverage(layer));
            }
        }
        total_features += count;
        layers.insert(layer.to_owned(), json!({
            "feature_count": count,
            "empty_or_null_geometry_count": empty,
            "wrong_srid_count": wrong_srid,
            "invalid_geometry_count": invalid
        }));
    }
    Ok(json!({
        "schema": schema,
        "table_prefix": prefix,
        "feature_count": total_features,
        "layers": Value::Object(layers),
        "validation": "passed",
        "publication": "not_published"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_unique_tables_only_in_import_schema() {
        let style = render_style("tiles_import", "j0123456789abcdef").unwrap();
        assert!(style.contains("define_way_table('j0123456789abcdef_osm_roads'"));
        assert!(style.contains("{ schema = 'tiles_import' }"));
        assert!(!style.contains("{ schema = 'tiles' }"));
    }

    #[test]
    fn rejects_unsafe_identifiers() {
        assert!(render_style("tiles", "jgood").is_err());
        assert!(render_style("tiles_import", "bad;drop").is_err());
        assert!(render_style("tiles_import", "JUPPER").is_err());
    }

    #[test]
    fn parses_postgis_extent() {
        assert_eq!(parse_extent("BOX(107.3 -7.1,107.8 -6.7)").unwrap(), (107.3, -7.1, 107.8, -6.7));
    }

    #[test]
    fn rejects_malformed_extent() {
        assert!(parse_extent("not an extent").is_err());
    }
}
