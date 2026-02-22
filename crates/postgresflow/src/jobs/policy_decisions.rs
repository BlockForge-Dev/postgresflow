use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PolicyDecisionRow {
    pub id: Uuid,
    pub job_id: Uuid,
    pub decision: String,    // THROTTLED / DELAYED / QUARANTINED
    pub reason_code: String, // IN_FLIGHT_EXCEEDED / RETRY_RATE_EXCEEDED ...
    pub details_json: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct PolicyDecisionsRepo {
    pool: PgPool,
}

impl PolicyDecisionsRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert_decision(
        &self,
        job_id: Uuid,
        decision: &str,
        reason_code: &str,
        details_json: Value,
    ) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO policy_decisions (
              id, dataset_id, job_id, decision, reason_code, details_json
            )
            VALUES (
              $1,
              (SELECT dataset_id FROM jobs WHERE id = $2 LIMIT 1),
              $2, $3, $4, $5
            )
            "#,
        )
        .bind(id)
        .bind(job_id)
        .bind(decision)
        .bind(reason_code)
        .bind(details_json)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    pub async fn list_for_job(&self, job_id: Uuid) -> anyhow::Result<Vec<PolicyDecisionRow>> {
        let rows = sqlx::query_as::<_, PolicyDecisionRow>(
            r#"
            SELECT id, job_id, decision, reason_code, details_json, created_at
            FROM policy_decisions
            WHERE job_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(job_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
}
