use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::jobs::model::JobStatus;
use crate::jobs::{AttemptsRepo, JobsRepo, PolicyDecisionsRepo};

#[derive(Debug, Serialize)]
pub enum NextAction {
    RetryAt(DateTime<Utc>),
    Dlq { reason: Option<String> },
    Replayable,
    Running,
    Queued,
    Succeeded,
    Unknown,
}

#[derive(Debug, Serialize)]
pub struct DebugView {
    pub job_id: Uuid,
    pub status: String,
    pub attempts: serde_json::Value,
    pub decisions: serde_json::Value,
    pub next_action: NextAction,
}

pub async fn build_debug_view(
    jobs: &JobsRepo,
    attempts: &AttemptsRepo,
    decisions: &PolicyDecisionsRepo,
    job_id: Uuid,
) -> anyhow::Result<Option<DebugView>> {
    let Some(job) = jobs.get_job(job_id).await? else {
        return Ok(None);
    };

    let tl = crate::jobs::timeline::build_timeline(jobs, attempts, decisions, job_id).await?;
    let attempts_json = serde_json::to_value(&tl.as_ref().map(|t| &t.attempts)).unwrap_or_default();
    let decisions_json =
        serde_json::to_value(&tl.as_ref().map(|t| &t.decisions)).unwrap_or_default();

    let next_action = match job.status.as_str() {
        "queued" => NextAction::Queued,
        "running" => NextAction::Running,
        "succeeded" => NextAction::Succeeded,
        "dlq" => NextAction::Dlq {
            reason: job.dlq_reason_code.clone(),
        },
        "failed" => NextAction::Replayable,
        _ => NextAction::Unknown,
    };

    Ok(Some(DebugView {
        job_id,
        status: job.status.clone(),
        attempts: attempts_json,
        decisions: decisions_json,
        next_action,
    }))
}
