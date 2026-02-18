-- Helps workers efficiently find runnable jobs in a queue.
-- Query shape:
--   WHERE queue = $1 AND status='queued' AND run_at <= now()
--   ORDER BY priority DESC, run_at ASC, created_at ASC
CREATE INDEX IF NOT EXISTS jobs_runnable_idx
  ON jobs (queue, status, run_at, priority DESC, created_at);
