#!/usr/bin/env bash
# ============================================================================
# ADR-0001 门禁检查：上游仓库未修改校验（Unix Bash 版）
#
# 校验工作区中是否有 sz-orm 核心包的文件被修改但未提交。
# 如果检测到未提交的修改，输出详细差异并返回非零退出码。
#
# ADR-0001：严禁修改上游 sz-rust / sz-orm 仓库的任何文件。
# 此脚本在 sz-orm 自身仓库中运行时，确保核心包的变更
# 已正确提交并附带相应的测试/文档更新。
#
# 用法：
#   ./scripts/check-upstream-unmodified.sh
#   ./scripts/check-upstream-unmodified.sh --warn-only
# ============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

WARN_ONLY=0
for arg in "$@"; do
    case "$arg" in
        --warn-only) WARN_ONLY=1 ;;
        *) echo "未知参数: $arg"; exit 1 ;;
    esac
done

# 颜色输出
if [ -t 1 ]; then
    COLOR_CYAN="\033[36m"
    COLOR_GREEN="\033[32m"
    COLOR_RED="\033[31m"
    COLOR_YELLOW="\033[33m"
    COLOR_RESET="\033[0m"
else
    COLOR_CYAN=""
    COLOR_GREEN=""
    COLOR_RED=""
    COLOR_YELLOW=""
    COLOR_RESET=""
fi

# sz-orm 核心包列表
CORE_PACKAGES=(
    "sz-orm-core"
    "sz-orm-auth"
    "sz-orm-config"
    "sz-orm-macros"
    "sz-orm-dtx"
    "sz-orm-query-builder"
)

echo ""
echo -e "${COLOR_CYAN}========================================${COLOR_RESET}"
echo -e "${COLOR_CYAN}[11] ADR-0001：上游仓库未修改检查${COLOR_RESET}"
echo -e "${COLOR_CYAN}========================================${COLOR_RESET}"

# 获取所有未提交的修改
all_changes=$(git diff --name-only HEAD 2>/dev/null; git ls-files --others --exclude-standard 2>/dev/null)

if [ -z "$all_changes" ]; then
    echo -e "${COLOR_GREEN}[OK] 无未提交的修改${COLOR_RESET}"
    exit 0
fi

# 检查是否有核心包的修改
violations=""
for pkg in "${CORE_PACKAGES[@]}"; do
    while IFS= read -r file; do
        if [[ "$file" == packages/"$pkg"/* ]]; then
            violations="${violations}${file}\n"
            break
        fi
    done <<< "$all_changes"
done

if [ -z "$violations" ]; then
    echo -e "${COLOR_GREEN}[OK] ADR-0001 通过：核心包无未提交修改${COLOR_RESET}"
    exit 0
fi

# 发现核心包修改
echo ""
echo -e "${COLOR_YELLOW}[INFO] 检测到核心包有未提交修改：${COLOR_RESET}"
echo -e "$violations" | while IFS= read -r line; do
    [ -n "$line" ] && echo "  $line"
done

echo ""
echo -e "${COLOR_YELLOW}根据 ADR-0001，修改核心包必须满足以下条件：${COLOR_RESET}"
echo -e "${COLOR_YELLOW}  1. 所有变更必须通过 10 道门禁检查${COLOR_RESET}"
echo -e "${COLOR_YELLOW}  2. API 签名变更必须同步更新所有调用方和测试${COLOR_RESET}"
echo -e "${COLOR_YELLOW}  3. 文档（AGENTS.md / engineering-practices.md）必须与代码一致${COLOR_RESET}"
echo -e "${COLOR_YELLOW}  4. 必须有对应的测试覆盖（新增/修改的功能）${COLOR_RESET}"

if [ "$WARN_ONLY" -eq 1 ]; then
    echo ""
    echo -e "${COLOR_YELLOW}[WARN] --warn-only 模式：仅警告，不阻断${COLOR_RESET}"
    exit 0
fi

# 运行文档一致性检查
echo ""
echo "正在运行文档一致性检查..."
if ! python3 "$SCRIPT_DIR/check-doc-consistency.py"; then
    echo ""
    echo -e "${COLOR_RED}[FAIL] ADR-0001 检查未通过：文档与代码不一致${COLOR_RESET}"
    echo -e "${COLOR_RED}请运行 'python3 scripts/check-doc-consistency.py --fix' 自动修复${COLOR_RESET}"
    exit 11
fi

echo ""
echo -e "${COLOR_GREEN}[OK] ADR-0001 通过：核心包修改已附带文档更新${COLOR_RESET}"
exit 0
