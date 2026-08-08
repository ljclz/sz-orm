#!/usr/bin/env pwsh
# M3.2: npm 包构建脚本
# 使用 napi-rs 三平台构建，产出 .node 二进制 + index.d.ts
#
# 用法：
#   .\scripts\build_napi.ps1              # 当前平台构建
#   .\scripts\build_napi.ps1 -AllPlatforms # 三平台构建
#
# 验收标准：
#   - 三平台各产出 .node 文件 + index.d.ts
#   - npm install @sz-orm/core + require('@sz-orm/core') 成功

param(
    [switch]$AllPlatforms
)

$ErrorActionPreference = "Stop"
Set-Location "$PSScriptRoot\..\packages\sz-orm-js"

Write-Host "=== npm 包构建脚本 ===" -ForegroundColor Cyan

# 检查 napi 是否安装
if (-not (Get-Command napi -ErrorAction SilentlyContinue)) {
    Write-Host "安装 @napi-rs/cli..." -ForegroundColor Yellow
    npm install -g @napi-rs/cli
}

$outputDir = "$PSScriptRoot\..\target\napi"
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null

if ($AllPlatforms) {
    # 三平台构建
    $platforms = @(
        "x86_64-unknown-linux-gnu",
        "x86_64-pc-windows-msvc",
        "x86_64-apple-darwin"
    )

    foreach ($target in $platforms) {
        Write-Host "`n构建 $target..." -ForegroundColor Yellow
        napi build --release --target $target --output-dir $outputDir
    }
} else {
    # 当前平台构建
    Write-Host "`n构建当前平台..." -ForegroundColor Yellow
    napi build --release
}

# 列出产出的 .node 文件
$nodeFiles = Get-ChildItem "$outputDir\*.node" -ErrorAction SilentlyContinue
if (-not $nodeFiles) {
    $nodeFiles = Get-ChildItem ".\*.node" -ErrorAction SilentlyContinue
}

if ($nodeFiles) {
    Write-Host "`n=== 构建完成 ===" -ForegroundColor Green
    Write-Host "Node 二进制文件：" -ForegroundColor Green
    $nodeFiles | ForEach-Object { Write-Host "  $($_.Name) ($([math]::Round($_.Length / 1KB, 2)) KB)" }

    # 检查 index.d.ts
    $dtsFile = ".\index.d.ts"
    if (Test-Path $dtsFile) {
        Write-Host "  index.d.ts ✓" -ForegroundColor Green
    } else {
        Write-Host "  index.d.ts 未找到" -ForegroundColor Yellow
    }
} else {
    Write-Host "`n未找到 .node 文件" -ForegroundColor Red
    exit 1
}