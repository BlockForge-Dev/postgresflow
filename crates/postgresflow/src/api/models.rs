// crates/postgresflow/src/api/models.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct JobListItem {
    pub id: Uuid,
    pub queue: String,
    pub job_type: String,
    pub status: String,

    pub run_at: DateTime<Utc>,
    pub priority: i32,
    pub max_attempts: i32,

    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,

    pub dlq_reason_code: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
