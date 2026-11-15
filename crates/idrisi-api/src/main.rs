use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use serde_json::Value;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::{env, net::SocketAddr, time::Duration};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    pool: Option<PgPool>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    version: &'static str,
}

#[derive(Serialize, sqlx::FromRow)]
struct DatasetSummary {
    id: Uuid,
    slug: String,
    name: String,
    description: Option<String>,
    domain: String,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}


#[derive(Serialize)]
struct DatasetTileCatalog {
    dataset_id: Uuid,
    dataset_slug: String,
    dataset_name: String,
    domain: String,
    version: String,
    published_at: Option<chrono::DateTime<chrono::Utc>>,
    feature_count: Option<i64>,
    source_checksum: Option<String>,
    attribution: String,
    tilejson_base: String,
    sources: Vec<TileSource>,
}

#[derive(Serialize)]
struct TileSource {
    layer_id: Uuid,
    layer_slug: String,
    name: String,
    chart_domain: String,
    geometry_type: Option<String>,
    min_zoom: i16,
    max_zoom: i16,
    tile_source: String,
    tilejson_url: String,
    attribution: String,
    style: Value,
}

#[derive(sqlx::FromRow)]
struct LayerRecord {
    id: Uuid,
    slug: String,
    name: String,
    chart_domain: String,
    geometry_type: Option<String>,
    min_zoom: i16,
    max_zoom: i16,
    attribution_override: Option<String>,
    style: Value,
    metadata: Value,
}

fn martin_source_for_layer(slug: &str, metadata: &Value) -> Option<&'static str> {
    if let Some(source) = metadata.get("tile_source").and_then(Value::as_str) {
        return match source {
            "osm_roads" => Some("osm_roads"),
            "osm_places" => Some("osm_places"),
            "osm_buildings" => Some("osm_buildings"),
            "osm_water" => Some("osm_water"),
            _ => None,
        };
    }
    match slug {
        "roads" | "osm_roads" => Some("osm_roads"),
        "places" | "osm_places" | "pois" => Some("osm_places"),
        "buildings" | "osm_buildings" => Some("osm_buildings"),
        "water" | "osm_water" => Some("osm_water"),
        _ => None,
    }
}

async fn get_dataset_tile_catalog(
    State(state): State<AppState>,
    Path(dataset_slug): Path<String>,
) -> Result<Json<DatasetTileCatalog>, StatusCode> {
    let Some(pool) = state.pool else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let dataset = sqlx::query_as::<_, (Uuid, String, String, String, String, Option<chrono::DateTime<chrono::Utc>>, Option<i64>, Option<String>, String)>(
        "SELECT d.id, d.slug, d.name, d.domain, v.version, v.published_at,
                v.feature_count, v.source_checksum, COALESCE(s.attribution, 'Unknown attribution')
         FROM datasets d
         JOIN dataset_versions v ON v.dataset_id = d.id AND v.status = 'published'
         LEFT JOIN data_sources s ON s.id = v.source_id
         WHERE d.slug = $1 AND d.status = 'published'
         ORDER BY v.published_at DESC NULLS LAST, v.created_at DESC
         LIMIT 1",
    )
    .bind(&dataset_slug)
    .fetch_optional(&pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, dataset_slug, "failed to resolve published dataset tile catalog");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let layers = sqlx::query_as::<_, LayerRecord>(
        "SELECT id, slug, name, chart_domain, geometry_type, min_zoom, max_zoom,
                attribution_override, style, metadata
         FROM layers
         WHERE dataset_id = $1 AND is_enabled = true
         ORDER BY slug",
    )
    .bind(dataset.0)
    .fetch_all(&pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, dataset_slug, "failed to list dataset tile layers");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let sources = layers
        .into_iter()
        .filter_map(|layer| {
            let tile_source = martin_source_for_layer(&layer.slug, &layer.metadata)?;
            Some(TileSource {
                layer_id: layer.id,
                layer_slug: layer.slug,
                name: layer.name,
                chart_domain: layer.chart_domain,
                geometry_type: layer.geometry_type,
                min_zoom: layer.min_zoom,
                max_zoom: layer.max_zoom,
                tile_source: tile_source.to_owned(),
                tilejson_url: format!("/api/idrisi/tiles/{tile_source}"),
                attribution: layer.attribution_override.clone().unwrap_or_else(|| dataset.8.clone()),
                style: layer.style,
            })
        })
        .collect();

    Ok(Json(DatasetTileCatalog {
        dataset_id: dataset.0,
        dataset_slug: dataset.1,
        dataset_name: dataset.2,
        domain: dataset.3,
        version: dataset.4,
        published_at: dataset.5,
        feature_count: dataset.6,
        source_checksum: dataset.7,
        attribution: dataset.8,
        tilejson_base: "/api/idrisi/tiles".to_owned(),
        sources,
    }))
}

