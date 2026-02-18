use axum::response::Html;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::api::models::JobListItem;
use crate::jobs::enqueue_guard::EnqueueGuard;
use crate::jobs::ingest_decisions::IngestDecisionsRepo;
use crate::jobs::metrics::MetricsRepo;
use crate::jobs::model::NewJob;
use crate::jobs::{AttemptsRepo, JobsRepo, PolicyDecisionsRepo};

pub mod models;

#[derive(Clone)]
pub struct ApiState {
    pub jobs: JobsRepo,
    pub attempts: AttemptsRepo,
    pub policy_decisions: PolicyDecisionsRepo,
    pub ingest_decisions: IngestDecisionsRepo,
    pub metrics: MetricsRepo,
    pub enqueue_guard: EnqueueGuard,
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/", get(admin_index))
        // Admin / inspect
        .route("/jobs", get(list_jobs).post(enqueue_job))
        .route("/jobs/:id/timeline", get(get_timeline))
        .route("/jobs/:id/explain", get(explain_job))
        .route("/jobs/:id/replay", post(replay_job))
        .route("/dlq", get(list_dlq))
        .route("/ingest/decisions", get(list_ingest_decisions))
        // Metrics
        .route("/metrics", get(metrics))
        .route("/metrics/prom", get(metrics_prom))
        // Health
        .route("/health", get(health))
        .with_state(state)
}

const ADMIN_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>PostgresFlow Admin</title>
  <style>
    :root {
      color-scheme: light;
      --bg: #f6f7fb;
      --panel: #ffffff;
      --border: #d7dbe6;
      --text: #1b1f2a;
      --muted: #5b6275;
      --accent: #1f6feb;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      font-family: "Segoe UI", "Helvetica Neue", Arial, sans-serif;
      background: var(--bg);
      color: var(--text);
    }
    header {
      padding: 20px 24px;
      border-bottom: 1px solid var(--border);
      background: var(--panel);
    }
    h1 { margin: 0; font-size: 20px; }
    main {
      padding: 16px 24px 32px;
      display: grid;
      gap: 16px;
      grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    }
    section {
      background: var(--panel);
      border: 1px solid var(--border);
      border-radius: 10px;
      padding: 12px 14px;
      box-shadow: 0 1px 2px rgba(0,0,0,0.04);
    }
    h2 { margin: 0 0 8px; font-size: 16px; }
    .muted { color: var(--muted); font-size: 12px; }
    label { display: block; font-size: 12px; margin: 6px 0 4px; }
    input, textarea {
      width: 100%;
      padding: 8px;
      border: 1px solid var(--border);
      border-radius: 6px;
      font-family: inherit;
    }
    button {
      margin-top: 8px;
      padding: 8px 12px;
      border: 1px solid var(--accent);
      background: var(--accent);
      color: white;
      border-radius: 6px;
      cursor: pointer;
    }
    pre {
      margin: 10px 0 0;
      padding: 10px;
      background: #0f172a;
      color: #e5e7eb;
      border-radius: 8px;
      font-size: 12px;
      overflow: auto;
      min-height: 120px;
    }
  </style>
