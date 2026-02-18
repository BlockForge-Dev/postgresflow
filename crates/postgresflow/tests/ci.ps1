$ErrorActionPreference = "Stop"

Write-Host "Running migrations-based tests..." -ForegroundColor Cyan
cargo test -p postgresflow

Write-Host "Running worker crate tests..." -ForegroundColor Cyan
cargo test -p worker
