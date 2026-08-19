$env:PATH = "C:\Users\Administrator\.cargo\bin;" + $env:PATH
$env:RUST_MIN_STACK = "134217728"
$env:CARGO_INCREMENTAL = "0"

$topoFile = "E:\vue\test\鲜视达\rust\sz-orm\docs\spec\cratesio_publish_all\topo-order.txt"
$logFile = "E:\vue\test\鲜视达\rust\sz-orm\docs\spec\cratesio_publish_all\publish-log.txt"

$packages = Get-Content $topoFile | Where-Object { $_.Trim() -ne "" } | ForEach-Object { $_.Trim() }

$startIdx = if ($args[0]) { [int]$args[0] } else { 1 }
$endIdx = if ($args[1]) { [int]$args[1] } else { $packages.Count }

Write-Host "发布范围: $startIdx 到 $endIdx"

$success = @()
$skipped = @()
$failed = @()

for ($i = $startIdx - 1; $i -lt $endIdx -and $i -lt $packages.Count; $i++) {
    $pkg = $packages[$i]
    $num = $i + 1
    $ts = Get-Date -Format "HH:mm:ss"
    Write-Host "[$ts] ($num) $pkg ..." -NoNewline

    $result = cargo publish --allow-dirty -p $pkg 2>&1
    $resultStr = $result | Out-String

    if ($LASTEXITCODE -eq 0) {
        Write-Host " OK"
        $success += $pkg
        Add-Content -Path $logFile -Value "[$ts] OK: $pkg"
    } elseif ($resultStr -match "already exists|already uploaded") {
        Write-Host " SKIP"
        $skipped += $pkg
        Add-Content -Path $logFile -Value "[$ts] SKIP: $pkg"
    } else {
        $errTail = (($resultStr -split "`n") | Where-Object { $_.Trim() -ne "" } | Select-Object -Last 8) -join " | "
        Write-Host " FAIL"
        Write-Host "  $errTail"
        $failed += "${pkg} | ${errTail}"
        Add-Content -Path $logFile -Value "[$ts] FAIL: $pkg - $errTail"
    }
}

Write-Host ""
Write-Host "=== 汇总 (范围 $startIdx-$endIdx) ==="
Write-Host "OK($($success.Count)): $($success -join ', ')"
Write-Host "SKIP($($skipped.Count)): $($skipped -join ', ')"
Write-Host "FAIL($($failed.Count)): $($failed -join '; ')"