<#
.SYNOPSIS
    计算 sz-orm workspace 43 包的依赖拓扑排序
.DESCRIPTION
    解析 workspace 各包 Cargo.toml 的 sz-orm-* 依赖（path 与 version 依赖均计入），
    构建 DAG，用 Kahn 算法变体拓扑排序（入度相同按包名字典序打破并列，确保唯一可复现）。
    检测循环依赖并报错。
.OUTPUTS
    stdout 每行一个包名（被依赖的包在前）
.PARAMETER WorkspaceRoot
    workspace 根目录，默认为脚本所在目录的上级
.EXAMPLE
    .\scripts\compute_topology.ps1
#>
param(
    [string]$WorkspaceRoot = (Split-Path (Split-Path $MyInvocation.MyCommand.Path))
)

$ErrorActionPreference = "Stop"

$rootToml = Join-Path $WorkspaceRoot "Cargo.toml"
if (-not (Test-Path $rootToml)) {
    Write-Error "Cargo.toml not found at $rootToml"
    exit 1
}

$rootContent = Get-Content $rootToml -Raw

$members = @()
if ($rootContent -match 'members\s*=\s*\[([^\]]+)\]') {
    $memberStr = $matches[1]
    $members = [regex]::Matches($memberStr, '"([^"]+)"') | ForEach-Object { $_.Groups[1].Value }
}

$pkgToPath = @{}
$pkgToDeps = @{}

foreach ($member in $members) {
    $tomlPath = Join-Path $WorkspaceRoot "$member\Cargo.toml"
    if (-not (Test-Path $tomlPath)) { continue }

    $content = Get-Content $tomlPath -Raw

    $pkgName = $null
    if ($content -match '(?m)^\[package\][\s\S]*?name\s*=\s*"([^"]+)"') {
        $pkgName = $matches[1]
    } elseif ($content -match '(?m)^name\s*=\s*"([^"]+)"') {
        $pkgName = $matches[1]
    }

    if (-not $pkgName) { continue }

    $pkgToPath[$pkgName] = $member
    $pkgToDeps[$pkgName] = @()

    $depPatterns = @(
        '(?m)^\[dependencies[\.\w]*\][\s\S]*?(?=^\[|\z)'
    )

    $depSections = [regex]::Matches($content, '(?m)^\[dependencies(\.[\w-]+)?\][\s\S]*?(?=^\[[^\]]*\]|\z)')

    foreach ($section in $depSections) {
        $sectionText = $section.Value

        $inlineMatches = [regex]::Matches($sectionText, '(?m)^(sz-orm-[\w-]+)\s*=')
        foreach ($m in $inlineMatches) {
            $depName = $m.Groups[1].Value
            if ($depName -ne $pkgName) {
                $pkgToDeps[$pkgName] += $depName
            }
        }

        $tableMatches = [regex]::Matches($sectionText, '(?m)^(sz-orm-[\w-]+)\s*=\s*\{')
        foreach ($m in $tableMatches) {
            $depName = $m.Groups[1].Value
            if ($depName -ne $pkgName) {
                $pkgToDeps[$pkgName] += $depName
            }
        }
    }

    $pkgToDeps[$pkgName] = $pkgToDeps[$pkgName] | Sort-Object -Unique
}

$allPkgs = $pkgToDeps.Keys | Sort-Object

$indegree = @{}
$adjList = @{}
foreach ($pkg in $allPkgs) {
    $indegree[$pkg] = 0
    $adjList[$pkg] = @()
}

foreach ($pkg in $allPkgs) {
    foreach ($dep in $pkgToDeps[$pkg]) {
        if ($allPkgs -contains $dep) {
            $adjList[$dep] += $pkg
            $indegree[$pkg]++
        }
    }
}

$queue = [System.Collections.Generic.SortedSet[string]]::new()
foreach ($pkg in $allPkgs) {
    if ($indegree[$pkg] -eq 0) {
        $queue.Add($pkg) | Out-Null
    }
}

$result = @()
while ($queue.Count -gt 0) {
    $pkg = $queue.Min
    $queue.Remove($pkg) | Out-Null
    $result += $pkg

    $neighbors = $adjList[$pkg] | Sort-Object -Unique
    foreach ($neighbor in $neighbors) {
        $indegree[$neighbor]--
        if ($indegree[$neighbor] -eq 0) {
            $queue.Add($neighbor) | Out-Null
        }
    }
}

if ($result.Count -ne $allPkgs.Count) {
    $remaining = $allPkgs | Where-Object { $result -notcontains $_ }
    Write-Error "Circular dependency detected! Packages involved: $($remaining -join ', ')"
    exit 1
}

foreach ($pkg in $result) {
    Write-Output $pkg
}