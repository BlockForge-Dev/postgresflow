# pgflow Test Matrix (must stay green)

This file proves the test suite covers the product’s invariants.

---

## Constitution (5 Laws)

### Law 1 — Every failure has a recorded reason code

Covered by:

- tests/attempts.rs::finish_failed_sets_error_fields
- tests/dlq.rs::non_retryable_goes_to_dlq_immediately

### Law 2 — Every job is replayable

Covered by:

- tests/replay.rs

### Law 3 — Retries are bounded and budgeted

Covered by:

- tests/retries.rs
- tests/dlq.rs::exhausted_retries_moves_job_to_dlq_and_preserves_attempts
- tests/storm_control.rs
- tests/policy_decisions.rs

### Law 4 — System protects itself from abuse

Covered by:

- tests/storm_control.rs
- tests/policy_decisions.rs

### Law 5 — Debug without logs

Covered by:

- tests/timeline.rs
- tests/policy_timeline.rs
- tests/error_classification.rs

---

## Reliability (chaos-ish)

Covered by:

- tests/leasing.rs::lease_expires_then_other_worker_can_claim
- tests/leasing.rs::leasing_two_workers_never_claim_same_job
- tests/reliability_worker_crash.rs

Notes:

- DB restart chaos test is not automated yet (script planned).

---

## Load & Cost

Status:

- Not automated in cargo test yet.
- Planned: benches/ and a script that prints throughput/latency.

Planned artifacts:

- benches/load.rs (criterion or custom harness)
- scripts/test/load.ps1 (runs workers + pushes N jobs)
