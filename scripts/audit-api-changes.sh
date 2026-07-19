#!/usr/bin/env bash
# ============================================================================
# SZ-ORM 公共 API 行为变更审计脚本（Unix Bash 版）
#
# 基于 git diff 检测公共 API 的签名/行为变更：
#   1. pub fn / pub async fn 签名变化
#   2. pub struct / pub enum 字段变化
#   3. pub trait 方法签名变化
#   4. 返回类型变化（Result<...>、Box<dyn ...> 等）
#   5. 错误类型变化（PoolError、TxError、DbError 等变体）
#   6. panic! / unwrap / expect 行为相关变更
#
# 用法：
#   ./scripts/audit-api-changes.sh                  # 与 HEAD~1 对比
#   ./scripts/audit-api-changes.sh --base main      # 与 main 分支对比
#   ./scripts/audit-api-changes.sh --quiet          # 静默模式（仅警告，不阻断）
#   ./scripts/audit-api-changes.sh --strict         # 严格模式（未覆盖则 exit 1）
# ============================================================================

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

BASE="HEAD~1"
QUIET=0
STRICT=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --base) BASE="$2"; shift 2 ;;
        --quiet) QUIET=1; shift ;;
        --strict) STRICT=1; shift ;;
        *) echo "未知参数: $1"; exit 1 ;;
    esac
done

# ============================================================================
# 1. 检测 git 仓库
# ============================================================================
if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "[ERROR] 不在 git 仓库内"
    exit 1
fi

# ============================================================================
# 2. 输出头部
# ============================================================================
if [ -t 1 ]; then
    COLOR_CYAN="\033[36m"
    COLOR_GREEN="\033[32m"
    COLOR_RED="\033[31m"
    COLOR_YELLOW="\033[33m"
    COLOR_DARK="\033[90m"
    COLOR_RESET="\033[0m"
else
    COLOR_CYAN=""
    COLOR_GREEN=""
    COLOR_RED=""
    COLOR_YELLOW=""
    COLOR_DARK=""
    COLOR_RESET=""
fi

echo -e "${COLOR_CYAN}========================================${COLOR_RESET}"
echo -e "${COLOR_CYAN}  公共 API 行为变更审计${COLOR_RESET}"
echo -e "${COLOR_CYAN}  对比基准: $BASE${COLOR_RESET}"
echo -e "${COLOR_CYAN}========================================${COLOR_RESET}"

# ============================================================================
# 3. 收集 API 变更
# ============================================================================
DIFF_OUTPUT=$(git diff "$BASE" --unified=0 -- "packages/*/src/*.rs" 2>/dev/null)

if [ -z "$DIFF_OUTPUT" ]; then
    echo -e "${COLOR_GREEN}[OK] 没有检测到代码变更${COLOR_RESET}"
    exit 0
fi

# 提取新增的 pub API
ADDED_APIS=$(echo "$DIFF_OUTPUT" | grep -E '^\+\s*pub\s+(async\s+)?fn\s+\w+' | sed -E 's/^\+\s*pub\s+(async\s+)?fn\s+(\w+).*/\2/' | sort -u)
REMOVED_APIS=$(echo "$DIFF_OUTPUT" | grep -E '^-\s*pub\s+(async\s+)?fn\s+\w+' | sed -E 's/^-\s*pub\s+(async\s+)?fn\s+(\w+).*/\2/' | sort -u)

ADDED_STRUCTS=$(echo "$DIFF_OUTPUT" | grep -E '^\+\s*pub\s+struct\s+\w+' | sed -E 's/^\+\s*pub\s+struct\s+(\w+).*/\1/' | sort -u)
REMOVED_STRUCTS=$(echo "$DIFF_OUTPUT" | grep -E '^-\s*pub\s+struct\s+\w+' | sed -E 's/^-\s*pub\s+struct\s+(\w+).*/\1/' | sort -u)

ADDED_ENUMS=$(echo "$DIFF_OUTPUT" | grep -E '^\+\s*pub\s+enum\s+\w+' | sed -E 's/^\+\s*pub\s+enum\s+(\w+).*/\1/' | sort -u)
REMOVED_ENUMS=$(echo "$DIFF_OUTPUT" | grep -E '^-\s*pub\s+enum\s+\w+' | sed -E 's/^-\s*pub\s+enum\s+(\w+).*/\1/' | sort -u)

