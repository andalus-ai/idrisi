use chrono::Duration as ChronoDuration;
use idrisi_ingestion::{
    osm_import::{dataset_bounds, validate_import},
    repository::{ClaimedJob, JobRepository, JobStage},
};
use serde_json::Value;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::{env, error::Error, time::Duration};
use tokio::sync::watch;
use tracing::{error, info, warn};
use uuid::Uuid;

type PublisherError = Box<dyn Error + Send + Sync>;
const SUPPORTED_JOB_TYPES: &[&str] = &["osm.dataset.publish"];

struct Config {
    database_url: String,
    worker_id: String,
    poll_interval: Duration,
    lease_duration: Duration,
    heartbeat_interval: Duration,
}

impl Config {
    fn from_env() -> Result<Self, PublisherError> {
        let database_url = env::var("DATABASE_URL")?;
        let worker_id = env::var("IDRISI_PUBLISHER_ID")
            .unwrap_or_else(|_| format!("publisher-{}", Uuid::new_v4()));
        let poll_interval = Duration::from_secs(
            env::var("IDRISI_PUBLISHER_POLL_SECONDS")
                .unwrap_or_else(|_| "5".to_owned())
                .parse()?,
        );
        let lease_duration = Duration::from_secs(
            env::var("IDRISI_PUBLISHER_LEASE_SECONDS")
                .unwrap_or_else(|_| "300".to_owned())
                .parse()?,
        );
        let heartbeat_interval = Duration::from_secs(
            env::var("IDRISI_PUBLISHER_HEARTBEAT_SECONDS")
                .unwrap_or_else(|_| "30".to_owned())
                .parse()?,
        );
        if worker_id.trim().is_empty() || poll_interval.is_zero() || heartbeat_interval.is_zero() {
            return Err("publisher identity and intervals must be valid".into());
        }
        if lease_duration.is_zero()
            || lease_duration > Duration::from_secs(24 * 60 * 60)
            || lease_duration < heartbeat_interval * 2
        {
            return Err("publisher lease must be at least twice the heartbeat interval".into());
        }
        Ok(Self {
            database_url,
            worker_id,
            poll_interval,
            lease_duration,
            heartbeat_interval,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), PublisherError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&config.database_url)
        .await?;
    let repository = JobRepository::new(pool.clone());

    info!(worker_id = %config.worker_id, "Idrisi dataset publisher started");
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });

    loop {
        if *shutdown_rx.borrow() {
            break;
        }
        tokio::select! {
            _ = shutdown_rx.changed() => {
                info!("shutdown requested; publisher will stop claiming new jobs");
                break;
            }
            result = repository.claim(
                &config.worker_id,
                Uuid::new_v4(),
                config.lease_duration,
                SUPPORTED_JOB_TYPES,
            ) => {
                match result {
                    Ok(Some(job)) => {
                        info!(job_id = %job.id, source_job_id = ?job.source_job_id, attempt = job.attempt_count, "claimed publication job");
                        process_job(&repository, &pool, job, &config, shutdown_rx.clone()).await;
                    }
                    Ok(None) => tokio::time::sleep(config.poll_interval).await,
                    Err(error) => {
                        error!(%error, "failed to claim publication job");
                        tokio::time::sleep(config.poll_interval).await;
                    }
                }
            }
        }
    }

    info!("Idrisi dataset publisher stopped");
    Ok(())
}

