# scripts/check-all-features.ps1
# 门禁 10：Feature 全组合编译
# 对所有包运行 --all-features，但对 sz-orm-queue 排除 kafka feature（Windows 无 librdkafka C 库）

$ErrorActionPreference = "Stop"
$env:CARGO_INCREMENTAL = "0"

Write-Host "=== Gate 10: Feature all-combinations check ==="

# 获取 workspace 中所有包名（排除 workspace 本身）
$packages = cargo metadata --format-version 1 2>$null | ConvertFrom-Json | Select-Object -ExpandProperty packages | Where-Object { $_.name -ne "sz-orm" } | Select-Object -ExpandProperty name | Sort-Object -Unique

$failed = @()
$skipped = @()

foreach ($pkg in $packages) {
    if ($pkg -eq "sz-orm-queue") {
        # sz-orm-queue: 使用 all-real-no-native 而非 --all-features
        # 因为 rdkafka cmake-build 在 Windows 上栈溢出，pulsar 需要 protoc
        Write-Host "  [CHECK] $pkg --features all-real-no-native (kafka/pulsar excluded)"
        $output = cargo check -p $pkg --all-targets --features all-real-no-native 2>&1
        if ($LASTEXITCODE -ne 0) {
            $failed += "$pkg (features: all-real-no-native)"
            Write-Host "  [FAIL] $pkg" -ForegroundColor Red
            Write-Host $output
        } else {
            Write-Host "  [PASS] $pkg" -ForegroundColor Green
        }
    } else {
        Write-Host "  [CHECK] $pkg --all-features"
        $output = cargo check -p $pkg --all-targets --all-features 2>&1
        if ($LASTEXITCODE -ne 0) {
            $failed += "$pkg (features: all)"
            Write-Host "  [FAIL] $pkg" -ForegroundColor Red
            Write-Host $output
        } else {
            Write-Host "  [PASS] $pkg" -ForegroundColor Green
        }
    }
}

if ($failed.Count -gt 0) {
    Write-Host ""
    Write-Host "=== FAILED: $($failed.Count) packages ===" -ForegroundColor Red
    $failed | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
    exit 1
} else {
    Write-Host ""
    Write-Host "=== ALL PASSED ===" -ForegroundColor Green
    exit 0
}