</head>
<body>
  <header>
    <h1>PostgresFlow Admin</h1>
    <div class="muted">Endpoints: GET /jobs, POST /jobs, /jobs/:id/timeline, /jobs/:id/explain, /jobs/:id/replay, /dlq, /metrics, /metrics/prom</div>
  </header>
  <main>
    <section>
      <h2>Metrics</h2>
      <div class="muted">Queue depth, jobs/sec, success rate, retry rate, mean latency (last 60s).</div>
      <label>Queue (optional)</label>
      <input id="metrics-queue" placeholder="default" />
      <button onclick="loadMetrics()">Refresh</button>
      <pre id="metrics-out">{}</pre>
    </section>
    <section>
      <h2>Enqueue Job</h2>
      <label>Queue</label>
      <input id="enqueue-queue" placeholder="default" />
      <label>Job Type</label>
      <input id="enqueue-type" placeholder="email_send" />
      <label>Payload JSON</label>
      <textarea id="enqueue-payload" rows="4">{}</textarea>
      <label>Run At (optional ISO 8601)</label>
      <input id="enqueue-runat" placeholder="2026-02-16T12:34:56Z" />
      <label>Priority (optional)</label>
      <input id="enqueue-priority" placeholder="0" />
      <label>Max Attempts (optional)</label>
      <input id="enqueue-max-attempts" placeholder="25" />
      <button onclick="enqueueJob()">Enqueue</button>
      <pre id="enqueue-out">{}</pre>
    </section>
    <section>
      <h2>List Jobs</h2>
      <label>Queue</label>
      <input id="jobs-queue" placeholder="default" />
      <label>Status</label>
      <input id="jobs-status" placeholder="queued | running | succeeded | failed | dlq" />
      <label>Limit</label>
      <input id="jobs-limit" placeholder="100" />
      <button onclick="listJobs()">Fetch</button>
      <pre id="jobs-out">{}</pre>
    </section>
    <section>
      <h2>DLQ View</h2>
      <label>Queue (optional)</label>
      <input id="dlq-queue" placeholder="default" />
      <label>Limit</label>
      <input id="dlq-limit" placeholder="100" />
      <button onclick="listDlq()">Fetch</button>
      <pre id="dlq-out">{}</pre>
    </section>
    <section>
      <h2>Timeline</h2>
      <label>Job ID</label>
      <input id="timeline-id" placeholder="uuid" />
      <button onclick="getTimeline()">Fetch</button>
      <pre id="timeline-out">{}</pre>
    </section>
    <section>
      <h2>Explain Job</h2>
      <label>Job ID</label>
      <input id="explain-id" placeholder="uuid" />
      <button onclick="getExplain()">Fetch</button>
      <pre id="explain-out">{}</pre>
    </section>
    <section>
      <h2>Replay Job</h2>
      <label>Job ID</label>
      <input id="replay-id" placeholder="uuid" />
      <label>Override Queue (optional)</label>
      <input id="replay-queue" placeholder="priority" />
      <label>Override Run At (optional ISO 8601)</label>
      <input id="replay-runat" placeholder="2026-02-16T12:34:56Z" />
      <button onclick="replayJob()">Replay</button>
      <pre id="replay-out">{}</pre>
    </section>
  </main>
  <script>
    function buildHeaders(extra) {
      return Object.assign({}, extra || {});
    }

    async function show(path, targetId, opts) {
      opts = opts || {};
      opts.headers = buildHeaders(opts.headers);
      const res = await fetch(path, opts);
      const txt = await res.text();
      let out = txt;
      try { out = JSON.stringify(JSON.parse(txt), null, 2); } catch (e) {}
      document.getElementById(targetId).textContent = out;
    }

    function loadMetrics() {
      const q = document.getElementById("metrics-queue").value.trim();
      const params = new URLSearchParams();
      if (q) params.set("queue", q);
      const path = params.toString() ? "/metrics?" + params.toString() : "/metrics";
      show(path, "metrics-out");
    }

    function listJobs() {
      const params = new URLSearchParams();
      const queue = document.getElementById("jobs-queue").value.trim();
      const status = document.getElementById("jobs-status").value.trim();
      const limit = document.getElementById("jobs-limit").value.trim();
      if (queue) params.set("queue", queue);
      if (status) params.set("status", status);
      if (limit) params.set("limit", limit);
      show("/jobs?" + params.toString(), "jobs-out");
    }

    function enqueueJob() {
      const queue = document.getElementById("enqueue-queue").value.trim();
      const jobType = document.getElementById("enqueue-type").value.trim();
      const payloadRaw = document.getElementById("enqueue-payload").value.trim();
      const runAt = document.getElementById("enqueue-runat").value.trim();
      const priority = document.getElementById("enqueue-priority").value.trim();
      const maxAttempts = document.getElementById("enqueue-max-attempts").value.trim();

      if (!jobType) {
        document.getElementById("enqueue-out").textContent = "job_type is required";
        return;
      }

      let payload = {};
      if (payloadRaw) {
        try {
          payload = JSON.parse(payloadRaw);
        } catch (e) {
          document.getElementById("enqueue-out").textContent = "payload must be valid JSON";
          return;
        }
      }

      const body = { job_type: jobType, payload_json: payload };
      if (queue) body.queue = queue;
      if (runAt) body.run_at = runAt;
      if (priority) body.priority = parseInt(priority, 10);
      if (maxAttempts) body.max_attempts = parseInt(maxAttempts, 10);

      show("/jobs", "enqueue-out", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body)
      });
    }

    function listDlq() {
      const params = new URLSearchParams();
      const queue = document.getElementById("dlq-queue").value.trim();
      const limit = document.getElementById("dlq-limit").value.trim();
      if (queue) params.set("queue", queue);
      if (limit) params.set("limit", limit);
      show("/dlq?" + params.toString(), "dlq-out");
    }

    function getTimeline() {
      const id = document.getElementById("timeline-id").value.trim();
      if (!id) return;
      show("/jobs/" + id + "/timeline", "timeline-out");
    }

    function getExplain() {
      const id = document.getElementById("explain-id").value.trim();
      if (!id) return;
      show("/jobs/" + id + "/explain", "explain-out");
    }

    function replayJob() {
      const id = document.getElementById("replay-id").value.trim();
      if (!id) return;
      const queue = document.getElementById("replay-queue").value.trim();
      const runAt = document.getElementById("replay-runat").value.trim();
      const body = {};
      if (queue) body.queue = queue;
      if (runAt) body.run_at = runAt;
      show("/jobs/" + id + "/replay", "replay-out", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body)
      });
    }
  </script>
