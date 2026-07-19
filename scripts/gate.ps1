<#
.SYNOPSIS
    SZ-ORM 集成层强制门禁脚本（Windows PowerShell 版）

.DESCRIPTION
    在 push / 合并 / 部署前强制执行全项目集成验证。
    任何一步失败立即停止并返回非零退出码。

    包含 7 道关卡：
      1. cargo fmt --check        格式检查
      2. cargo check --workspace  全项目编译检查（含 all-features）
      3. cargo clippy             严格模式（-D warnings）
      4. cargo test --workspace   全项目测试
      5. cargo doc                文档构建（捕获断裂的 doc 链接）
      6. API 变更扫描             检测公共 API 签名变化（如果有的话）
      7. 契约测试                 单独跑契约测试套件

.EXAMPLE
    ./scripts/gate.ps1
    ./scripts/gate.ps1 -SkipTests   # 跳过测试（紧急修复时用）
    ./scripts/gate.ps1 -Fast        # 只跑前 3 关（最快验证）

.NOTES
    门禁失败时返回非零退出码，pre-push hook 会阻止推送。
#>

[CmdletBinding()]
param(
    [switch]$SkipTests,
    [switch]$Fast
)

$ErrorActionPreference = "Stop"

# 切换到项目根（scripts/ 的父目录）
# 注：$PSScriptRoot 在某些 PowerShell 调用方式下可能为空，使用 $MyInvocation 作为后备
$ScriptDir = $PSScriptRoot
if (-not $ScriptDir) {
    $ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
}
$ProjectRoot = Split-Path -Parent $ScriptDir
if (-not $ProjectRoot) {
    $ProjectRoot = (Get-Location).Path
}
Set-Location $ProjectRoot

$StartTime = Get-Date
$FailedStep = $null
$StepCount = 0

function Write-Step($name) {
    $script:StepCount++
    $line = "[$($script:StepCount)] $name"
    Write-Host ""
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host $line -ForegroundColor Cyan
    Write-Host "========================================" -ForegroundColor Cyan
}

function Write-Ok($msg) {
    Write-Host "[OK] $msg" -ForegroundColor Green
}

function Write-Fail($msg) {
    Write-Host "[FAIL] $msg" -ForegroundColor Red
    $script:FailedStep = $msg
}

function Invoke-Step($name, $scriptBlock) {
    Write-Step $name
    $stepStart = Get-Date
    try {
        & $scriptBlock
        if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne $null) {
            Write-Fail "$name 失败 (exit=$LASTEXITCODE)"
            return $false
        }
        $elapsed = ((Get-Date) - $stepStart).TotalSeconds
        Write-Ok "$name 通过 (${elapsed}s)"
        return $true
    } catch {
        Write-Fail "$name 异常: $_"
        return $false
    }
}

# ============================================================================
# 关卡 1: 格式检查
# ============================================================================
$ok = Invoke-Step "格式检查 (cargo fmt --check)" {
    cargo fmt --all -- --check
}
if (-not $ok) { 
    Write-Host ""
    Write-Host "提示: 运行 'cargo fmt --all' 自动修复格式" -ForegroundColor Yellow
    exit 1 
}

# ============================================================================
# 关卡 2: 全项目编译检查（含 all-features）
# ============================================================================
$ok = Invoke-Step "全项目编译检查 (cargo check --workspace --all-features)" {
    cargo check --workspace --all-features --all-targets
}
if (-not $ok) { exit 2 }

# ============================================================================
# 关卡 3: clippy 严格模式（零警告）
# ============================================================================
$ok = Invoke-Step "Clippy 严格检查 (-- -D warnings)" {
    cargo clippy --workspace --all-targets --all-features -- -D warnings
}
if (-not $ok) {
    Write-Host ""
    Write-Host "提示: 运行 'cargo clippy --fix --workspace --all-targets --all-features' 自动修复" -ForegroundColor Yellow
    exit 3
}

if ($Fast) {
    Write-Host ""
    Write-Host "[Fast 模式] 跳过后续测试/文档/契约关卡" -ForegroundColor Yellow
    $TotalElapsed = ((Get-Date) - $StartTime).TotalSeconds
    Write-Host ""
    Write-Host "========================================" -ForegroundColor Green
    Write-Host "  门禁通过（Fast 模式，3 关）— ${TotalElapsed}s" -ForegroundColor Green
    Write-Host "========================================" -ForegroundColor Green
    exit 0
}

# ============================================================================
# 关卡 4: 全项目测试
# ============================================================================
if (-not $SkipTests) {
    $ok = Invoke-Step "全项目测试 (cargo test --workspace)" {
        cargo test --workspace
    }
    if (-not $ok) { exit 4 }
}

# ============================================================================
# 关卡 5: 文档构建（捕获断裂的 doc 链接）
# ============================================================================
$ok = Invoke-Step "文档构建 (cargo doc)" {
    $env:RUSTDOCFLAGS = "-D warnings"
    cargo doc --workspace --no-deps --all-features
}
if (-not $ok) { exit 5 }

# ============================================================================
# 关卡 6: API 变更扫描（如果存在 audit 脚本且有 git diff）
# ============================================================================
if (Test-Path "scripts/audit-api-changes.ps1") {
    Write-Step "API 变更扫描"
    $auditStart = Get-Date
    try {
        & "$PSScriptRoot/audit-api-changes.ps1" -Quiet
        $elapsed = ((Get-Date) - $auditStart).TotalSeconds
        Write-Ok "API 变更扫描通过 (${elapsed}s)"
    } catch {
        Write-Fail "API 变更扫描失败: $_"
        exit 6
    }
}

# ============================================================================
# 关卡 7: 契约测试套件
# ============================================================================
if (-not $SkipTests) {
    $ok = Invoke-Step "契约测试 (cargo test --test contracts)" {
        cargo test -p sz-orm-core --test contracts
    }
    if (-not $ok) { exit 7 }
}

# ============================================================================
# 汇总
# ============================================================================
$TotalElapsed = ((Get-Date) - $StartTime).TotalSeconds

Write-Host ""
Write-Host "========================================" -ForegroundColor Green
Write-Host "  门禁全部通过 ($StepCount 关)" -ForegroundColor Green
Write-Host "  总耗时: ${TotalElapsed}s" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green

exit 0
