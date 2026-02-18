use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{postgres::PgPoolOptions, FromRow, PgPool};
use std::env;
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!(
            "pgflowctl <command>\n\
             Commands:\n\
             - reset\n\
             - seed <n>\n\
             - demo\n\
             - timeline <job_id>\n\
             - demo-timeline\n\
             \n\
             Uses DATABASE_URL or TEST_DATABASE_URL.\n"
        );
        std::process::exit(2);
    }

    let url = env::var("DATABASE_URL")
        .or_else(|_| env::var("TEST_DATABASE_URL"))
        .expect("DATABASE_URL or TEST_DATABASE_URL must be set");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await?;

    match args[1].as_str() {
        "reset" => reset(&pool).await?,
        "seed" => {
            let n: i64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
            seed(&pool, n).await?;
        }
        "demo" => {
            reset(&pool).await?;
            seed(&pool, 5).await?;
            show_counts(&pool).await?;
        }
        "timeline" => {
            let id = args.get(2).expect("usage: pgflowctl timeline <job_id>");
            let job_id: Uuid = id.parse()?;
            print_timeline(&pool, job_id).await?;
        }
        "demo-timeline" => {
            reset(&pool).await?;
            let job_id = seed_one_with_failed_attempt(&pool, "default", "fail_me").await?;
            println!("\n=== TIMELINE for {job_id} ===");
            print_timeline(&pool, job_id).await?;
        }
        other => {
            eprintln!("Unknown command: {other}");
            std::process::exit(2);
        }
    }

    Ok(())
}

async fn reset(pool: &PgPool) -> anyhow::Result<()> {
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
    .execute(pool)
    .await?;

    println!("reset OK");
    Ok(())
}

async fn seed(pool: &PgPool, n: i64) -> anyhow::Result<()> {
    for i in 0..n {
        let job_type = if i % 2 == 0 { "demo_ok" } else { "fail_me" };

        let job_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO jobs (queue, job_type, payload_json, run_at, status, priority, max_attempts)
            VALUES ('default', $1, '{}'::jsonb, now(), 'queued', 0, 5)
            RETURNING id
            "#,
        )
        .bind(job_type)
        .fetch_one(pool)
        .await?;

        println!("+ inserted job {job_type} id={job_id}");
    }
    Ok(())
}

async fn show_counts(pool: &PgPool) -> anyhow::Result<()> {
    let queued: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status='queued'")
        .fetch_one(pool)
        .await?;
    let running: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status='running'")
        .fetch_one(pool)
        .await?;
    let dlq: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status='dlq'")
        .fetch_one(pool)
        .await?;

    println!("jobs: queued={queued} running={running} dlq={dlq}");
    Ok(())
}

async fn seed_one_with_failed_attempt(
    pool: &PgPool,
    queue: &str,
    job_type: &str,
) -> anyhow::Result<Uuid> {
    // 1) Insert job
    let job_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO jobs (queue, job_type, payload_json, run_at, status, priority, max_attempts)
        VALUES ($1, $2, '{}'::jsonb, now(), 'queued', 0, 5)
        RETURNING id
        "#,
    )
    .bind(queue)
    .bind(job_type)
    .fetch_one(pool)
    .await?;

    // 2) Simulate lease
    sqlx::query(
        r#"
        UPDATE jobs
        SET status='running',
            locked_by='demo-worker',
            locked_at=now(),
            lock_expires_at=now() + interval '30 seconds'
        WHERE id=$1
        "#
    )
    .bind(job_id)
    .execute(pool)
    .await?;

    // 3) Attempt failed with Law 1 fields: error_code + error_message
    sqlx::query(
        r#"
        INSERT INTO job_attempts (
            job_id, attempt_no, started_at, finished_at, status,
            error_code, error_message, worker_id, latency_ms
        )
        VALUES ($1, 1, now(), now(), 'failed',
                'TIMEOUT', 'demo timeout', 'demo-worker', 12)
        "#
    )
    .bind(job_id)
    .execute(pool)
    .await?;

    // 4) Reschedule
    sqlx::query(
        r#"
        UPDATE jobs
        SET status='queued',
            run_at=now() + interval '5 seconds',
            locked_by=NULL,
            locked_at=NULL,
            lock_expires_at=NULL
        WHERE id=$1
        "#
    )
    .bind(job_id)
    .execute(pool)
    .await?;

    Ok(job_id)
}

async fn print_timeline(pool: &PgPool, job_id: Uuid) -> anyhow::Result<()> {
    #[derive(FromRow)]
    struct JobRow {
        id: Uuid,
        queue: String,
        job_type: String,
        status: String,
        run_at: DateTime<Utc>,
        locked_by: Option<String>,
        lock_expires_at: Option<DateTime<Utc>>,
        dlq_at: Option<DateTime<Utc>>,
        dlq_reason_code: Option<String>,
    }

    let job: JobRow = sqlx::query_as(
        r#"
        SELECT id, queue, job_type, status, run_at, locked_by, lock_expires_at, dlq_at, dlq_reason_code
        FROM jobs
        WHERE id = $1
        "#
    )
    .bind(job_id)
    .fetch_one(pool)
    .await?;

    println!(
        "JOB: id={} queue={} type={} status={} run_at={} locked_by={:?} lock_expires_at={:?} dlq_at={:?} dlq_reason={:?}",
        job.id,
        job.queue,
        job.job_type,
        job.status,
        job.run_at,
        job.locked_by,
        job.lock_expires_at,
        job.dlq_at,
        job.dlq_reason_code
    );

    // NOTE: use error_code/error_message (NOT reason_message)
    #[derive(FromRow)]
    struct EventRow {
        ts: Option<DateTime<Utc>>,
        kind: Option<String>,
        data: Option<Value>,
    }

    let events: Vec<EventRow> = sqlx::query_as(
        r#"
        SELECT ts, kind, data
        FROM (
          SELECT
            COALESCE(finished_at, started_at) AS ts,
            'attempt' AS kind,
            jsonb_build_object(
              'attempt_no', attempt_no,
              'status', status,
              'started_at', started_at,
              'finished_at', finished_at,
              'error_code', error_code,
              'error_message', error_message,
              'worker_id', worker_id,
              'latency_ms', latency_ms
            ) AS data
          FROM job_attempts
          WHERE job_id = $1

          UNION ALL

          SELECT
            created_at AS ts,
            'policy' AS kind,
            jsonb_build_object(
              'decision', decision,
              'reason_code', reason_code,
              'details_json', details_json
            ) AS data
          FROM policy_decisions
          WHERE job_id = $1
        ) x
        ORDER BY ts ASC
        "#
    )
    .bind(job_id)
    .fetch_all(pool)
    .await?;

    for e in events {
        let ts = e
            .ts
            .map(|t: DateTime<Utc>| t.to_rfc3339())
            .unwrap_or_else(|| "-".to_string());
        let kind = e.kind.unwrap_or_else(|| "-".to_string());
        let data = e
            .data
            .map(|v: Value| v.to_string())
            .unwrap_or_else(|| "{}".to_string());
        println!("{ts} | {kind} | {data}");
    }

    Ok(())
}
