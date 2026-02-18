# Release Process

## Versioning
Use semantic versioning:
- MAJOR: breaking API/behavior changes
- MINOR: backward-compatible features
- PATCH: fixes and small improvements

Current package versions are declared in:
- `crates/postgresflow/Cargo.toml`
- `crates/worker/Cargo.toml`

## Pre-Release Checklist
- [ ] CI is green on main branch
- [ ] local checks pass:
  - `cargo check --workspace`
  - `cargo test --workspace --no-run`
  - `.\scripts\tests\ci.ps1` with DB env vars set
- [ ] README and docs updated
- [ ] migrations reviewed for rollout safety
- [ ] benchmark notes captured for significant performance-impacting changes

## Release Steps
1. Create release branch from `main`.
2. Bump crate versions as needed.
3. Update release notes (new file or tag notes) with:
   - features
   - fixes
   - migration notes
   - operational changes
4. Open and merge release PR.
5. Tag release commit (example):

```bash
git tag -a v0.2.0 -m "Release v0.2.0"
git push origin v0.2.0
```

6. Publish release notes in GitHub.

## Post-Release Verification
- Deploy to target environment.
- Validate:
  - `/health` returns `ok`
  - `/metrics` reports expected queue snapshot
  - enqueue + process + timeline flow works
- Monitor retry/DLQ rates for regressions.

## Rollback Plan
If release causes regressions:
1. Stop rollout.
2. Roll back application image/version.
3. If migration is incompatible, apply explicit down/repair plan before traffic restoration.
4. Capture incident summary and add follow-up actions.

