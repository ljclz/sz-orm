$env:PATH = "C:\Users\Administrator\.cargo\bin;" + $env:PATH
$env:RUST_MIN_STACK = "134217728"
$env:CARGO_INCREMENTAL = "0"

$logFile = "E:\vue\test\鲜视达\rust\sz-orm\docs\spec\cratesio_publish_all\publish-log.txt"

# 重试包列表（按依赖顺序）
$retryPackages = @(
    "sz-orm-health",
    "sz-orm-observability",
    "sz-orm-wasm",
    "sz-orm-fusion",
    "sz-orm-cabi",
    "sz-orm-cpp",
    "sz-orm-go",
    "sz-orm-java"
)

$success = @()
$skipped = @()
$failed = @()

foreach ($pkg in $retryPackages) {
    $ts = Get-Date -Format "HH:mm:ss"
    Write-Host "[$ts] 重试 $pkg ..." -NoNewline

    $result = cargo publish --allow-dirty -p $pkg 2>&1
    $resultStr = $result | Out-String

    if ($LASTEXITCODE -eq 0) {
        Write-Host " OK"
        $success += $pkg
        Add-Content -Path $logFile -Value "[$ts] OK(retry): $pkg"
    } elseif ($resultStr -match "already exists|already uploaded") {
        Write-Host " SKIP"
        $skipped += $pkg
        Add-Content -Path $logFile -Value "[$ts] SKIP(retry): $pkg"
    } else {
        $errTail = (($resultStr -split "`n") | Where-Object { $_.Trim() -ne "" } | Select-Object -Last 6) -join " | "
        Write-Host " FAIL"
        Write-Host "  $errTail"
        $failed += "${pkg} | ${errTail}"
        Add-Content -Path $logFile -Value "[$ts] FAIL(retry): ${pkg} - ${errTail}"
    }

    # 每个包之间等待 10 秒，避免速率限制
    Start-Sleep -Seconds 10
}

Write-Host ""
Write-Host "=== 重试汇总 ==="
Write-Host "OK($($success.Count)): $($success -join ', ')"
Write-Host "SKIP($($skipped.Count)): $($skipped -join ', ')"
Write-Host "FAIL($($failed.Count)): $($failed -join '; ')"