-- Cover batch dequeue path:
--   WHERE queue=$1 AND status='queued' AND run_at<=now()
--   ORDER BY priority DESC, run_at ASC, created_at ASC
CREATE INDEX IF NOT EXISTS jobs_dequeue_cover_idx
  ON jobs (queue, status, priority DESC, run_at ASC, created_at ASC, id)
  INCLUDE (job_type, payload_json, max_attempts)
  WHERE status = 'queued';
