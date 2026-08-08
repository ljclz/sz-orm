#!/usr/bin/env pwsh
# M3.6: npm 发布脚本
# 发布主包 @sz-orm/core + 平台子包，发布前校验三平台 .node 完整 + jest 通过
#
# 用法：
#   .\scripts\publish_npm.ps1 -DryRun   # 干跑（构建当前平台 + 测试，不发布）
#   .\scripts\publish_npm.ps1           # 正式发布（需 NPM_TOKEN 环境变量）
#
# 验收标准：
#   - 发布前校验三平台 .node 二进制完整 + jest 全部通过
#   - 缺失平台 → 阻断发布 + 输出缺失平台列表
#   - npm token 通过环境变量 NPM_TOKEN 传入
#   - -DryRun 干跑成功

param(
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

$repoRoot = "$PSScriptRoot\.."
$jsPkg = "$repoRoot\packages\sz-orm-js"

Write-Host "=== npm 发布脚本 ===" -ForegroundColor Cyan

# 三平台定义
$platforms = @(
    @{ name = "linux-x64-gnu";  target = "x86_64-unknown-linux-gnu";  subpkg = "@sz-orm/core-linux-x64-gnu" },
    @{ name = "win32-x64-msvc";  target = "x86_64-pc-windows-msvc";    subpkg = "@sz-orm/core-win32-x64-msvc" },
    @{ name = "darwin-x64";      target = "x86_64-apple-darwin";       subpkg = "@sz-orm/core-darwin-x64" }
)

Set-Location $jsPkg

if (-not (Get-Command npx -ErrorAction SilentlyContinue)) {
    Write-Host "npm/npx 未安装，阻断发布！" -ForegroundColor Red
    exit 1
}

if (-not (Test-Path "node_modules")) {
    Write-Host "安装依赖..." -ForegroundColor Yellow
    npm install
}

$napiDir = "$repoRoot\target\napi"
New-Item -ItemType Directory -Force -Path $napiDir | Out-Null

# ── 步骤 1：构建当前平台 napi 绑定（生成 index.js + .node） ──
Write-Host "`n[1/4] 构建当前平台 napi 绑定..." -ForegroundColor Yellow

if (-not (Test-Path "index.js") -or -not (Get-ChildItem "*.node" -ErrorAction SilentlyContinue)) {
    & "$repoRoot\scripts\build_napi.ps1"
    if ($LASTEXITCODE -ne 0) {
        Write-Host "napi 构建失败，阻断发布！" -ForegroundColor Red
        exit 1
    }
} else {
    Write-Host "index.js + .node 已存在，跳过构建" -ForegroundColor Green
}

# ── 步骤 2：校验 jest 全部通过 ──
Write-Host "`n[2/4] 校验 jest..." -ForegroundColor Yellow

$prevEAP = $ErrorActionPreference
$ErrorActionPreference = "Continue"
$jestOutput = & npx jest tests/ --verbose 2>&1
$jestExitCode = $LASTEXITCODE
$ErrorActionPreference = $prevEAP

$jestOutput | Out-File -FilePath "$napiDir\jest_output.txt" -Encoding utf8

if ($jestExitCode -ne 0) {
    Write-Host "`njest 校验失败，阻断发布！" -ForegroundColor Red
    $jestOutput | Select-Object -First 20 | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    exit 1
}
Write-Host "jest 全部通过 ✓" -ForegroundColor Green

# ── 步骤 3：校验三平台 .node 二进制完整 ──
Write-Host "`n[3/4] 校验三平台 .node 二进制..." -ForegroundColor Yellow

$missingPlatforms = @()
$foundPlatforms = @()

foreach ($p in $platforms) {
    $nodeFile = Get-ChildItem "$napiDir\*$($p.target)*.node" -ErrorAction SilentlyContinue
    if (-not $nodeFile) {
        $nodeFile = Get-ChildItem "$jsPkg\*$($p.name)*.node" -ErrorAction SilentlyContinue
    }
    if (-not $nodeFile) {
        $nodeFile = Get-ChildItem "$jsPkg\*.node" -ErrorAction SilentlyContinue | Where-Object { $_.Name -match $p.target }
    }
    if (-not $nodeFile) {
        $nodeFile = Get-ChildItem "$napiDir\*.node" -ErrorAction SilentlyContinue | Where-Object { $_.Name -match $p.target }
    }
    if ($nodeFile) {
        $foundPlatforms += $p.name
        Write-Host "  $($p.name) ✓" -ForegroundColor Green
    } else {
        $missingPlatforms += $p.name
        Write-Host "  $($p.name) ✗ 缺失" -ForegroundColor Red
    }
}

if ($DryRun) {
    # DryRun 模式：只要存在 .node 文件即通过
    $anyNode = Get-ChildItem "$jsPkg\*.node" -ErrorAction SilentlyContinue
    if (-not $anyNode) {
        $anyNode = Get-ChildItem "$napiDir\*.node" -ErrorAction SilentlyContinue
    }
    if ($anyNode) {
        Write-Host "`nDryRun: 当前平台 .node 存在 ✓" -ForegroundColor Green
        Write-Host "`n=== 干跑成功 ===" -ForegroundColor Green
        Write-Host "jest: PASS | .node: $($anyNode.Name)" -ForegroundColor Green
        Write-Host "提示：正式发布需三平台完整，请运行 .\scripts\build_napi.ps1 -AllPlatforms" -ForegroundColor Yellow
        exit 0
    } else {
        Write-Host "`nDryRun: 当前平台 .node 缺失" -ForegroundColor Red
        exit 1
    }
}

if ($missingPlatforms.Count -gt 0) {
    Write-Host "`n缺失平台 .node 二进制，阻断发布！" -ForegroundColor Red
    Write-Host "缺失平台：$($missingPlatforms -join ', ')" -ForegroundColor Red
    Write-Host "请运行：.\scripts\build_napi.ps1 -AllPlatforms" -ForegroundColor Yellow
    exit 1
}
Write-Host "三平台 .node 二进制完整 ✓" -ForegroundColor Green

# ── 步骤 4：校验 NPM_TOKEN + 发布到 npm ──
Write-Host "`n[4/4] 发布到 npm..." -ForegroundColor Yellow

if (-not $env:NPM_TOKEN) {
    Write-Host "环境变量 NPM_TOKEN 未设置，阻断发布！" -ForegroundColor Red
    Write-Host "请设置：`$env:NPM_TOKEN = 'npm_xxxxxxxx'" -ForegroundColor Yellow
    exit 1
}
Write-Host "NPM_TOKEN 已设置 ✓" -ForegroundColor Green

# 配置 npm auth
npm config set //registry.npmjs.org/:_authToken $env:NPM_TOKEN

# 发布平台子包
foreach ($p in $platforms) {
    $subpkgDir = "$jsPkg\platform-packages\$($p.name)"
    if (Test-Path $subpkgDir) {
        Write-Host "  发布 $($p.subpkg)..." -ForegroundColor Yellow
        Set-Location $subpkgDir
        npm publish --access public
        if ($LASTEXITCODE -ne 0) {
            Write-Host "  $($p.subpkg) 发布失败！" -ForegroundColor Red
            exit 1
        }
    }
}

# 发布主包
Set-Location $jsPkg
Write-Host "  发布 @sz-orm/core..." -ForegroundColor Yellow
npm publish --access public

if ($LASTEXITCODE -ne 0) {
    Write-Host "`n@sz-orm/core 发布失败！" -ForegroundColor Red
    exit 1
}

# 清理 token
npm config delete //registry.npmjs.org/:_authToken

Write-Host "`n=== 发布成功 ===" -ForegroundColor Green
Write-Host "已发布 @sz-orm/core + $($platforms.Count) 个平台子包" -ForegroundColor Green
