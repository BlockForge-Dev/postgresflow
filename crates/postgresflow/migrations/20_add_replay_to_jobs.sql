-- Add lineage pointer (Postgres supports this)
ALTER TABLE jobs
ADD COLUMN IF NOT EXISTS replay_of_job_id uuid NULL;

-- Add FK only if it doesn't already exist (Postgres does NOT support "IF NOT EXISTS" here)
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_constraint
    WHERE conname = 'jobs_replay_of_fk'
  ) THEN
    ALTER TABLE jobs
      ADD CONSTRAINT jobs_replay_of_fk
      FOREIGN KEY (replay_of_job_id)
      REFERENCES jobs(id)
      ON DELETE SET NULL;
  END IF;
END $$;

-- Helpful index for lineage queries
CREATE INDEX IF NOT EXISTS jobs_replay_of_idx
ON jobs(replay_of_job_id);
