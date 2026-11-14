//! Lease-aware supervision primitives shared by ingestion worker stages.
//!
//! Long-running acquisition, inspection, and import operations must not outlive
//! their database lease. This helper renews the lease while an operation runs
//! and drops that operation immediately if ownership has been lost.

use crate::repository::{JobRepository, LeaseError};
use std::{future::Future, time::Duration};
use tokio::time::{interval_at, Instant};

#[derive(Debug)]
pub enum SupervisionError<E> {
    Operation(E),
    Lease(LeaseError),
    InvalidHeartbeatInterval,
}

impl<E: std::fmt::Display> std::fmt::Display for SupervisionError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Operation(error) => write!(f, "worker stage failed: {error}"),
            Self::Lease(error) => write!(f, "worker lease supervision failed: {error}"),
            Self::InvalidHeartbeatInterval => {
                write!(f, "heartbeat interval must be positive and shorter than the lease")
            }
        }
    }
}

impl<E: std::fmt::Debug + std::fmt::Display> std::error::Error for SupervisionError<E> {}

/// Runs one stage while periodically renewing its lease.
///
/// The interval must be positive and shorter than the lease duration. If a
/// heartbeat fails (including a stale fencing token), the stage future is
/// dropped so a worker cannot continue expensive work after losing ownership.
/// Callers should use cancellation-safe stage operations or ensure child
/// processes are configured with kill-on-drop.
pub async fn run_with_heartbeat<F, T, E>(
    repository: &JobRepository,
    job_id: uuid::Uuid,
    lease_token: uuid::Uuid,
    lease_duration: Duration,
    heartbeat_interval: Duration,
    operation: F,
) -> Result<T, SupervisionError<E>>
where
    F: Future<Output = Result<T, E>>,
{
    if heartbeat_interval.is_zero() || heartbeat_interval >= lease_duration {
        return Err(SupervisionError::InvalidHeartbeatInterval);
    }

    let mut ticker = interval_at(Instant::now() + heartbeat_interval, heartbeat_interval);
    tokio::pin!(operation);

    loop {
        tokio::select! {
            result = &mut operation => {
                return result.map_err(SupervisionError::Operation);
            }
            _ = ticker.tick() => {
                repository
                    .heartbeat(job_id, lease_token, lease_duration)
                    .await
                    .map_err(SupervisionError::Lease)?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_zero_heartbeat_interval_before_polling_database() {
        let result = run_with_heartbeat(
            // The function validates the interval before using this repository.
            &JobRepository::new(sqlx::PgPool::connect_lazy("postgres://unused:unused@localhost/unused").unwrap()),
            uuid::Uuid::nil(),
            uuid::Uuid::nil(),
            Duration::from_secs(30),
            Duration::ZERO,
            async { Ok::<_, &'static str>(()) },
        )
        .await;
        assert!(matches!(result, Err(SupervisionError::InvalidHeartbeatInterval)));
    }

    #[tokio::test]
    async fn rejects_interval_equal_to_lease_duration() {
        let result = run_with_heartbeat(
            &JobRepository::new(sqlx::PgPool::connect_lazy("postgres://unused:unused@localhost/unused").unwrap()),
            uuid::Uuid::nil(),
            uuid::Uuid::nil(),
            Duration::from_secs(30),
            Duration::from_secs(30),
            async { Ok::<_, &'static str>(()) },
        )
        .await;
        assert!(matches!(result, Err(SupervisionError::InvalidHeartbeatInterval)));
    }
}
