# sz-orm v1.2.1 crates.io publish script
# Publish all 41 packages in dependency order
$ErrorActionPreference = "Continue"
if (-not $env:CARGO_REGISTRY_TOKEN) { Write-Error "CARGO_REGISTRY_TOKEN environment variable not set. Please set it before running this script."; exit 1 }

$wsRoot = "e:\vue\test\鲜视达\rust\sz-orm"
Set-Location -LiteralPath $wsRoot

# Publish order: leaf -> core -> direct deps -> high-level
$order = @(
    "sz-orm-macros",
    "sz-orm-core",
    "sz-orm-query-builder",
    "sz-orm-sql-validator",
    "sz-orm-sqlx",
    "sz-orm-crypto",
    "sz-orm-auth",
    "sz-orm-config",
    "sz-orm-logger",
    "sz-orm-limit",
    "sz-orm-masking",
    "sz-orm-audit",
    "sz-orm-health",
    "sz-orm-swagger",
    "sz-orm-ai",
    "sz-orm-back",
    "sz-orm-batch",
    "sz-orm-dtx",
    "sz-orm-es",
    "sz-orm-graphql",
    "sz-orm-grpc",
    "sz-orm-mig",
    "sz-orm-mqtt",
    "sz-orm-observability",
    "sz-orm-postgis",
    "sz-orm-queue",
    "sz-orm-rw",
    "sz-orm-scheduler",
    "sz-orm-search",
    "sz-orm-sharding",
    "sz-orm-storage",
    "sz-orm-timeseries",
    "sz-orm-tracing",
    "sz-orm-vector",
    "sz-orm-wasm",
    "sz-orm-websocket",
    "sz-orm-lc",
    "sz-orm-oracle",
    "sz-orm-mssql",
    "sz-orm-actix",
    "sz-orm-axum"
)

$success = @()
$failed = @()
$skipped = @()

Write-Host "=========================================="
Write-Host "sz-orm v1.2.1 crates.io publish"
Write-Host "Total packages: $($order.Count)"
Write-Host "=========================================="

for ($i = 0; $i -lt $order.Count; $i++) {
    $pkg = $order[$i]
    Write-Host ""
    Write-Host "[$($i + 1)/$($order.Count)] Publishing $pkg ..."
    $output = cargo publish --allow-dirty -p $pkg 2>&1
    $exitCode = $LASTEXITCODE
    if ($exitCode -eq 0) {
        Write-Host "  SUCCESS: $pkg" -ForegroundColor Green
        $success += $pkg
    } elseif ($output -match "already exists") {
        Write-Host "  SKIPPED: $pkg (already published)" -ForegroundColor Yellow
        $skipped += $pkg
    } else {
        Write-Host "  FAILED: $pkg (exit=$exitCode)" -ForegroundColor Red
        $output | Select-Object -Last 5 | ForEach-Object { Write-Host "    $_" -ForegroundColor Red }
        $failed += $pkg
        Start-Sleep -Seconds 10
    }
    Start-Sleep -Seconds 3
}

Write-Host ""
Write-Host "=========================================="
Write-Host "Publish Summary"
Write-Host "=========================================="
Write-Host "Success: $($success.Count)" -ForegroundColor Green
Write-Host "Skipped: $($skipped.Count)" -ForegroundColor Yellow
Write-Host "Failed:  $($failed.Count)" -ForegroundColor Red
if ($failed.Count -gt 0) {
    Write-Host "Failed packages:" -ForegroundColor Red
    $failed | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
}
Write-Host ""
Write-Host "Success packages:" -ForegroundColor Green
$success | ForEach-Object { Write-Host "  - $_" }
