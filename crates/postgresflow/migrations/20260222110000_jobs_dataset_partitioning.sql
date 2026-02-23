-- Hyper-partition jobs by dataset_id (queue + hour bucket).
-- Example dataset_id: default_20260223_09

-- 1) Ensure jobs has dataset_id data before conversion.
ALTER TABLE jobs
  ADD COLUMN IF NOT EXISTS dataset_id text;

UPDATE jobs
SET dataset_id = 'legacy'
WHERE dataset_id IS NULL;

ALTER TABLE jobs
  ALTER COLUMN dataset_id SET DEFAULT 'legacy';

ALTER TABLE jobs
  ALTER COLUMN dataset_id SET NOT NULL;

-- 2) Convert jobs -> list partitioned parent if not already partitioned.
DO $$
DECLARE
  is_partitioned boolean;
  has_replay_of boolean;
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

  IF NOT is_partitioned THEN
    ALTER TABLE jobs RENAME TO jobs_unpartitioned_dataset;

    -- Drop FKs pointing at the old jobs table before swap.
    IF EXISTS (
      SELECT 1 FROM pg_constraint WHERE conname = 'job_attempts_job_id_fkey'
    ) THEN
      ALTER TABLE job_attempts DROP CONSTRAINT job_attempts_job_id_fkey;
    END IF;

    IF EXISTS (
      SELECT 1 FROM pg_constraint WHERE conname = 'policy_decisions_job_id_fkey'
    ) THEN
      ALTER TABLE policy_decisions DROP CONSTRAINT policy_decisions_job_id_fkey;
    END IF;

    CREATE TABLE jobs (
      dataset_id text NOT NULL,
      id uuid NOT NULL DEFAULT gen_random_uuid(),

      queue text NOT NULL,
      job_type text NOT NULL,
      payload_json jsonb NOT NULL DEFAULT '{}'::jsonb,

      run_at timestamptz NOT NULL DEFAULT now(),

      status text NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued','running','succeeded','failed','dlq','canceled')),

      priority int NOT NULL DEFAULT 0,
      max_attempts int NOT NULL DEFAULT 25,

      locked_at timestamptz NULL,
      locked_by text NULL,
      lock_expires_at timestamptz NULL,

      last_error_code text NULL,
      last_error_message text NULL,

      dlq_reason_code text NULL,
      dlq_at timestamptz NULL,

      created_at timestamptz NOT NULL DEFAULT now(),
      updated_at timestamptz NOT NULL DEFAULT now(),

      replay_of_job_id uuid NULL,

      PRIMARY KEY (dataset_id, id)
    ) PARTITION BY LIST (dataset_id);

    CREATE TABLE jobs_ds_legacy PARTITION OF jobs FOR VALUES IN ('legacy');
    CREATE TABLE jobs_ds_default PARTITION OF jobs DEFAULT;

    SELECT EXISTS (
      SELECT 1
      FROM information_schema.columns
      WHERE table_schema = 'public'
        AND table_name = 'jobs_unpartitioned_dataset'
        AND column_name = 'replay_of_job_id'
    )
    INTO has_replay_of;

    IF has_replay_of THEN
      INSERT INTO jobs (
        dataset_id,
        id, queue, job_type, payload_json, run_at, status, priority, max_attempts,
        locked_at, locked_by, lock_expires_at,
        last_error_code, last_error_message,
        dlq_reason_code, dlq_at,
        created_at, updated_at,
        replay_of_job_id
      )
      SELECT
        dataset_id,
        id, queue, job_type, payload_json, run_at, status, priority, max_attempts,
        locked_at, locked_by, lock_expires_at,
        last_error_code, last_error_message,
        dlq_reason_code, dlq_at,
        created_at, updated_at,
        replay_of_job_id
      FROM jobs_unpartitioned_dataset;
    ELSE
      INSERT INTO jobs (
        dataset_id,
        id, queue, job_type, payload_json, run_at, status, priority, max_attempts,
        locked_at, locked_by, lock_expires_at,
        last_error_code, last_error_message,
        dlq_reason_code, dlq_at,
        created_at, updated_at
      )
      SELECT
        dataset_id,
        id, queue, job_type, payload_json, run_at, status, priority, max_attempts,
        locked_at, locked_by, lock_expires_at,
        last_error_code, last_error_message,
        dlq_reason_code, dlq_at,
        created_at, updated_at
      FROM jobs_unpartitioned_dataset;
    END IF;

    DROP TABLE jobs_unpartitioned_dataset;
  END IF;
END $$;

-- 3) Recreate trigger + indexes on partitioned jobs parent.
DROP TRIGGER IF EXISTS trg_jobs_updated_at ON jobs;
CREATE TRIGGER trg_jobs_updated_at
BEFORE UPDATE ON jobs
FOR EACH ROW
EXECUTE FUNCTION set_updated_at();

DROP INDEX IF EXISTS jobs_runnable_idx;
DROP INDEX IF EXISTS jobs_dequeue_cover_idx;
DROP INDEX IF EXISTS jobs_dequeue_lease_idx;

CREATE INDEX IF NOT EXISTS jobs_id_lookup_idx
  ON jobs(id);

CREATE INDEX IF NOT EXISTS jobs_queue_dataset_pick_idx
  ON jobs(queue, status, run_at, created_at, dataset_id)
  WHERE status = 'queued';

