use crate::jobs::{AttemptsRepo, JobsRepo, PolicyDecisionsRepo};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct JobTimeline {
    pub job_id: Uuid,
    pub status: String,
    pub queue: String,
    pub job_type: String,
    pub run_at: DateTime<Utc>,

    pub next_run_at: Option<DateTime<Utc>>,
    pub last_worker_id: Option<String>,
    pub last_error: Option<LastError>,

    // keep existing attempts list (backwards compatible)
    pub attempts: Vec<TimelineAttempt>,

    // ✅ new: unified ordered narrative (attempts + policy decisions)
    pub story: Vec<TimelineEvent>,
}

#[derive(Debug, Serialize)]
pub struct TimelineAttempt {
    pub id: Uuid,
    pub attempt_no: i32,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub latency_ms: Option<i32>,
    pub worker_id: String,
    pub suggested_action: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LastError {
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind")]
pub enum TimelineEvent {
    Attempt {
        at: DateTime<Utc>,
        id: Uuid,
        attempt_no: i32,
        status: String,
        worker_id: String,
        error_code: Option<String>,
        error_message: Option<String>,
        suggested_action: Option<String>,
        latency_ms: Option<i32>,
    },
    PolicyDecision {
        at: DateTime<Utc>,
        id: Uuid,
        decision: String,
        reason_code: String,
        details_json: serde_json::Value,
    },
}

pub async fn build_timeline(
    jobs: &JobsRepo,
    attempts: &AttemptsRepo,
    policy_decisions: &PolicyDecisionsRepo,
    job_id: Uuid,
) -> anyhow::Result<Option<JobTimeline>> {
    let job = match jobs.get_job(job_id).await? {
        Some(j) => j,
        None => return Ok(None),
    };

    let raw_attempts = attempts.list_attempts_for_job(job_id).await?;
    let policy_rows = policy_decisions.list_for_job(job_id).await?;

    let last_worker_id = raw_attempts.last().map(|a| a.worker_id.clone());
    let last_failed = raw_attempts.iter().rev().find(|a| a.status == "failed");

    let last_error = last_failed.map(|a| LastError {
        error_code: a.error_code.clone(),
        error_message: a.error_message.clone(),
    });

    let next_run_at = if job.status == "queued" {
        Some(job.run_at)
    } else {
        None
    };

    // suggested action mapping
    use crate::jobs::error_codes;

    let attempts_out: Vec<TimelineAttempt> = raw_attempts
        .iter()
        .cloned()
        .map(|a| {
            let suggested = a
                .error_code
                .as_deref()
                .map(|code| error_codes::suggested_action(code).to_string());

            TimelineAttempt {
                id: a.id,
                attempt_no: a.attempt_no,
                status: a.status,
                started_at: a.started_at,
                finished_at: a.finished_at,
                error_code: a.error_code,
                error_message: a.error_message,
                latency_ms: a.latency_ms,
                worker_id: a.worker_id,
                suggested_action: suggested,
            }
        })
        .collect();

    // ✅ build unified story
    let mut story: Vec<TimelineEvent> = Vec::new();

    for a in &attempts_out {
        story.push(TimelineEvent::Attempt {
            at: a.started_at,
            id: a.id,
            attempt_no: a.attempt_no,
            status: a.status.clone(),
            worker_id: a.worker_id.clone(),
            error_code: a.error_code.clone(),
            error_message: a.error_message.clone(),
            suggested_action: a.suggested_action.clone(),
            latency_ms: a.latency_ms,
        });
    }

    for p in policy_rows {
        story.push(TimelineEvent::PolicyDecision {
            at: p.created_at,
            id: p.id,
            decision: p.decision,
            reason_code: p.reason_code,
            details_json: p.details_json,
        });
    }

    // sort by time, stable tie-break so output is deterministic
    story.sort_by(|a, b| {
        let (ta, ka) = match a {
            TimelineEvent::Attempt { at, attempt_no, .. } => (*at, (1i32, *attempt_no)),
            TimelineEvent::PolicyDecision { at, .. } => (*at, (0i32, 0)),
        };
        let (tb, kb) = match b {
            TimelineEvent::Attempt { at, attempt_no, .. } => (*at, (1i32, *attempt_no)),
            TimelineEvent::PolicyDecision { at, .. } => (*at, (0i32, 0)),
        };
        ta.cmp(&tb).then(ka.cmp(&kb))
    });

    Ok(Some(JobTimeline {
        job_id: job.id,
        status: job.status,
        queue: job.queue,
        job_type: job.job_type,
        run_at: job.run_at,
        next_run_at,
        last_worker_id,
        last_error,
        attempts: attempts_out,
        story,
    }))
}
