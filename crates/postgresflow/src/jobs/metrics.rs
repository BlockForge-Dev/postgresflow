use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

#[derive(Debug, Serialize)]
pub struct Metrics {
    pub at: DateTime<Utc>,

    pub queue: String,
    pub runnable_queue_depth: i64,

    // last 60s window
    pub jobs_per_sec: f64,
    pub success_rate: f64,
    pub retry_rate: f64,
    pub mean_latency_ms: f64,
}

#[derive(Clone)]
pub struct MetricsRepo {
    pool: PgPool,
}

impl MetricsRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn snapshot_all(&self) -> anyhow::Result<Vec<Metrics>> {
        let queues: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT DISTINCT queue
            FROM jobs
            ORDER BY queue
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(queues.len());
        for queue in queues {
            out.push(self.snapshot_for_queue(&queue).await?);
        }

        Ok(out)
    }

    pub async fn snapshot_for_queue(&self, queue: &str) -> anyhow::Result<Metrics> {
        // Depth (runnable queued)
        let depth: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM jobs
            WHERE queue = $1
              AND status = 'queued'
              AND run_at <= now()
            "#,
        )
        .bind(queue)
        .fetch_one(&self.pool)
        .await?;

        // Attempts window stats (last 60 seconds)
        // - throughput ~ attempts finished per sec
        // - success_rate = succeeded / finished
        // - retry_rate = attempts with attempt_no >=2 / total attempts started
        // - mean latency = avg(latency_ms) for finished attempts
        let row = sqlx::query!(
            r#"
            WITH a AS (
              SELECT a.*
              FROM job_attempts a
              JOIN jobs j ON j.id = a.job_id
              WHERE j.queue = $1
                AND a.started_at >= now() - interval '60 seconds'
            ),
            finished AS (
              SELECT *
              FROM a
              WHERE finished_at IS NOT NULL
            )
            SELECT
              (SELECT COUNT(*) FROM finished)::float8 AS finished_count,
              (SELECT COUNT(*) FROM finished WHERE status = 'succeeded')::float8 AS succeeded_count,
              (SELECT COUNT(*) FROM a WHERE attempt_no >= 2)::float8 AS retry_count,
              (SELECT COUNT(*) FROM a)::float8 AS started_count,
              COALESCE((SELECT AVG(latency_ms)::float8 FROM finished), 0.0) AS mean_latency_ms
            "#,
            queue
        )
        .fetch_one(&self.pool)
        .await?;

        let finished_count = row.finished_count.unwrap_or(0.0);
        let succeeded_count = row.succeeded_count.unwrap_or(0.0);
        let retry_count = row.retry_count.unwrap_or(0.0);
        let started_count = row.started_count.unwrap_or(0.0);
        let mean_latency_ms = row.mean_latency_ms.unwrap_or(0.0);

        let jobs_per_sec = finished_count / 60.0;

        let success_rate = if finished_count > 0.0 {
            succeeded_count / finished_count
        } else {
            0.0
        };

        let retry_rate = if started_count > 0.0 {
            retry_count / started_count
        } else {
            0.0
        };

        Ok(Metrics {
            at: Utc::now(),
            queue: queue.to_string(),
            runnable_queue_depth: depth,
            jobs_per_sec,
            success_rate,
            retry_rate,
            mean_latency_ms,
        })
    }
}
