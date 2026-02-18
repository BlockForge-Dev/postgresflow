-- 014_enqueue_rate_counters.sql
-- Rate limiting uses per-queue rolling windows (minute buckets).
-- We use a single row per (queue, window_start) with atomic increment.

CREATE TABLE IF NOT EXISTS enqueue_rate_counters (
    queue         text NOT NULL,
    window_start  timestamptz NOT NULL, -- truncated to minute
    count         bigint NOT NULL DEFAULT 0,
    PRIMARY KEY (queue, window_start)
);

CREATE INDEX IF NOT EXISTS enqueue_rate_counters_window_idx
ON enqueue_rate_counters (window_start DESC);
