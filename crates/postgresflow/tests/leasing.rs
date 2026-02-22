// tests/leasing.rs
mod common;

use common::{insert_job, setup_db};

use postgresflow::jobs::JobsRepo;
use sqlx::PgPool;
use std::collections::HashSet;
use std::time::Duration;
use uuid::Uuid;

use chrono::{Duration as ChronoDuration, Utc};
use serial_test::serial;

async fn insert_job_with(
    pool: &PgPool,
    queue: &str,
    job_type: &str,
    run_at_offset_secs: i64,
    priority: i32,
) -> Uuid {
    let run_at = Utc::now() + ChronoDuration::seconds(run_at_offset_secs);

    let rec = sqlx::query!(
        r#"
        INSERT INTO jobs (queue, job_type, payload_json, run_at, status, priority, max_attempts)
        VALUES ($1, $2, '{}'::jsonb, $3, 'queued', $4, 5)
        RETURNING id
        "#,
        queue,
        job_type,
        run_at,
        priority
    )
    .fetch_one(pool)
    .await
    .expect("failed to insert job_with");

    rec.id
}

async fn get_job_status_and_locked_by(pool: &PgPool, id: Uuid) -> (String, Option<String>) {
    sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, locked_by FROM jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
#[serial] // ✅ critical: no parallel DB interference
async fn leasing_two_workers_never_claim_same_job() {
    let pool = setup_db().await;
    let repo = JobsRepo::new(pool.clone());

    let _job_id = insert_job(&pool, "default").await;

    let repo_a = repo.clone();
    let repo_b = repo.clone();

    let (a, b) = tokio::join!(
        async move {
            repo_a
                .lease_one_job("default", "worker-a", 30)
                .await
                .unwrap()
        },
        async move {
            repo_b
                .lease_one_job("default", "worker-b", 30)
                .await
                .unwrap()
        },
    );

    let got_a = a.is_some();
    let got_b = b.is_some();

    // XOR: exactly one worker should win the lease
    assert!(
        got_a ^ got_b,
        "expected exactly one worker to lease the job, got_a={got_a}, got_b={got_b}"
    );

    // Verify the single row is running and locked by one of the workers
    let (locked_by, status): (Option<String>, String) =
        sqlx::query_as("SELECT locked_by, status FROM jobs LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(status, "running");
    assert!(
        locked_by.as_deref() == Some("worker-a") || locked_by.as_deref() == Some("worker-b"),
        "job should be locked by one of the workers"
    );
}

#[tokio::test]
#[serial]
async fn lease_expires_then_other_worker_can_claim() {
    let pool = setup_db().await;
    let repo = JobsRepo::new(pool.clone());

    let job_id = insert_job(&pool, "default").await;

    // Worker A leases with 1 second lease
    let leased_a = repo
        .lease_one_job("default", "worker-a", 1)
        .await
        .unwrap()
        .expect("worker-a should lease job");

    assert_eq!(leased_a.id, job_id);

    // Simulate worker dying: don't mark succeeded, just wait for expiry
    tokio::time::sleep(Duration::from_millis(1200)).await;

    // Your recovery mechanism
    let reaped = repo.reap_expired_locks().await.unwrap();
    assert_eq!(reaped, 1, "expected to reap exactly one expired lock");

    // Worker B should now lease the same job
    let leased_b = repo
        .lease_one_job("default", "worker-b", 30)
        .await
        .unwrap()
        .expect("worker-b should lease after expiry");

    assert_eq!(leased_b.id, job_id);
    assert_eq!(leased_b.locked_by.as_deref(), Some("worker-b"));
}

#[tokio::test]
#[serial]
async fn leasing_respects_priority_then_run_at() {
    let pool = setup_db().await;
    let repo = JobsRepo::new(pool.clone());

    // runnable now, lower priority
    let low = insert_job_with(&pool, "default", "low", 0, 0).await;

    // runnable now, higher priority
    let high = insert_job_with(&pool, "default", "high", 0, 10).await;

    // not runnable yet, even though priority is highest
    let future = insert_job_with(&pool, "default", "future", 30, 100).await;

    let j1 = repo
        .lease_one_job("default", "worker-a", 30)
        .await
        .unwrap()
        .expect("expected a job");
    assert_eq!(j1.id, high);

    let j2 = repo
        .lease_one_job("default", "worker-a", 30)
        .await
        .unwrap()
        .expect("expected second job");
    assert_eq!(j2.id, low);

    // Future job remains queued/unlocked
    let (status, locked_by) = get_job_status_and_locked_by(&pool, future).await;
    assert_eq!(status, "queued");
    assert_eq!(locked_by, None);
}

#[tokio::test]
#[serial]
async fn delayed_job_is_not_leased_before_run_at() {
    let pool = setup_db().await;
    let repo = JobsRepo::new(pool.clone());

    let delayed = insert_job_with(&pool, "default", "delayed", 2, 0).await;

    // Immediately: should lease nothing
    let leased = repo.lease_one_job("default", "worker-a", 30).await.unwrap();
    assert!(leased.is_none(), "should not lease before run_at");

    // After run_at passes
    tokio::time::sleep(Duration::from_millis(2200)).await;

    let leased2 = repo
        .lease_one_job("default", "worker-a", 30)
        .await
        .unwrap()
        .expect("should lease after run_at");
    assert_eq!(leased2.id, delayed);
}

#[tokio::test]
#[serial]
async fn workers_only_lease_from_their_queue() {
    let pool = setup_db().await;
    let repo = JobsRepo::new(pool.clone());

    let qa = insert_job_with(&pool, "queue-a", "a1", 0, 0).await;
    let qb = insert_job_with(&pool, "queue-b", "b1", 0, 0).await;

    let leased_a = repo
        .lease_one_job("queue-a", "worker-a", 30)
        .await
        .unwrap()
        .expect("worker-a should lease from queue-a");
    assert_eq!(leased_a.id, qa);

    let leased_b = repo
        .lease_one_job("queue-b", "worker-b", 30)
        .await
        .unwrap()
        .expect("worker-b should lease from queue-b");
    assert_eq!(leased_b.id, qb);
}

#[tokio::test]
#[serial]
async fn reap_only_reaps_expired_running_jobs() {
    let pool = setup_db().await;
    let repo = JobsRepo::new(pool.clone());

    let job_id = insert_job(&pool, "default").await;

    // Lease with a long lease (not expired)
    let _leased = repo
        .lease_one_job("default", "worker-a", 30)
        .await
        .unwrap()
        .expect("should lease");

    // Immediately reap -> should reap 0 because it hasn't expired
    let reaped = repo.reap_expired_locks().await.unwrap();
    assert_eq!(reaped, 0, "should not reap active leases");

    // Job should still be running and locked by worker-a
    let (status, locked_by) = get_job_status_and_locked_by(&pool, job_id).await;
    assert_eq!(status, "running");
    assert_eq!(locked_by.as_deref(), Some("worker-a"));
}

#[tokio::test]
#[serial]
async fn batch_leasing_claims_expected_count_without_duplicates() {
    let pool = setup_db().await;
    let repo = JobsRepo::new(pool.clone());

    for _ in 0..5 {
        let _ = insert_job(&pool, "default").await;
    }

    let leased_1 = repo
        .lease_jobs_batch("default", "worker-a", 30, 3)
        .await
        .unwrap();
    assert_eq!(leased_1.len(), 3);

    let ids_1: HashSet<Uuid> = leased_1.iter().map(|j| j.id).collect();
    assert_eq!(ids_1.len(), leased_1.len(), "duplicate job in first batch");

    let leased_2 = repo
        .lease_jobs_batch("default", "worker-a", 30, 3)
        .await
        .unwrap();
    assert_eq!(leased_2.len(), 2);

    let ids_2: HashSet<Uuid> = leased_2.iter().map(|j| j.id).collect();
    assert_eq!(ids_2.len(), leased_2.len(), "duplicate job in second batch");

    assert!(
        ids_1.is_disjoint(&ids_2),
        "the same job was leased in two batches"
    );
}
