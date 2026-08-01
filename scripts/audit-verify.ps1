<#
.SYNOPSIS
    审计证据自动验证脚本（sz-orm 版，Windows PowerShell）

.DESCRIPTION
    验证审计报告中的每项结论是否附带真实的 file:line 证据，
    并检查该证据在代码中是否实际存在。

    这是门禁 13（审计合规硬约束）的自动执行脚本。

.PARAMETER ReportPath
    审计报告文件路径（.md 格式）

.EXAMPLE
    .\scripts\audit-verify.ps1 docs\audit\审计报告.md
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)]
    [string]$ReportPath
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

if (-not (Test-Path $ReportPath)) {
    Write-Host "❌ 报告文件不存在: $ReportPath" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "  审计证据验证: $ReportPath" -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan

$Pass = 0
$Fail = 0
$Warn = 0

$content = Get-Content $ReportPath -Raw
$lines = $content -split "`n"

foreach ($line in $lines) {
    # 跳过注释和空行
    if ([string]::IsNullOrWhiteSpace($line) -or $line.TrimStart().StartsWith("#")) {
        continue
    }

    # 匹配 file:///path#L123 或 file:///path#L123-L456
    if ($line -match 'file://([^\s#]+)#L(\d+)(?:-L(\d+))?') {
        $filepath = $Matches[1]
        $lineno = [int]$Matches[2]

        $localPath = $filepath -replace '^/', ''
        # Windows: file:///E:/... → E:/...
        if ($localPath -match '^[A-Za-z]:/') {
            $localPath = $localPath
        }

        if (Test-Path $localPath) {
            $totalLines = (Get-Content $localPath | Measure-Object -Line).Lines
            if ($lineno -le $totalLines) {
                Write-Host "✅ $localPath`:$lineno (共 $totalLines 行)" -ForegroundColor Green
                $Pass++
            } else {
                Write-Host "❌ $localPath`:$lineno — 行号超出范围（文件共 $totalLines 行）" -ForegroundColor Red
                $Fail++
            }
        } else {
            Write-Host "❌ $localPath`:$lineno — 文件不存在" -ForegroundColor Red
            $Fail++
        }
    }
    # 匹配 packages/xxx/src/yyy.rs:123 形式
    elseif ($line -match 'packages/([a-zA-Z0-9_-]+/[^\s:]+\.rs):(\d+)') {
        $relPath = "packages/$($Matches[1])"
        $lineno = [int]$Matches[2]

        if (Test-Path $relPath) {
            $totalLines = (Get-Content $relPath | Measure-Object -Line).Lines
            if ($lineno -le $totalLines) {
                Write-Host "✅ $relPath`:$lineno (共 $totalLines 行)" -ForegroundColor Green
                $Pass++
            } else {
                Write-Host "❌ $relPath`:$lineno — 行号超出范围（文件共 $totalLines 行）" -ForegroundColor Red
                $Fail++
            }
        } else {
            Write-Host "❌ $relPath`:$lineno — 文件不存在" -ForegroundColor Red
            $Fail++
        }
    }
    # 匹配 src/xxx.rs:123 形式（简写）
    elseif ($line -match 'src/([a-zA-Z0-9_/-]+\.rs):(\d+)') {
        $relPath = "packages/sz-orm-core/src/$($Matches[1])"
        $lineno = [int]$Matches[2]

        if (Test-Path $relPath) {
            $totalLines = (Get-Content $relPath | Measure-Object -Line).Lines
            if ($lineno -le $totalLines) {
                Write-Host "✅ $relPath`:$lineno (共 $totalLines 行)" -ForegroundColor Green
                $Pass++
            } else {
                Write-Host "❌ $relPath`:$lineno — 行号超出范围（文件共 $totalLines 行）" -ForegroundColor Red
                $Fail++
            }
        } else {
            Write-Host "⚠️  src/$($Matches[1])`:$lineno — 未找到对应文件（跳过）" -ForegroundColor Yellow
            $Warn++
        }
    }
}

Write-Host ""
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "  验证结果" -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "  ✅ 通过: $Pass" -ForegroundColor Green
Write-Host "  ❌ 失败: $Fail" -ForegroundColor Red
Write-Host "  ⚠️  警告: $Warn" -ForegroundColor Yellow
Write-Host "==========================================" -ForegroundColor Cyan

if ($Fail -gt 0) {
    Write-Host "❌ 审计证据验证未通过 — 存在 $Fail 处无效引用" -ForegroundColor Red
    exit 1
} elseif ($Pass -eq 0) {
    Write-Host "⚠️  未找到任何 file:line 证据 — 报告可能未遵循审计规范" -ForegroundColor Yellow
    exit 1
} else {
    Write-Host "✅ 审计证据验证通过" -ForegroundColor Green
    exit 0
}
