<#
.SYNOPSIS
    安装 SZ-ORM 本地 git hooks（pre-push 集成门禁）

.DESCRIPTION
    把 scripts/pre-push 复制到 .git/hooks/pre-push 并赋予执行权限（Unix）/ 直接可执行（Windows）。
    pre-push hook 会在每次 git push 前自动调用 scripts/gate.ps1（Windows）或 scripts/gate.sh（Unix）。

.EXAMPLE
    ./scripts/install-hooks.ps1
#>

[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$GitDir = Join-Path $ProjectRoot ".git"
$HooksDir = Join-Path $GitDir "hooks"

if (-not (Test-Path $GitDir)) {
    Write-Host "[ERROR] 未找到 .git 目录，请确认在 git 仓库内运行" -ForegroundColor Red
    exit 1
}

if (-not (Test-Path $HooksDir)) {
    New-Item -ItemType Directory -Path $HooksDir -Force | Out-Null
}

# 创建 pre-push hook（PowerShell 调用 gate.ps1）
$HookContent = @'
#!/usr/bin/env powershell
# ============================================================================
# SZ-ORM pre-push hook — 自动调用集成门禁
# 安装方式：./scripts/install-hooks.ps1
# 卸载方式：rm .git/hooks/pre-push
# ============================================================================
$ErrorActionPreference = "Stop"

# 切换到仓库根
$RepoRoot = git rev-parse --show-toplevel
Set-Location $RepoRoot

# 读取 stdin（pre-push 协议：每行 <local ref> <local sha> <remote ref> <remote sha>）
while ($line = [Console]::In.ReadLine()) {
    # 仅消费，不做处理
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  pre-push 钩子：触发集成门禁" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# 跳过门禁：git push --no-verify
if ($env:SZ_ORM_SKIP_GATE -eq "1") {
    Write-Host "[SKIP] SZ_ORM_SKIP_GATE=1 已设置，跳过门禁" -ForegroundColor Yellow
    exit 0
}

# 调用 gate.ps1
$GateScript = Join-Path $RepoRoot "scripts/gate.ps1"
if (-not (Test-Path $GateScript)) {
    Write-Host "[ERROR] 未找到 $GateScript" -ForegroundColor Red
    exit 1
}

& $GateScript
$exitCode = $LASTEXITCODE

if ($exitCode -ne 0) {
    Write-Host ""
    Write-Host "[REJECT] 门禁失败 (exit=$exitCode)，推送被拒绝" -ForegroundColor Red
    Write-Host "如需强制推送（不推荐）：SZ_ORM_SKIP_GATE=1 git push" -ForegroundColor Yellow
    exit 1
}

Write-Host ""
Write-Host "[ACCEPT] 门禁通过，允许推送" -ForegroundColor Green
exit 0
'@

$HookPath = Join-Path $HooksDir "pre-push"
Set-Content -Path $HookPath -Value $HookContent -Encoding UTF8

# Windows 上 git 会通过 bash 调用 hook，所以 shebang 行用 powershell
# 实际上 git for Windows 会用 sh 调用 hook，我们改用 sh 兼容形式
$ShHookContent = @'
#!/bin/sh
# ============================================================================
# SZ-ORM pre-push hook — 自动调用集成门禁
# 安装方式：./scripts/install-hooks.ps1 或 ./scripts/install-hooks.sh
# 卸载方式：rm .git/hooks/pre-push
# ============================================================================

# 切换到仓库根
REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"

# 读取 stdin（pre-push 协议）
while read -r _local_ref _local_sha _remote_ref _remote_sha; do
    : # 仅消费
done

echo ""
echo "========================================"
echo "  pre-push 钩子：触发集成门禁"
echo "========================================"

# 跳过门禁：SZ_ORM_SKIP_GATE=1 git push
if [ "$SZ_ORM_SKIP_GATE" = "1" ]; then
    echo "[SKIP] SZ_ORM_SKIP_GATE=1 已设置，跳过门禁"
    exit 0
fi

# 调用 gate 脚本（Windows 用 ps1，Unix 用 sh）
if command -v powershell.exe >/dev/null 2>&1; then
    # Windows (Git Bash)
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "scripts/gate.ps1"
else
    # Unix
    bash scripts/gate.sh
fi

exit_code=$?

if [ "$exit_code" -ne 0 ]; then
    echo ""
    echo "[REJECT] 门禁失败 (exit=$exit_code)，推送被拒绝"
    echo "如需强制推送（不推荐）：SZ_ORM_SKIP_GATE=1 git push"
    exit 1
fi

echo ""
echo "[ACCEPT] 门禁通过，允许推送"
exit 0
'@

Set-Content -Path $HookPath -Value $ShHookContent -Encoding UTF8

Write-Host "[OK] pre-push hook 已安装到 $HookPath" -ForegroundColor Green
Write-Host ""
Write-Host "下次 git push 时会自动调用 scripts/gate.ps1（Windows）或 scripts/gate.sh（Unix）" -ForegroundColor Cyan
Write-Host ""
Write-Host "跳过门禁（紧急情况）：" -ForegroundColor Yellow
Write-Host "  SZ_ORM_SKIP_GATE=1 git push" -ForegroundColor Yellow
Write-Host "  或：git push --no-verify" -ForegroundColor Yellow
Write-Host ""
Write-Host "卸载：" -ForegroundColor Yellow
Write-Host "  rm .git/hooks/pre-push        (Unix)" -ForegroundColor Yellow
Write-Host "  del .git\hooks\pre-push       (Windows)" -ForegroundColor Yellow
