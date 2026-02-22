#!/usr/bin/env bash
set -euo pipefail

# Load local env file for benchmark knobs (e.g. PGFLOW_MAX_ATTEMPTS).
if [[ -f .env ]]; then
  # shellcheck disable=SC1091
  set -a
  source .env
  set +a
fi

WORKERS="${1:-1}"
JOBS="${2:-5000}"
MAX_ATTEMPTS="${PGFLOW_MAX_ATTEMPTS:-25}"
JOB_TYPE="${PGFLOW_LOAD_JOB_TYPE:-ok}"
DATASET_ID="${PGFLOW_DATASET_ID:-default_$(date -u +%Y%m%d_%H)}"
MIGRATION_WAIT_SECONDS="${PGFLOW_MIGRATION_WAIT_SECONDS:-120}"
BENCH_SECONDS="${PGFLOW_BENCH_SECONDS:-30}"

psql_db() {
  docker compose exec -T db psql -U postgres -d postgresflow_dev -t -A -c "$1"
}

echo "Starting compose..."
docker compose up -d --build

echo "Scaling workers to $WORKERS"
docker compose --profile worker up -d --build --force-recreate --scale worker="$WORKERS" worker
echo "Total workers: $((WORKERS+1)) (pgflow + worker profile)"

echo "Waiting for jobs.dataset_id migration (up to ${MIGRATION_WAIT_SECONDS}s)..."
READY=0
for _ in $(seq 1 "$MIGRATION_WAIT_SECONDS"); do
  READY=$(psql_db "
    SELECT CASE
      WHEN EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'jobs'
          AND column_name = 'dataset_id'
      ) THEN 1
      ELSE 0
    END;
  " | tr -d ' ')
  if [[ "$READY" == "1" ]]; then
    break
  fi
  sleep 1
done

if [[ "$READY" != "1" ]]; then
  echo "Timed out waiting for dataset_id schema; aborting benchmark."
  exit 1
fi

BASE_DONE=$(psql_db "SELECT COUNT(*) FROM jobs WHERE dataset_id='${DATASET_ID}' AND status='succeeded';" | tr -d ' ')
BASE_TOTAL=$(psql_db "SELECT COUNT(*) FROM jobs WHERE dataset_id='${DATASET_ID}';" | tr -d ' ')

echo "Seeding $JOBS jobs (dataset_id=$DATASET_ID, job_type=$JOB_TYPE, max_attempts=$MAX_ATTEMPTS)..."
docker compose exec -T db psql -U postgres -d postgresflow_dev <<SQL
DO \$\$
BEGIN
  IF to_regprocedure('public.ensure_jobs_dataset_partition(text)') IS NOT NULL THEN
    PERFORM public.ensure_jobs_dataset_partition('${DATASET_ID}');
  END IF;
END
\$\$;
INSERT INTO jobs(dataset_id, queue, job_type, payload_json, run_at, status, priority, max_attempts)
SELECT '${DATASET_ID}', 'default', '${JOB_TYPE}', '{}'::jsonb, now(), 'queued', 0, ${MAX_ATTEMPTS}
FROM generate_series(1,$JOBS);
SQL

echo "Measuring for ${BENCH_SECONDS}s..."
START=$(date +%s)
sleep "$BENCH_SECONDS"
END=$(date +%s)

DONE_AFTER=$(psql_db "SELECT COUNT(*) FROM jobs WHERE dataset_id='${DATASET_ID}' AND status='succeeded';" | tr -d ' ')
TOTAL_AFTER=$(psql_db "SELECT COUNT(*) FROM jobs WHERE dataset_id='${DATASET_ID}';" | tr -d ' ')
GLOBAL_TOTAL=$(psql_db "SELECT COUNT(*) FROM jobs;" | tr -d ' ')
ELAPSED=$((END-START))
DONE_DELTA=$((DONE_AFTER-BASE_DONE))
TOTAL_DELTA=$((TOTAL_AFTER-BASE_TOTAL))

if (( DONE_DELTA < 0 )); then DONE_DELTA=0; fi
if (( TOTAL_DELTA < 0 )); then TOTAL_DELTA=0; fi

JOBS_PER_SEC=$(python - <<PY
done=int("$DONE_DELTA")
elapsed=int("$ELAPSED")
print(f"{(done/elapsed if elapsed else 0):.2f}")
PY
)

echo "Succeeded (this run): $DONE_DELTA / $TOTAL_DELTA in ${ELAPSED}s"
echo "Dataset totals now: $DONE_AFTER succeeded / $TOTAL_AFTER rows (dataset_id=$DATASET_ID)"
echo "Global row count now: $GLOBAL_TOTAL"
echo "Approx throughput: $JOBS_PER_SEC jobs/sec"
echo "BENCH_RESULT workers=$WORKERS jobs_seeded=$JOBS bench_seconds=$ELAPSED dataset_id=$DATASET_ID succeeded_delta=$DONE_DELTA total_delta=$TOTAL_DELTA jobs_per_sec=$JOBS_PER_SEC global_rows=$GLOBAL_TOTAL"

echo "docker stats:"
docker stats --no-stream
