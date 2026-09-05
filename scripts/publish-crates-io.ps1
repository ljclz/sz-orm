$env:PATH = "C:\Users\Administrator\.cargo\bin;" + $env:PATH
if (-not $env:CARGO_REGISTRY_TOKEN) {
    Write-Host "错误：请先设置 CARGO_REGISTRY_TOKEN 环境变量"
    exit 1
}
$env:RUST_MIN_STACK = "134217728"
$env:CARGO_INCREMENTAL = "0"

$published = @()
$failed = @()
$maxRetries = 200

for ($i = 0; $i -lt $maxRetries; $i++) {
    $output = cargo publish --workspace --dry-run 2>&1 | Out-String
    if ($output -match "error: failed to prepare local package for uploading" -and $output -match "candidate versions found which didn't match") {
        $matches = [regex]::Matches($output, "requirement `sz-orm-[a-z0-9-]+ = ")
        if ($matches.Count -eq 0) {
            Write-Host "无法解析依赖包名，退出"
            Write-Host $output
            break
        }
        $dep = $matches[0].Value -replace "requirement `"", "" -replace " = `"", ""
        Write-Host "需要先发布: $dep"
        $pubOutput = cargo publish -p $dep 2>&1 | Out-String
        if ($pubOutput -match "Published") {
            Write-Host "已发布: $dep"
            $published += $dep
        } elseif ($pubOutput -match "already exists") {
            Write-Host "已存在(跳过): $dep"
            $published += $dep
        } else {
            Write-Host "发布失败: $dep"
            Write-Host $pubOutput
            $failed += $dep
            break
        }
    } elseif ($output -match "Uploading") {
        Write-Host "所有依赖已满足，执行 workspace 发布"
        break
    } else {
        Write-Host "未知状态，退出"
        Write-Host $output
        break
    }
}

Write-Host ""
Write-Host "已发布包: $($published.Count)"
$published | ForEach-Object { Write-Host "  $_" }
if ($failed.Count -gt 0) {
    Write-Host "失败包: $($failed.Count)"
    $failed | ForEach-Object { Write-Host "  $_" }
}