use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

pub async fn setup_db() -> PgPool {
    // Only works if dotenvy is in dev-dependencies.
    // If not, remove these two lines.
    let _ = dotenvy::dotenv();

    let url = std::env::var("TEST_DATABASE_URL").expect(
        "TEST_DATABASE_URL missing. Example: postgres://user:pass@localhost:5432/postgresflow_test",
    );

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&url)
        .await
        .expect("failed to connect to TEST_DATABASE_URL");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations failed");

    sqlx::query(
        r#"
        TRUNCATE TABLE
            policy_decisions,
            job_attempts,
            queue_policies,
            jobs
        RESTART IDENTITY CASCADE
        "#,
    )
    .execute(&pool)
    .await
    .expect("truncate failed");

    pool
}

#[allow(dead_code)]
pub async fn insert_job(pool: &PgPool, queue: &str) -> Uuid {
    let rec = sqlx::query!(
        r#"
        INSERT INTO jobs (
            queue,
            job_type,
            payload_json,
            run_at,
            status,
            priority,
            max_attempts
        )
        VALUES ($1, 'test_job', '{}'::jsonb, now(), 'queued', 0, 5)
        RETURNING id
        "#,
        queue
    )
    .fetch_one(pool)
    .await
    .expect("failed to insert job");

    rec.id
}
