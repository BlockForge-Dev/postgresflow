// crates/postgresflow/tests/error_classification.rs
mod common;

use common::setup_db;

use postgresflow::jobs::timeline::build_timeline;
use postgresflow::jobs::{AttemptsRepo, JobsRepo, PolicyDecisionsRepo};

use uuid::Uuid;

async fn insert_job(pool: &sqlx::PgPool) -> Uuid {
    let rec = sqlx::query!(
        r#"
        INSERT INTO jobs (queue, job_type, payload_json, run_at, status, priority, max_attempts)
        VALUES ('default', 'fail_me', '{}'::jsonb, now(), 'queued', 0, 5)
        RETURNING id
        "#
    )
    .fetch_one(pool)
    .await
    .unwrap();

    rec.id
}

#[tokio::test]
async fn timeline_includes_suggested_action_for_known_error_codes() {
    let pool = setup_db().await;

    let jobs = JobsRepo::new(pool.clone());
    let attempts = AttemptsRepo::new(pool.clone());
    let policy = PolicyDecisionsRepo::new(pool.clone()); // ✅ new

    let job_id = insert_job(&pool).await;

    // lease job so it becomes running
    let job = jobs
        .lease_one_job("default", "worker-1", 30)
        .await
        .unwrap()
        .expect("should lease job");
    assert_eq!(job.id, job_id);

    // start attempt
    let attempt = attempts.start_attempt(job_id, "worker-1").await.unwrap();

    // finish attempt as failed with a known code
    attempts
        .finish_failed(attempt.id, 12, "RATE_LIMIT", "429 from upstream")
        .await
        .unwrap();

    // timeline should include suggested action
    let tl = build_timeline(&jobs, &attempts, &policy, job_id) // ✅ new arg
        .await
        .unwrap()
        .expect("timeline exists");

    let a1 = tl.attempts.first().expect("has attempts");
    assert_eq!(a1.error_code.as_deref(), Some("RATE_LIMIT"));
    assert!(
        a1.suggested_action
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains("back off"),
        "expected a runbook-style suggestion for RATE_LIMIT"
    );
}
