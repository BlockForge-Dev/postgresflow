use serde_json::json;

mod common;
use common::setup_db;

use postgresflow::jobs::{
    AttemptsRepo, JobsRepo, PoliciesRepo, PolicyDecisionsRepo,
    timeline::{self, TimelineEvent},
};

#[tokio::test]
async fn timeline_includes_policy_decision_event() {
    let pool = setup_db().await;

    let jobs = JobsRepo::new(pool.clone());
    let attempts = AttemptsRepo::new(pool.clone());
    let policies = PoliciesRepo::new(pool.clone());
    let policy_decisions = PolicyDecisionsRepo::new(pool.clone());

    // 1) set strict policy so we will throttle
    // max_in_flight = 0 => always throttles when trying to lease
    policies
        .upsert_policy("default", 10_000, 0, 250)
        .await
        .unwrap();

    // 2) enqueue a runnable job
    let job_id = jobs
        .enqueue_now("default", "ok_job", json!({"x": 1}))
        .await
        .unwrap();

    // 3) attempt leasing -> should be throttled and return None
    let leased = jobs.lease_one_job("default", "worker-1", 10).await.unwrap();
    assert!(leased.is_none(), "expected throttle to return None");

    // 4) timeline should include a PolicyDecision event in story
    let tl = timeline::build_timeline(&jobs, &attempts, &policy_decisions, job_id)
        .await
        .unwrap()
        .expect("job should exist");

    let has_policy = tl.story.iter().any(|e| {
        matches!(
            e,
            TimelineEvent::PolicyDecision {
                decision,
                reason_code,
                ..
            } if decision == "THROTTLED" && reason_code == "IN_FLIGHT_EXCEEDED"
        )
    });

    assert!(
        has_policy,
        "expected story to include THROTTLED/IN_FLIGHT_EXCEEDED"
    );

    // Optional: prove it was written to DB too (not just timeline merge)
    let rows = policy_decisions.list_for_job(job_id).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].decision, "THROTTLED");
    assert_eq!(rows[0].reason_code, "IN_FLIGHT_EXCEEDED");
}
