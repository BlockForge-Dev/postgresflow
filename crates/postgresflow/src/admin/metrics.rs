use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use std::collections::HashMap;

#[derive(Clone)]
pub struct AdminState {
    pub pool: PgPool,
}

#[derive(Serialize)]
pub struct Metrics {
    pub now_utc: String,
    pub totals: Totals,
    pub per_queue: Vec<QueueMetrics>,
}

#[derive(Serialize)]
pub struct Totals {
    pub queued: i64,
    pub running: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub dlq: i64,
}

#[derive(Serialize)]
pub struct QueueMetrics {
    pub queue: String,
    pub runnable: i64,
    pub scheduled: i64,
    pub in_flight: i64,
    pub dlq: i64,
    pub attempts_last_min: i64,
}

#[derive(FromRow)]
struct TotalsRow {
    queued: i64,
    running: i64,
    succeeded: i64,
    failed: i64,
    dlq: i64,
}

#[derive(FromRow)]
struct QueueRow {
    queue: String,
    runnable: i64,
    scheduled: i64,
    in_flight: i64,
    dlq: i64,
}

#[derive(FromRow)]
struct AttemptsRow {
    queue: String,
    attempts_last_min: i64,
}

pub async fn metrics(State(st): State<AdminState>) -> Result<Json<Metrics>, (StatusCode, String)> {
    // Totals by status
    let totals_row = sqlx::query_as::<_, TotalsRow>(
        r#"
        SELECT
          COUNT(*) FILTER (WHERE status = 'queued')     AS queued,
          COUNT(*) FILTER (WHERE status = 'running')    AS running,
          COUNT(*) FILTER (WHERE status = 'succeeded')  AS succeeded,
          COUNT(*) FILTER (WHERE status = 'failed')     AS failed,
          COUNT(*) FILTER (WHERE status = 'dlq')        AS dlq
        FROM jobs
        "#,
    )
    .fetch_one(&st.pool)
    .await
    .map_err(db_err)?;

    // Per-queue job stats
    let q_rows = sqlx::query_as::<_, QueueRow>(
        r#"
        SELECT
          queue,
          COUNT(*) FILTER (WHERE status='queued' AND run_at <= now()) AS runnable,
          COUNT(*) FILTER (WHERE status='queued' AND run_at >  now()) AS scheduled,
          COUNT(*) FILTER (WHERE status='running' AND lock_expires_at > now()) AS in_flight,
          COUNT(*) FILTER (WHERE status='dlq') AS dlq
        FROM jobs
        GROUP BY queue
        ORDER BY queue
        "#,
    )
    .fetch_all(&st.pool)
    .await
    .map_err(db_err)?;

    // Attempts started in the last minute, grouped by queue
    let a_rows = sqlx::query_as::<_, AttemptsRow>(
        r#"
        SELECT j.queue AS queue, COUNT(*)::bigint AS attempts_last_min
        FROM job_attempts a
        JOIN jobs j ON j.id = a.job_id AND j.dataset_id = a.dataset_id
        WHERE a.started_at >= now() - interval '1 minute'
        GROUP BY j.queue
        "#,
    )
    .fetch_all(&st.pool)
    .await
    .map_err(db_err)?;

    // Build attempts_map: queue -> attempts_last_min
    let mut attempts_map: HashMap<String, i64> = HashMap::new();
    for r in a_rows {
        attempts_map.insert(r.queue, r.attempts_last_min);
    }

    // Merge into per_queue output
    let per_queue = q_rows
        .into_iter()
        .map(|r| {
            // IMPORTANT: look up before moving r.queue
            let attempts_last_min = attempts_map.get(&r.queue).copied().unwrap_or(0);

            QueueMetrics {
                queue: r.queue,
                runnable: r.runnable,
                scheduled: r.scheduled,
                in_flight: r.in_flight,
                dlq: r.dlq,
                attempts_last_min,
            }
        })
        .collect();

    let out = Metrics {
        now_utc: chrono::Utc::now().to_rfc3339(),
        totals: Totals {
            queued: totals_row.queued,
            running: totals_row.running,
            succeeded: totals_row.succeeded,
            failed: totals_row.failed,
            dlq: totals_row.dlq,
        },
        per_queue,
    };

    Ok(Json(out))
}

fn db_err(e: sqlx::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}"))
}
