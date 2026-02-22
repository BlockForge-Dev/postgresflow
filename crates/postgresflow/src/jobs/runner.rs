use crate::jobs::{
    attempts::AttemptsRepo,
    repo::JobsRepo,
    retry::{classify_error, next_delay_seconds, ErrorClass, RetryConfig},
};
use chrono::Utc;
use rand::{rngs::StdRng, SeedableRng};
use uuid::Uuid;

#[derive(Clone)]
pub struct JobRunner {
    jobs: JobsRepo,
    attempts: AttemptsRepo,
    retry_cfg: RetryConfig,
}

impl JobRunner {
    pub fn new(jobs: JobsRepo, attempts: AttemptsRepo, retry_cfg: RetryConfig) -> Self {
        Self {
            jobs,
            attempts,
            retry_cfg,
        }
    }

    pub async fn on_success(
        &self,
        job_id: Uuid,
        attempt_id: Uuid,
        worker_id: &str,
        latency_ms: i32,
    ) -> anyhow::Result<()> {
        self.attempts
            .finish_succeeded(attempt_id, latency_ms)
            .await?;

        self.jobs.mark_succeeded(job_id, worker_id).await?;
        Ok(())
    }

    pub async fn on_success_batch(
        &self,
        dataset_id: &str,
        updates: &[(Uuid, Uuid, i32)],
        worker_id: &str,
    ) -> anyhow::Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let attempt_updates: Vec<(Uuid, i32)> = updates
            .iter()
            .map(|(_, attempt_id, latency_ms)| (*attempt_id, *latency_ms))
            .collect();
        let job_ids: Vec<Uuid> = updates.iter().map(|(job_id, _, _)| *job_id).collect();

        self.attempts
            .finish_succeeded_batch(&attempt_updates)
            .await?;
        self.jobs
            .mark_succeeded_batch_for_dataset(dataset_id, &job_ids, worker_id)
            .await?;
        Ok(())
    }

    pub async fn on_failure(
        &self,
        job_id: Uuid,
        attempt_id: Uuid,
        worker_id: &str,
        latency_ms: i32,
        error_code: &str,
        error_message: &str,
        attempt_no: i32,
        max_attempts: i32,
    ) -> anyhow::Result<()> {
        // 1) Close out the attempt row (audit)
        self.attempts
            .finish_failed(attempt_id, latency_ms, error_code, error_message)
            .await?;

        // 2) Decide retry vs DLQ
        let class = classify_error(error_code);
        let can_retry = class == ErrorClass::Retryable && attempt_no < max_attempts;

        if can_retry {
            // retry: exponential backoff + jitter + cap
            let mut rng = StdRng::from_entropy();
            let delay_secs = next_delay_seconds(attempt_no, &self.retry_cfg, &mut rng);
            let next_run_at = Utc::now() + chrono::Duration::seconds(delay_secs);

            self.jobs
                .reschedule_for_retry(job_id, next_run_at, Some(error_code), Some(error_message))
                .await?;
        } else {
            // DLQ: retries exhausted OR non-retryable
            let reason_code = match class {
                ErrorClass::NonRetryable => "NON_RETRYABLE",
                ErrorClass::Retryable => "MAX_ATTEMPTS_EXCEEDED", // retryable but ran out
            };

            self.jobs
                .mark_dlq(
                    job_id,
                    worker_id,
                    reason_code,
                    Some(error_code),
                    Some(error_message),
                )
                .await?;
        }

        Ok(())
    }
}
