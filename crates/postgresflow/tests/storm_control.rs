// crates/postgresflow/tests/storm_control.rs
mod common;

use chrono::Utc;

use common::setup_db;
use postgresflow::jobs::{AttemptsRepo, JobsRepo, PolicyDecisionsRepo};
use serial_test::serial;
use uuid::Uuid;

async fn upsert_queue_policy(
    pool: &sqlx::PgPool,
    queue: &str,
    max_attempts_per_minute: i32,
    max_in_flight: i32,
    throttle_delay_ms: i32,
) {
    sqlx::query!(
        r#"
        INSERT INTO queue_policies (queue, max_attempts_per_minute, max_in_flight, throttle_delay_ms)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (queue) DO UPDATE
        SET max_attempts_per_minute = EXCLUDED.max_attempts_per_minute,
            max_in_flight = EXCLUDED.max_in_flight,
            throttle_delay_ms = EXCLUDED.throttle_delay_ms
        "#,
        queue,
        max_attempts_per_minute,
        max_in_flight,
        throttle_delay_ms
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_job_direct(pool: &sqlx::PgPool, queue: &str, job_type: &str) -> Uuid {
    let rec = sqlx::query!(
        r#"
        INSERT INTO jobs (queue, job_type, payload_json, run_at, status, priority, max_attempts)
        VALUES ($1, $2, '{}'::jsonb, now(), 'queued', 0, 5)
        RETURNING id
        "#,
        queue,
        job_type
    )
    .fetch_one(pool)
    .await
    .unwrap();

    rec.id
}

#[tokio::test]
#[serial]
async fn throttles_when_in_flight_exceeded() {
    let pool = setup_db().await;

    let jobs = JobsRepo::new(pool.clone());
    let policies = PolicyDecisionsRepo::new(pool.clone());

    upsert_queue_policy(&pool, "default", 10_000, 2, 500).await;

    let job1_id = insert_job_direct(&pool, "default", "job1").await;
    let job2_id = insert_job_direct(&pool, "default", "job2").await;

    let leased1 = jobs
        .lease_one_job("default", "worker-a", 10)
        .await
        .unwrap()
        .expect("should lease first job");
    assert_eq!(leased1.id, job1_id);

    upsert_queue_policy(&pool, "default", 10_000, 1, 500).await;

    let before = Utc::now();
    let leased2 = jobs.lease_one_job("default", "worker-b", 10).await.unwrap();

    assert!(leased2.is_none());

    let j2 = jobs.get_job(job2_id).await.unwrap().unwrap();
    assert!(j2.run_at > before, "expected run_at pushed forward");

    let rows = policies.list_for_job(job2_id).await.unwrap();
    assert!(!rows.is_empty(), "expected policy decision row");

    let last = rows.last().unwrap();
    assert_eq!(last.decision, "THROTTLED");
    assert_eq!(last.reason_code, "IN_FLIGHT_EXCEEDED");
}

#[tokio::test]
#[serial]
async fn throttles_when_retry_rate_exceeded() {
    let pool = setup_db().await;

    let jobs = JobsRepo::new(pool.clone());
    let attempts = AttemptsRepo::new(pool.clone());
    let policies = PolicyDecisionsRepo::new(pool.clone());

    upsert_queue_policy(&pool, "default", 1, 999_999, 500).await;

    let seed_job_id = insert_job_direct(&pool, "default", "seed").await;

    let leased_seed = jobs
        .lease_one_job("default", "worker-a", 10)
        .await
        .unwrap()
        .expect("should lease seed job");
    assert_eq!(leased_seed.id, seed_job_id);

    let _a1 = attempts
        .start_attempt(seed_job_id, "worker-a")
        .await
        .unwrap();

    let target_job_id = insert_job_direct(&pool, "default", "target").await;

    let before = Utc::now();
    let leased = jobs.lease_one_job("default", "worker-b", 10).await.unwrap();

    assert!(leased.is_none(), "expected throttle -> no lease");

    let j = jobs.get_job(target_job_id).await.unwrap().unwrap();
    assert!(j.run_at > before, "expected run_at pushed forward");

    let rows = policies.list_for_job(target_job_id).await.unwrap();
    assert!(!rows.is_empty(), "expected policy decision row");

    let last = rows.last().unwrap();
    assert_eq!(last.decision, "THROTTLED");
    assert_eq!(last.reason_code, "RETRY_RATE_EXCEEDED");
}
