# SZ-ORM crates.io auto-publish script
# Publish 1 package at a time, wait for rate limit to clear between each
# Usage: powershell -ExecutionPolicy Bypass -File scripts\publish-all.ps1

$ErrorActionPreference = "Continue"
$workspace = "E:\vue\test\鲜视达\rust\sz-orm"
Set-Location $workspace

$packages = @(
    "sz-orm-sqlx",
    "sz-orm-vector",
    "sz-orm-auth",
    "sz-orm-batch",
    "sz-orm-postgis",
    "sz-orm-timeseries",
    "sz-orm-search",
    "sz-orm-queue",
    "sz-orm-mqtt",
    "sz-orm-websocket",
    "sz-orm-tracing",
    "sz-orm-logger",
    "sz-orm-health",
    "sz-orm-audit",
    "sz-orm-masking",
    "sz-orm-swagger",
    "sz-orm-limit",
    "sz-orm-scheduler",
    "sz-orm-config",
    "sz-orm-storage",
    "sz-orm-grpc",
    "sz-orm-graphql",
    "sz-orm-dtx",
    "sz-orm-rw",
    "sz-orm-sharding",
    "sz-orm-lc",
    "sz-orm-wasm",
    "sz-orm-mig",
    "sz-orm-back",
    "sz-orm-es"
)

# sz-orm-crypto already published (verified via "already exists")
# sz-orm-vector and sz-orm-sqlx still pending

$total = $packages.Count
$published = 0
$failed = @()
$alreadyExists = @()

function Get-RateLimitWaitSeconds {
    param([string]$outputStr)
    if ($outputStr -match "try again after (\w+), (\d+) (\w+) (\d+) (\d+):(\d+):(\d+) GMT") {
        $day = $matches[2]
        $monthStr = $matches[3]
        $year = $matches[4]
        $hour = $matches[5]
        $minute = $matches[6]
        $second = $matches[7]
        $monthMap = @{Jan=1;Feb=2;Mar=3;Apr=4;May=5;Jun=6;Jul=7;Aug=8;Sep=9;Oct=10;Nov=11;Dec=12}
        $month = $monthMap[$monthStr]
        $retryUtc = [DateTime]::new([int]$year, $month, [int]$day, [int]$hour, [int]$minute, [int]$second, [DateTimeKind]::Utc)
        $nowUtc = [DateTime]::UtcNow
        $diff = $retryUtc - $nowUtc
        if ($diff.TotalSeconds -gt 0) {
            return [int]$diff.TotalSeconds + 10
        }
        return 30
    }
    return 300
}

Write-Output "[$(Get-Date -Format 'HH:mm:ss')] Start publishing $total packages (1 at a time)"

foreach ($pkg in $packages) {
    $maxRetries = 5
    $retry = 0
    $success = $false

    while ($retry -lt $maxRetries -and -not $success) {
        $retry++
        Write-Output "[$(Get-Date -Format 'HH:mm:ss')] Publish $pkg (try $retry/$maxRetries)..."

        $output = & cargo publish -p $pkg 2>&1
        $outputStr = ($output | Out-String)

        if ($outputStr -match "Published $pkg v1.0.0" -or $outputStr -match "Uploaded $pkg v1.0.0") {
            $published++
            $success = $true
            Write-Output "[$(Get-Date -Format 'HH:mm:ss')] OK $pkg ($published/$total)"
            Start-Sleep -Seconds 5
        }
        elseif ($outputStr -match "already exists") {
            $alreadyExists += $pkg
            $published++
            $success = $true
            Write-Output "[$(Get-Date -Format 'HH:mm:ss')] SKIP $pkg exists ($published/$total)"
        }
        elseif ($outputStr -match "429 Too Many Requests") {
            $waitSec = Get-RateLimitWaitSeconds -outputStr $outputStr
            Write-Output "[$(Get-Date -Format 'HH:mm:ss')] RATE_LIMIT $pkg, wait $waitSec sec..."
            Start-Sleep -Seconds $waitSec
        }
        else {
            $len = [Math]::Min(800, $outputStr.Length)
            Write-Output "[$(Get-Date -Format 'HH:mm:ss')] FAIL ${pkg} ->"
            Write-Output $outputStr.Substring(0, $len)
            Start-Sleep -Seconds 30
        }
    }

    if (-not $success) {
        $failed += $pkg
        Write-Output "[$(Get-Date -Format 'HH:mm:ss')] FAILED $pkg after $maxRetries retries"
    }
}

Write-Output ""
Write-Output "[$(Get-Date -Format 'HH:mm:ss')] === DONE ==="
Write-Output "Published: $published/$total"
if ($alreadyExists.Count -gt 0) { Write-Output "Already existed: $($alreadyExists -join ', ')" }
if ($failed.Count -gt 0) { Write-Output "Failed: $($failed -join ', ')" }
