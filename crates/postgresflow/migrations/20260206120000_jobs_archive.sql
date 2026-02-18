-- Archive table for bounded storage
CREATE TABLE IF NOT EXISTS jobs_archive (
  id uuid PRIMARY KEY,

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
);

CREATE INDEX IF NOT EXISTS jobs_archive_queue_status_run_at_idx
  ON jobs_archive(queue, status, run_at);

CREATE INDEX IF NOT EXISTS jobs_archive_archived_at_idx
  ON jobs_archive(archived_at);
