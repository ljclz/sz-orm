<#
.SYNOPSIS
    sz-pay 下游验证脚本
.DESCRIPTION
    升级 sz-pay 的 sz-orm-* 依赖版本，验证构建和回归测试。
    流程：升级版本号 → cargo build → cargo test → 报告结果
.PARAMETER SzPayRoot
    sz-pay 项目根目录
.PARAMETER TargetVersion
    目标 sz-orm 版本号，默认 2.3.0
.PARAMETER RemovePatch
    移除 [patch.crates-io] 段（需 crates.io 已发布）
.EXAMPLE
    .\scripts\verify_sz_pay.ps1
    .\scripts\verify_sz_pay.ps1 -RemovePatch
#>
param(
    [string]$SzPayRoot = "E:\vue\test\sz-pay\server\sz-rust",
    [string]$TargetVersion = "2.3.0",
    [switch]$RemovePatch
)

$ErrorActionPreference = "Continue"

$cargoToml = Join-Path $SzPayRoot "Cargo.toml"
if (-not (Test-Path $cargoToml)) { Write-Error "sz-pay Cargo.toml not found at $cargoToml"; exit 1 }

Write-Output "[$(Get-Date -Format 'HH:mm:ss')] === sz-pay 下游验证 ==="
Write-Output "[$(Get-Date -Format 'HH:mm:ss')] SzPayRoot: $SzPayRoot"
Write-Output "[$(Get-Date -Format 'HH:mm:ss')] TargetVersion: $TargetVersion"

Write-Output "[$(Get-Date -Format 'HH:mm:ss')] [1/4] 升级依赖版本号..."
$content = Get-Content $cargoToml -Raw -Encoding UTF8
$pkgs = @('sz-orm-core', 'sz-orm-sqlx', 'sz-orm-config', 'sz-orm-auth', 'sz-orm-macros', 'sz-orm-queue', 'sz-orm-scheduler')
foreach ($pkg in $pkgs) {
    $oldPattern = "${pkg} = `"2.1.0`""
    $newPattern = "${pkg} = `"${TargetVersion}`""
    if ($content -match [regex]::Escape($oldPattern)) {
        $content = $content -replace [regex]::Escape($oldPattern), $newPattern
        Write-Output "  ${pkg}: 2.1.0 -> $TargetVersion"
    } elseif ($content -match [regex]::Escape($newPattern)) {
        Write-Output "  ${pkg}: already $TargetVersion"
    } else {
        Write-Output "  ${pkg}: WARNING - pattern not found"
    }
}

if ($RemovePatch) {
    Write-Output "[$(Get-Date -Format 'HH:mm:ss')] [1.5] 移除 [patch.crates-io] 段..."
    $patchPattern = '(?ms)\n# v2\.3\.0 本地覆盖.*?\[patch\.crates-io\][\s\S]*?\z'
    $content = [regex]::Replace($content, $patchPattern, "`n")
    Write-Output "  [patch.crates-io] removed"
}

Set-Content -Path $cargoToml -Value $content -Encoding UTF8 -NoNewline

Write-Output "[$(Get-Date -Format 'HH:mm:ss')] [2/4] cargo build..."
$env:CARGO_INCREMENTAL = 0
$buildOutput = & cargo build --manifest-path $cargoToml 2>&1 | Out-String
if ($buildOutput -match '(?m)^error') {
    Write-Output "  BUILD FAIL"
    Write-Output $buildOutput.Substring(0, [Math]::Min(500, $buildOutput.Length))
    exit 1
} else {
    Write-Output "  BUILD OK"
}

Write-Output "[$(Get-Date -Format 'HH:mm:ss')] [3/4] cargo test --lib..."
$testOutput = & cargo test --lib --manifest-path $cargoToml 2>&1
$testStr = ($testOutput | Out-String)
$testResults = $testOutput | Select-String -Pattern 'test result:'
$passed = 0; $failed = 0; $ignored = 0
foreach ($r in $testResults) {
    if ($r.Line -match '(\d+) passed; (\d+) failed; (\d+) ignored') {
        $passed += [int]$matches[1]; $failed += [int]$matches[2]; $ignored += [int]$matches[3]
    }
}
Write-Output "  Results: $passed passed, $failed failed, $ignored ignored"

if ($failed -gt 0) {
    Write-Output "  TEST FAIL - regression detected!"
    exit 1
} else {
    Write-Output "  TEST OK - zero regression"
}

Write-Output "[$(Get-Date -Format 'HH:mm:ss')] [4/4] 验证完成"
Write-Output "[$(Get-Date -Format 'HH:mm:ss')] === sz-pay 验证通过 ==="
Write-Output "  Passed: $passed"
Write-Output "  Failed: $failed"
Write-Output "  Ignored: $ignored"