-- 013_ingest_decisions.sql
-- Records enqueue-time denials/throttles (payload too large, enqueue rate exceeded, etc.)
-- This is separate from policy_decisions because policy_decisions is tied to a job_id.

CREATE TABLE IF NOT EXISTS ingest_decisions (
    id            uuid PRIMARY KEY,
    queue         text NOT NULL,
    decision      text NOT NULL,        -- 'DENIED' | 'THROTTLED' etc.
    reason_code   text NOT NULL,        -- 'PAYLOAD_TOO_LARGE' | 'ENQUEUE_RATE_EXCEEDED' | ...
    details_json  jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS ingest_decisions_queue_created_at_idx
ON ingest_decisions (queue, created_at DESC);
