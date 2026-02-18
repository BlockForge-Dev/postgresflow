-- 0003_job_attempts.sql

CREATE TABLE IF NOT EXISTS job_attempts (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  attempt_no INT NOT NULL,

  started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  finished_at TIMESTAMPTZ NULL,

  status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed')),

  error_code TEXT NULL,
  error_message TEXT NULL,

  latency_ms INT NULL,
  worker_id TEXT NOT NULL,

  CONSTRAINT job_attempts_job_attempt_no_uq UNIQUE (job_id, attempt_no)
);

CREATE INDEX IF NOT EXISTS job_attempts_job_id_idx
  ON job_attempts(job_id);

CREATE INDEX IF NOT EXISTS job_attempts_job_started_at_idx
  ON job_attempts(job_id, started_at DESC);
