#!/usr/bin/env pwsh
# M3.5: PyPI 发布脚本
# 使用 maturin publish 发布 wheel 到 PyPI，发布前校验 pytest
#
# 用法：
#   .\scripts\publish_pypi.ps1 -DryRun   # 干跑（构建 + 测试，不发布）
#   .\scripts\publish_pypi.ps1           # 正式发布（需 PYPI_TOKEN 环境变量）
#
# 验收标准：
#   - 发布前校验 pytest 全部通过（未验证阻断发布）
#   - PyPI token 通过环境变量 PYPI_TOKEN 传入，不硬编码
#   - -DryRun 干跑成功

param(
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

$repoRoot = "$PSScriptRoot\.."
$pythonPkg = "$repoRoot\packages\sz-orm-python"

Write-Host "=== PyPI 发布脚本 ===" -ForegroundColor Cyan

# ── 步骤 1：校验 pytest 全部通过 ──
Write-Host "`n[1/4] 校验 pytest..." -ForegroundColor Yellow

if (-not (Get-Command pytest -ErrorAction SilentlyContinue)) {
    Write-Host "安装 pytest..." -ForegroundColor Yellow
    pip install pytest pytest-asyncio
}

$pytestStdout = "$repoRoot\target\pytest_stdout.txt"
$pytestStderr = "$repoRoot\target\pytest_stderr.txt"
New-Item -ItemType Directory -Force -Path "$repoRoot\target" | Out-Null

& pytest "$pythonPkg\tests\" -v --tb=short 1>$pytestStdout 2>$pytestStderr
$pytestExitCode = $LASTEXITCODE

if ($pytestExitCode -ne 0) {
    Write-Host "`npytest 校验失败，阻断发布！" -ForegroundColor Red
    if (Test-Path $pytestStderr) {
        Get-Content $pytestStderr | Select-Object -First 30 | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    }
    exit 1
}
Write-Host "pytest 全部通过 ✓" -ForegroundColor Green

# ── 步骤 2：构建 wheel ──
Write-Host "`n[2/4] 构建 wheel..." -ForegroundColor Yellow
& "$repoRoot\scripts\build_python_wheel.ps1"
if ($LASTEXITCODE -ne 0) {
    Write-Host "Wheel 构建失败，阻断发布！" -ForegroundColor Red
    exit 1
}

$wheelDir = "$repoRoot\target\wheels"
$wheels = Get-ChildItem "$wheelDir\*.whl" -ErrorAction SilentlyContinue
if (-not $wheels) {
    Write-Host "未找到 wheel 文件，阻断发布！" -ForegroundColor Red
    exit 1
}
Write-Host "Wheel 文件：" -ForegroundColor Green
$wheels | ForEach-Object { Write-Host "  $($_.Name)" }

# ── 步骤 3：DryRun 模式退出 ──
if ($DryRun) {
    Write-Host "`n[3/4] DryRun 模式 — 跳过发布" -ForegroundColor Yellow
    Write-Host "`n=== 干跑成功 ===" -ForegroundColor Green
    Write-Host "pytest: PASS | wheel: $($wheels.Count) 个" -ForegroundColor Green
    exit 0
}

# ── 步骤 3（正式）：校验 PYPI_TOKEN ──
Write-Host "`n[3/4] 校验 PYPI_TOKEN..." -ForegroundColor Yellow
if (-not $env:PYPI_TOKEN) {
    Write-Host "环境变量 PYPI_TOKEN 未设置，阻断发布！" -ForegroundColor Red
    Write-Host "请设置：`$env:PYPI_TOKEN = 'pypi-xxxxxxxx'" -ForegroundColor Yellow
    exit 1
}
Write-Host "PYPI_TOKEN 已设置 ✓" -ForegroundColor Green

# ── 步骤 4：发布到 PyPI ──
Write-Host "`n[4/4] 发布到 PyPI..." -ForegroundColor Yellow

Set-Location $pythonPkg

if (-not (Get-Command maturin -ErrorAction SilentlyContinue)) {
    pip install maturin
}

$env:MATURIN_PYPI_TOKEN = $env:PYPI_TOKEN
maturin publish --release --out $wheelDir

if ($LASTEXITCODE -ne 0) {
    Write-Host "`nPyPI 发布失败！" -ForegroundColor Red
    exit 1
}

Write-Host "`n=== 发布成功 ===" -ForegroundColor Green
Write-Host "已发布 $($wheels.Count) 个 wheel 到 PyPI" -ForegroundColor Green
$wheels | ForEach-Object { Write-Host "  $($_.Name)" }