//! Durable PostgreSQL job leasing for ingestion workers.
//!
//! Claims use SKIP LOCKED and a per-claim fencing token. Every mutation by a
//! worker is conditional on that token, so a worker whose lease expired cannot
//! advance or publish a job after another worker has reclaimed it.

use sqlx::{PgPool, FromRow};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct ClaimedJob {
    pub id: Uuid,
    pub dataset_id: Uuid,
    pub dataset_version_id: Option<Uuid>,
    pub source_uri: String,
    pub source_checksum: Option<String>,
    pub importer_name: String,
    pub importer_version: Option<String>,
    pub attempt_count: i32,
    pub staging_schema: Option<String>,
    pub lease_token: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStage {
    Acquiring,
    Checksum,
    Inspecting,
    Importing,
    Validating,
    Publishing,
}

impl JobStage {
    fn as_str(self) -> &'static str {
        match self {
            Self::Acquiring => "acquiring",
            Self::Checksum => "checksum",
            Self::Inspecting => "inspecting",
            Self::Importing => "importing",
            Self::Validating => "validating",
            Self::Publishing => "publishing",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LeaseError {
    #[error("lease duration must be between 1 second and 24 hours")]
    InvalidLeaseDuration,
    #[error("job lease is no longer owned by this worker")]
    LeaseLost,
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Clone)]
pub struct JobRepository {
    pool: PgPool,
}

impl JobRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Atomically claims one queued job or reclaims one whose lease expired.
    /// Jobs that exhausted their retry budget are terminally failed first.
    pub async fn claim(
        &self,
        worker_id: &str,
        lease_token: Uuid,
        lease_duration: Duration,
    ) -> Result<Option<ClaimedJob>, LeaseError> {
        if worker_id.trim().is_empty()
            || lease_duration.is_zero()
            || lease_duration > Duration::from_secs(24 * 60 * 60)
        {
            return Err(LeaseError::InvalidLeaseDuration);
        }
        let seconds = i64::try_from(lease_duration.as_secs())
            .map_err(|_| LeaseError::InvalidLeaseDuration)?;
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"UPDATE ingestion_jobs
               SET status = 'failed', stage = 'complete',
                   error_code = 'lease_expired_attempts_exhausted',
                   error_message = 'Worker lease expired after the maximum number of attempts',
                   finished_at = now(), lease_owner = NULL, lease_token = NULL,
                   lease_expires_at = NULL, updated_at = now()
               WHERE status = 'running' AND lease_expires_at < now()
                 AND attempt_count >= max_attempts"#,
        )
        .execute(&mut *tx)
        .await?;

        let job = sqlx::query_as::<_, ClaimedJob>(
            r#"WITH candidate AS (
                   SELECT id
                   FROM ingestion_jobs
                   WHERE attempt_count < max_attempts
                     AND (status = 'queued'
                          OR (status = 'running' AND lease_expires_at < now()))
                   ORDER BY queued_at, id
                   FOR UPDATE SKIP LOCKED
                   LIMIT 1
               )
               UPDATE ingestion_jobs AS job
               SET status = 'running',
                   attempt_count = job.attempt_count + 1,
                   started_at = COALESCE(job.started_at, now()),
                   heartbeat_at = now(),
                   lease_expires_at = now() + make_interval(secs => $3::double precision),
                   lease_owner = $1,
                   lease_token = $2,
                   updated_at = now()
               FROM candidate
               WHERE job.id = candidate.id
               RETURNING job.id, job.dataset_id, job.dataset_version_id,
                         job.source_uri, job.source_checksum, job.importer_name,
                         job.importer_version, job.attempt_count, job.staging_schema,
                         job.lease_token"#,
        )
        .bind(worker_id)
        .bind(lease_token)
        .bind(seconds as f64)
        .fetch_optional(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(job)
    }

    /// Extends a lease only while the caller still owns its fencing token.
    pub async fn heartbeat(
        &self,
        job_id: Uuid,
        lease_token: Uuid,
        lease_duration: Duration,
    ) -> Result<(), LeaseError> {
        let seconds = checked_seconds(lease_duration)?;
        let result = sqlx::query(
            r#"UPDATE ingestion_jobs
               SET heartbeat_at = now(),
                   lease_expires_at = now() + make_interval(secs => $3::double precision),
                   updated_at = now()
               WHERE id = $1 AND status = 'running' AND lease_token = $2
                 AND lease_expires_at > now()"#,
        )
        .bind(job_id)
        .bind(lease_token)
        .bind(seconds as f64)
        .execute(&self.pool)
        .await?;
        ensure_one_row(result.rows_affected())
    }

    /// Advances a stage and bounded progress for the current lease holder.
    pub async fn set_stage(
        &self,
        job_id: Uuid,
        lease_token: Uuid,
        stage: JobStage,
        progress_percent: i16,
    ) -> Result<(), LeaseError> {
        if !(0..=99).contains(&progress_percent) {
            return Err(LeaseError::InvalidLeaseDuration);
        }
        let result = sqlx::query(
            r#"UPDATE ingestion_jobs
               SET stage = $3, progress_percent = $4,
                   heartbeat_at = now(), updated_at = now()
               WHERE id = $1 AND status = 'running' AND lease_token = $2
                 AND lease_expires_at > now()"#,
        )
        .bind(job_id)
        .bind(lease_token)
        .bind(stage.as_str())
        .bind(progress_percent)
        .execute(&self.pool)
        .await?;
        ensure_one_row(result.rows_affected())
    }

    /// Marks a job successful after the caller's publication transaction commits.
    pub async fn complete(
        &self,
        job_id: Uuid,
        lease_token: Uuid,
        result_metadata: serde_json::Value,
    ) -> Result<(), LeaseError> {
        let result = sqlx::query(
            r#"UPDATE ingestion_jobs
               SET status = 'succeeded', stage = 'complete', progress_percent = 100,
                   result_metadata = $3, finished_at = now(), heartbeat_at = now(),
                   lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL,
                   updated_at = now()
               WHERE id = $1 AND status = 'running' AND lease_token = $2
                 AND lease_expires_at > now()"#,
        )
        .bind(job_id)
        .bind(lease_token)
        .bind(result_metadata)
        .execute(&self.pool)
        .await?;
        ensure_one_row(result.rows_affected())
    }

    /// Records a terminal, sanitized failure for the current lease holder.
    pub async fn fail(
        &self,
        job_id: Uuid,
        lease_token: Uuid,
        error_code: &str,
        error_message: &str,
    ) -> Result<(), LeaseError> {
        let result = sqlx::query(
            r#"UPDATE ingestion_jobs
               SET status = 'failed', stage = 'complete',
                   error_code = $3, error_message = $4,
                   finished_at = now(), lease_owner = NULL, lease_token = NULL,
                   lease_expires_at = NULL, updated_at = now()
               WHERE id = $1 AND status = 'running' AND lease_token = $2
                 AND lease_expires_at > now()"#,
        )
        .bind(job_id)
        .bind(lease_token)
        .bind(error_code)
        .bind(error_message)
        .execute(&self.pool)
        .await?;
        ensure_one_row(result.rows_affected())
    }
}

fn checked_seconds(duration: Duration) -> Result<i64, LeaseError> {
    if duration.is_zero() || duration > Duration::from_secs(24 * 60 * 60) {
        return Err(LeaseError::InvalidLeaseDuration);
    }
    i64::try_from(duration.as_secs()).map_err(|_| LeaseError::InvalidLeaseDuration)
}

fn ensure_one_row(rows: u64) -> Result<(), LeaseError> {
    if rows == 1 {
        Ok(())
    } else {
        Err(LeaseError::LeaseLost)
    }
}