CREATE INDEX IF NOT EXISTS jobs_dataset_queue_runnable_idx
  ON jobs(dataset_id, queue, status, priority DESC, run_at ASC, created_at ASC, id)
  WHERE status = 'queued';

CREATE INDEX IF NOT EXISTS jobs_lock_expiry_idx
  ON jobs(status, lock_expires_at)
  WHERE status = 'running';

CREATE INDEX IF NOT EXISTS jobs_replay_of_idx
  ON jobs(replay_of_job_id);

CREATE INDEX IF NOT EXISTS jobs_created_at_id_desc_idx
  ON jobs(created_at DESC, id DESC);

-- 4) Runtime helper to create dataset partitions on demand.
CREATE OR REPLACE FUNCTION public.ensure_jobs_dataset_partition(p_dataset_id text)
RETURNS void
LANGUAGE plpgsql
AS $$
DECLARE
  part_name text;
  bound_exists boolean;
  base_name text;
BEGIN
  IF p_dataset_id IS NULL OR btrim(p_dataset_id) = '' THEN
    RETURN;
  END IF;

  SELECT EXISTS (
    SELECT 1
    FROM pg_class part
    JOIN pg_inherits inh ON inh.inhrelid = part.oid
    JOIN pg_class parent ON parent.oid = inh.inhparent
    JOIN pg_namespace n ON n.oid = parent.relnamespace
    WHERE n.nspname = 'public'
      AND parent.relname = 'jobs'
      AND pg_get_expr(part.relpartbound, part.oid) LIKE format('%%(%L)%%', p_dataset_id)
  )
  INTO bound_exists;

  IF bound_exists THEN
    RETURN;
  END IF;

  base_name := regexp_replace(lower(p_dataset_id), '[^a-z0-9]+', '_', 'g');
  base_name := trim(both '_' from base_name);
  IF base_name = '' THEN
    base_name := 'dataset';
  END IF;

  part_name := format(
    'jobs_ds_%s_%s',
    left(base_name, 32),
    substr(md5(p_dataset_id), 1, 8)
  );

  EXECUTE format(
    'CREATE TABLE IF NOT EXISTS public.%I PARTITION OF public.jobs FOR VALUES IN (%L)',
    part_name,
    p_dataset_id
  );
END;
$$;

-- Prime current + next-hour default queue partitions.
SELECT public.ensure_jobs_dataset_partition(
  'default_' || to_char(now() AT TIME ZONE 'UTC', 'YYYYMMDD_HH24')
);
SELECT public.ensure_jobs_dataset_partition(
  'default_' || to_char((now() + interval '1 hour') AT TIME ZONE 'UTC', 'YYYYMMDD_HH24')
);

-- 5) Carry dataset_id into child tables and restore cascade FKs.
ALTER TABLE job_attempts
  ADD COLUMN IF NOT EXISTS dataset_id text;

UPDATE job_attempts a
SET dataset_id = j.dataset_id
FROM jobs j
WHERE a.dataset_id IS NULL
  AND j.id = a.job_id;

UPDATE job_attempts
SET dataset_id = 'legacy'
WHERE dataset_id IS NULL;

ALTER TABLE job_attempts
  ALTER COLUMN dataset_id SET DEFAULT 'legacy';

ALTER TABLE job_attempts
  ALTER COLUMN dataset_id SET NOT NULL;

DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'job_attempts_dataset_job_fkey'
  ) THEN
    ALTER TABLE job_attempts DROP CONSTRAINT job_attempts_dataset_job_fkey;
  END IF;

  ALTER TABLE job_attempts
    ADD CONSTRAINT job_attempts_dataset_job_fkey
    FOREIGN KEY (dataset_id, job_id)
    REFERENCES jobs(dataset_id, id)
    ON DELETE CASCADE;
END $$;

CREATE INDEX IF NOT EXISTS job_attempts_dataset_job_id_idx
  ON job_attempts(dataset_id, job_id);

ALTER TABLE policy_decisions
  ADD COLUMN IF NOT EXISTS dataset_id text;

UPDATE policy_decisions p
SET dataset_id = j.dataset_id
FROM jobs j
WHERE p.dataset_id IS NULL
  AND j.id = p.job_id;

UPDATE policy_decisions
SET dataset_id = 'legacy'
WHERE dataset_id IS NULL;

ALTER TABLE policy_decisions
  ALTER COLUMN dataset_id SET DEFAULT 'legacy';

ALTER TABLE policy_decisions
  ALTER COLUMN dataset_id SET NOT NULL;

DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'policy_decisions_dataset_job_fkey'
  ) THEN
    ALTER TABLE policy_decisions DROP CONSTRAINT policy_decisions_dataset_job_fkey;
  END IF;

  ALTER TABLE policy_decisions
    ADD CONSTRAINT policy_decisions_dataset_job_fkey
    FOREIGN KEY (dataset_id, job_id)
    REFERENCES jobs(dataset_id, id)
    ON DELETE CASCADE;
END $$;

CREATE INDEX IF NOT EXISTS policy_decisions_dataset_job_id_idx
  ON policy_decisions(dataset_id, job_id);

-- Replay lineage becomes best-effort once jobs is partitioned by dataset_id.
DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'jobs_replay_of_fk'
  ) THEN
    ALTER TABLE jobs DROP CONSTRAINT jobs_replay_of_fk;
  END IF;
END $$;
