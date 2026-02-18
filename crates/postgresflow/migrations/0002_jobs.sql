CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- jobs = the single source of truth for "work to be done"
CREATE TABLE IF NOT EXISTS jobs (
  id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),

  queue        text NOT NULL,
  job_type     text NOT NULL,
  payload_json jsonb NOT NULL DEFAULT '{}'::jsonb,

  run_at       timestamptz NOT NULL DEFAULT now(),

  status       text NOT NULL DEFAULT 'queued'
    CHECK (status IN ('queued','running','succeeded','failed','dlq','canceled')),

  priority     int NOT NULL DEFAULT 0,
  max_attempts int NOT NULL DEFAULT 25,

  created_at   timestamptz NOT NULL DEFAULT now(),
  updated_at   timestamptz NOT NULL DEFAULT now()
);

-- updated_at auto bump
CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS trigger AS $$
BEGIN
  NEW.updated_at = now();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_jobs_updated_at ON jobs;
CREATE TRIGGER trg_jobs_updated_at
BEFORE UPDATE ON jobs
FOR EACH ROW
EXECUTE FUNCTION set_updated_at();

-- runnable scan index (future-proof for leasing + scheduling)
CREATE INDEX IF NOT EXISTS jobs_runnable_idx
  ON jobs (queue, status, run_at, priority DESC, created_at);
