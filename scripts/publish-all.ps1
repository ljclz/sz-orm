$env:PATH = "C:\Users\Administrator\.cargo\bin;" + $env:PATH
if (-not $env:CARGO_REGISTRY_TOKEN) { Write-Host "请设置 CARGO_REGISTRY_TOKEN"; exit 1 }
$env:RUST_MIN_STACK = "134217728"
$env:CARGO_INCREMENTAL = "0"

$published = @("sz-orm-adaptive", "sz-orm-masking", "sz-orm-anomaly", "sz-orm-audit", "sz-orm-config", "sz-orm-crypto", "sz-orm-graph")

function Publish-Package {
    param([string]$pkg)
    if ($published -contains $pkg) { return $true }
    Write-Host "publishing: $pkg"
    $output = cargo publish -p $pkg 2>&1 | Out-String
    if ($output -match "Published") {
        Write-Host "PUBLISHED: $pkg"
        $published += $pkg
        return $true
    }
    if ($output -match "already exists") {
        Write-Host "EXISTS: $pkg"
        $published += $pkg
        return $true
    }
    $m = [regex]::Match($output, 'requirement `(sz-orm-[a-z0-9-]+) = ')
    if ($m.Success) {
        $dep = $m.Groups[1].Value
        Write-Host "$pkg needs $dep first"
        if (Publish-Package $dep) {
            return Publish-Package $pkg
        }
    }
    Write-Host "FAILED: $pkg"
    return $false
}

$targets = @(
    "sz-orm-macros", "sz-orm-query-builder", "sz-orm-sql-validator",
    "sz-orm-graphql", "sz-orm-core", "sz-orm-sqlx",
    "sz-orm-mig", "sz-orm-back", "sz-orm-websocket", "sz-orm-mqtt",
    "sz-orm-storage", "sz-orm-queue", "sz-orm-auth", "sz-orm-scheduler",
    "sz-orm-tracing", "sz-orm-es", "sz-orm-limit", "sz-orm-grpc",
    "sz-orm-dtx", "sz-orm-rw", "sz-orm-sharding", "sz-orm-logger",
    "sz-orm-swagger", "sz-orm-health", "sz-orm-batch", "sz-orm-wasm",
    "sz-orm-lc", "sz-orm-observability", "sz-orm-postgis", "sz-orm-timeseries",
    "sz-orm-search", "sz-orm-vector", "sz-orm-oracle", "sz-orm-mssql",
    "sz-orm-axum", "sz-orm-actix", "sz-orm-explain",
    "sz-orm-flamegraph", "sz-orm-fusion", "sz-orm-n1-lint",
    "sz-orm-cabi", "sz-orm-go", "sz-orm-java", "sz-orm-cpp",
    "sz-orm-designer", "sz-orm-advisor", "sz-orm-diagnosis",
    "sz-orm-parallel", "sz-orm-stream", "sz-orm-ai",
    "sz-orm-ai-designer", "sz-orm-ai-migration", "sz-orm-studio", "sz-orm-lsp",
    "sz-orm-agent", "sz-orm-governance", "sz-orm-nl-query", "sz-orm-model-ops",
    "sz-orm-multimodal", "sz-orm-mcp"
)

foreach ($t in $targets) {
    Publish-Package $t | Out-Null
}

Write-Host ""
Write-Host "Published $($published.Count) packages:"
$published | ForEach-Object { Write-Host "  $_" }
