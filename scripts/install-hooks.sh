#!/usr/bin/env bash
# ============================================================================
# 安装 SZ-ORM 本地 git hooks（pre-push 集成门禁）— Unix 版
#
# 用法：
#   ./scripts/install-hooks.sh
# ============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
GIT_DIR="$PROJECT_ROOT/.git"
HOOKS_DIR="$GIT_DIR/hooks"

if [ ! -d "$GIT_DIR" ]; then
    echo "[ERROR] 未找到 .git 目录，请确认在 git 仓库内运行"
    exit 1
fi

mkdir -p "$HOOKS_DIR"

# 创建 pre-push hook（sh 兼容，能跨平台运行）
cat > "$HOOKS_DIR/pre-push" << 'HOOK_EOF'
#!/bin/sh
# ============================================================================
# SZ-ORM pre-push hook — 自动调用集成门禁
# 安装方式：./scripts/install-hooks.sh
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
HOOK_EOF

chmod +x "$HOOKS_DIR/pre-push"

echo "[OK] pre-push hook 已安装到 $HOOKS_DIR/pre-push"
echo ""
echo "下次 git push 时会自动调用 scripts/gate.ps1（Windows）或 scripts/gate.sh（Unix）"
echo ""
echo "跳过门禁（紧急情况）："
echo "  SZ_ORM_SKIP_GATE=1 git push"
echo "  或：git push --no-verify"
echo ""
echo "卸载："
echo "  rm .git/hooks/pre-push"
