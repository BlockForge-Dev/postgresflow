# PostgresFlow API Reference

## Base
- Base URL: `http://localhost:3003`
- Content type: JSON for request/response bodies
- Auth: none built-in (secure this at network/gateway layer)

## Health

### `GET /health`
Returns simple liveness response.

Response:

```text
ok
```

## Jobs

### `POST /jobs`
Create a new job.

Request body:

```json
{
  "queue": "default",
  "job_type": "email_send",
  "payload_json": { "user_id": 123 },
  "run_at": "2026-02-16T12:34:56Z",
  "priority": 0,
  "max_attempts": 25
}
```

Field notes:
- `job_type` required and non-empty
- `queue` optional, defaults to `default`
- `payload_json` required JSON value
- `run_at` optional, defaults to now
- `priority` optional, defaults to `0`
- `max_attempts` optional, defaults to `25` and must be `> 0`

Success response:

```json
{ "job_id": "uuid" }
```

Common errors:
- `400` invalid input (`job_type` missing, `max_attempts <= 0`)
- `413` payload rejected by size guard (`PAYLOAD_TOO_LARGE`)
- `429` enqueue rate exceeded (`ENQUEUE_RATE_EXCEEDED`)
- `500` internal server error

### `GET /jobs`
List jobs with optional filters and cursor pagination.

Query params:
- `queue` optional
- `status` optional
- `limit` optional (clamped to `1..500`, default `100`)
- `cursor_created_at` optional RFC3339 timestamp
- `cursor_id` optional UUID

Response:

```json
{
  "items": [
    {
      "id": "uuid",
      "queue": "default",
      "job_type": "email_send",
      "status": "queued",
      "run_at": "2026-02-16T12:34:56Z",
      "priority": 0,
      "max_attempts": 25,
      "last_error_code": null,
      "last_error_message": null,
      "dlq_reason_code": null,
      "created_at": "2026-02-16T12:34:56Z",
      "updated_at": "2026-02-16T12:34:56Z"
    }
  ],
  "next_cursor_created_at": "2026-02-16T12:34:56Z",
  "next_cursor_id": "uuid"
}
```

### `GET /dlq`
Same response shape as `GET /jobs`, with status forced to `dlq`.

Query params:
- `queue` optional
- `limit` optional
- cursor params same as `GET /jobs`

## Timeline and Explain

### `GET /jobs/:id/timeline`
Returns timeline detail for a job.

Response:
- `200` with timeline document
- `404` if job not found
- `500` on server error

Timeline includes:
- job metadata (`job_id`, `status`, `queue`, `job_type`, `run_at`)
- attempt list
- ordered story stream (`Attempt` + `PolicyDecision` events)
- `last_error` and suggested actions where available

### `GET /jobs/:id/explain`
Returns summary diagnosis of job state.

Response:

```json
{
  "job_id": "uuid",
  "status": "queued",
  "queue": "default",
  "job_type": "email_send",
  "summary": "Retry scheduled. Next run at ...",
  "attempts": 2,
  "failed_attempts": 1,
  "next_run_at": "2026-02-16T12:34:56Z",
  "last_error": {
    "error_code": "TIMEOUT",
    "error_message": "dependency timeout"
  },
  "dlq_reason_code": null,
  "suggested_action": "Check upstream dependency health..."
}
```

## Replay

### `POST /jobs/:id/replay`
Create a new queued job from an existing job.

Request body:

```json
{
  "queue": "priority",
  "run_at": "2026-02-16T12:34:56Z"
}
```

Both fields are optional. If omitted:
- queue defaults to source job queue
- run_at defaults to now

Response:

```json
{
  "new_job_id": "uuid",
  "replay_of_job_id": "uuid"
}
```

## Ingest Decisions

### `GET /ingest/decisions`
List enqueue-time decisions (for guardrail visibility).

Query params:
- `queue` optional
- `limit` optional (clamped to `1..500`, default `100`)

Response item fields:
- `id`
- `queue`
- `decision`
- `reason_code`
- `details_json`
- `created_at`

## Metrics

### `GET /metrics`
JSON metrics snapshot for one queue or all queues.

Query params:
- `queue` optional

Response:

```json
{
  "now_utc": "2026-02-16T12:34:56Z",
  "queues": [
    {
      "at": "2026-02-16T12:34:56Z",
      "queue": "default",
      "runnable_queue_depth": 12,
      "jobs_per_sec": 4.2,
      "success_rate": 0.96,
      "retry_rate": 0.08,
      "mean_latency_ms": 43.5
    }
  ]
}
```

### `GET /metrics/prom`
Prometheus text endpoint with:
- `pgflow_queue_depth`
- `pgflow_running_jobs`
- `pgflow_jobs_succeeded_last_60s`
- `pgflow_jobs_failed_last_60s`

## Admin UI

### `GET /`
Returns an HTML admin page that calls the endpoints above.

