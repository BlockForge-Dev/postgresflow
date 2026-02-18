#!/usr/bin/env bash
set -euo pipefail

WORKERS="${1:-1}"
JOBS="${2:-5000}"

echo "Starting compose..."
docker compose up -d --build

echo "Scaling workers to $WORKERS"
docker compose --profile worker up -d --scale worker="$WORKERS"
echo "Total workers: $((WORKERS+1)) (pgflow + worker profile)"

echo "Seeding $JOBS jobs..."
docker exec -i postgresflow-db-1 psql -U postgres -d postgresflow_dev <<SQL
INSERT INTO jobs(queue, job_type, payload_json, run_at, status, priority, max_attempts)
SELECT 'default', 'ok', '{}'::jsonb, now(), 'queued', 0, 25
FROM generate_series(1,$JOBS);
SQL

echo "Measuring for 30s..."
START=$(date +%s)
sleep 30
END=$(date +%s)

DONE=$(docker exec -i postgresflow-db-1 psql -U postgres -d postgresflow_dev -t -c "SELECT COUNT(*) FROM jobs WHERE status='succeeded';" | tr -d ' ')
TOTAL=$(docker exec -i postgresflow-db-1 psql -U postgres -d postgresflow_dev -t -c "SELECT COUNT(*) FROM jobs;" | tr -d ' ')
ELAPSED=$((END-START))

echo "Succeeded: $DONE / $TOTAL in ${ELAPSED}s"
echo "Approx throughput: $(python - <<PY
done=int("$DONE")
elapsed=int("$ELAPSED")
print(done/elapsed if elapsed else 0)
PY
) jobs/sec"

echo "docker stats:"
docker stats --no-stream
