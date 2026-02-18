-- 1) Queue policies (enterprise safety knobs)
CREATE TABLE IF NOT EXISTS queue_policies (
  queue TEXT PRIMARY KEY,
  max_attempts_per_minute INT NOT NULL DEFAULT 60,
  max_in_flight INT NOT NULL DEFAULT 50,
  throttle_delay_ms INT NOT NULL DEFAULT 500
);

-- 2) Indexes to make checks cheap
-- in-flight check: jobs WHERE queue=? AND status='running'
CREATE INDEX IF NOT EXISTS jobs_queue_status_idx
ON jobs(queue, status);

-- retry-rate check: attempts within last minute joined to jobs.queue
CREATE INDEX IF NOT EXISTS job_attempts_started_at_idx
ON job_attempts(started_at);

-- Optional but helpful for join speed if not already indexed:
CREATE INDEX IF NOT EXISTS job_attempts_job_id_idx
ON job_attempts(job_id);


ALTER TABLE jobs
ADD COLUMN IF NOT EXISTS policy_last_decision TEXT NULL,
ADD COLUMN IF NOT EXISTS policy_last_decided_at TIMESTAMPTZ NULL;
