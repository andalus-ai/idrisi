use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Serialize;
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
    use super::app;
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
