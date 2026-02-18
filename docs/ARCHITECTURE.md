# PostgresFlow Architecture

## Scope
PostgresFlow is a Postgres-backed job queue with:

- durable enqueue
- worker leasing with lock expiry
- retries and DLQ routing
- admin/inspection API
- queue policy and ingest guardrails

The current workspace has two crates:

- `crates/postgresflow`: shared library + admin API + repositories
- `crates/worker`: worker runtime (poll, execute handler, record outcome)

## High-Level Design

```text
Producer -> POST /jobs -> jobs table (queued)
                              |
                              v
                       Worker lease loop
             (SELECT ... FOR UPDATE SKIP LOCKED)
                              |
                              v
                   handler execution + attempts
                              |
             +----------------+----------------+
             |                                 |
             v                                 v
        success path                     failure path
   jobs.status=succeeded          retry backoff or DLQ transition
                                        |
                                        v
                               job_attempts + last_error

Admin API reads jobs / attempts / decisions / metrics
Maintenance loop archives/prunes old succeeded data
```

## Runtime Components

### Worker process (`crates/worker/src/main.rs`)
- loads env config
- creates DB pool
- optionally runs migrations (`PGFLOW_MIGRATE_ON_STARTUP`)
- spawns:
  - admin API task (optional via `PGFLOW_ADMIN_ADDR`)
  - maintenance task (archive/prune)
  - worker loop task (lease + execute)

### Repositories (`crates/postgresflow/src/jobs/*.rs`)
- `JobsRepo`: enqueue, lease, state transitions, replay, listing
- `AttemptsRepo`: attempt rows and completion data
- `PoliciesRepo`: per-queue throttle policy config
- `PolicyDecisionsRepo`: stores throttle decisions
- `IngestDecisionsRepo`: stores enqueue denials/throttles
- `MetricsRepo`: queue and throughput snapshots
- `MaintenanceRepo`: archive + history prune

### Admin API (`crates/postgresflow/src/api/mod.rs`)
- list/enqueue jobs
- timeline/explain/replay
- DLQ and ingest decision views
- JSON and Prometheus metrics
- health endpoint

## Data Model (Core Tables)
- `jobs`: source of truth for queued/running/completed/DLQ jobs
- `job_attempts`: immutable per-attempt execution history
- `queue_policies`: queue-level storm-control limits
- `policy_decisions`: recorded throttle decisions tied to `job_id`
- `ingest_decisions`: enqueue denials/throttles (pre-job)
- `enqueue_rate_counters`: minute bucket counters for enqueue rate limiting
- `jobs_archive`: archived succeeded jobs for bounded primary table growth

Migrations live in `crates/postgresflow/migrations`.

## Job Lifecycle
1. Producer calls `POST /jobs`.
2. Enqueue guard validates payload size and queue rate.
3. Job row is inserted with `status='queued'`.
4. Worker leases one runnable job in queue order:
   - priority DESC
   - run_at ASC
   - created_at ASC
5. Worker starts attempt, runs handler, records latency and error code/message.
6. Outcome:
   - success: `status='succeeded'`
   - retryable failure: requeue with exponential backoff + jitter
   - non-retryable or max attempts reached: `status='dlq'`

## Correctness and Delivery Semantics
- Leasing uses `FOR UPDATE SKIP LOCKED` to prevent dual lease.
- Expired running locks are reaped and re-queued.
- Delivery model is at-least-once.
- Handlers must be idempotent.

## Reliability and Maintenance
- Periodic maintenance:
  - archive succeeded jobs older than cutoff
  - prune old history rows for succeeded jobs
- Cutoffs controlled by:
  - `ARCHIVE_SUCCEEDED_AFTER_DAYS`
  - `PRUNE_HISTORY_AFTER_DAYS`
  - `MAINTENANCE_INTERVAL_SECS`

## Security Boundary (Current State)
- Admin API currently has no built-in auth.
- Keep it on localhost/private networks or behind trusted gateway/TLS.

## Scaling Model
- Horizontal scale by adding workers.
- Throughput is primarily bounded by Postgres write/read capacity.
- Queue-level policy controls protect system during bursts.

