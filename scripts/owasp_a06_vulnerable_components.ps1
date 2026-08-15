﻿#!/usr/bin/env pwsh
<#
.SYNOPSIS
OWASP A06: 易受攻击和过时组件深化渗透测试（PowerShell）

.DESCRIPTION
对应 REQ-V49-006（OWASP A06 深化）
运行 cargo audit / cargo deny check 验证依赖安全性。
#>

param(
    [switch]$SkipSbom
)

$ErrorActionPreference = "Stop"
$exitCode = 0

function Invoke-CveAudit {
    Write-Host "[A06-1] CVE 审计 (cargo audit)..." -ForegroundColor Cyan
    $result = & cargo audit 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  FAIL: cargo audit 发现未忽略的 RUSTSEC 公告" -ForegroundColor Red
        Write-Host $result
        $script:exitCode = 1
    } else {
        Write-Host "  PASS: 无未忽略公告" -ForegroundColor Green
    }
}

function Invoke-LicenseCheck {
    Write-Host "[A06-2] 许可证检查 (cargo deny check licenses)..." -ForegroundColor Cyan
    $result = & cargo deny check licenses 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  FAIL: 发现 copyleft 或不在白名单的许可证" -ForegroundColor Red
        Write-Host $result
        $script:exitCode = 1
    } else {
        Write-Host "  PASS: 全部许可证在白名单" -ForegroundColor Green
    }
}

function Invoke-YankedCheck {
    Write-Host "[A06-3] Yanked 检,检查 (cargo deny check)..." -ForegroundColor Cyan
    $result = & cargo deny check 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  WARN: 发现 yanked 依赖或其它问题" -ForegroundColor Yellow
        Write-Host $result
    } else {
        Write-Host "  PASS: 无 yanked 依赖" -ForegroundColor Green
    }
}

function Invoke-DuplicateCheck {
    Write-Host "[A06-4] 重复依赖检查 (cargo deny check bans)..." -ForegroundColor Cyan
    $result = & cargo deny check bans 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  WARN: 发现重复依赖（版本碎片化）" -ForegroundColor Yellow
        Write-Host $result
    } else {
        Write-Host "  PASS: 无重复依赖" -ForegroundColor Green
    }
}

function Invoke-SourceCheck {
    Write-Host "[A06-5] 依赖来源检查 (cargo deny check sources)..." -ForegroundColor Cyan
    $result = & cargo deny check sources 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  FAIL: 发现非 crates.io 来源" -ForegroundColor Red
        Write-Host $result
        $script:exitCode = 1
    } else {
        Write-Host "  PASS: 全部依赖来自 crates.io" -ForegroundColor Green
    }
}

function Invoke-SbomGeneration {
    if ($SkipSbom) {
        Write-Host "[A06-6] SBOM 生成跳过（--SkipSbom）" -ForegroundColor Yellow
        return
    }
    Write-Host "[A06-6] SBOM 生成 (cargo cyclonedx)..." -ForegroundColor Cyan
    $cyclonedx = Get-Command cargo-cyclonedx -ErrorAction SilentlyContinue
    if (-not $cyclonedx) {
        Write-Host "  SKIP: cargo cyclonedx 未安装" -ForegroundColor Yellow
        return
    }
    $result = & cargo cyclonedx 2>&1
    if (Test-Path "sbom.json") {
        Write-Host "  PASS: sbom.json 已生成" -ForegroundColor Green
    } else {
        Write-Host "  WARN: sbom.json 未生成" -ForegroundColor Yellow
    }
}

Write-Host "=== OWASP A06: 易受攻击和过时组件深化渗透测试 ===" -ForegroundColor White
Write-Host ""

Invoke-CveAudit
Invoke-LicenseCheck
Invoke-YankedCheck
Invoke-DuplicateCheck
Invoke-SourceCheck
Invoke-SbomGeneration

Write-Host ""
if ($exitCode -eq 0) {
    Write-Host "=== A06 审计完成: 全部通过 ===" -ForegroundColor Green
} else {
    Write-Host "=== A06 审计完成: 存在失败项 ===" -ForegroundColor Red
}
exit $exitCode