async fn live() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "alive",
        service: "idrisi-api",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn ready(State(state): State<AppState>) -> Result<Json<HealthResponse>, StatusCode> {
    let Some(pool) = state.pool else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    sqlx::query("SELECT 1")
        .execute(&pool)
        .await
        .map_err(|error| {
            tracing::warn!(%error, "PostGIS readiness check failed");
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    Ok(Json(HealthResponse {
        status: "ready",
        service: "idrisi-api",
        version: env!("CARGO_PKG_VERSION"),
    }))
}

async fn list_datasets(
    State(state): State<AppState>,
) -> Result<Json<Vec<DatasetSummary>>, StatusCode> {
    let Some(pool) = state.pool else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let datasets = sqlx::query_as::<_, DatasetSummary>(
        "SELECT id, slug, name, description, domain, status, created_at, updated_at
         FROM datasets ORDER BY name ASC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, "failed to list datasets");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(datasets))
}

async fn get_dataset(
    State(state): State<AppState>,
    Path(dataset_id): Path<Uuid>,
) -> Result<Json<DatasetSummary>, StatusCode> {
    let Some(pool) = state.pool else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let dataset = sqlx::query_as::<_, DatasetSummary>(
        "SELECT id, slug, name, description, domain, status, created_at, updated_at
         FROM datasets WHERE id = $1",
    )
    .bind(dataset_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| {
        tracing::error!(%error, "failed to retrieve dataset");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(dataset))
}

fn app(pool: Option<PgPool>) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/v1/catalog/datasets", get(list_datasets))
        .route("/v1/catalog/datasets/{dataset_id}", get(get_dataset))
        .route("/v1/catalog/datasets/{dataset_slug}/tile-catalog", get(get_dataset_tile_catalog))
        .with_state(AppState { pool })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let database_url = env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL must be configured for the Idrisi API")?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("PostGIS connection established and catalog migrations applied");

    let bind = env::var("IDRISI_API_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());
    let address: SocketAddr = bind.parse()?;
    let listener = TcpListener::bind(address).await?;

    tracing::info!(%address, "Idrisi domain API listening");

    axum::serve(listener, app(Some(pool)))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install termination signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received");
}

#[cfg(test)]
mod tests {
    use super::{app, martin_source_for_layer};
    use serde_json::json;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn liveness_endpoint_returns_alive() {
        let response = app(None)
            .oneshot(
                Request::builder()
                    .uri("/health/live")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn readiness_fails_without_database() {
        let response = app(None)
            .oneshot(
                Request::builder()
                    .uri("/health/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        );
    }
}
    #[test]
    fn tile_source_mapping_is_allowlisted() {
        assert_eq!(martin_source_for_layer("roads", &json!({})), Some("osm_roads"));
        assert_eq!(
            martin_source_for_layer("custom", &json!({"tile_source":"osm_water"})),
            Some("osm_water")
        );
        assert_eq!(
            martin_source_for_layer("custom", &json!({"tile_source":"private_schema.secret"})),
            None
        );
        assert_eq!(martin_source_for_layer("unknown", &json!({})), None);
    }

