//! PostGIS validation for per-attempt OSM staging schemas.
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum OSMImportError {
    #[error("unsafe OSM staging schema")]
    InvalidSchema,
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

fn validate_schema(schema: &str) -> bool {
    let Some(suffix) = schema.strip_prefix("idrisi_stage_") else {
        return false;
    };
    suffix.len() == 32 && suffix.chars().all(|character| character.is_ascii_hexdigit())
}

pub async fn create_namespace(
    pool: &PgPool,
    lease_token: Uuid,
) -> Result<(String, String), OSMImportError> {
    let result = sqlx::query_as::<_, (String, String)>(
        "SELECT data_schema, middle_schema FROM idrisi_create_osm_staging_schemas($1)",
    )
    .bind(lease_token)
    .fetch_one(pool)
    .await?;
    if !validate_schema(&result.0)
        || !result.1.starts_with("idrisi_mid_")
        || result.1.len() != "idrisi_mid_".len() + 32
    {
        return Err(OSMImportError::InvalidSchema);
    }
    Ok(result)
}

pub async fn drop_namespace(pool: &PgPool, lease_token: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT idrisi_drop_osm_staging_schemas($1)")
        .bind(lease_token)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn dataset_bounds(
    pool: &PgPool,
    dataset_id: Uuid,
) -> Result<DatasetBounds, OSMImportError> {
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
    bounds: &DatasetBounds,
) -> Result<Value, OSMImportError> {
    if !validate_schema(schema) {
        return Err(OSMImportError::InvalidSchema);
    }
    let mut layers = serde_json::Map::new();
    let mut total_features = 0_i64;
    for layer in ["osm_roads", "osm_places", "osm_buildings", "osm_water"] {
        let query = format!(
            "SELECT COUNT(*)::bigint, \
             COUNT(*) FILTER (WHERE geom IS NULL OR ST_IsEmpty(geom))::bigint, \
             COUNT(*) FILTER (WHERE geom IS NOT NULL AND ST_SRID(geom) <> 4326)::bigint, \
             COUNT(*) FILTER (WHERE geom IS NOT NULL AND NOT ST_IsValid(geom))::bigint, \
             ST_Extent(geom)::text \
             FROM {schema}.{layer}"
        );
        let (count, empty, wrong_srid, invalid, extent): (i64, i64, i64, i64, Option<String>) =
            sqlx::query_as(&query)
                .fetch_one(pool)
                .await
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
        "data_schema": schema,
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
    fn validates_only_lease_derived_data_schemas() {
        assert!(validate_schema("idrisi_stage_0123456789abcdef0123456789abcdef"));
        assert!(!validate_schema("tiles"));
        assert!(!validate_schema("idrisi_stage_0123;drop"));
    }

    #[test]
    fn parses_postgis_extent() {
        assert_eq!(
            parse_extent("BOX(107.3 -7.1,107.8 -6.7)").unwrap(),
            (107.3, -7.1, 107.8, -6.7)
        );
    }

    #[test]
    fn rejects_malformed_extent() {
        assert!(parse_extent("not an extent").is_err());
    }
}
