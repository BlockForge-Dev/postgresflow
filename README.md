## Step-by-Step Setup (Windows PowerShell)

1. Prerequisites:

- Docker Desktop is installed and running.
- Rust toolchain is installed (`rustup`, `cargo`).

2. Create your local env file:

```powershell
cp .env.example .env
```

3. Start services:

```powershell
docker compose up --build -d
```

4. Verify service health:

```powershell
Invoke-WebRequest -Uri http://localhost:3003/health -UseBasicParsing
```

5. Run the full postgresflow test suite (with strict warnings):

```powershell
docker compose up -d db
$env:TEST_DATABASE_URL="postgres://postgres:postgres@localhost:5433/postgresflow_test"
$env:DATABASE_URL=$env:TEST_DATABASE_URL
$env:RUST_TEST_THREADS="1"
$env:RUSTFLAGS="-D warnings"
.\scripts\tests\ci.ps1
```

6. Stop services when done:

```powershell
docker compose down
```

## Documentation Index

- Architecture: `docs/ARCHITECTURE.md`
- API reference: `docs/API.md`
- Operations runbook: `docs/OPERATIONS.md`
- Benchmarking methodology: `docs/BENCHMARKING.md`
- Contributor workflow: `CONTRIBUTING.md`
- Release process: `docs/RELEASE.md`

## Quickstart (Docker)

1. Copy env template:

- Windows PowerShell:
  cp .env.example .env

2. Start:

docker compose up --build

3. Seed demo jobs (no psql needed):

docker compose exec pgflow ./pgflowctl demo

4. View admin UI / API:

http://localhost:3003/

Endpoints:

- GET /jobs
- POST /jobs
- /jobs/:id/timeline
- /jobs/:id/explain
- /jobs/:id/replay
- /dlq
- /ingest/decisions
- /metrics (JSON)
- /metrics/prom (Prometheus text)
- /health

Disable the admin API by setting `PGFLOW_ADMIN_ADDR=off` in `.env`.
Set `PGFLOW_API_TOKEN` to require `x-api-key: <token>` (or `Authorization: Bearer <token>`) on admin API endpoints.

## Verification (Exact Steps)

1. DLQ view:

curl http://localhost:3003/dlq

2. Metrics (last 60s window):

curl http://localhost:3003/metrics

3. Timeline for a failed job:

curl http://localhost:3003/jobs/JOB_ID/timeline

## Testing Before Commit (Detailed)

1. Fast compile and test-build checks (no database needed):

```powershell
cargo check -p postgresflow
cargo test -p postgresflow --no-run
```

Expected result: both commands finish successfully. Warnings are acceptable; errors are not.

2. Targeted API behavior test tied to timeline endpoints in `crates/postgresflow/src/api/mod.rs`:

```powershell
$env:TEST_DATABASE_URL="postgres://postgres:postgres@localhost:5433/postgresflow_test"
cargo test -p postgresflow --test timeline -- --nocapture
```

Expected result: `test timeline_shows_attempt_story ... ok`.

If you see `TEST_DATABASE_URL missing`, set the variable and rerun.

3. Full integration suite:

```powershell
$env:TEST_DATABASE_URL="postgres://postgres:postgres@localhost:5433/postgresflow_test"
$env:DATABASE_URL=$env:TEST_DATABASE_URL
$env:RUST_TEST_THREADS="1"
$env:RUSTFLAGS="-D warnings"
cargo test -p postgresflow -- --nocapture
```

Equivalent helper script:

```powershell
$env:TEST_DATABASE_URL="postgres://postgres:postgres@localhost:5433/postgresflow_test"
$env:DATABASE_URL=$env:TEST_DATABASE_URL
.\scripts\tests\ci.ps1
```

4. Manual API smoke test (add `x-api-key` header if `PGFLOW_API_TOKEN` is set):

```powershell
Invoke-RestMethod -Method Post -Uri http://localhost:3003/jobs -ContentType "application/json" -Body '{"queue":"default","job_type":"email_send","payload_json":{"user_id":123}}'
Invoke-RestMethod -Uri "http://localhost:3003/jobs?limit=5"
Invoke-RestMethod -Uri "http://localhost:3003/metrics"
Invoke-RestMethod -Uri "http://localhost:3003/dlq"
```

