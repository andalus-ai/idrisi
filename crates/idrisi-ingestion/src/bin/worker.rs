use chrono::Duration as ChronoDuration;
use idrisi_ingestion::{
    acquisition::{download, DownloadPolicy},
    inspector::{inspect, InspectionPolicy},
    repository::{ClaimedJob, JobRepository, JobStage},
};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use std::{collections::HashSet, env, error::Error, path::PathBuf, time::Duration};
use tokio::sync::watch;
use tracing::{error, info, warn};
use uuid::Uuid;

type WorkerError = Box<dyn Error + Send + Sync>;
const SUPPORTED_JOB_TYPES: &[&str] = &["osm.pbf.stage"];

struct Config {
    database_url: String,
    worker_id: String,
    data_dir: PathBuf,
    allowed_hosts: HashSet<String>,
    max_source_bytes: u64,
    poll_interval: Duration,
    lease_duration: Duration,
    heartbeat_interval: Duration,
    request_timeout: Duration,
    osmium_bin: String,
    inspection_timeout: Duration,
    max_inspection_report_bytes: usize,
    osmium_executable: PathBuf,
    inspection_timeout: Duration,
}

impl Config {
    fn from_env() -> Result<Self, WorkerError> {
        let database_url = env::var("DATABASE_URL")?;
        let worker_id =
            env::var("IDRISI_WORKER_ID").unwrap_or_else(|_| format!("worker-{}", Uuid::new_v4()));
        let data_dir = PathBuf::from(
            env::var("IDRISI_WORKER_DATA_DIR")
                .unwrap_or_else(|_| "/var/lib/idrisi/ingestion".to_owned()),
        );
        let allowed_hosts = env::var("IDRISI_WORKER_ALLOWED_HOSTS")
            .unwrap_or_else(|_| "download.geofabrik.de".to_owned())
            .split(',')
            .map(str::trim)
            .filter(|host| !host.is_empty())
            .map(str::to_ascii_lowercase)
            .collect::<HashSet<_>>();
        if allowed_hosts.is_empty() {
            return Err("IDRISI_WORKER_ALLOWED_HOSTS must contain at least one host".into());
        }

        let max_source_bytes = env::var("IDRISI_WORKER_MAX_SOURCE_BYTES")
            .unwrap_or_else(|_| (20_u64 * 1024 * 1024 * 1024).to_string())
            .parse::<u64>()?;
        let poll_interval = Duration::from_secs(
            env::var("IDRISI_WORKER_POLL_SECONDS")
                .unwrap_or_else(|_| "5".to_owned())
                .parse()?,
        );
        let lease_duration = Duration::from_secs(
            env::var("IDRISI_WORKER_LEASE_SECONDS")
                .unwrap_or_else(|_| "300".to_owned())
                .parse()?,
        );
        let heartbeat_interval = Duration::from_secs(
            env::var("IDRISI_WORKER_HEARTBEAT_SECONDS")
                .unwrap_or_else(|_| "30".to_owned())
                .parse()?,
        );
        let request_timeout = Duration::from_secs(
            env::var("IDRISI_WORKER_REQUEST_TIMEOUT_SECONDS")
                .unwrap_or_else(|_| "14400".to_owned())
                .parse()?,
        );
        let osmium_bin = env::var("IDRISI_WORKER_OSMIUM_BIN")
            .unwrap_or_else(|_| "/usr/bin/osmium".to_owned());
        let osmium_executable = PathBuf::from(
            env::var("IDRISI_OSMIUM_PATH").unwrap_or_else(|_| "/usr/bin/osmium".to_owned()),
        );
        let inspection_timeout = Duration::from_secs(
            env::var("IDRISI_WORKER_INSPECTION_TIMEOUT_SECONDS")
                .unwrap_or_else(|_| "1800".to_owned())
                .parse()?,
        );
        let max_inspection_report_bytes = env::var("IDRISI_WORKER_MAX_INSPECTION_REPORT_BYTES")
            .unwrap_or_else(|_| (2 * 1024 * 1024).to_string())
            .parse::<usize>()?;
        if max_source_bytes == 0
            || poll_interval.is_zero()
            || heartbeat_interval.is_zero()
            || request_timeout.is_zero()
            || inspection_timeout.is_zero()
            || max_inspection_report_bytes == 0
            || osmium_executable.as_os_str().is_empty()
        {
            return Err("worker byte and duration settings must be positive".into());
        }

        Ok(Self {
            database_url,
            worker_id,
            data_dir,
            allowed_hosts,
            max_source_bytes,
            poll_interval,
            lease_duration,
            heartbeat_interval,
            request_timeout,
            osmium_bin,
            inspection_timeout,
            max_inspection_report_bytes,
            osmium_executable,
            inspection_timeout,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), WorkerError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env()?;
    if config.lease_duration.is_zero()
        || config.lease_duration > Duration::from_secs(24 * 60 * 60)
        || config.lease_duration < config.heartbeat_interval * 2
    {
        return Err("worker lease must be at least twice the heartbeat interval".into());
    }

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&config.database_url)
        .await?;
    let repository = JobRepository::new(pool);

    info!(
        worker_id = %config.worker_id,
        allowed_hosts = ?config.allowed_hosts,
        "Idrisi ingestion worker started"
    );

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
                info!("shutdown requested; worker will stop claiming new jobs");
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
                        info!(job_id = %job.id, job_type = %job.job_type, attempt = job.attempt_count, "claimed ingestion job");
                        process_job(&repository, job, &config, shutdown_rx.clone()).await;
                    }
                    Ok(None) => tokio::time::sleep(config.poll_interval).await,
                    Err(error) => {
                        error!(%error, "failed to claim ingestion job");
                        tokio::time::sleep(config.poll_interval).await;
                    }
                }
            }
        }
    }

    info!("Idrisi ingestion worker stopped");
    Ok(())
}

