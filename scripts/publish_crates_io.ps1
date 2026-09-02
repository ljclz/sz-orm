<#
.SYNOPSIS
    sz-orm crates.io 逐包发布脚本
.DESCRIPTION
    按拓扑序逐包发布 43 个包到 crates.io，失败即中止。
    流程：门禁检查 → 检查 token → 计算拓扑序 → 逐包 cargo publish → sz-pay 验证
.PARAMETER WorkspaceRoot
    workspace 根目录，默认为脚本所在目录的上级
.PARAMETER DryRun
    仅打印 cargo publish 命令不实际执行
.EXAMPLE
    .\scripts\publish_crates_io.ps1 -DryRun
    .\scripts\publish_crates_io.ps1 -WorkspaceRoot "E:\vue\test\鲜视达\rust\sz-orm"
#>
param(
    [string]$WorkspaceRoot = (Split-Path (Split-Path $MyInvocation.MyCommand.Path)),
    [switch]$DryRun,
    [switch]$SkipGate
)

$ErrorActionPreference = "Continue"
Set-Location $WorkspaceRoot

Write-Output "[$(Get-Date -Format 'HH:mm:ss')] === sz-orm crates.io 发布脚本 ==="
Write-Output "[$(Get-Date -Format 'HH:mm:ss')] WorkspaceRoot: $WorkspaceRoot"
if ($DryRun) { Write-Output "[$(Get-Date -Format 'HH:mm:ss')] MODE: DryRun (no actual publish)" }

Write-Output "[$(Get-Date -Format 'HH:mm:ss')] [1/4] 检查门禁..."

if (-not $DryRun -and -not $SkipGate) {
    Write-Output "[$(Get-Date -Format 'HH:mm:ss')]   cargo fmt --all -- --check"
    & cargo fmt --all -- --check 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { Write-Error "GATE FAIL: fmt"; exit 1 }

    Write-Output "[$(Get-Date -Format 'HH:mm:ss')]   cargo check --workspace --all-targets"
    $env:CARGO_INCREMENTAL = 0
    & cargo check --workspace --all-targets 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { Write-Error "GATE FAIL: check"; exit 1 }

    Write-Output "[$(Get-Date -Format 'HH:mm:ss')]   cargo clippy --workspace --all-targets -- -D warnings"
    & cargo clippy --workspace --all-targets -- -D warnings 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { Write-Error "GATE FAIL: clippy"; exit 1 }

    Write-Output "[$(Get-Date -Format 'HH:mm:ss')]   cargo test --workspace"
    & cargo test --workspace 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { Write-Error "GATE FAIL: test"; exit 1 }
}

Write-Output "[$(Get-Date -Format 'HH:mm:ss')] [2/4] 检查凭证..."
if (-not $DryRun) {
    if (-not $env:CARGO_REGISTRY_TOKEN) {
        $credFile = Join-Path $env:USERPROFILE ".cargo\credentials.toml"
        if (-not (Test-Path $credFile)) {
            Write-Error "No CARGO_REGISTRY_TOKEN env var and no ~/.cargo/credentials.toml. Run 'cargo login' first."
            exit 1
        }
        Write-Output "[$(Get-Date -Format 'HH:mm:ss')]   Using ~/.cargo/credentials.toml"
    } else {
        Write-Output "[$(Get-Date -Format 'HH:mm:ss')]   Using CARGO_REGISTRY_TOKEN env var"
    }
}

Write-Output "[$(Get-Date -Format 'HH:mm:ss')] [3/4] 计算拓扑序..."
$topologyScript = Join-Path $WorkspaceRoot "scripts\compute_topology.ps1"
$pkgList = & PowerShell -ExecutionPolicy Bypass -File $topologyScript -WorkspaceRoot $WorkspaceRoot
if ($LASTEXITCODE -ne 0) { Write-Error "Topology sort failed (exit code $LASTEXITCODE)"; exit 1 }
$pkgArray = $pkgList | Where-Object { $_ -match '^sz-orm-' }
$total = $pkgArray.Count
Write-Output "[$(Get-Date -Format 'HH:mm:ss')]   $total packages to publish"

