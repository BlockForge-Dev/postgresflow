# scripts/test/ci.ps1
$ErrorActionPreference = "Stop"

if (-not $env:TEST_DATABASE_URL) {
    Write-Error "TEST_DATABASE_URL is missing. Example: postgres://postgres:postgres@localhost:5432/postgresflow_test"
    exit 1
}

$env:RUSTFLAGS = "-D warnings"
$env:RUST_TEST_THREADS = "1"

cargo test -p postgresflow -- --nocapture
