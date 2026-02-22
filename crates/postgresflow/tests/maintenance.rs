use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

mod common;
use common::setup_db;

use postgresflow::jobs::maintenance::MaintenanceRepo;
use postgresflow::jobs::JobsRepo;

#[tokio::test]
async fn archives_old_succeeded_jobs_and_prunes_history() {
    let pool = setup_db().await;

    let jobs = JobsRepo::new(pool.clone());
    let maint = MaintenanceRepo::new(pool.clone());

    // "Old" and cutoff timestamps
    let old = Utc::now() - Duration::days(30);
    let cutoff = Utc::now() - Duration::days(7);

    // 1) Insert a succeeded job that is definitely old (and still in jobs table)
    let job_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO jobs (
            id, queue, job_type, payload_json, run_at, status, priority, max_attempts,
            locked_at, locked_by, lock_expires_at,
            dlq_reason_code, dlq_at,
            created_at, updated_at,
            replay_of_job_id
        )
        VALUES (
            $1, 'default', 'ok_job', $2::jsonb, $3, 'succeeded', 0, 25,
            NULL, NULL, NULL,
            NULL, NULL,
            $3, $3,
            NULL
        )
        "#,
        job_id,
        json!({"a": 1}),
        old,
    )
    .execute(&pool)
    .await
    .unwrap();

    // 2) Insert an old attempt row (so it qualifies for pruning)
    let attempt_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO job_attempts (
            id, job_id, attempt_no,
            started_at, finished_at,
            status, error_code, error_message,
            latency_ms, worker_id
        )
        VALUES (
            $1, $2, 1,
            $3, $3,
            'succeeded', NULL, NULL,
            123, 'worker-1'
        )
        "#,
        attempt_id,
        job_id,
        old
    )
    .execute(&pool)
    .await
    .unwrap();

    // 3) Insert an old policy decision row (so it qualifies for pruning)
    let policy_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO policy_decisions (
            id, job_id, decision, reason_code, details_json, created_at
        )
        VALUES (
            $1, $2, 'THROTTLED', 'IN_FLIGHT_EXCEEDED', $3::jsonb, $4
        )
        "#,
        policy_id,
        job_id,
        json!({"x": 1}),
        old
    )
    .execute(&pool)
    .await
    .unwrap();

    // Sanity: both exist BEFORE pruning
    let attempts_before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM job_attempts WHERE job_id = $1")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attempts_before, 1);

    let policy_before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM policy_decisions WHERE job_id = $1")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(policy_before, 1);

    // 4) ✅ PRUNE HISTORY FIRST (while job still exists)
    let (a_del, p_del) = maint
        .delete_history_for_succeeded_older_than(cutoff, 1000)
        .await
        .unwrap();

    assert!(a_del >= 1, "expected >= 1 attempt deleted, got {a_del}");
    assert!(
        p_del >= 1,
        "expected >= 1 policy decision deleted, got {p_del}"
    );

    // Confirm history is gone
    let attempts_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM job_attempts WHERE job_id = $1")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attempts_after, 0);

    let policy_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM policy_decisions WHERE job_id = $1")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(policy_after, 0);

    // 5) ✅ THEN ARCHIVE
    let archived = maint
        .archive_succeeded_older_than(cutoff, 1000)
        .await
        .unwrap();
    assert_eq!(archived, 1, "expected exactly 1 job archived");

    // job removed from main table
    let main = jobs.get_job(job_id).await.unwrap();
    assert!(main.is_none());

    // exists in archive
    let archived_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs_archive WHERE id = $1")
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(archived_count, 1);
}
