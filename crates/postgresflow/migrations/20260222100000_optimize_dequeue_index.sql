-- Replace oversized dequeue index with a lean lease index.
DROP INDEX IF EXISTS jobs_dequeue_cover_idx;

CREATE INDEX IF NOT EXISTS jobs_dequeue_lease_idx
  ON jobs (queue, status, priority DESC, run_at ASC, created_at ASC, id)
  WHERE status = 'queued';