ADDED_TRAITS=$(echo "$DIFF_OUTPUT" | grep -E '^\+\s*pub\s+trait\s+\w+' | sed -E 's/^\+\s*pub\s+trait\s+(\w+).*/\1/' | sort -u)
REMOVED_TRAITS=$(echo "$DIFF_OUTPUT" | grep -E '^-\s*pub\s+trait\s+\w+' | sed -E 's/^-\s*pub\s+trait\s+(\w+).*/\1/' | sort -u)

# 行为变更（panic/unwrap/expect/Result 返回类型/错误变体）
BEHAVIOR_PANIC=$(echo "$DIFF_OUTPUT" | grep -cE '^[+-].*\bpanic!\(' || echo 0)
BEHAVIOR_UNWRAP=$(echo "$DIFF_OUTPUT" | grep -cE '^[+-].*\.unwrap\(\)' || echo 0)
BEHAVIOR_EXPECT=$(echo "$DIFF_OUTPUT" | grep -cE '^[+-].*\.expect\(' || echo 0)
BEHAVIOR_RESULT=$(echo "$DIFF_OUTPUT" | grep -cE '^[+-].*->\s*Result<' || echo 0)
BEHAVIOR_OPTION=$(echo "$DIFF_OUTPUT" | grep -cE '^[+-].*->\s*Option<' || echo 0)
BEHAVIOR_ERRORS=$(echo "$DIFF_OUTPUT" | grep -cE '^[+-].*(PoolError|TxError|DbError|CacheError)::\w+' || echo 0)

# ============================================================================
# 4. 汇总输出
# ============================================================================
HAS_CHANGES=0
[ -n "$ADDED_APIS" ] && HAS_CHANGES=1
[ -n "$REMOVED_APIS" ] && HAS_CHANGES=1
[ -n "$ADDED_STRUCTS" ] && HAS_CHANGES=1
[ -n "$REMOVED_STRUCTS" ] && HAS_CHANGES=1
[ -n "$ADDED_ENUMS" ] && HAS_CHANGES=1
[ -n "$REMOVED_ENUMS" ] && HAS_CHANGES=1
[ -n "$ADDED_TRAITS" ] && HAS_CHANGES=1
[ -n "$REMOVED_TRAITS" ] && HAS_CHANGES=1
[ "$BEHAVIOR_PANIC" -gt 0 ] && HAS_CHANGES=1
[ "$BEHAVIOR_UNWRAP" -gt 0 ] && HAS_CHANGES=1
[ "$BEHAVIOR_RESULT" -gt 0 ] && HAS_CHANGES=1
[ "$BEHAVIOR_ERRORS" -gt 0 ] && HAS_CHANGES=1

if [ "$HAS_CHANGES" -eq 0 ]; then
    echo -e "${COLOR_GREEN}[OK] 未检测到公共 API 或行为变更${COLOR_RESET}"
    exit 0
fi

echo ""
echo -e "${COLOR_GREEN}--- 新增公共 API ---${COLOR_RESET}"
[ -n "$ADDED_APIS" ] && echo "  + fn: $ADDED_APIS" | tr '\n' ',' | sed 's/,/, /g; s/, $//'
[ -n "$ADDED_STRUCTS" ] && echo "  + struct: $ADDED_STRUCTS" | tr '\n' ',' | sed 's/,/, /g; s/, $//'
[ -n "$ADDED_ENUMS" ] && echo "  + enum: $ADDED_ENUMS" | tr '\n' ',' | sed 's/,/, /g; s/, $//'
[ -n "$ADDED_TRAITS" ] && echo "  + trait: $ADDED_TRAITS" | tr '\n' ',' | sed 's/,/, /g; s/, $//'

echo ""
echo -e "${COLOR_RED}--- 移除/修改公共 API ---${COLOR_RESET}"
[ -n "$REMOVED_APIS" ] && echo "  - fn: $REMOVED_APIS" | tr '\n' ',' | sed 's/,/, /g; s/, $//'
[ -n "$REMOVED_STRUCTS" ] && echo "  - struct: $REMOVED_STRUCTS" | tr '\n' ',' | sed 's/,/, /g; s/, $//'
[ -n "$REMOVED_ENUMS" ] && echo "  - enum: $REMOVED_ENUMS" | tr '\n' ',' | sed 's/,/, /g; s/, $//'
[ -n "$REMOVED_TRAITS" ] && echo "  - trait: $REMOVED_TRAITS" | tr '\n' ',' | sed 's/,/, /g; s/, $//'

