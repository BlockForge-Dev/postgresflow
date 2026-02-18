use chrono::{DateTime, Utc};

use serde_json::Value;

use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Job {
    pub replay_of_job_id: Option<Uuid>,

    pub id: Uuid,
    pub queue: String,
    pub job_type: String,
    pub payload_json: Value,
    pub run_at: DateTime<Utc>,
    pub status: String,
    pub priority: i32,
    pub max_attempts: i32,

    pub locked_at: Option<DateTime<Utc>>,
    pub locked_by: Option<String>,
    pub lock_expires_at: Option<DateTime<Utc>>,

    pub dlq_reason_code: Option<String>,
    pub dlq_at: Option<DateTime<Utc>>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewJob {
    pub queue: String,
    pub job_type: String,
    pub payload_json: Value,
    pub run_at: DateTime<Utc>,
    pub priority: i32,
    pub max_attempts: i32,
}

pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Dlq,
    Canceled,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Running => "running",
            JobStatus::Succeeded => "succeeded",
            JobStatus::Failed => "failed",
            JobStatus::Dlq => "dlq",
            JobStatus::Canceled => "canceled",
        }
    }
}
