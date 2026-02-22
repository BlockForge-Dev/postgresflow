-- Partition jobs_archive by archived_at for lower bloat and cheaper retention operations.
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
      AND c.relname = 'jobs_archive'
  )
  INTO is_partitioned;

  IF NOT is_partitioned THEN
    IF to_regclass('public.jobs_archive_unpartitioned') IS NULL
       AND to_regclass('public.jobs_archive') IS NOT NULL THEN
      ALTER TABLE public.jobs_archive RENAME TO jobs_archive_unpartitioned;
    END IF;

    IF to_regclass('public.jobs_archive') IS NULL THEN
      CREATE TABLE public.jobs_archive (
        id uuid NOT NULL,
        replay_of_job_id uuid NULL,
        queue text NOT NULL,
        job_type text NOT NULL,
        payload_json jsonb NOT NULL,
        run_at timestamptz NOT NULL,
        status text NOT NULL,
        priority int NOT NULL,
        max_attempts int NOT NULL,
        dlq_reason_code text NULL,
        dlq_at timestamptz NULL,
        created_at timestamptz NOT NULL,
        updated_at timestamptz NOT NULL,
        archived_at timestamptz NOT NULL DEFAULT now()
      ) PARTITION BY RANGE (archived_at);
    END IF;
  END IF;
END $$;

CREATE OR REPLACE FUNCTION public.ensure_jobs_archive_partition(ts timestamptz)
RETURNS void
LANGUAGE plpgsql
AS $$
DECLARE
  from_ts timestamptz := date_trunc('month', ts);
  to_ts timestamptz := from_ts + interval '1 month';
  part_name text := format('jobs_archive_%s', to_char(from_ts, 'YYYYMM'));
BEGIN
  EXECUTE format(
    'CREATE TABLE IF NOT EXISTS public.%I PARTITION OF public.jobs_archive FOR VALUES FROM (%L) TO (%L)',
    part_name,
    from_ts,
    to_ts
  );
END;
$$;

SELECT public.ensure_jobs_archive_partition(now());
SELECT public.ensure_jobs_archive_partition(now() + interval '1 month');

CREATE TABLE IF NOT EXISTS public.jobs_archive_default
  PARTITION OF public.jobs_archive DEFAULT;

DO $$
BEGIN
  IF to_regclass('public.jobs_archive_unpartitioned') IS NOT NULL THEN
    INSERT INTO public.jobs_archive (
      id, replay_of_job_id,
      queue, job_type, payload_json,
      run_at, status, priority, max_attempts,
      dlq_reason_code, dlq_at,
      created_at, updated_at, archived_at
    )
    SELECT
      id, replay_of_job_id,
      queue, job_type, payload_json,
      run_at, status, priority, max_attempts,
      dlq_reason_code, dlq_at,
      created_at, updated_at, archived_at
    FROM public.jobs_archive_unpartitioned;

    DROP TABLE public.jobs_archive_unpartitioned;
  END IF;
END $$;

CREATE INDEX IF NOT EXISTS jobs_archive_lookup_id_idx
  ON public.jobs_archive(id);

CREATE INDEX IF NOT EXISTS jobs_archive_queue_status_run_at_idx
  ON public.jobs_archive(queue, status, run_at);

CREATE INDEX IF NOT EXISTS jobs_archive_archived_at_idx
  ON public.jobs_archive(archived_at);