echo ""
echo -e "${COLOR_YELLOW}--- 行为相关变更 ---${COLOR_RESET}"
[ "$BEHAVIOR_PANIC" -gt 0 ] && echo -e "  [panic!] ${BEHAVIOR_PANIC} 处变更"
[ "$BEHAVIOR_UNWRAP" -gt 0 ] && echo -e "  [unwrap()] ${BEHAVIOR_UNWRAP} 处变更"
[ "$BEHAVIOR_EXPECT" -gt 0 ] && echo -e "  [expect()] ${BEHAVIOR_EXPECT} 处变更"
[ "$BEHAVIOR_RESULT" -gt 0 ] && echo -e "  [Result 返回] ${BEHAVIOR_RESULT} 处变更"
[ "$BEHAVIOR_OPTION" -gt 0 ] && echo -e "  [Option 返回] ${BEHAVIOR_OPTION} 处变更"
[ "$BEHAVIOR_ERRORS" -gt 0 ] && echo -e "  [错误变体] ${BEHAVIOR_ERRORS} 处变更"

# ============================================================================
# 5. 查找受影响的调用方
# ============================================================================
echo ""
echo -e "${COLOR_CYAN}--- 受影响的调用方 ---${COLOR_RESET}"

# 合并所有变更的 API 名称
ALL_CHANGED_APIS=$(echo "$ADDED_APIS$REMOVED_APIS" | sort -u)

for api_name in $ALL_CHANGED_APIS; do
    # 跳过太通用的名称（避免误报）
    if [ ${#api_name} -lt 4 ]; then continue; fi
    case "$api_name" in
        new|get|set|len|is_|with|build|run) continue ;;
    esac

    # 在工作空间内搜索调用方（排除定义文件本身）
    callers=$(grep -rn --include="*.rs" "\b${api_name}\b" packages/ cli/ examples/ 2>/dev/null |
        grep -v "packages/.*-core/src/" |
        grep -v "packages/.*-core/tests/" |
        awk -F: '{print $1}' |
        sort -u)

    if [ -n "$callers" ]; then
        caller_count=$(echo "$callers" | wc -l)
        echo -e "  [${api_name}] ${caller_count} 个调用方："
        echo "$callers" | head -5 | sed 's/^/      - /'
        if [ "$caller_count" -gt 5 ]; then
            echo "      ... 还有 $((caller_count - 5)) 个"
        fi
    fi
done

# ============================================================================
# 6. 测试同步性检查
# ============================================================================
echo ""
echo -e "${COLOR_CYAN}--- 测试同步性检查 ---${COLOR_RESET}"

# src/ 文件变更数
SRC_CHANGED=$(git diff "$BASE" --name-only -- "packages/*/src/*.rs" 2>/dev/null | wc -l)
# tests/ 文件变更数
TESTS_CHANGED=$(git diff "$BASE" --name-only -- "packages/*/tests/*.rs" 2>/dev/null | wc -l)
# 是否有 API 变更
API_CHANGED=0
[ -n "$ADDED_APIS" ] || [ -n "$REMOVED_APIS" ] && API_CHANGED=1

if [ "$SRC_CHANGED" -gt 0 ] && [ "$TESTS_CHANGED" -eq 0 ] && [ "$API_CHANGED" -eq 1 ]; then
    echo -e "  ${COLOR_YELLOW}[WARN] src/ 有 API 变更但 tests/ 未修改${COLOR_RESET}"
    echo -e "  ${COLOR_YELLOW}请确认是否需要更新测试${COLOR_RESET}"
    TEST_SYNC_FAIL=1
else
    echo -e "  ${COLOR_GREEN}[OK] src/ 和 tests/ 修改同步${COLOR_RESET}"
    TEST_SYNC_FAIL=0
fi

# ============================================================================
# 7. 输出建议
# ============================================================================
echo ""
echo -e "${COLOR_CYAN}--- 审计建议 ---${COLOR_RESET}"
echo "  1. 确认所有新增/修改的 API 是否已在 docs/api-contracts.md 中登记"
echo "  2. 确认所有调用方是否已适配新的 API 签名"
echo "  3. 确认行为变更（panic/Result/错误类型）是否已同步到契约测试"
echo "  4. 运行 'cargo test -p sz-orm-core --test contracts' 验证契约"

# ============================================================================
# 8. 严格模式退出码
# ============================================================================
if [ "$STRICT" -eq 1 ] && [ "$TEST_SYNC_FAIL" -eq 1 ]; then
    echo ""
    echo -e "${COLOR_RED}[STRICT] 检测到 API 变更但未同步测试，退出码 1${COLOR_RESET}"
    exit 1
fi

# Quiet 模式：始终返回 0
if [ "$QUIET" -eq 1 ]; then
    exit 0
fi

exit 0
