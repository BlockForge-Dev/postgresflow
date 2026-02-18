use sqlx::PgPool;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct QueuePolicy {
    pub queue: String,
    pub max_attempts_per_minute: i32,
    pub max_in_flight: i32,
    pub throttle_delay_ms: i32,
}

#[derive(Clone)]
pub struct PoliciesRepo {
    pool: PgPool,
}

impl PoliciesRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_policy(&self, queue: &str) -> anyhow::Result<Option<QueuePolicy>> {
        let rec = sqlx::query_as::<_, QueuePolicy>(
            r#"
            SELECT queue, max_attempts_per_minute, max_in_flight, throttle_delay_ms
            FROM queue_policies
            WHERE queue = $1
            "#,
        )
        .bind(queue)
        .fetch_optional(&self.pool)
        .await?;

        Ok(rec)
    }

    pub async fn upsert_policy(
        &self,
        queue: &str,
        max_attempts_per_minute: i32,
        max_in_flight: i32,
        throttle_delay_ms: i32,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO queue_policies(queue, max_attempts_per_minute, max_in_flight, throttle_delay_ms)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT(queue) DO UPDATE
            SET max_attempts_per_minute = EXCLUDED.max_attempts_per_minute,
                max_in_flight = EXCLUDED.max_in_flight,
                throttle_delay_ms = EXCLUDED.throttle_delay_ms
            "#,
            queue,
            max_attempts_per_minute,
            max_in_flight,
            throttle_delay_ms
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
