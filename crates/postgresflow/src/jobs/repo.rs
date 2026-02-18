// crates/postgresflow/src/jobs/repo.rs

use crate::api::models::JobListItem;
use crate::jobs::model::{Job, JobStatus, NewJob};
use chrono::{DateTime, Utc};
use serde_json::json;
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

    // ----------------------------
    // Enqueue helpers
    // ----------------------------

    pub async fn enqueue(&self, job: NewJob) -> anyhow::Result<Uuid> {
        let rec = sqlx::query!(
            r#"
            INSERT INTO jobs (queue, job_type, payload_json, run_at, status, priority, max_attempts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#,
            job.queue,
            job.job_type,
            job.payload_json,
            job.run_at,
            JobStatus::Queued.as_str(),
            job.priority,
            job.max_attempts,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(rec.id)
    }

    pub async fn enqueue_now(
        &self,
        queue: &str,
        job_type: &str,
        payload_json: serde_json::Value,
    ) -> anyhow::Result<Uuid> {
        self.enqueue(NewJob {
            queue: queue.to_string(),
            job_type: job_type.to_string(),
            payload_json,
            run_at: Utc::now(),
            priority: 0,
            max_attempts: 25,
        })
        .await
    }

    pub async fn enqueue_in(
        &self,
        queue: &str,
        job_type: &str,
        payload_json: serde_json::Value,
        delay_secs: i64,
    ) -> anyhow::Result<Uuid> {
        self.enqueue(NewJob {
            queue: queue.to_string(),
            job_type: job_type.to_string(),
            payload_json,
            run_at: Utc::now() + chrono::Duration::seconds(delay_secs),
            priority: 0,
            max_attempts: 25,
        })
        .await
    }

    pub async fn enqueue_at(
        &self,
        queue: &str,
        job_type: &str,
        payload_json: serde_json::Value,
        run_at: DateTime<Utc>,
    ) -> anyhow::Result<Uuid> {
        self.enqueue(NewJob {
            queue: queue.to_string(),
            job_type: job_type.to_string(),
            payload_json,
            run_at,
            priority: 0,
            max_attempts: 25,
        })
        .await
    }

    // ----------------------------
    // Reads
    // ----------------------------

    pub async fn get_job(&self, job_id: Uuid) -> anyhow::Result<Option<Job>> {
        let job = sqlx::query_as::<_, Job>("SELECT * FROM jobs WHERE id = $1")
            .bind(job_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(job)
    }

    // ----------------------------
    // List / DLQ views (Admin API support)
    // ----------------------------

    /// Cursor-paginated list of jobs.
    /// Cursor is (created_at, id) ordered DESC.
    ///
    /// - queue/status are optional filters
    /// - limit is clamped to [1, 500]
    pub async fn list_jobs(
        &self,
        queue: Option<&str>,
        status: Option<&str>,
        limit: i64,
        cursor_created_at: Option<DateTime<Utc>>,
        cursor_id: Option<Uuid>,
    ) -> anyhow::Result<Vec<JobListItem>> {
        let limit = limit.clamp(1, 500);

        let rows = match (queue, status, cursor_created_at, cursor_id) {
            (Some(q), Some(st), Some(ca), Some(cid)) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, queue, job_type, status,
                        run_at, priority, max_attempts,
                        last_error_code, last_error_message,
                        dlq_reason_code,
                        created_at, updated_at
                    FROM jobs
                    WHERE queue = $1 AND status = $2
                      AND (created_at, id) < ($3, $4)
                    ORDER BY created_at DESC, id DESC
                    LIMIT $5
                    "#,
                )
                .bind(q)
                .bind(st)
                .bind(ca)
                .bind(cid)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (Some(q), Some(st), _, _) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, queue, job_type, status,
                        run_at, priority, max_attempts,
                        last_error_code, last_error_message,
                        dlq_reason_code,
                        created_at, updated_at
                    FROM jobs
                    WHERE queue = $1 AND status = $2
                    ORDER BY created_at DESC, id DESC
                    LIMIT $3
                    "#,
                )
                .bind(q)
                .bind(st)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (Some(q), None, Some(ca), Some(cid)) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, queue, job_type, status,
                        run_at, priority, max_attempts,
                        last_error_code, last_error_message,
                        dlq_reason_code,
                        created_at, updated_at
                    FROM jobs
                    WHERE queue = $1
                      AND (created_at, id) < ($2, $3)
                    ORDER BY created_at DESC, id DESC
                    LIMIT $4
                    "#,
                )
                .bind(q)
                .bind(ca)
                .bind(cid)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (Some(q), None, _, _) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, queue, job_type, status,
                        run_at, priority, max_attempts,
                        last_error_code, last_error_message,
                        dlq_reason_code,
                        created_at, updated_at
                    FROM jobs
                    WHERE queue = $1
                    ORDER BY created_at DESC, id DESC
                    LIMIT $2
                    "#,
                )
                .bind(q)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (None, Some(st), Some(ca), Some(cid)) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, queue, job_type, status,
                        run_at, priority, max_attempts,
                        last_error_code, last_error_message,
                        dlq_reason_code,
                        created_at, updated_at
                    FROM jobs
                    WHERE status = $1
                      AND (created_at, id) < ($2, $3)
                    ORDER BY created_at DESC, id DESC
                    LIMIT $4
                    "#,
                )
                .bind(st)
                .bind(ca)
                .bind(cid)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (None, Some(st), _, _) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, queue, job_type, status,
                        run_at, priority, max_attempts,
                        last_error_code, last_error_message,
                        dlq_reason_code,
                        created_at, updated_at
                    FROM jobs
                    WHERE status = $1
                    ORDER BY created_at DESC, id DESC
                    LIMIT $2
                    "#,
                )
                .bind(st)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (None, None, Some(ca), Some(cid)) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, queue, job_type, status,
                        run_at, priority, max_attempts,
                        last_error_code, last_error_message,
                        dlq_reason_code,
                        created_at, updated_at
                    FROM jobs
                    WHERE (created_at, id) < ($1, $2)
                    ORDER BY created_at DESC, id DESC
                    LIMIT $3
                    "#,
                )
                .bind(ca)
                .bind(cid)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (None, None, _, _) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, queue, job_type, status,
                        run_at, priority, max_attempts,
                        last_error_code, last_error_message,
                        dlq_reason_code,
                        created_at, updated_at
                    FROM jobs
                    ORDER BY created_at DESC, id DESC
                    LIMIT $1
                    "#,
                )
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
        };

        Ok(rows)
    }

    // ----------------------------
    // Metrics snapshot (for /metrics)
    // ----------------------------

    /// Returns: (queued, running, succeeded_last_60s, failed_or_dlq_last_60s)
    pub async fn metrics_snapshot(&self) -> anyhow::Result<(i64, i64, i64, i64)> {
        let queued: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status = 'queued'")
            .fetch_one(&self.pool)
            .await?;

        let running: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status = 'running'")
            .fetch_one(&self.pool)
            .await?;

        let succeeded_last_60s: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM jobs
            WHERE status = 'succeeded'
              AND updated_at >= now() - interval '60 seconds'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let failed_last_60s: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM jobs
            WHERE status IN ('failed', 'dlq')
              AND updated_at >= now() - interval '60 seconds'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok((queued, running, succeeded_last_60s, failed_last_60s))
    }

    // ----------------------------
    // Leasing + Storm Control + Policy Decisions Log (Milestone 11)
    // ----------------------------

    /// Lease exactly one runnable job for this worker.
    ///
    /// Correctness: SELECT ... FOR UPDATE SKIP LOCKED
    ///
    /// Storm-control gates (per queue):
    /// - max_in_flight (jobs.status='running')
    /// - max_attempts_per_minute (attempts started in last 60s)
    ///
    /// If exceeded:
    /// - write a row into policy_decisions
    /// - reschedule the candidate slightly (throttle_delay_ms)
    /// - return None
    ///
    ///
    //Exactly-once-at-a-time leasing (no two workers claim the same job)
    //Please give me ONE ready job from queue X, and make it officially mine for N seconds, so nobody else can take it.
    pub async fn lease_one_job(
        &self,
        queue: &str,
        worker_id: &str,
        lease_seconds: i64,
    ) -> anyhow::Result<Option<Job>> {
        let mut tx = self.pool.begin().await?;

        // 0) Load queue policy (defaults: basically unlimited)
        // schema assumed: queue_policies(queue PK, max_attempts_per_minute, max_in_flight, throttle_delay_ms)
        let policy = sqlx::query_as::<_, (i32, i32, i32)>(
            r#"
            SELECT max_attempts_per_minute, max_in_flight, throttle_delay_ms
            FROM queue_policies
            WHERE queue = $1
            "#,
        )
        .bind(queue)
        .fetch_optional(&mut *tx)
        .await?;

        let (max_attempts_per_minute, max_in_flight, throttle_delay_ms) =
            policy.unwrap_or((i32::MAX / 4, i32::MAX / 4, 250));

        // 1) In-flight count for this queue
        let in_flight: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM jobs
            WHERE queue = $1 AND status = 'running'
            "#,
        )
        .bind(queue)
        .fetch_one(&mut *tx)
        .await?;

        // 2) Attempts started in last 60 seconds for this queue
        let attempts_last_min: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM job_attempts a
            JOIN jobs j ON j.id = a.job_id
            WHERE j.queue = $1
              AND a.started_at >= now() - interval '60 seconds'
            "#,
        )
        .bind(queue)
        .fetch_one(&mut *tx)
        .await?;

        // 3) Select a candidate job *and lock it*.
        //    This lock ensures only one worker can throttle/lease this job at a time.
        let candidate = sqlx::query_as::<_, Job>(
            r#"
            SELECT *
            FROM jobs
            WHERE queue = $1
              AND status = 'queued'
              AND run_at <= now()
            ORDER BY priority DESC, run_at ASC, created_at ASC
            FOR UPDATE SKIP LOCKED
            LIMIT 1
            "#,
        )
        .bind(queue)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(job) = candidate else {
            tx.commit().await?;
            return Ok(None);
        };

        let job_id = job.id;

        // 4) Enforce storm-control policies.
        //    If throttled, write policy_decisions + reschedule candidate slightly.
        if in_flight >= max_in_flight as i64 {
            let decision_id = Uuid::new_v4();
            sqlx::query!(
                r#"
                INSERT INTO policy_decisions (id, job_id, decision, reason_code, details_json)
                VALUES ($1, $2, 'THROTTLED', 'IN_FLIGHT_EXCEEDED', $3)
                "#,
                decision_id,
                job_id,
                json!({
                    "queue": queue,
                    "in_flight": in_flight,
                    "max_in_flight": max_in_flight,
                    "throttle_delay_ms": throttle_delay_ms
                })
            )
            .execute(&mut *tx)
            .await?;

            sqlx::query!(
                r#"
                UPDATE jobs
                SET run_at = now() + ($2::int * interval '1 millisecond'),
                    updated_at = now()
                WHERE id = $1
                "#,
                job_id,
                throttle_delay_ms
            )
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            return Ok(None);
        }

        if attempts_last_min >= max_attempts_per_minute as i64 {
            let decision_id = Uuid::new_v4();
            sqlx::query!(
                r#"
                INSERT INTO policy_decisions (id, job_id, decision, reason_code, details_json)
                VALUES ($1, $2, 'THROTTLED', 'RETRY_RATE_EXCEEDED', $3)
                "#,
                decision_id,
                job_id,
                json!({
                    "queue": queue,
                    "attempts_last_minute": attempts_last_min,
                    "max_attempts_per_minute": max_attempts_per_minute,
                    "throttle_delay_ms": throttle_delay_ms
                })
            )
            .execute(&mut *tx)
            .await?;

            sqlx::query!(
                r#"
                UPDATE jobs
                SET run_at = now() + ($2::int * interval '1 millisecond'),
                    updated_at = now()
                WHERE id = $1
                "#,
                job_id,
                throttle_delay_ms
            )
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            return Ok(None);
        }

        // 5) Lease the job for real
        let leased = sqlx::query_as::<_, Job>(
            r#"
            UPDATE jobs
            SET status = 'running',
                locked_by = $2,
                locked_at = now(),
                lock_expires_at = now() + ($3::int * interval '1 second'),
                updated_at = now()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(job_id)
        .bind(worker_id)
        .bind(lease_seconds)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(Some(leased))
    }

    // ----------------------------
    // Maintenance
    // ----------------------------

    pub async fn reap_expired_locks(&self) -> anyhow::Result<u64> {
        let res = sqlx::query!(
            r#"
            UPDATE jobs
            SET status = 'queued',
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL,
                updated_at = now()
            WHERE status = 'running'
              AND lock_expires_at IS NOT NULL
              AND lock_expires_at < now()
            "#
        )
        .execute(&self.pool)
        .await?;

        Ok(res.rows_affected())
    }

    // ----------------------------
    // State transitions
    // ----------------------------

    pub async fn mark_succeeded(&self, job_id: Uuid, worker_id: &str) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            UPDATE jobs
            SET status = 'succeeded',
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL,
                updated_at = now()
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

    pub async fn reschedule_for_retry(
        &self,
        job_id: Uuid,
        next_run_at: DateTime<Utc>,
        last_error_code: Option<&str>,
        last_error_message: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            UPDATE jobs
            SET status = 'queued',
                run_at = $2,
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL,
                updated_at = now(),
                last_error_code = $3,
                last_error_message = $4
            WHERE id = $1
            "#,
            job_id,
            next_run_at,
            last_error_code,
            last_error_message
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_failed(
        &self,
        job_id: Uuid,
        worker_id: &str,
        last_error_code: Option<&str>,
        last_error_message: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            UPDATE jobs
            SET status = 'failed',
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL,
                updated_at = now(),
                last_error_code = $3,
                last_error_message = $4
            WHERE id = $1
              AND locked_by = $2
            "#,
            job_id,
            worker_id,
            last_error_code,
            last_error_message
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_dlq(
        &self,
        job_id: Uuid,
        worker_id: &str,
        reason_code: &str,
        last_error_code: Option<&str>,
        last_error_message: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            UPDATE jobs
            SET status = 'dlq',
                dlq_reason_code = $3,
                dlq_at = now(),
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL,
                updated_at = now(),
                last_error_code = $4,
                last_error_message = $5
            WHERE id = $1
              AND locked_by = $2
            "#,
            job_id,
            worker_id,
            reason_code,
            last_error_code,
            last_error_message
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ----------------------------
    // Replay
    // ----------------------------

    pub async fn replay_job(
        &self,
        job_id: Uuid,
        override_queue: Option<&str>,
        override_run_at: Option<DateTime<Utc>>,
    ) -> anyhow::Result<Uuid> {
        let mut tx = self.pool.begin().await?;

        let src = sqlx::query_as::<_, Job>(
            r#"
            SELECT *
            FROM jobs
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .fetch_one(&mut *tx)
        .await?;

        let new_queue = override_queue.unwrap_or(src.queue.as_str()).to_string();
        let new_run_at = override_run_at.unwrap_or_else(|| Utc::now());

        let rec = sqlx::query!(
            r#"
            INSERT INTO jobs (
                queue, job_type, payload_json, run_at, status, priority, max_attempts,
                locked_at, locked_by, lock_expires_at,
                dlq_reason_code, dlq_at,
                replay_of_job_id
            )
            VALUES (
                $1, $2, $3, $4, 'queued', $5, $6,
                NULL, NULL, NULL,
                NULL, NULL,
                $7
            )
            RETURNING id
            "#,
            new_queue,
            src.job_type,
            src.payload_json,
            new_run_at,
            src.priority,
            src.max_attempts,
            src.id,
        )
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(rec.id)
    }
}
