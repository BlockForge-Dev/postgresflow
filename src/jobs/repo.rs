use crate::jobs::model::{Job, JobStatus, NewJob};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct JobsRepo {
    pool: PgPool,
}

impl JobsRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Lease exactly one runnable job for this worker.
    /// Uses SELECT ... FOR UPDATE SKIP LOCKED to avoid double-claim.
    pub async fn lease_one_job(
        &self,
        queue: &str,
        worker_id: &str,
        lease_seconds: i64,
    ) -> anyhow::Result<Option<Job>> {
        let mut tx = self.pool.begin().await?;

        let job = sqlx::query_as::<_, Job>(
            r#"
            WITH candidate AS (
              SELECT id
              FROM jobs
              WHERE queue = $1
                AND status = 'queued'
                AND run_at <= now()
              ORDER BY priority DESC, run_at ASC, created_at ASC
              FOR UPDATE SKIP LOCKED
              LIMIT 1
            )
            UPDATE jobs j
            SET status = 'running',
                locked_by = $2,
                locked_at = now(),
                lock_expires_at = now() + ($3::int * interval '1 second')
            FROM candidate
            WHERE j.id = candidate.id
            RETURNING j.*
            "#,
        )
        .bind(queue)
        .bind(worker_id)
        .bind(lease_seconds)
        .fetch_optional(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(job)
    }

    pub async fn reap_expired_locks(&self) -> anyhow::Result<u64> {
        let res = sqlx::query!(
            r#"
            UPDATE jobs
            SET status = 'queued',
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL
            WHERE status = 'running'
              AND lock_expires_at IS NOT NULL
              AND lock_expires_at < now()
            "#
        )
        .execute(&self.pool)
        .await?;

        Ok(res.rows_affected())
    }
    /// Mark a job succeeded and clear locks.
    /// The `locked_by = worker_id` guard prevents other workers from finishing your job.
    pub async fn mark_succeeded(&self, job_id: Uuid, worker_id: &str) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            UPDATE jobs
            SET status = 'succeeded',
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL
            WHERE id = $1
              AND locked_by = $2
            "#,
            job_id,
            worker_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

//     /// enqueue(queue, job_type, payload, run_at) -> job_id
//     pub async fn enqueue(&self, job: NewJob) -> anyhow::Result<Uuid> {
//         let rec = sqlx::query!(
//             r#"
//             INSERT INTO jobs (queue, job_type, payload_json, run_at, status, priority, max_attempts)
//             VALUES ($1, $2, $3, $4, $5, $6, $7)
//             RETURNING id
//             "#,
//             job.queue,
//             job.job_type,
//             job.payload_json,
//             job.run_at,
//             JobStatus::Queued.as_str(),
//             job.priority,
//             job.max_attempts,
//         )
//         .fetch_one(&self.pool)
//         .await?;

//         Ok(rec.id)
//     }

//     /// Query runnable jobs (debug/inspection). Leasing is what workers use.
//     pub async fn list_runnable(&self, queue: &str, limit: i64) -> anyhow::Result<Vec<Job>> {
//         let jobs = sqlx::query_as::<_, Job>(
//             r#"
//             SELECT *
//             FROM jobs
//             WHERE queue = $1
//               AND status = 'queued'
//               AND run_at <= now()
//             ORDER BY priority DESC, run_at ASC, created_at ASC
//             LIMIT $2
//             "#,
//         )
//         .bind(queue)
//         .bind(limit)
//         .fetch_all(&self.pool)
//         .await?;

//         Ok(jobs)
//     }

//     /// helper: enqueue "now"
//     pub async fn enqueue_now(
//         &self,
//         queue: &str,
//         job_type: &str,
//         payload_json: serde_json::Value,
//     ) -> anyhow::Result<Uuid> {
//         self.enqueue(NewJob {
//             queue: queue.to_string(),
//             job_type: job_type.to_string(),
//             payload_json,
//             run_at: Utc::now(),
//             priority: 0,
//             max_attempts: 25,
//         })
//         .await
//     }

//     /// helper: enqueue at a specific time
//     pub async fn enqueue_at(
//         &self,
//         queue: &str,
//         job_type: &str,
//         payload_json: serde_json::Value,
//         run_at: DateTime<Utc>,
//     ) -> anyhow::Result<Uuid> {
//         self.enqueue(NewJob {
//             queue: queue.to_string(),
//             job_type: job_type.to_string(),
//             payload_json,
//             run_at,
//             priority: 0,
//             max_attempts: 25,
//         })
//         .await
//     }
// }
