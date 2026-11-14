use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPool, FromRow};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct IngestionJob {
    pub id: Uuid,
    pub job_type: String,
    pub status: String,
    pub payload: serde_json::Value,
    pub attempts: i32,
    pub max_attempts: i32,
    pub available_at: DateTime<Utc>,
    pub locked_by: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub heartbeat_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("worker_id must not be empty")]
    EmptyWorkerId,
    #[error("lease duration must be positive")]
    InvalidLease,
    #[error("retry delay must not be negative")]
    InvalidRetryDelay,
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
}

fn validate_worker(worker_id: &str) -> Result<(), QueueError> {
    if worker_id.trim().is_empty() { return Err(QueueError::EmptyWorkerId); }
    Ok(())
}
fn validate_lease(lease: Duration) -> Result<(), QueueError> {
    if lease <= Duration::zero() { return Err(QueueError::InvalidLease); }
    Ok(())
}

/// Atomically claims one eligible job. SKIP LOCKED allows concurrent workers.
pub async fn claim_next(pool: &PgPool, worker_id: &str, lease: Duration) -> Result<Option<IngestionJob>, QueueError> {
    validate_worker(worker_id)?;
    validate_lease(lease)?;
    let seconds = lease.num_seconds().max(1) as i32;
    let job = sqlx::query_as::<_, IngestionJob>(r#"
        WITH candidate AS (
          SELECT id FROM ingestion_jobs
          WHERE ((status = 'queued' AND available_at <= now())
             OR (status = 'running' AND lease_expires_at < now()))
            AND attempts < max_attempts
          ORDER BY priority DESC, available_at ASC, created_at ASC
          FOR UPDATE SKIP LOCKED LIMIT 1
        )
        UPDATE ingestion_jobs AS job
        SET status = 'running', attempts = job.attempts + 1,
            locked_by = $1, lease_expires_at = now() + make_interval(secs => $2),
            heartbeat_at = now(), started_at = COALESCE(job.started_at, now()),
            updated_at = now(),
            last_error = CASE WHEN job.status = 'running'
              THEN COALESCE(job.last_error, 'previous worker lease expired')
              ELSE job.last_error END
        FROM candidate WHERE job.id = candidate.id
        RETURNING job.id, job.job_type, job.status, job.payload, job.attempts,
          job.max_attempts, job.available_at, job.locked_by, job.lease_expires_at,
          job.heartbeat_at, job.last_error
    "#).bind(worker_id).bind(seconds).fetch_optional(pool).await?;
    Ok(job)
}

/// Renew only an unexpired lease still owned by this worker.
pub async fn heartbeat(pool: &PgPool, job_id: Uuid, worker_id: &str, lease: Duration) -> Result<bool, QueueError> {
    validate_worker(worker_id)?;
    validate_lease(lease)?;
    let result = sqlx::query("UPDATE ingestion_jobs SET lease_expires_at = now() + make_interval(secs => $3), heartbeat_at = now(), updated_at = now() WHERE id = $1 AND status = 'running' AND locked_by = $2 AND lease_expires_at > now()")
        .bind(job_id).bind(worker_id).bind(lease.num_seconds().max(1) as i32).execute(pool).await?;
    Ok(result.rows_affected() == 1)
}

/// Complete only while the worker still owns a valid lease.
pub async fn complete(pool: &PgPool, job_id: Uuid, worker_id: &str) -> Result<bool, QueueError> {
    validate_worker(worker_id)?;
    let result = sqlx::query("UPDATE ingestion_jobs SET status = 'succeeded', locked_by = NULL, lease_expires_at = NULL, finished_at = now(), updated_at = now() WHERE id = $1 AND status = 'running' AND locked_by = $2 AND lease_expires_at > now()")
        .bind(job_id).bind(worker_id).execute(pool).await?;
    Ok(result.rows_affected() == 1)
}

/// Retry with delay while attempts remain; otherwise mark the job terminally failed.
pub async fn fail_or_retry(pool: &PgPool, job_id: Uuid, worker_id: &str, error: &str, retry_delay: Duration) -> Result<bool, QueueError> {
    validate_worker(worker_id)?;
    if retry_delay < Duration::zero() { return Err(QueueError::InvalidRetryDelay); }
    let result = sqlx::query(r#"
      UPDATE ingestion_jobs
      SET status = CASE WHEN attempts < max_attempts THEN 'queued' ELSE 'failed' END,
          available_at = CASE WHEN attempts < max_attempts
            THEN now() + make_interval(secs => $4) ELSE available_at END,
          locked_by = NULL, lease_expires_at = NULL,
          finished_at = CASE WHEN attempts < max_attempts THEN NULL ELSE now() END,
          last_error = LEFT($3, 4000), updated_at = now()
      WHERE id = $1 AND status = 'running' AND locked_by = $2 AND lease_expires_at > now()
    "#).bind(job_id).bind(worker_id).bind(error).bind(retry_delay.num_seconds().max(0) as i32).execute(pool).await?;
    Ok(result.rows_affected() == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_blank_worker_identity() {
        assert!(matches!(validate_worker(" "), Err(QueueError::EmptyWorkerId)));
    }
    #[test]
    fn rejects_non_positive_lease() {
        assert!(matches!(validate_lease(Duration::zero()), Err(QueueError::InvalidLease)));
        assert!(matches!(validate_lease(Duration::seconds(-1)), Err(QueueError::InvalidLease)));
    }
}
