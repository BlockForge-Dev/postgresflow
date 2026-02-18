-- 015_attempt_reason_code_constraint.sql
-- Ensure job_attempts.reason_code exists, then enforce allowed values (Law 1).

DO $$
BEGIN
  -- 1) Add the column if it doesn't exist
  IF NOT EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'job_attempts'
      AND column_name = 'reason_code'
  ) THEN
    ALTER TABLE public.job_attempts
      ADD COLUMN reason_code text;
  END IF;

  -- 2) Add constraint if it doesn't exist
  IF NOT EXISTS (
    SELECT 1
    FROM pg_constraint c
    JOIN pg_class t ON t.oid = c.conrelid
    JOIN pg_namespace n ON n.oid = t.relnamespace
    WHERE c.conname = 'job_attempts_reason_code_allowed'
      AND t.relname = 'job_attempts'
      AND n.nspname = 'public'
  ) THEN
    ALTER TABLE public.job_attempts
      ADD CONSTRAINT job_attempts_reason_code_allowed
      CHECK (
        reason_code IS NULL OR reason_code IN (
          'TIMEOUT',
          'NON_RETRYABLE',
          'HTTP_ERROR',
          'DB_ERROR',
          'BAD_PAYLOAD',
          'UNKNOWN'
        )
      );
  END IF;
END
$$;
