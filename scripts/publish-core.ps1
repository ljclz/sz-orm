$env:PATH = "C:\Users\Administrator\.cargo\bin;" + $env:PATH
if (-not $env:CARGO_REGISTRY_TOKEN) { Write-Host "请设置 CARGO_REGISTRY_TOKEN"; exit 1 }
$env:RUST_MIN_STACK = "134217728"
$env:CARGO_INCREMENTAL = "0"

$target = "sz-orm-core"
$published = @("sz-orm-adaptive", "sz-orm-masking", "sz-orm-anomaly")
$maxIter = 100

for ($i = 0; $i -lt $maxIter; $i++) {
    Write-Host "--- iteration $i ---"
    $output = cargo publish -p $target --dry-run 2>&1 | Out-String

    if ($output -match "aborting upload due to dry run") {
        Write-Host "$target dry-run ok, publishing"
        $pubOut = cargo publish -p $target 2>&1 | Out-String
        if ($pubOut -match "Published") {
            Write-Host "PUBLISHED: $target"
            $published += $target
        }
        break
    }

    $m = [regex]::Match($output, 'requirement `(sz-orm-[a-z0-9-]+) = ')
    if ($m.Success) {
        $dep = $m.Groups[1].Value
        if ($published -contains $dep) {
            Write-Host "$dep already published, waiting 60s for index"
            Start-Sleep -Seconds 60
            continue
        }
        Write-Host "need to publish: $dep"
        $depOut = cargo publish -p $dep 2>&1 | Out-String
        if ($depOut -match "Published") {
            Write-Host "PUBLISHED: $dep"
            $published += $dep
        } elseif ($depOut -match "already exists") {
            Write-Host "EXISTS: $dep"
            $published += $dep
        } else {
            Write-Host "FAILED: $dep"
            Write-Host $depOut
            break
        }
    } else {
        Write-Host "unknown state, stopping"
        break
    }
}

Write-Host ""
Write-Host "Published $($published.Count) packages:"
$published | ForEach-Object { Write-Host "  $_" }
