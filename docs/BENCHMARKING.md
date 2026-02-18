# Benchmarking Guide

## Goal
Produce repeatable throughput and latency measurements for PostgresFlow under controlled load.

This guide standardizes:
- test setup
- workload parameters
- data to capture
- report format

## Prerequisites
- Docker Desktop running
- repository built successfully
- idle local machine (avoid unrelated heavy workloads)

## Baseline Environment Record
Before running benchmarks, capture:
- OS and version
- CPU model and core count
- RAM size
- Docker Desktop version
- Postgres image version (`postgres:17` from `docker-compose.yml`)
- git commit hash

## Benchmark Scenarios
Run at least these scenarios:

| Scenario | Workers (profile) | Total workers | Jobs |
|---|---:|---:|---:|
| S1 | 1 | 2 | 5,000 |
| S2 | 2 | 3 | 10,000 |
| S3 | 4 | 5 | 20,000 |
| S4 | 8 | 9 | 40,000 |

Total workers = `pgflow` + `worker` profile replicas.

## Run Command

From Git Bash or WSL:

```bash
bash scripts/load/run.sh WORKERS JOBS
```

Examples:

```bash
bash scripts/load/run.sh 1 5000
bash scripts/load/run.sh 4 20000
```

The script:
- builds and starts compose services
- scales workers
- inserts test jobs
- measures completion over 30s
- prints approximate jobs/sec and `docker stats`

## Recommended Additional Measurements
The load script gives throughput but not percentile latency. Capture these after each run:

```sql
SELECT
  percentile_cont(0.50) WITHIN GROUP (ORDER BY latency_ms) AS p50_ms,
  percentile_cont(0.95) WITHIN GROUP (ORDER BY latency_ms) AS p95_ms,
  percentile_cont(0.99) WITHIN GROUP (ORDER BY latency_ms) AS p99_ms
FROM job_attempts
WHERE finished_at >= now() - interval '5 minutes';
```

Queue outcome mix:

```sql
SELECT status, COUNT(*) FROM jobs GROUP BY status ORDER BY status;
```

## Interpreting Results
- Increasing workers should improve throughput until DB saturation.
- Rising retry rate often indicates handler instability or dependency pressure.
- High queue depth with low jobs/sec indicates lease/handler or DB bottleneck.
- p95/p99 latency is a better user-facing signal than mean alone.

## Reporting Template
Use one table per commit/config:

| Commit | Scenario | Jobs/sec | p50 ms | p95 ms | p99 ms | Success rate | Retry rate | Notes |
|---|---|---:|---:|---:|---:|---:|---:|---|

Notes should include:
- config changes (lease time, policy limits, worker count)
- schema/index changes
- any observed DB warnings (WAL, CPU saturation, locks)

## Fairness Rules
- Run each scenario at least 3 times.
- Report median across runs.
- Keep config and dataset constant between compared commits.
- Do not compare debug and release mode runs together.

## Cleanup

```bash
docker compose down
```