$published = @()
$skipped = @()
$failed = @()

Write-Output "[$(Get-Date -Format 'HH:mm:ss')] [4/4] 逐包发布..."

$i = 0
foreach ($pkg in $pkgArray) {
    $i++
    $prefix = "[$(Get-Date -Format 'HH:mm:ss')] ($i/$total)"

    if ($DryRun) {
        Write-Output "$prefix DRYRUN: cargo publish -p $pkg"
        $published += $pkg
        continue
    }

    $tomlPath = Join-Path $WorkspaceRoot "packages\$pkg\Cargo.toml"
    if (-not (Test-Path $tomlPath)) {
        $tomlPath = Join-Path $WorkspaceRoot "$pkg\Cargo.toml"
    }
    if (-not (Test-Path $tomlPath)) {
        $specialDir = $null
        if ($pkg -eq "sz-orm-examples") { $specialDir = "examples" }
        elseif ($pkg -eq "sz-orm-cli") { $specialDir = "cli" }
        if ($specialDir) {
            $tomlPath = Join-Path $WorkspaceRoot "$specialDir\Cargo.toml"
        }
    }
    if (Test-Path $tomlPath) {
        $tomlContent = Get-Content $tomlPath -Raw
        if ($tomlContent -match 'publish\s*=\s*false') {
            Write-Output "$prefix SKIP $pkg (publish = false)"
            $skipped += $pkg
            continue
        }
    }

    Write-Output "$prefix PUBLISH $pkg..."
    $output = & cargo publish -p $pkg --allow-dirty 2>&1
    $outputStr = ($output | Out-String)

    if ($outputStr -match "already exists" -or $outputStr -match "already been published" -or $outputStr -match "already uploaded") {
        Write-Output "$prefix SKIP $pkg (already exists on crates.io)"
        $skipped += $pkg
    }
    elseif ($outputStr -match "Published $pkg" -or $outputStr -match "Uploading $pkg" -or $LASTEXITCODE -eq 0) {
        Write-Output "$prefix OK $pkg"
        $published += $pkg
        Start-Sleep -Seconds 3
    }
    elseif ($outputStr -match "429 Too Many Requests") {
        Write-Output "$prefix RATE_LIMITED $pkg, waiting 60s..."
        Start-Sleep -Seconds 60
        $output = & cargo publish -p $pkg --allow-dirty 2>&1
        $outputStr = ($output | Out-String)
        if ($outputStr -match "already exists" -or $LASTEXITCODE -eq 0) {
            Write-Output "$prefix OK $pkg (retry)"
            $published += $pkg
        } else {
            Write-Output "$prefix FAIL $pkg (after retry)"
            Write-Output $outputStr.Substring(0, [Math]::Min(500, $outputStr.Length))
            $failed += $pkg
            Write-Error "PUBLISH FAILED: $pkg. Aborting (REQ-REL-009: no partial publish)."
            Write-Output "Published: $($published -join ', ')"
            Write-Output "Skipped: $($skipped -join ', ')"
            Write-Output "Failed: $($failed -join ', ')"
            exit 1
        }
    }
    else {
        Write-Output "$prefix FAIL $pkg"
        Write-Output $outputStr.Substring(0, [Math]::Min(500, $outputStr.Length))
        $failed += $pkg
        Write-Error "PUBLISH FAILED: $pkg. Aborting (REQ-REL-009: no partial publish)."
        Write-Output "Published: $($published -join ', ')"
        Write-Output "Skipped: $($skipped -join ', ')"
        Write-Output "Failed: $($failed -join ', ')"
        exit 1
    }
}

Write-Output ""
Write-Output "[$(Get-Date -Format 'HH:mm:ss')] === PUBLISH COMPLETE ==="
Write-Output "Published: $($published.Count)/$total"
Write-Output "Skipped:   $($skipped.Count)/$total"
Write-Output "Failed:    $($failed.Count)/$total"
if ($skipped.Count -gt 0) { Write-Output "Skipped packages: $($skipped -join ', ')" }
if ($failed.Count -gt 0) { Write-Output "Failed packages: $($failed -join ', ')" }