</body>
</html>
"#;

pub async fn admin_index() -> Html<&'static str> {
    Html(ADMIN_HTML)
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn internal_err(e: anyhow::Error) -> (StatusCode, String) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("internal error: {e}"),
    )
}

#[derive(Debug, Deserialize)]
pub struct ReplayRequest {
    pub queue: Option<String>,
    pub run_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct ReplayResponse {
    pub new_job_id: Uuid,
    pub replay_of_job_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct EnqueueRequest {
    pub queue: Option<String>,
    pub job_type: String,
    pub payload_json: Value,
    pub run_at: Option<DateTime<Utc>>,
    pub priority: Option<i32>,
    pub max_attempts: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct EnqueueResponse {
    pub job_id: Uuid,
}

fn enqueue_err(e: anyhow::Error) -> (StatusCode, String) {
    let msg = e.to_string();
    if msg.contains("PAYLOAD_TOO_LARGE") {
        (StatusCode::PAYLOAD_TOO_LARGE, msg)
    } else if msg.contains("ENQUEUE_RATE_EXCEEDED") {
        (StatusCode::TOO_MANY_REQUESTS, msg)
    } else {
        internal_err(e)
    }
}

pub async fn enqueue_job(
    State(state): State<ApiState>,
    Json(body): Json<EnqueueRequest>,
) -> Result<Json<EnqueueResponse>, (StatusCode, String)> {
    let EnqueueRequest {
        queue,
        job_type,
        payload_json,
        run_at,
        priority,
        max_attempts,
    } = body;

    if job_type.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "job_type is required".into()));
    }

    let queue = queue.unwrap_or_else(|| "default".to_string());
    let payload_bytes = serde_json::to_vec(&payload_json)
        .map_err(|e| internal_err(e.into()))?
        .len();

    state
        .enqueue_guard
        .check_payload(&queue, payload_bytes)
        .await
        .map_err(enqueue_err)?;
    state
        .enqueue_guard
        .check_rate(&queue)
        .await
        .map_err(enqueue_err)?;

    let max_attempts = max_attempts.unwrap_or(25);
    if max_attempts <= 0 {
        return Err((StatusCode::BAD_REQUEST, "max_attempts must be > 0".into()));
    }

    let job_id = state
        .jobs
        .enqueue(NewJob {
            queue,
            job_type,
            payload_json,
            run_at: run_at.unwrap_or_else(Utc::now),
            priority: priority.unwrap_or(0),
            max_attempts,
        })
        .await
        .map_err(internal_err)?;

