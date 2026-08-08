#!/usr/bin/env pwsh
# M4.8: WASM 产物 gzip 体积断言脚本
# 验证 wasm-opt 优化后 gzip 体积 ≤ 1MB
#
# 用法：.\scripts\check_wasm_size.ps1

$ErrorActionPreference = "Stop"

$wasmFile = "$PSScriptRoot\..\target\wasm32-unknown-unknown\release\sz_orm_wasm.wasm"
$limit = 1048576  # 1MB

if (-not (Test-Path $wasmFile)) {
    Write-Host "WASM 文件不存在，请先运行: cargo build --target wasm32-unknown-unknown -p sz-orm-wasm --features js --release" -ForegroundColor Red
    exit 1
}

$rawSize = (Get-Item $wasmFile).Length

# 使用 Python 计算 gzip 体积
$wasmPath = $wasmFile -replace '\\', '/'
$gzipSize = python -c "import gzip; print(len(gzip.compress(open(r'$wasmPath','rb').read())))"

Write-Host "=== WASM 体积报告 ===" -ForegroundColor Cyan
Write-Host "Raw:  $([math]::Round($rawSize / 1KB, 2)) KB ($rawSize bytes)" -ForegroundColor White
Write-Host "Gzip: $([math]::Round($gzipSize / 1KB, 2)) KB ($gzipSize bytes)" -ForegroundColor White
Write-Host "Limit: 1MB ($limit bytes)" -ForegroundColor White

if ([int]$gzipSize -le $limit) {
    Write-Host "`nPASS: gzip 体积 ≤ 1MB" -ForegroundColor Green
    exit 0
} else {
    Write-Host "`nFAIL: gzip 体积 > 1MB" -ForegroundColor Red
    Write-Host "超标: $([int]$gzipSize - $limit) bytes" -ForegroundColor Red
    exit 1
}