Expected result: enqueue returns a `job_id`, list/metrics/dlq endpoints return JSON without `Authorization` headers.

5. Teardown (if you started infra for tests):

```powershell
docker compose down
```

## Test Evidence (Local)

This project was tested locally using Docker Compose and the admin API. Example evidence below
shows successful enqueue, retry/DLQ path, timeline, metrics, and guardrails.
Outputs will differ per run.

Environment:

- OS: Windows
- Start: `docker compose up --build`

Commands and sample outputs:

Enqueue success:

```
Invoke-RestMethod -Method Post -Uri http://localhost:3003/jobs -ContentType "application/json" -Body $body
# { "job_id": "JOB_ID_SUCCESS" }
```

Enqueue failure (DLQ path):

```
Invoke-RestMethod -Method Post -Uri http://localhost:3003/jobs -ContentType "application/json" -Body $failBody
# { "job_id": "JOB_ID_FAIL" }
```

Timeline (success):

```
Invoke-RestMethod -Uri "http://localhost:3003/jobs/JOB_ID_SUCCESS/timeline"
# status: succeeded
```

Timeline (fail + retries):

```
Invoke-RestMethod -Uri "http://localhost:3003/jobs/JOB_ID_FAIL/timeline"
# status: queued -> retries -> dlq after max_attempts
```

DLQ:

```
Invoke-RestMethod -Uri "http://localhost:3003/dlq"
# items: [ ... job_type=fail_me, status=dlq, dlq_reason_code=MAX_ATTEMPTS_EXCEEDED ... ]
```

Metrics (JSON + Prom):

```
Invoke-RestMethod -Uri "http://localhost:3003/metrics"
Invoke-WebRequest -Uri "http://localhost:3003/metrics/prom" -UseBasicParsing
```

Guardrail (payload too large):

```
Invoke-RestMethod -Method Post -Uri http://localhost:3003/jobs -ContentType "application/json" -Body $bigBody
# HTTP 413 PAYLOAD_TOO_LARGE
Invoke-RestMethod -Uri "http://localhost:3003/ingest/decisions"
# reason_code: PAYLOAD_TOO_LARGE
```

## Live Metrics Watch (Demo Run)

PowerShell:

.\scripts\metrics\watch.ps1

Optional parameters:

- `-IntervalSeconds 1`
- `-Queue default`
- `-BaseUrl http://localhost:3003`

## Integration Tests (Docker DB)

```powershell
docker compose up -d db
$env:TEST_DATABASE_URL="postgres://postgres:postgres@localhost:5433/postgresflow_test"
$env:DATABASE_URL=$env:TEST_DATABASE_URL
$env:RUST_TEST_THREADS="1"
$env:RUSTFLAGS="-D warnings"
.\scripts\tests\ci.ps1
```

If you see `TEST_DATABASE_URL missing` or `DATABASE_URL must be set`, set both env vars as shown above.

## Benchmarks

Load test script (Docker):

bash scripts/load/run.sh WORKERS JOBS

Example:

PGFLOW_BENCH_SECONDS=30 bash scripts/load/run.sh 2 50000
PGFLOW_BENCH_SECONDS=30 bash scripts/load/run.sh 8 100000

Outputs:
- run-local succeeded delta for the selected dataset
- dataset and global row counts
- approximate jobs/sec
- a machine-readable summary line:
  `BENCH_RESULT workers=... jobs_per_sec=... global_rows=...`

Useful benchmark env vars:
- `PGFLOW_DATASET_ID` to pin a dataset/partition for repeated runs
- `PGFLOW_BENCH_SECONDS` to change the sample window
- `PGFLOW_MAX_ATTEMPTS` and `PGFLOW_LOAD_JOB_TYPE` for workload shape

Script notes:
- auto-waits for `jobs.dataset_id` migration
- creates dataset partition via `ensure_jobs_dataset_partition(...)` when available
- recreates worker profile containers so code/env changes are applied per run

Outputs approximate jobs/sec and basic container stats.
Note: total workers = 1 (pgflow) + <workers> (worker profile).
Windows: run from Git Bash or WSL.

For repeatable benchmark methodology and reporting format, see `docs/BENCHMARKING.md`.

## Throughput Features

