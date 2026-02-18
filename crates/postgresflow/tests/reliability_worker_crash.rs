use serde_json::json;
use sqlx::PgPool;

use postgresflow::db;
use postgresflow::jobs::{AttemptsRepo, JobsRepo};

async fn test_pool() -> PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for integration tests");
    db::make_pool(&database_url).await.expect("pool")
}

#[tokio::test]
async fn worker_crash_mid_job_another_worker_recovers_after_lease_expiry() -> anyhow::Result<()> {
    let pool = test_pool().await;

    // Clean
    sqlx::query("TRUNCATE TABLE job_attempts RESTART IDENTITY CASCADE")
        .execute(&pool)
        .await?;
    sqlx::query("TRUNCATE TABLE jobs RESTART IDENTITY CASCADE")
        .execute(&pool)
        .await?;

    let jobs = JobsRepo::new(pool.clone());
    let attempts = AttemptsRepo::new(pool.clone());

    let job_id = jobs.enqueue_now("default", "fail_me", json!({})).await?;

    // Worker A leases with a 1s lease, starts attempt, then "crashes" (no finish)
    let job = jobs
        .lease_one_job("default", "workerA", 1)
        .await?
        .expect("leased");
    assert_eq!(job.id, job_id);

    let _attempt1 = attempts.start_attempt(job_id, "workerA").await?;

    // Wait for lease to expire
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Worker B reaps and leases
    jobs.reap_expired_locks().await?;
    let job2 = jobs
        .lease_one_job("default", "workerB", 10)
        .await?
        .expect("leased by B");
    assert_eq!(job2.id, job_id);

    let attempt2 = attempts.start_attempt(job_id, "workerB").await?;
    attempts
        .finish_failed(attempt2.id, 2, "recovered after crash", "TIMEOUT")
        .await?;

    // Sanity: job exists and status is one of allowed states
    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id = $1")
        .bind(job_id)
        .fetch_one(&pool)
        .await?;
    assert!(
        matches!(
            status.as_str(),
            "running" | "queued" | "dlq" | "failed" | "succeeded"
        ),
        "unexpected status: {status}"
    );

    let attempt_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM job_attempts WHERE job_id = $1")
            .bind(job_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(attempt_count, 2);

    Ok(())
}
