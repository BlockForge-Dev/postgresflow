#!/usr/bin/env bash
set -euo pipefail

echo "Starting compose..."
docker compose up -d --build

echo "Seeding 200 jobs..."
docker exec -i postgresflow-db-1 psql -U postgres -d postgresflow <<'SQL'
INSERT INTO jobs(queue, job_type, payload_json, run_at, status, priority, max_attempts)
SELECT 'default', 'ok', '{}'::jsonb, now(), 'queued', 0, 25
FROM generate_series(1,200);
SQL

echo "Restarting DB..."
docker compose restart db

echo "Waiting 10s..."
sleep 10

echo "Queue status:"
docker exec -i postgresflow-db-1 psql -U postgres -d postgresflow -c \
"SELECT status, COUNT(*) FROM jobs GROUP BY status ORDER BY status;"

echo "Done."