- Dataset partitioning on `jobs.dataset_id` (hourly/default bucket strategy supported).
- Batch dequeue/lease path with dequeue index tuning for queue scans.
- Batch attempt lifecycle writes (`start_attempts_batch`) and batch success acks.
- Archive/prune maintenance hooks for long-term table/index health.
- Optional API key auth on admin API via `PGFLOW_API_TOKEN`.

## Integration (Production)

### Enqueue via HTTP

POST /jobs

Example:

curl -X POST http://localhost:3003/jobs \
 -H "Content-Type: application/json" \
 -d '{"queue":"default","job_type":"email_send","payload_json":{"user_id":123},"priority":0,"max_attempts":25}'

### Enqueue via Rust

Use `JobsRepo::enqueue_*` plus `EnqueueGuard` in your producer service to enforce limits
and write ingest_decisions for rejected payloads/rates.

### Worker Logic

Replace the simulated work in `crates/worker/src/main.rs` with real job handlers.
Pattern:

- lease job
- start attempt
- run handler based on job_type
- call `runner.on_success(...)` or `runner.on_failure(...)`
  Handlers should return meaningful error codes (e.g., `TIMEOUT`, `BAD_PAYLOAD`, `UNKNOWN_JOB_TYPE`).
  Handlers can be registered with per-handler concurrency limits and timeouts in `crates/worker/src/handlers.rs`.

### Scaling Workers

Start extra workers (admin stays on pgflow):

docker compose --profile worker up -d --scale worker=4

### Security and Isolation

- Bind admin to localhost or a private network in production.
- Run the admin API behind a reverse proxy with TLS if exposed beyond localhost.

### Configuration (Key Env Vars)

- `PGFLOW_MAX_PAYLOAD_BYTES` and `PGFLOW_MAX_ENQUEUE_PER_MINUTE` for guardrails.
- `PGFLOW_LEASE_SECONDS` for lock timeouts.
- `PGFLOW_DEQUEUE_BATCH_SIZE` for batch leasing per poll.
- `PGFLOW_REAP_INTERVAL_MS` to control orphan-lease reap cadence.
- `PGFLOW_VERBOSE_JOB_LOGS` to enable/disable per-job hot-path logs.
- `PGFLOW_DB_MAX_CONNECTIONS` and `PGFLOW_DB_ACQUIRE_TIMEOUT_SECS` for pool sizing.
- `PGFLOW_DISABLE_SYNC_COMMIT` and `PGFLOW_DISABLE_JIT` for DB session tuning.
- `ARCHIVE_SUCCEEDED_AFTER_DAYS` and `PRUNE_HISTORY_AFTER_DAYS` for retention.
- `PGFLOW_API_TOKEN` to require `x-api-key` / bearer token on admin endpoints.

## Production Checklist

- Configure enqueue guardrails: `PGFLOW_MAX_PAYLOAD_BYTES`, `PGFLOW_MAX_ENQUEUE_PER_MINUTE`.
- Tune retry policy and max_attempts per job type.
- Move admin API behind TLS if exposed beyond localhost.
- Monitor `/metrics/prom` and DB health (connections, WAL, disk).

## Invariants

- Every attempt is recorded with error_code, error_message, worker_id, and latency.
- Every retry or DLQ transition preserves lineage in job history.
- Leasing is exclusive per job (no two workers hold the same job at once).
- Execution is at-least-once; handlers must be idempotent.
- DLQ is explicit and queryable with a reason code.
- Policy decisions (throttles) are persisted for inspection.
- Replay creates a new job with a pointer to the original.

## Why This Pattern

- One datastore (Postgres) for both durability and coordination.
- Strong consistency and transactional leasing.
- Low ops overhead: no extra brokers, simple local setup.

## Tradeoffs and Constraints

- Throughput is bounded by a single Postgres primary.
- Heavy write volume competes with application workloads.
- Very large payloads or extremely high enqueue rates require guardrails.
- Long-running jobs reduce effective concurrency unless tuned.
- Exactly-once execution is not guaranteed; design handlers accordingly.

## What Scales

- Horizontal workers (stateless).
- Multiple queues with per-queue policies.
- Read-heavy admin and metrics endpoints.

## Constraints to Watch

- Index health on jobs/job_attempts for high volumes.
- Connection pool sizing in workers and admin API.
- Disk I/O and WAL growth under sustained throughput.
- Vacuum/autovacuum tuning for large job tables.


