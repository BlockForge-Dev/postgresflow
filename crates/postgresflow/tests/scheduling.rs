mod common;

use common::setup_db;
use postgresflow::jobs::JobsRepo;
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn scheduled_job_is_not_leased_early_and_is_leased_after_run_at() {
    let pool = setup_db().await;
    let repo = JobsRepo::new(pool.clone());

    // schedule for ~2 seconds later
    let _job_id = repo
        .enqueue_in("default", "scheduled", json!({"k":"v"}), 2)
        .await
        .unwrap();

    // should not lease early
    let early = repo.lease_one_job("default", "worker-a", 30).await.unwrap();
    assert!(early.is_none(), "should not lease before run_at");

    // shortly after run_at passes, should lease
    tokio::time::sleep(Duration::from_millis(2300)).await;

    let leased = repo.lease_one_job("default", "worker-a", 30).await.unwrap();

    assert!(leased.is_some(), "should lease after run_at");
}
