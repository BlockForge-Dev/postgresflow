mod common;

use common::{insert_job, setup_db};

use postgresflow::jobs::{AttemptsRepo, JobsRepo};
use serial_test::serial;

#[tokio::test]
#[serial]
async fn leasing_creates_and_finishes_attempt_row() {
    let pool = setup_db().await;

    let jobs_repo = JobsRepo::new(pool.clone());
    let attempts_repo = AttemptsRepo::new(pool.clone());

    // Insert 1 runnable job
    let job_id = insert_job(&pool, "default").await;

    // Lease it
    let job = jobs_repo
        .lease_one_job("default", "worker-a", 30)
        .await
        .unwrap()
        .expect("should lease");

    assert_eq!(job.id, job_id);

    // Start attempt
    let attempt = attempts_repo
        .start_attempt(job.id, "worker-a")
        .await
        .unwrap();
    assert_eq!(attempt.job_id, job.id);
    assert_eq!(attempt.attempt_no, 1);
    assert_eq!(attempt.status, "running");

    // Finish succeeded
    attempts_repo
        .finish_succeeded(attempt.id, 123)
        .await
        .unwrap();

    // Confirm attempt history
    let attempts = attempts_repo.list_attempts_for_job(job.id).await.unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].attempt_no, 1);
    assert_eq!(attempts[0].status, "succeeded");
    assert_eq!(attempts[0].latency_ms, Some(123));
}

#[tokio::test]
#[serial]
async fn multiple_attempts_increment_attempt_no() {
    let pool = setup_db().await;

    let attempts_repo = AttemptsRepo::new(pool.clone());

    let job_id = insert_job(&pool, "default").await;

    let a1 = attempts_repo
        .start_attempt(job_id, "worker-a")
        .await
        .unwrap();
    let a2 = attempts_repo
        .start_attempt(job_id, "worker-a")
        .await
        .unwrap();

    assert_eq!(a1.attempt_no, 1);
    assert_eq!(a2.attempt_no, 2);

    let attempts = attempts_repo.list_attempts_for_job(job_id).await.unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].attempt_no, 1);
    assert_eq!(attempts[1].attempt_no, 2);
}

#[tokio::test]
#[serial]
async fn finish_failed_sets_error_fields() {
    let pool = setup_db().await;

    let attempts_repo = AttemptsRepo::new(pool.clone());

    let job_id = insert_job(&pool, "default").await;

    let attempt = attempts_repo
        .start_attempt(job_id, "worker-a")
        .await
        .unwrap();
    attempts_repo
        .finish_failed(attempt.id, 77, "TIMEOUT", "request timed out")
        .await
        .unwrap();

    let attempts = attempts_repo.list_attempts_for_job(job_id).await.unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].status, "failed");
    assert_eq!(attempts[0].latency_ms, Some(77));
    assert_eq!(attempts[0].error_code.as_deref(), Some("TIMEOUT"));
    assert_eq!(
        attempts[0].error_message.as_deref(),
        Some("request timed out")
    );
}
