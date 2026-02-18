//  this is like the security gate + baggage check at an airport before you enter the terminal (the jobs table).

// If your bag is too big → you don’t enter.

// If too many people are trying to enter at once → some are stopped to prevent stampede.
// And every time someone is denied, the guard writes it down in a logbook table (ingest_decisions) so you can prove later: “we denied this for reason X”.

use chrono::{DateTime, Timelike, Utc};
use serde_json::json;
use sqlx::PgPool;

use crate::jobs::ingest_decisions::IngestDecisionsRepo;

#[derive(Clone, Debug)]
pub struct EnqueueGuardConfig {
    pub max_payload_bytes: usize,
    pub max_enqueues_per_minute_per_queue: i64,
}

impl Default for EnqueueGuardConfig {
    fn default() -> Self {
        Self {
            max_payload_bytes: 256 * 1024,             // 256KB default
            max_enqueues_per_minute_per_queue: 10_000, // very high default (safe)
        }
    }
}

/// Enqueue-time protection: payload-size + enqueue rate limiting.
/// Writes ingest_decisions rows for denials so Law 4 is provable without logs.
#[derive(Clone)]
pub struct EnqueueGuard {
    pool: PgPool,
    decisions: IngestDecisionsRepo,
    cfg: EnqueueGuardConfig,
}

impl EnqueueGuard {
    pub fn new(pool: PgPool, decisions: IngestDecisionsRepo, cfg: EnqueueGuardConfig) -> Self {
        Self {
            pool,
            decisions,
            cfg,
        }
    }

    pub fn max_payload_bytes(&self) -> usize {
        self.cfg.max_payload_bytes
    }

    pub async fn check_payload(&self, queue: &str, payload_bytes: usize) -> anyhow::Result<()> {
        if payload_bytes > self.cfg.max_payload_bytes {
            let _ = self
                .decisions
                .record(
                    queue,
                    "DENIED",
                    "PAYLOAD_TOO_LARGE",
                    json!({
                        "max_payload_bytes": self.cfg.max_payload_bytes,
                        "payload_bytes": payload_bytes
                    }),
                )
                .await?;
            anyhow::bail!("PAYLOAD_TOO_LARGE");
        }
        Ok(())
    }

    pub async fn check_rate(&self, queue: &str) -> anyhow::Result<()> {
        let now = Utc::now();
        let window_start =
            DateTime::<Utc>::from_timestamp(now.timestamp() - (now.second() as i64), 0)
                .unwrap_or_else(Utc::now);

        let mut tx = self.pool.begin().await?;

        let count: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO enqueue_rate_counters(queue, window_start, count)
            VALUES ($1, $2, 1)
            ON CONFLICT (queue, window_start)
            DO UPDATE SET count = enqueue_rate_counters.count + 1
            RETURNING count
            "#,
        )
        .bind(queue)
        .bind(window_start)
        .fetch_one(&mut *tx)
        //SQLx treats the transaction as an executor (like a “connection handle” you can run queries on).
        // Running a query through a transaction requires mutable access to that transaction object, because the transaction’s internal state is being used/advanced
        .await?;

        if count > self.cfg.max_enqueues_per_minute_per_queue as i64 {
            // record deny
            let _ = self
                .decisions
                .record(
                    queue,
                    "DENIED",
                    "ENQUEUE_RATE_EXCEEDED",
                    json!({
                        "max_per_minute": self.cfg.max_enqueues_per_minute_per_queue,
                        "count_this_minute": count
                    }),
                )
                .await?;

            tx.commit().await?;
            anyhow::bail!("ENQUEUE_RATE_EXCEEDED");
        }

        tx.commit().await?;
        Ok(())
    }
}