    Ok(Json(EnqueueResponse { job_id }))
}

pub async fn replay_job(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ReplayRequest>,
) -> Result<Json<ReplayResponse>, (StatusCode, String)> {
    let new_id = state
        .jobs
        .replay_job(id, body.queue.as_deref(), body.run_at)
        .await
        .map_err(internal_err)?;

    Ok(Json(ReplayResponse {
        new_job_id: new_id,
        replay_of_job_id: id,
    }))
}

pub async fn get_timeline(
    Path(id): Path<Uuid>,
    State(state): State<ApiState>,
) -> impl IntoResponse {
    match crate::jobs::timeline::build_timeline(
        &state.jobs,
        &state.attempts,
        &state.policy_decisions,
        id,
    )
    .await
    {
        Ok(Some(tl)) => (StatusCode::OK, Json(tl)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: "job not found".into(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorBody {
                error: format!("internal error: {e}"),
            }),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct ListJobsQuery {
    pub queue: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub cursor_created_at: Option<DateTime<Utc>>,
    pub cursor_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct ListJobsResponse {
    pub items: Vec<JobListItem>,
    pub next_cursor_created_at: Option<DateTime<Utc>>,
    pub next_cursor_id: Option<Uuid>,
}

pub async fn list_jobs(
    State(state): State<ApiState>,
    Query(q): Query<ListJobsQuery>,
) -> Result<Json<ListJobsResponse>, (StatusCode, String)> {
    let items = state
        .jobs
        .list_jobs(
            q.queue.as_deref(),
            q.status.as_deref(),
            q.limit.unwrap_or(100),
            q.cursor_created_at,
            q.cursor_id,
        )
        .await
        .map_err(internal_err)?;

    let (next_cursor_created_at, next_cursor_id) = items
        .last()
        .map(|x| (Some(x.created_at), Some(x.id)))
        .unwrap_or((None, None));

    Ok(Json(ListJobsResponse {
        items,
        next_cursor_created_at,
        next_cursor_id,
    }))
}

pub async fn list_dlq(
    State(state): State<ApiState>,
    Query(mut q): Query<ListJobsQuery>,
) -> Result<Json<ListJobsResponse>, (StatusCode, String)> {
    // force status=dlq
    q.status = Some("dlq".to_string());
    list_jobs(State(state), Query(q)).await
}

#[derive(Debug, Deserialize)]
pub struct ListIngestDecisionsQuery {
    pub queue: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct IngestDecisionRow {
    pub id: Uuid,
    pub queue: String,
    pub decision: String,
    pub reason_code: String,
    pub details_json: Value,
    pub created_at: DateTime<Utc>,
}

pub async fn list_ingest_decisions(
    State(state): State<ApiState>,
    Query(q): Query<ListIngestDecisionsQuery>,
) -> Result<Json<Vec<IngestDecisionRow>>, (StatusCode, String)> {
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let rows = state
        .ingest_decisions
        .list_recent(q.queue.as_deref(), limit)
        .await
        .map_err(internal_err)?;

    Ok(Json(
        rows.into_iter()
            .map(
                |(id, queue, decision, reason_code, details_json, created_at)| IngestDecisionRow {
                    id,
                    queue,
                    decision,
                    reason_code,
                    details_json,
                    created_at,
                },
            )
            .collect(),
    ))
}

#[derive(Debug, Deserialize)]
pub struct MetricsQuery {
    pub queue: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    pub now_utc: DateTime<Utc>,
    pub queues: Vec<crate::jobs::metrics::Metrics>,
}

pub async fn metrics(
    State(state): State<ApiState>,
    Query(q): Query<MetricsQuery>,
) -> Result<Json<MetricsResponse>, (StatusCode, String)> {
    let queues = if let Some(queue) = q.queue {
        vec![state
            .metrics
            .snapshot_for_queue(&queue)
            .await
            .map_err(internal_err)?]
    } else {
        state.metrics.snapshot_all().await.map_err(internal_err)?
    };

    Ok(Json(MetricsResponse {
        now_utc: Utc::now(),
        queues,
    }))
}

pub async fn metrics_prom(State(state): State<ApiState>) -> Response {
    // Minimal Prometheus text format (no extra crate needed).
    match state.jobs.metrics_snapshot().await {
        Ok((queued, running, succeeded_last_60s, failed_last_60s)) => {
            let body = format!(
                concat!(
                    "# HELP pgflow_queue_depth Number of queued jobs\n",
                    "# TYPE pgflow_queue_depth gauge\n",
                    "pgflow_queue_depth {}\n",
                    "# HELP pgflow_running_jobs Number of running jobs\n",
                    "# TYPE pgflow_running_jobs gauge\n",
                    "pgflow_running_jobs {}\n",
                    "# HELP pgflow_jobs_succeeded_last_60s Jobs succeeded in last 60s\n",
                    "# TYPE pgflow_jobs_succeeded_last_60s gauge\n",
                    "pgflow_jobs_succeeded_last_60s {}\n",
                    "# HELP pgflow_jobs_failed_last_60s Jobs failed/dlq in last 60s\n",
                    "# TYPE pgflow_jobs_failed_last_60s gauge\n",
                    "pgflow_jobs_failed_last_60s {}\n"
                ),
                queued, running, succeeded_last_60s, failed_last_60s
            );

            (StatusCode::OK, body).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("metrics error: {e}"),
        )
            .into_response(),
    }
}

#[derive(Debug, Serialize)]
pub struct ExplainResponse {
    pub job_id: Uuid,
    pub status: String,
    pub queue: String,
    pub job_type: String,
    pub summary: String,
    pub attempts: i32,
    pub failed_attempts: i32,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_error: Option<crate::jobs::timeline::LastError>,
    pub dlq_reason_code: Option<String>,
    pub suggested_action: Option<String>,
}

pub async fn explain_job(
    Path(id): Path<Uuid>,
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let job = match state.jobs.get_job(id).await {
        Ok(Some(job)) => job,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorBody {
                    error: "job not found".into(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorBody {
                    error: format!("internal error: {e}"),
                }),
            )
                .into_response();
        }
    };

    let timeline = match crate::jobs::timeline::build_timeline(
        &state.jobs,
        &state.attempts,
        &state.policy_decisions,
        id,
    )
    .await
    {
        Ok(Some(tl)) => tl,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorBody {
                    error: "job not found".into(),
                }),
            )
                .into_response()
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorBody {
                    error: format!("internal error: {e}"),
                }),
            )
                .into_response()
        }
    };

    let attempts = timeline.attempts.len() as i32;
    let failed_attempts = timeline
        .attempts
        .iter()
        .filter(|a| a.status == "failed")
        .count() as i32;

    let suggested_action = timeline
        .last_error
        .as_ref()
        .and_then(|e| e.error_code.as_deref())
        .map(|code| crate::jobs::error_codes::suggested_action(code).to_string());

    let summary = match timeline.status.as_str() {
        "succeeded" => format!("Succeeded after {} attempt(s).", attempts.max(1)),
        "running" => "Currently running.".to_string(),
        "dlq" => format!(
            "Moved to DLQ after {} attempt(s). Reason: {}.",
            attempts.max(1),
            job.dlq_reason_code
                .clone()
                .unwrap_or_else(|| "UNKNOWN".to_string())
        ),
        "queued" => {
            if timeline.last_error.is_some() {
                let next_run = timeline
                    .next_run_at
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_else(|| "unknown".to_string());
                format!("Retry scheduled. Next run at {}.", next_run)
            } else {
                "Queued and waiting to run.".to_string()
            }
        }
        other => format!("Status: {other}."),
    };

    (
        StatusCode::OK,
        Json(ExplainResponse {
            job_id: timeline.job_id,
            status: timeline.status,
            queue: timeline.queue,
            job_type: timeline.job_type,
            summary,
            attempts,
            failed_attempts,
            next_run_at: timeline.next_run_at,
            last_error: timeline.last_error,
            dlq_reason_code: job.dlq_reason_code,
            suggested_action,
        }),
    )
        .into_response()
}

pub async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}
