CREATE TABLE IF NOT EXISTS _pgflow_meta (
  id      bigserial PRIMARY KEY,
  note    text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);