async fn process_job(
    repository: &JobRepository,
    job: ClaimedJob,
    config: &Config,
    mut shutdown: watch::Receiver<bool>,
) {
    let job_id = job.id;
    let lease_token = job.lease_token;
    let attempt_dir = config
        .data_dir
        .join(job_id.to_string())
        .join(format!("attempt-{}-{}", job.attempt_count, lease_token));
    if *shutdown.borrow() {
        let _ = repository.release(job_id, lease_token).await;
        return;
    }
    let work = async {
        if job.job_type != "osm.pbf.stage" || job.importer_name != "geofabrik-pbf-download" {
            return Err("job type or importer is not registered by this worker".to_owned());
        }
        if !job.source_uri.to_ascii_lowercase().ends_with(".osm.pbf") {
            return Err("source must be an .osm.pbf file".to_owned());
        }

        repository
            .set_stage(job_id, lease_token, JobStage::Acquiring, 10)
            .await
            .map_err(|_| "job stage update failed".to_owned())?;

        tokio::fs::create_dir_all(&attempt_dir)
            .await
            .map_err(|_| "could not create job staging directory".to_owned())?;
        let destination = attempt_dir.join("source.osm.pbf");

        let policy = DownloadPolicy {
            allowed_hosts: config.allowed_hosts.clone(),
            max_bytes: config.max_source_bytes,
            request_timeout: config.request_timeout,
        };
        let receipt = download(&job.source_uri, &destination, &policy)
            .await
            .map_err(|_| "source acquisition failed".to_owned())?;

        repository
            .set_stage(job_id, lease_token, JobStage::Checksum, 80)
            .await
            .map_err(|_| "job stage update failed".to_owned())?;

        if let Some(expected) = job.source_checksum.as_deref() {
            let expected = expected.strip_prefix("sha256:").unwrap_or(expected);
            if !expected.eq_ignore_ascii_case(&receipt.sha256) {
                let _ = tokio::fs::remove_file(&destination).await;
                return Err("source SHA-256 did not match the queued checksum".to_owned());
            }
        }

        repository
            .set_stage(job_id, lease_token, JobStage::Inspecting, 90)
            .await
            .map_err(|error| error.to_string())?;
        let inspection = inspect(
            &destination,
            &InspectionPolicy {
                executable: config.osmium_executable.clone(),
                deadline: config.inspection_timeout,
                max_json_bytes: 4 * 1024 * 1024,
            },
        )
        .await
        .map_err(|error| error.to_string())?;

        Ok(json!({
            "artifact": "source.osm.pbf",
            "artifact_path": destination,
            "bytes": receipt.bytes,
            "sha256": receipt.sha256,
            "pbf": {
                "file_size_bytes": inspection.file_size_bytes,
                "nodes": inspection.nodes,
                "ways": inspection.ways,
                "relations": inspection.relations,
                "bbox": inspection.bbox,
            "source_uri": job.source_uri,
            "inspection": {
                "file_size_bytes": inspection.file_size_bytes,
                "node_count": inspection.node_count,
                "way_count": inspection.way_count,
                "relation_count": inspection.relation_count,
                "data_bbox": inspection.data_bbox,
                "first_timestamp": inspection.first_timestamp,
                "last_timestamp": inspection.last_timestamp,
                "replication_timestamp": inspection.replication_timestamp,
                "replication_sequence": inspection.replication_sequence
            },
            "state": "staged_and_inspected",
            "next_step": "osm.pbf.import"
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
                    warn!(job_id = %job_id, %error, "worker lost its lease; abandoning current handler");
                    let _ = tokio::fs::remove_dir_all(&attempt_dir).await;
                    return;
                }
            }
            changed = shutdown.changed() => {
                if changed.is_ok() && *shutdown.borrow() {
                    warn!(job_id = %job_id, "shutdown requested; releasing job without consuming an attempt");
                    let _ = repository.release(job_id, lease_token).await;
                    let _ = tokio::fs::remove_dir_all(&attempt_dir).await;
                    return;
                }
            }
        }
    };

    match outcome {
        Ok(metadata) => {
            if let Err(error) = repository.complete(job_id, lease_token, metadata).await {
                warn!(job_id = %job_id, %error, "could not complete job; lease may have expired");
            } else {
                info!(job_id = %job_id, "source artifact staged successfully");
            }
        }
        Err(message) => {
            let delay = retry_delay(job.attempt_count);
            if let Err(error) = repository
                .retry(
                    job_id,
                    lease_token,
                    "source_staging_failed",
                    &message,
                    delay,
                )
                .await
            {
                warn!(job_id = %job_id, %error, "could not record job failure; lease may have expired");
            } else {
                warn!(job_id = %job_id, error = %message, "source staging failed; retry policy applied");
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
