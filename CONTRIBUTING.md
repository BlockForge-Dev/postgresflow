# Contributing Guide

## Development Setup
1. Clone repository.
2. Copy env file:

```powershell
cp .env.example .env
```

3. Start database:

```powershell
docker compose up -d db
```

4. Set test env vars:

```powershell
$env:TEST_DATABASE_URL="postgres://postgres:postgres@localhost:5433/postgresflow_test"
$env:DATABASE_URL=$env:TEST_DATABASE_URL
```

## Project Structure
- `crates/postgresflow`: core library, repositories, API, migrations
- `crates/worker`: runtime worker binary
- `scripts/tests/ci.ps1`: local CI-like test runner
- `scripts/load/run.sh`: load benchmark runner

## Local Quality Checks
Run before opening a PR:

```powershell
cargo check --workspace
cargo test --workspace --no-run
$env:RUST_TEST_THREADS="1"
$env:RUSTFLAGS="-D warnings"
.\scripts\tests\ci.ps1
```

If tests fail, include failure output and root cause in the PR description.

## Coding Expectations
- Keep changes scoped and reviewable.
- Add tests for behavior changes and regressions.
- Update docs for user-facing or operational changes.
- Keep handlers idempotent (at-least-once delivery model).

## Migrations
- Add new SQL files under `crates/postgresflow/migrations`.
- Make migrations forward-safe and additive when possible.
- Document migration intent in PR description.

## Pull Request Checklist
- [ ] builds locally
- [ ] tests pass locally
- [ ] docs updated where needed
- [ ] migration impact explained (if any)
- [ ] operational impact explained (env vars, tuning, rollout steps)

## Commit Message Style
Use concise imperative summaries, for example:
- `Add queue policy decision timeline event`
- `Fix reliability test DB setup`
- `Document benchmark methodology`

## Reporting Bugs
Include:
- commit hash
- exact command run
- full error output
- expected vs actual behavior
- local environment details

