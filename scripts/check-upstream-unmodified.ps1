<#
.SYNOPSIS
    ADR-0001 门禁检查：上游仓库未修改校验（Windows PowerShell 版）

.DESCRIPTION
    校验工作区中是否有 sz-orm 核心包的文件被修改但未提交。
    如果检测到未提交的修改，输出详细差异并返回非零退出码。

    ADR-0001：严禁修改上游 sz-rust / sz-orm 仓库的任何文件。
    此脚本在 sz-orm 自身仓库中运行时，确保核心包的变更
    已正确提交并附带相应的测试/文档更新。

.EXAMPLE
    ./scripts/check-upstream-unmodified.ps1
    ./scripts/check-upstream-unmodified.ps1 -WarnOnly  # 仅警告不阻断
#>

[CmdletBinding()]
param(
    [switch]$WarnOnly
)

$ErrorActionPreference = "Stop"

# 切换到项目根
$ScriptDir = $PSScriptRoot
if (-not $ScriptDir) {
    $ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
}
$ProjectRoot = Split-Path -Parent $ScriptDir
if (-not $ProjectRoot) {
    $ProjectRoot = (Get-Location).Path
}
Set-Location $ProjectRoot

# sz-orm 核心包列表（修改这些包需要额外的测试/文档审查）
$CorePackages = @(
    "sz-orm-core",
    "sz-orm-auth",
    "sz-orm-config",
    "sz-orm-macros",
    "sz-orm-dtx",
    "sz-orm-query-builder"
)

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "[11] ADR-0001：上游仓库未修改检查" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# 获取所有未提交的修改
$modified = git diff --name-only HEAD 2>$null
$untracked = git ls-files --others --exclude-standard 2>$null
$allChanges = @($modified) + @($untracked)

if (-not $allChanges -or $allChanges.Count -eq 0) {
    Write-Host "[OK] 无未提交的修改" -ForegroundColor Green
    exit 0
}

# 检查是否有核心包的修改
$violations = @()
foreach ($file in $allChanges) {
    foreach ($pkg in $CorePackages) {
        if ($file -like "packages/$pkg/*") {
            $violations += $file
            break
        }
    }
}

if ($violations.Count -eq 0) {
    Write-Host "[OK] ADR-0001 通过：核心包无未提交修改" -ForegroundColor Green
    exit 0
}

# 发现核心包修改，输出详细信息
Write-Host ""
Write-Host "[INFO] 检测到核心包有未提交修改：" -ForegroundColor Yellow
$violations | ForEach-Object { Write-Host "  $_" }

Write-Host ""
Write-Host "根据 ADR-0001，修改核心包必须满足以下条件：" -ForegroundColor Yellow
Write-Host "  1. 所有变更必须通过 10 道门禁检查" -ForegroundColor Yellow
Write-Host "  2. API 签名变更必须同步更新所有调用方和测试" -ForegroundColor Yellow
Write-Host "  3. 文档（AGENTS.md / engineering-practices.md）必须与代码一致" -ForegroundColor Yellow
Write-Host "  4. 必须有对应的测试覆盖（新增/修改的功能）" -ForegroundColor Yellow

if ($WarnOnly) {
    Write-Host ""
    Write-Host "[WARN] -WarnOnly 模式：仅警告，不阻断" -ForegroundColor Yellow
    exit 0
}

# 运行文档一致性检查
Write-Host ""
Write-Host "正在运行文档一致性检查..." -ForegroundColor Cyan
$docCheck = python "$ScriptDir/check-doc-consistency.py" 2>&1
$docExit = $LASTEXITCODE

if ($docExit -ne 0) {
    Write-Host ""
    Write-Host "[FAIL] ADR-0001 检查未通过：文档与代码不一致" -ForegroundColor Red
    Write-Host "请运行 'python scripts/check-doc-consistency.py --fix' 自动修复" -ForegroundColor Red
    exit 11
}

Write-Host ""
Write-Host "[OK] ADR-0001 通过：核心包修改已附带文档更新" -ForegroundColor Green
exit 0