async fn process_job(
    repository: &JobRepository,
    pool: &PgPool,
    job: ClaimedJob,
    config: &Config,
    mut shutdown: watch::Receiver<bool>,
) {
    let job_id = job.id;
    let lease_token = job.lease_token;
    if *shutdown.borrow() {
        let _ = repository.release(job_id, lease_token).await;
        return;
    }

    let work = async {
        if job.job_type != "osm.dataset.publish" || job.importer_name != "idrisi-publisher" {
            return Err("publication job type or importer is not registered".to_owned());
        }
        let source_job_id = job
            .source_job_id
            .ok_or_else(|| "publication job must reference a source import job".to_owned())?;

        repository
            .set_stage(job_id, lease_token, JobStage::Validating, 35)
            .await
            .map_err(|_| "publication job lease was lost".to_owned())?;

        let source = sqlx::query_as::<_, (Uuid, String, String, Option<String>, Value)>(
            "SELECT dataset_id, status, job_type, staging_schema, result_metadata
             FROM ingestion_jobs WHERE id = $1",
        )
        .bind(source_job_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| "could not load source import metadata".to_owned())?
        .ok_or_else(|| "source import job does not exist".to_owned())?;

        let (source_dataset_id, source_status, source_type, staging_schema, source_metadata) =
            source;
        if source_dataset_id != job.dataset_id
            || source_status != "succeeded"
            || source_type != "osm.pbf.import"
            || source_metadata.get("state").and_then(Value::as_str)
                != Some("imported_validated_unpublished")
        {
            return Err(
                "source import is not a validated unpublished job for this dataset".to_owned(),
            );
        }

        let schema = staging_schema
            .ok_or_else(|| "source import did not record its staging schema".to_owned())?;
        let metadata_schema = source_metadata
            .pointer("/staging/data_schema")
            .and_then(Value::as_str)
            .ok_or_else(|| "source import metadata has no data schema".to_owned())?;
        if schema != metadata_schema {
            return Err("source import staging schema metadata is inconsistent".to_owned());
        }

        let bounds = dataset_bounds(pool, job.dataset_id)
            .await
            .map_err(|error| format!("could not read dataset coverage: {error}"))?;
        let validation = validate_import(pool, &schema, &bounds)
            .await
            .map_err(|error| format!("pre-publication validation failed: {error}"))?;

        repository
            .set_stage(job_id, lease_token, JobStage::Publishing, 90)
            .await
            .map_err(|_| "publication job lease was lost".to_owned())?;

        let result = sqlx::query_scalar::<_, Value>("SELECT idrisi_publish_osm_dataset($1, $2)")
            .bind(job_id)
            .bind(lease_token)
            .fetch_one(pool)
            .await
            .map_err(|error| format!("atomic publication transaction failed: {error}"))?;

        Ok(serde_json::json!({
            "publication": result,
            "pre_publication_validation": validation
        }))
    };

    tokio::pin!(work);
    let mut heartbeat = tokio::time::interval(config.heartbeat_interval);
    heartbeat.tick().await;

    let outcome: Result<Value, String> = loop {
        tokio::select! {
            result = &mut work => break result,
            _ = heartbeat.tick() => {
                if let Err(error) = repository.heartbeat(job_id, lease_token, config.lease_duration).await {
                    warn!(job_id = %job_id, %error, "publisher lost its lease; abandoning publication handler");
                    return;
                }
            }
            changed = shutdown.changed() => {
                if changed.is_ok() && *shutdown.borrow() {
                    warn!(job_id = %job_id, "shutdown requested; releasing publication job");
                    let _ = repository.release(job_id, lease_token).await;
                    return;
                }
            }
        }
    };

    match outcome {
        Ok(result) => {
            // The SECURITY DEFINER publication function atomically completed the job
            // together with the table promotion and dataset-version record.
            info!(job_id = %job_id, result = %result, "dataset publication committed");
        }
        Err(message) => {
            let delay = retry_delay(job.attempt_count);
            if let Err(error) = repository
                .retry(
                    job_id,
                    lease_token,
                    "dataset_publication_failed",
                    &message,
                    delay,
                )
                .await
            {
                warn!(job_id = %job_id, %error, "could not record publication failure; lease may have expired");
            } else {
                warn!(job_id = %job_id, error = %message, "dataset publication failed; retry policy applied");
            }
        }
    }
}

fn retry_delay(attempt: i32) -> ChronoDuration {
    let exponent = u32::try_from(attempt.saturating_sub(1)).unwrap_or(0).min(5);
    ChronoDuration::seconds((30_i64 * 2_i64.pow(exponent)).min(1800))
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut terminate = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
mod tests {
    use super::retry_delay;

    #[test]
    fn retry_backoff_is_bounded() {
        assert_eq!(retry_delay(1).num_seconds(), 30);
        assert_eq!(retry_delay(2).num_seconds(), 60);
        assert_eq!(retry_delay(20).num_seconds(), 960);
    }
}
