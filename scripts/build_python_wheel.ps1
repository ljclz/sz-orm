#!/usr/bin/env pwsh
# M3.1: Python wheel 构建脚本
# 使用 maturin 三平台交叉编译，产出 .whl 制品
#
# 用法：
#   .\scripts\build_python_wheel.ps1              # 当前平台构建
#   .\scripts\build_python_wheel.ps1 -AllPlatforms # 三平台构建（需 Docker）
#
# 验收标准：
#   - 三平台各产出 .whl 文件
#   - 干净 venv pip install <wheel> + python -c "import sz_orm" 成功

param(
    [switch]$AllPlatforms
)

$ErrorActionPreference = "Stop"
Set-Location "$PSScriptRoot\..\packages\sz-orm-python"

Write-Host "=== Python Wheel 构建脚本 ===" -ForegroundColor Cyan

# 检查 maturin 是否安装
if (-not (Get-Command maturin -ErrorAction SilentlyContinue)) {
    Write-Host "安装 maturin..." -ForegroundColor Yellow
    pip install maturin
}

$outputDir = "$PSScriptRoot\..\target\wheels"
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null

if ($AllPlatforms) {
    # 三平台交叉编译
    $platforms = @(
        @{ target = "x86_64-unknown-linux-gnu"; interpreter = "python3.10" },
        @{ target = "x86_64-pc-windows-msvc"; interpreter = "python3.10" },
        @{ target = "x86_64-apple-darwin"; interpreter = "python3.10" }
    )

    foreach ($p in $platforms) {
        Write-Host "`n构建 $($_.target)..." -ForegroundColor Yellow
        maturin build --release --target $p.target -i $p.interpreter --out $outputDir
    }
} else {
    # 当前平台构建
    Write-Host "`n构建当前平台..." -ForegroundColor Yellow
    maturin build --release --out $outputDir
}

# 列出产出的 wheel 文件
$wheels = Get-ChildItem "$outputDir\*.whl" -ErrorAction SilentlyContinue
if ($wheels) {
    Write-Host "`n=== 构建完成 ===" -ForegroundColor Green
    Write-Host "Wheel 文件：" -ForegroundColor Green
    $wheels | ForEach-Object { Write-Host "  $($_.Name) ($([math]::Round($_.Length / 1KB, 2)) KB)" }
} else {
    Write-Host "`n未找到 wheel 文件" -ForegroundColor Red
    exit 1
}