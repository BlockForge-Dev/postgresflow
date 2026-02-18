use postgresflow::jobs::JobsRepo;
use sqlx::PgPool;
use uuid::Uuid;

mod common;

async fn upsert_policy(
    pool: &PgPool,
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

async fn insert_job(pool: &PgPool, queue: &str, job_type: &str) -> Uuid {
    let rec = sqlx::query!(
        r#"
        INSERT INTO jobs (queue, job_type, payload_json, run_at, status, priority, max_attempts)
        VALUES ($1, $2, '{}'::jsonb, now(), 'queued', 0, 3)
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

async fn mark_running(pool: &PgPool, job_id: Uuid) {
    sqlx::query!(
        r#"
        UPDATE jobs
        SET status = 'running',
            locked_by = 'someone',
            locked_at = now(),
            lock_expires_at = now() + interval '60 seconds'
        WHERE id = $1
        "#,
        job_id
    )
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn writes_policy_decision_when_in_flight_exceeded() {
    let pool = common::setup_db().await;
    let jobs = JobsRepo::new(pool.clone());

    let queue = "q_policy";
    // allow no in-flight at all, so any running job triggers throttling
    upsert_policy(&pool, queue, 9999, 0, 500).await;

    // create one RUNNING job -> in_flight becomes 1
    let running_id = insert_job(&pool, queue, "running_job").await;
    mark_running(&pool, running_id).await;

    // create one QUEUED job that should be blocked
    let queued_id = insert_job(&pool, queue, "blocked_job").await;

    // try leasing - should return None because in-flight exceeded
    let leased = jobs.lease_one_job(queue, "worker-a", 10).await.unwrap();
    assert!(leased.is_none());

    // verify a policy decision row exists for the queued job
    let row = sqlx::query!(
        r#"
        SELECT decision, reason_code
        FROM policy_decisions
        WHERE job_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        queued_id
    )
    .fetch_optional(&pool)
    .await
    .unwrap();

    let row = row.expect("expected policy_decisions row, found none");
    assert_eq!(row.decision, "THROTTLED");
    assert_eq!(row.reason_code, "IN_FLIGHT_EXCEEDED");

    // also verify job got rescheduled into the future
    let job = jobs.get_job(queued_id).await.unwrap().unwrap();
    assert!(job.run_at > chrono::Utc::now());
}
