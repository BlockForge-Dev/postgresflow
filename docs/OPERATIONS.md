# Operations Runbook

## Runtime Topology
- `db`: Postgres
- `pgflow`: worker + optional admin API
- `worker` (profile): additional worker replicas without admin API

## Required Environment
- `DATABASE_URL` required at runtime
- `PGFLOW_WORKER_ID` optional (defaults from hostname/fallback)
- `PGFLOW_QUEUE` optional (default `default`)
- `PGFLOW_LEASE_SECONDS` optional (default `10`)
- `PGFLOW_ADMIN_ADDR` optional (`off` disables admin API)
- `PGFLOW_MIGRATE_ON_STARTUP` optional
- `PGFLOW_MAX_PAYLOAD_BYTES` optional
- `PGFLOW_MAX_ENQUEUE_PER_MINUTE` optional

Maintenance envs:
- `ARCHIVE_SUCCEEDED_AFTER_DAYS` default `7`
- `PRUNE_HISTORY_AFTER_DAYS` default `7`
- `MAINTENANCE_INTERVAL_SECS` default `60`

## Start and Stop

Start:

```powershell
docker compose up --build -d
```

Scale workers:

```powershell
docker compose --profile worker up -d --scale worker=4
```

Stop:

```powershell
docker compose down
```

## Smoke Checks
Health:

```powershell
Invoke-WebRequest -Uri http://localhost:3003/health -UseBasicParsing
```

Metrics:

```powershell
Invoke-RestMethod -Uri "http://localhost:3003/metrics"
```

DLQ:

```powershell
Invoke-RestMethod -Uri "http://localhost:3003/dlq"
```

## Observability
Use:
- `/metrics` for JSON snapshots
- `/metrics/prom` for Prometheus scraping
- `/ingest/decisions` for enqueue denials/rate events
- `/jobs/:id/timeline` and `/jobs/:id/explain` for incident triage

Key operational signals:
- runnable queue depth growth
- jobs/sec drop
- retry rate spike
- DLQ growth
- repeated policy decision reason codes

## Incident Runbooks

### Queue depth grows continuously
1. Check `/metrics` for `runnable_queue_depth`, `jobs_per_sec`, `retry_rate`.
2. Inspect worker logs for repeated handler failures/timeouts.
3. Verify DB health and connection limits.
4. Scale workers if DB has headroom.
5. If throttling is expected, review `queue_policies` and `policy_decisions`.

### Jobs stuck in running
1. Confirm lease duration (`PGFLOW_LEASE_SECONDS`).
2. Ensure `reap_expired_locks` is active (worker loop logs).
3. Check clock skew and DB time correctness.
4. Inspect handler hangs or long-running operations.

### DLQ spike
1. Query `/dlq` and inspect `dlq_reason_code`.
2. Pull timelines for representative jobs.
3. Group by `last_error_code`.
4. Fix handler/dependency issue.
5. Replay selected jobs via `POST /jobs/:id/replay`.

### Enqueue rejected
1. Check `/ingest/decisions`.
2. If `PAYLOAD_TOO_LARGE`, reduce payload or raise `PGFLOW_MAX_PAYLOAD_BYTES`.
3. If `ENQUEUE_RATE_EXCEEDED`, smooth producer traffic or raise rate limit.

## Backup and Restore (Docker Compose Local)

Backup:

```powershell
New-Item -ItemType Directory -Force -Path backups | Out-Null
docker compose exec -T db pg_dump -U postgres postgresflow_dev > backups/postgresflow_dev.sql
```

Restore:

```powershell
Get-Content backups/postgresflow_dev.sql | docker compose exec -T db psql -U postgres -d postgresflow_dev
```

For production, use managed backup strategy with point-in-time recovery.

## Capacity and Tuning Notes
- tune DB connection pool and worker count together
- monitor WAL growth and autovacuum
- ensure indexes used by runnable scans and attempt lookups remain healthy
- benchmark changes before production rollout

## Security Notes
- Admin API has no built-in auth currently.
- Do not expose it publicly without gateway auth and TLS.
- Restrict network access to trusted operators/services.

