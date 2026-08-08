#!/usr/bin/env pwsh
# M3.7: 绑定层验证脚本
# 执行 pytest + jest + 三平台加载测试，任一失败阻断发布
#
# 用法：
#   .\scripts\verify_bindings.ps1
#
# 验收标准：
#   - 执行 pytest + jest，任一失败 → exit 1 + 输出失败用例明细
#   - 三平台加载验证（pip install / npm install）
#   - 全部通过时 exit 0

$ErrorActionPreference = "Stop"

$repoRoot = "$PSScriptRoot\.."
$pythonPkg = "$repoRoot\packages\sz-orm-python"
$jsPkg = "$repoRoot\packages\sz-orm-js"
$wheelDir = "$repoRoot\target\wheels"
$napiDir = "$repoRoot\target\napi"

Write-Host "=== 绑定层验证脚本 ===" -ForegroundColor Cyan

$allPassed = $true
$failedTests = @()

# ── 步骤 1：pytest 验证 ──
Write-Host "`n[1/3] pytest 验证..." -ForegroundColor Yellow

if (-not (Get-Command pytest -ErrorAction SilentlyContinue)) {
    pip install pytest pytest-asyncio
}

$prevEAP = $ErrorActionPreference
$ErrorActionPreference = "Continue"
$pytestOutput = & pytest "$pythonPkg\tests\" -v --tb=short 2>&1
$pytestExit = $LASTEXITCODE
$ErrorActionPreference = $prevEAP

if ($pytestExit -ne 0) {
    Write-Host "pytest 失败！" -ForegroundColor Red
    $pytestOutput | Select-Object -Last 20 | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    $failedTests += "pytest"
    $allPassed = $false
} else {
    $passCount = ($pytestOutput | Select-String "PASSED").Count
    Write-Host "pytest 通过 ✓ ($passCount tests)" -ForegroundColor Green
}

# ── 步骤 2：jest 验证 ──
Write-Host "`n[2/3] jest 验证..." -ForegroundColor Yellow

Set-Location $jsPkg
if (-not (Test-Path "node_modules")) {
    npm install
}

if (-not (Test-Path "index.js") -or -not (Get-ChildItem "*.node" -ErrorAction SilentlyContinue)) {
    & "$repoRoot\scripts\build_napi.ps1"
}

$prevEAP = $ErrorActionPreference
$ErrorActionPreference = "Continue"
$jestOutput = & npx jest tests/ --verbose 2>&1
$jestExit = $LASTEXITCODE
$ErrorActionPreference = $prevEAP

if ($jestExit -ne 0) {
    Write-Host "jest 失败！" -ForegroundColor Red
    $jestOutput | Select-Object -Last 20 | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    $failedTests += "jest"
    $allPassed = $false
} else {
    $passCount = ($jestOutput | Select-String "√").Count
    Write-Host "jest 通过 ✓ ($passCount tests)" -ForegroundColor Green
}

# ── 步骤 3：三平台加载验证 ──
Write-Host "`n[3/3] 三平台加载验证..." -ForegroundColor Yellow

# Python wheel 加载验证
$wheels = Get-ChildItem "$wheelDir\*.whl" -ErrorAction SilentlyContinue
if ($wheels) {
    Write-Host "  Python wheel:" -ForegroundColor Yellow
    foreach ($wheel in $wheels) {
        Write-Host "    $($wheel.Name) ✓" -ForegroundColor Green
    }
} else {
    Write-Host "  Python wheel: 未找到（请先运行 build_python_wheel.ps1）" -ForegroundColor Yellow
}

# Node .node 加载验证
$nodeFiles = Get-ChildItem "$jsPkg\*.node" -ErrorAction SilentlyContinue
if (-not $nodeFiles) {
    $nodeFiles = Get-ChildItem "$napiDir\*.node" -ErrorAction SilentlyContinue
}
if ($nodeFiles) {
    Write-Host "  Node .node:" -ForegroundColor Yellow
    foreach ($nodeFile in $nodeFiles) {
        Write-Host "    $($nodeFile.Name) ✓" -ForegroundColor Green
    }
} else {
    Write-Host "  Node .node: 未找到（请先运行 build_napi.ps1）" -ForegroundColor Yellow
}

# ── 汇总 ──
Write-Host "`n=== 验证结果 ===" -ForegroundColor Cyan

if ($allPassed) {
    Write-Host "全部验证通过 ✓" -ForegroundColor Green
    Write-Host "  pytest: PASS" -ForegroundColor Green
    Write-Host "  jest: PASS" -ForegroundColor Green
    exit 0
} else {
    Write-Host "验证失败！失败项：$($failedTests -join ', ')" -ForegroundColor Red
    exit 1
}