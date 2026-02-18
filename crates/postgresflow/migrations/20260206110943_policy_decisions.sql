-- Add up migration script here
-- policy_decisions

CREATE TABLE IF NOT EXISTS policy_decisions (
  id uuid PRIMARY KEY,
  job_id uuid NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,

  decision text NOT NULL,        -- THROTTLED / DELAYED / QUARANTINED
  reason_code text NOT NULL,     -- IN_FLIGHT_EXCEEDED / RETRY_RATE_EXCEEDED / etc

  details_json jsonb NOT NULL DEFAULT '{}'::jsonb,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS policy_decisions_job_id_created_at_idx
  ON policy_decisions(job_id, created_at);
