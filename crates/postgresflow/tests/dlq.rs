mod common;

use common::setup_db;
use postgresflow::jobs::retry::RetryConfig;
use postgresflow::jobs::runner::JobRunner;
use postgresflow::jobs::{AttemptsRepo, JobsRepo};

use sqlx::Row;
use std::time::Instant;
use uuid::Uuid;

async fn insert_job(pool: &sqlx::PgPool, queue: &str, job_type: &str, max_attempts: i32) -> Uuid {
    let rec = sqlx::query!(
        r#"
        INSERT INTO jobs (queue, job_type, payload_json, run_at, status, priority, max_attempts)
        VALUES ($1, $2, '{}'::jsonb, now(), 'queued', 0, $3)
        RETURNING id
        "#,
        queue,
        job_type,
        max_attempts
    )
    .fetch_one(pool)
    .await
    .expect("insert job failed");

    rec.id
}

#[tokio::test]
async fn exhausted_retries_moves_job_to_dlq_and_preserves_attempts() {
    let pool = setup_db().await;

    let jobs = JobsRepo::new(pool.clone());
    let attempts = AttemptsRepo::new(pool.clone());
    let runner = JobRunner::new(jobs.clone(), attempts.clone(), RetryConfig::default());

    // max_attempts = 2 -> attempt 1 retry, attempt 2 -> DLQ
    let job_id = insert_job(&pool, "default", "always_fail", 2).await;

    // Lease it (simulates worker owning it)
    let job = jobs
        .lease_one_job("default", "worker-1", 30)
        .await
        .unwrap()
        .expect("should lease job");
    assert_eq!(job.id, job_id);

    // Attempt 1 fails => should reschedule
    let a1 = attempts.start_attempt(job_id, "worker-1").await.unwrap();
    let start = Instant::now();
    runner
        .on_failure(
            job_id,
            a1.id,
            "worker-1",
            start.elapsed().as_millis() as i32,
            "TIMEOUT",
            "sim timeout",
            a1.attempt_no,
            job.max_attempts,
        )
        .await
        .unwrap();

    // Re-lease after reschedule: force it runnable now for test simplicity
    sqlx::query!("UPDATE jobs SET run_at = now() WHERE id = $1", job_id)
        .execute(&pool)
        .await
        .unwrap();

    let job2 = jobs
        .lease_one_job("default", "worker-1", 30)
        .await
        .unwrap()
        .expect("should lease again");
    assert_eq!(job2.id, job_id);

    // Attempt 2 fails => should DLQ
    let a2 = attempts.start_attempt(job_id, "worker-1").await.unwrap();
    let start2 = Instant::now();
    runner
        .on_failure(
            job_id,
            a2.id,
            "worker-1",
            start2.elapsed().as_millis() as i32,
            "TIMEOUT",
            "sim timeout",
            a2.attempt_no,
            job2.max_attempts,
        )
        .await
        .unwrap();

    // Assert job is DLQ and fields are set
    let row = sqlx::query("SELECT status, dlq_reason_code, dlq_at FROM jobs WHERE id = $1")
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    let status: String = row.get("status");
    let dlq_reason_code: Option<String> = row.get("dlq_reason_code");
    let dlq_at: Option<chrono::DateTime<chrono::Utc>> = row.get("dlq_at");

    assert_eq!(status, "dlq");
    assert_eq!(dlq_reason_code.as_deref(), Some("MAX_ATTEMPTS_EXCEEDED"));
    assert!(dlq_at.is_some(), "dlq_at should be set");

    // Attempts preserved (should be 2)
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM job_attempts WHERE job_id = $1")
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(count, 2, "attempt history must be preserved");
}

#[tokio::test]
async fn non_retryable_goes_to_dlq_immediately() {
    let pool = setup_db().await;

    let jobs = JobsRepo::new(pool.clone());
    let attempts = AttemptsRepo::new(pool.clone());
    let runner = JobRunner::new(jobs.clone(), attempts.clone(), RetryConfig::default());

    let job_id = insert_job(&pool, "default", "bad_payload", 10).await;

    let job = jobs
        .lease_one_job("default", "worker-1", 30)
        .await
        .unwrap()
        .expect("should lease job");
    assert_eq!(job.id, job_id);

    let a1 = attempts.start_attempt(job_id, "worker-1").await.unwrap();

    runner
        .on_failure(
            job_id,
            a1.id,
            "worker-1",
            1,
            "BAD_PAYLOAD",
            "invalid json",
            a1.attempt_no,
            job.max_attempts,
        )
        .await
        .unwrap();

    let (status, reason): (String, Option<String>) =
        sqlx::query_as("SELECT status, dlq_reason_code FROM jobs WHERE id = $1")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(status, "dlq");
    assert_eq!(reason.as_deref(), Some("NON_RETRYABLE"));
}
