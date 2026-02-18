mod common;

use chrono::{Duration as ChronoDuration, Utc};
use common::setup_db;
use postgresflow::jobs::JobsRepo;
use sqlx::PgPool;
use uuid::Uuid;

async fn insert_job_full(pool: &PgPool, queue: &str, job_type: &str) -> Uuid {
    let rec = sqlx::query!(
        r#"
        INSERT INTO jobs (queue, job_type, payload_json, run_at, status, priority, max_attempts)
        VALUES ($1, $2, '{"hello":"world"}'::jsonb, now(), 'succeeded', 7, 3)
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
async fn replay_creates_new_job_with_lineage() {
    let pool = setup_db().await;
    let repo = JobsRepo::new(pool.clone());

    let old_id = insert_job_full(&pool, "default", "my_job").await;

    let new_id = repo.replay_job(old_id, None, None).await.unwrap();

    // new job exists
    let row = sqlx::query!(
        r#"
        SELECT id, queue, job_type, status, replay_of_job_id
        FROM jobs
        WHERE id = $1
        "#,
        new_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(row.status, "queued");
    assert_eq!(row.queue, "default");
    assert_eq!(row.job_type, "my_job");
    assert_eq!(row.replay_of_job_id, Some(old_id));
}

#[tokio::test]
async fn replay_allows_overrides() {
    let pool = setup_db().await;
    let repo = JobsRepo::new(pool.clone());

    let old_id = insert_job_full(&pool, "default", "my_job").await;

    let run_at = Utc::now() + ChronoDuration::seconds(30);
    let new_id = repo
        .replay_job(old_id, Some("priority-queue"), Some(run_at))
        .await
        .unwrap();

    let row = sqlx::query!(
        r#"
        SELECT queue, run_at, replay_of_job_id
        FROM jobs
        WHERE id = $1
        "#,
        new_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(row.queue, "priority-queue");
    assert_eq!(row.replay_of_job_id, Some(old_id));

    // run_at should be close (db now vs rust now differences can exist; compare >=)
    assert!(row.run_at >= run_at - ChronoDuration::seconds(1));
}
