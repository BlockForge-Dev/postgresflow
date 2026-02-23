-- Add lineage pointer (Postgres supports this)
ALTER TABLE jobs
ADD COLUMN IF NOT EXISTS replay_of_job_id uuid NULL;

-- Add FK only for non-partitioned jobs tables where jobs(id) can be a direct FK target.
-- Partitioned jobs uses (dataset_id, id) and replay lineage is handled as best-effort.
DO $$
DECLARE
  is_partitioned boolean;
BEGIN
  SELECT EXISTS (
    SELECT 1
    FROM pg_partitioned_table pt
    JOIN pg_class c ON c.oid = pt.partrelid
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname = 'public'
      AND c.relname = 'jobs'
  )
  INTO is_partitioned;

  IF (NOT is_partitioned) AND NOT EXISTS (
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
