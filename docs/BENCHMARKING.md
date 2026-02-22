# Benchmarking Guide

## Goal
Measure throughput and job state outcomes in a repeatable way for the Postgres-only flow.

This guide standardizes:
- setup
- scenario matrix
- collection queries
- result format

## Prerequisites
- Docker Desktop running
- Git Bash or WSL for `bash scripts/load/run.sh ...`
- clean local machine (avoid heavy unrelated workloads)

## Record Environment Before Runs
- OS version
- CPU model and core count
- RAM
- Docker Desktop version
- Postgres image version from `docker-compose.yml`
- git commit hash (`git rev-parse --short HEAD`)

## Benchmark Script
Run:

```bash
bash scripts/load/run.sh WORKERS JOBS
```

The script:
- brings up compose services and worker replicas
- waits for `jobs.dataset_id` migration readiness
- seeds jobs into one dataset bucket
- runs a fixed measurement window
- prints:
  - per-run succeeded delta
  - dataset/global row counts
  - approximate jobs/sec
  - `BENCH_RESULT ...` (machine-readable summary line)

Useful env vars:
- `PGFLOW_BENCH_SECONDS` (default `30`)
- `PGFLOW_DATASET_ID` (default `default_YYYYMMDD_HH`)
- `PGFLOW_MAX_ATTEMPTS`
- `PGFLOW_LOAD_JOB_TYPE`

## Scenario Matrix
Run at least:

| Scenario | Worker profile replicas | Total workers | Jobs | Purpose |
|---|---:|---:|---:|---|
| B1 | 2 | 3 | 50,000 | verify stability and DLQ behavior |
| B2 | 4 | 5 | 100,000 | scale-up midpoint |
| B3 | 8 | 9 | 100,000 | high-contention throughput check |

Total workers = `pgflow` + `worker` profile replicas.

Example commands:

```bash
PGFLOW_BENCH_SECONDS=30 bash scripts/load/run.sh 2 50000
PGFLOW_BENCH_SECONDS=30 bash scripts/load/run.sh 8 100000
```

## Optional A/B Profiles
Use the same scenario matrix for each profile:

1. Safe baseline (default durability):
```bash
PGFLOW_PG_SYNC_COMMIT=on \
PGFLOW_PG_FSYNC=on \
PGFLOW_PG_FULL_PAGE_WRITES=on \
bash scripts/load/run.sh 8 100000
```

2. Local perf profile (benchmark-only, reduced durability):
```bash
PGFLOW_DISABLE_SYNC_COMMIT=1 \
PGFLOW_PG_SYNC_COMMIT=off \
bash scripts/load/run.sh 8 100000
```

Do not use reduced durability settings for production data.

## Extra SQL Checks After Each Run
Jobs by status for the benchmark dataset:

```sql
SELECT status, COUNT(*)
FROM jobs
WHERE dataset_id = 'default_20260222_14'
GROUP BY status
ORDER BY status;
```

Global row count:

```sql
SELECT COUNT(*) AS total_rows FROM jobs;
```

Latency percentiles (recent attempts):

```sql
SELECT
  percentile_cont(0.50) WITHIN GROUP (ORDER BY latency_ms) AS p50_ms,
  percentile_cont(0.95) WITHIN GROUP (ORDER BY latency_ms) AS p95_ms,
  percentile_cont(0.99) WITHIN GROUP (ORDER BY latency_ms) AS p99_ms
FROM job_attempts
WHERE finished_at >= now() - interval '5 minutes';
```

Partition utilization snapshot:

```sql
SELECT dataset_id, COUNT(*) AS rows
FROM jobs
GROUP BY dataset_id
ORDER BY rows DESC
LIMIT 20;
```

## Reporting Template
Use one row per scenario/profile:

| Commit | Scenario | Profile | Jobs/sec | Succeeded delta | Dataset rows | Global rows | p95 ms | DLQ count | Notes |
|---|---|---|---:|---:|---:|---:|---:|---:|---|

Notes should include:
- key env vars
- schema/index differences
- saturation signal (CPU, WAL, locks)

## Fairness Rules
- Run each scenario at least 3 times.
- Report median jobs/sec.
- Keep dataset naming strategy and run window constant when comparing commits.
- Recreate workers between runs (the script already does this).

## Cleanup

```bash
docker compose down
```
