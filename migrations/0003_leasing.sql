ALTER TABLE jobs
  ADD COLUMN IF NOT EXISTS locked_at timestamptz NULL,
  ADD COLUMN IF NOT EXISTS locked_by text NULL,
  ADD COLUMN IF NOT EXISTS lock_expires_at timestamptz NULL;

-- Helpful for reclaiming dead locks fast
CREATE INDEX IF NOT EXISTS jobs_lock_expiry_idx
  ON jobs (status, lock_expires_at)
  WHERE status = 'running';
