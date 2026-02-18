mod common;

use common::setup_db;
use postgresflow::jobs::retry::RetryConfig;
use postgresflow::jobs::runner::JobRunner;
use postgresflow::jobs::{AttemptsRepo, JobsRepo};

use serial_test::serial;
use uuid::Uuid;

async fn insert_fail_job(pool: &sqlx::PgPool, max_attempts: i32) -> Uuid {
    let rec = sqlx::query!(
        r#"
        INSERT INTO jobs (queue, job_type, payload_json, run_at, status, priority, max_attempts)
        VALUES ('default', 'fail_me', '{}'::jsonb, now(), 'queued', 0, $1)
        RETURNING id
        "#,
        max_attempts
    )
    .fetch_one(pool)
    .await
    .unwrap();

    rec.id
}

#[tokio::test]
#[serial]
async fn retry_schedules_increasing_run_at() {
    let pool = setup_db().await;

    let jobs = JobsRepo::new(pool.clone());
    let attempts = AttemptsRepo::new(pool.clone());

    let cfg = RetryConfig {
        base_seconds: 1,
        max_seconds: 15,
        jitter_pct: 0.0, // deterministic test
    };
    let runner = JobRunner::new(jobs.clone(), attempts.clone(), cfg);

    let job_id = insert_fail_job(&pool, 10).await;

    // lease + fail attempt 1 -> delay 1s
    let job = jobs
        .lease_one_job("default", "worker-a", 30)
        .await
        .unwrap()
        .unwrap();

    let attempt1 = attempts.start_attempt(job.id, "worker-a").await.unwrap();

    runner
        .on_failure(
            job.id,
            attempt1.id,
            "worker-a",
            10,
            "TIMEOUT",
            "t1",
            attempt1.attempt_no,
            job.max_attempts,
        )
        .await
        .unwrap();

    let (run_at1, status1): (chrono::DateTime<chrono::Utc>, String) =
        sqlx::query_as("SELECT run_at, status FROM jobs WHERE id = $1")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(status1, "queued");

    // Force time to pass by manually setting run_at=now() so we can lease again
    sqlx::query("UPDATE jobs SET run_at = now() WHERE id = $1")
        .bind(job_id)
        .execute(&pool)
        .await
        .unwrap();

    // lease + fail attempt 2 -> delay 2s
    let job2 = jobs
        .lease_one_job("default", "worker-a", 30)
        .await
        .unwrap()
        .unwrap();

    let attempt2 = attempts.start_attempt(job2.id, "worker-a").await.unwrap();

    runner
        .on_failure(
            job2.id,
            attempt2.id,
            "worker-a",
            10,
            "TIMEOUT",
            "t2",
            attempt2.attempt_no,
            job2.max_attempts,
        )
        .await
        .unwrap();

    let (run_at2, _status2): (chrono::DateTime<chrono::Utc>, String) =
        sqlx::query_as("SELECT run_at, status FROM jobs WHERE id = $1")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    // run_at2 should be later than run_at1 (since delay doubles)
    assert!(run_at2 > run_at1, "expected increasing backoff run_at");
}

#[tokio::test]
#[serial]

async fn non_retryable_goes_to_dlq() {
    let pool = common::setup_db().await;

    let jobs = JobsRepo::new(pool.clone());
    let attempts = AttemptsRepo::new(pool.clone());

    let retry_cfg = RetryConfig::default();
    let runner = JobRunner::new(jobs.clone(), attempts.clone(), retry_cfg);

    // enqueue a job
    let _job_id = jobs
        .enqueue_now("default", "test_job", serde_json::json!({}))
        .await
        .unwrap();

    // lease it
    let worker_id = "worker-1";
    let job = jobs
        .lease_one_job("default", worker_id, 10)
        .await
        .unwrap()
        .unwrap();

    let attempt = attempts.start_attempt(job.id, worker_id).await.unwrap();

    // fail with a NON-retryable error code
    runner
        .on_failure(
            job.id,
            attempt.id,
            worker_id,
            10,
            "BAD_PAYLOAD", // <- non-retryable (or whatever you classify as non-retryable)
            "bad payload",
            attempt.attempt_no,
            job.max_attempts,
        )
        .await
        .unwrap();

    // assert job is DLQ (not failed)
    let updated = jobs.get_job(job.id).await.unwrap().unwrap();
    assert_eq!(updated.status, "dlq");
    assert!(updated.dlq_at.is_some());
    assert_eq!(updated.dlq_reason_code.as_deref(), Some("NON_RETRYABLE"));
}
