// crates/postgresflow/tests/timeline.rs
mod common;

use common::setup_db;

use postgresflow::jobs::timeline::build_timeline;
use postgresflow::jobs::{AttemptsRepo, JobsRepo, PolicyDecisionsRepo};

use uuid::Uuid;

#[tokio::test]
async fn timeline_shows_attempt_story() {
    let pool = setup_db().await;

    let jobs = JobsRepo::new(pool.clone());
    let attempts = AttemptsRepo::new(pool.clone());
    let policy = PolicyDecisionsRepo::new(pool.clone()); // ✅ new

    // Insert a job directly (or use jobs.enqueue_now if you prefer)
    let job_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO jobs (queue, job_type, payload_json, run_at, status, priority, max_attempts)
        VALUES ('default', 'email_send', '{}'::jsonb, now(), 'queued', 0, 5)
        RETURNING id
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    // Attempt 1 fails
    let leased = jobs
        .lease_one_job("default", "worker-a", 30)
        .await
        .unwrap()
        .unwrap();

    let a1 = attempts.start_attempt(leased.id, "worker-a").await.unwrap();
    attempts
        .finish_failed(a1.id, 10, "timeout", "request timed out")
        .await
        .unwrap();

    // mimic retry scheduling
    jobs.reschedule_for_retry(
        job_id,
        chrono::Utc::now(),
        Some("timeout"),
        Some("request timed out"),
    )
    .await
    .unwrap();

    // Attempt 2 succeeds
    let leased2 = jobs
        .lease_one_job("default", "worker-b", 30)
        .await
        .unwrap()
        .unwrap();

    let a2 = attempts
        .start_attempt(leased2.id, "worker-b")
        .await
        .unwrap();
    attempts.finish_succeeded(a2.id, 5).await.unwrap();
    jobs.mark_succeeded(job_id, "worker-b").await.unwrap();

    let tl = build_timeline(&jobs, &attempts, &policy, job_id) // ✅ new arg
        .await
        .unwrap()
        .unwrap();

    assert_eq!(tl.job_id, job_id);
    assert_eq!(tl.status, "succeeded");
    assert_eq!(tl.last_worker_id.as_deref(), Some("worker-b"));
    assert_eq!(tl.attempts.len(), 2);

    assert_eq!(tl.attempts[0].attempt_no, 1);
    assert_eq!(tl.attempts[0].status, "failed");
    assert_eq!(tl.attempts[0].error_code.as_deref(), Some("timeout"));

    assert_eq!(tl.attempts[1].attempt_no, 2);
    assert_eq!(tl.attempts[1].status, "succeeded");
}
