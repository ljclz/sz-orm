#!/usr/bin/env bash
# ============================================================================
# SZ-ORM 集成层强制门禁脚本（Unix Bash 版）
#
# 在 push / 合并 / 部署前强制执行全项目集成验证。
# 任何一步失败立即停止并返回非零退出码。
#
# 包含 10 道关卡 + 2 道新增关卡：
#   1. cargo fmt --check        格式检查
#   2. cargo check --workspace  全项目编译检查（含 all-features）
#   3. cargo clippy             严格模式（-D warnings）
#   4. cargo test --workspace   全项目测试
#   5. cargo doc                文档构建（捕获断裂的 doc 链接）
#   6. API 变更扫描             检测公共 API 签名变化
#   7. 契约测试                 单独跑契约测试套件
#   8. 禁止占位实现检查         扫描 todo!/unimplemented!/unreachable!
#   9. SQL 注入扫描             扫描 SQL 拼接模式
#   10. Feature 全组合编译      cargo check --all-features
#   11. ADR-0001 上游未修改检查  核心包修改必须附带文档更新
#   12. 文档一致性检查          验证文档数据与实际代码一致
#
# 用法：
#   ./scripts/gate.sh
#   ./scripts/gate.sh --skip-tests   # 跳过测试（紧急修复时用）
#   ./scripts/gate.sh --fast         # 只跑前 3 关（最快验证）
# ============================================================================

set -euo pipefail

# 切换到项目根（scripts/ 的父目录）
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

SKIP_TESTS=0
FAST=0
for arg in "$@"; do
    case "$arg" in
        --skip-tests) SKIP_TESTS=1 ;;
        --fast) FAST=1 ;;
        *) echo "未知参数: $arg"; exit 1 ;;
    esac
done

START_TIME=$(date +%s)
STEP_COUNT=0
FAILED_STEP=""

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

write_step() {
    STEP_COUNT=$((STEP_COUNT + 1))
    echo ""
    echo -e "${COLOR_CYAN}========================================${COLOR_RESET}"
    echo -e "${COLOR_CYAN}[$STEP_COUNT] $1${COLOR_RESET}"
    echo -e "${COLOR_CYAN}========================================${COLOR_RESET}"
}

write_ok() {
    echo -e "${COLOR_GREEN}[OK] $1${COLOR_RESET}"
}

write_fail() {
    echo -e "${COLOR_RED}[FAIL] $1${COLOR_RESET}"
    FAILED_STEP="$1"
}

invoke_step() {
    local name="$1"
    shift
    write_step "$name"
    local step_start=$(date +%s)
    if "$@"; then
        local elapsed=$(($(date +%s) - step_start))
        write_ok "$name 通过 (${elapsed}s)"
        return 0
    else
        write_fail "$name 失败"
        return 1
    fi
}

# ============================================================================
# 关卡 1: 格式检查
# ============================================================================
if ! invoke_step "格式检查 (cargo fmt --check)" cargo fmt --all -- --check; then
    echo ""
    echo -e "${COLOR_YELLOW}提示: 运行 'cargo fmt --all' 自动修复格式${COLOR_RESET}"
    exit 1
fi

# ============================================================================
# 关卡 2: 全项目编译检查（含 all-features）
# ============================================================================
if ! invoke_step "全项目编译检查 (cargo check --workspace --all-features)" \
    cargo check --workspace --all-features --all-targets; then
    exit 2
fi

# ============================================================================
# 关卡 3: clippy 严格模式（零警告）
# ============================================================================
if ! invoke_step "Clippy 严格检查 (-- -D warnings)" \
    cargo clippy --workspace --all-targets --all-features -- -D warnings; then
    echo ""
    echo -e "${COLOR_YELLOW}提示: 运行 'cargo clippy --fix --workspace --all-targets --all-features' 自动修复${COLOR_RESET}"
    exit 3
fi

if [ "$FAST" -eq 1 ]; then
    echo ""
    echo -e "${COLOR_YELLOW}[Fast 模式] 跳过后续测试/文档/契约关卡${COLOR_RESET}"
    TOTAL_ELAPSED=$(($(date +%s) - START_TIME))
    echo ""
    echo -e "${COLOR_GREEN}========================================${COLOR_RESET}"
    echo -e "${COLOR_GREEN}  门禁通过（Fast 模式，3 关）— ${TOTAL_ELAPSED}s${COLOR_RESET}"
    echo -e "${COLOR_GREEN}========================================${COLOR_RESET}"
    exit 0
fi

# ============================================================================
# 关卡 4: 全项目测试
# ============================================================================
if [ "$SKIP_TESTS" -eq 0 ]; then
    if ! invoke_step "全项目测试 (cargo test --workspace)" cargo test --workspace; then
        exit 4
    fi
fi

# ============================================================================
# 关卡 5: 文档构建（捕获断裂的 doc 链接）
# ============================================================================
write_step "文档构建 (cargo doc)"
DOC_START=$(date +%s)
if RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features; then
    DOC_ELAPSED=$(($(date +%s) - DOC_START))
    write_ok "文档构建通过 (${DOC_ELAPSED}s)"
else
    write_fail "文档构建失败"
    exit 5
fi

# ============================================================================
# 关卡 6: API 变更扫描
# ============================================================================
if [ -f "scripts/audit-api-changes.sh" ]; then
    write_step "API 变更扫描"
    AUDIT_START=$(date +%s)
    if bash scripts/audit-api-changes.sh --quiet; then
        AUDIT_ELAPSED=$(($(date +%s) - AUDIT_START))
        write_ok "API 变更扫描通过 (${AUDIT_ELAPSED}s)"
    else
        write_fail "API 变更扫描失败"
        exit 6
    fi
fi

# ============================================================================
# 关卡 7: 契约测试套件
# ============================================================================
if [ "$SKIP_TESTS" -eq 0 ]; then
    if ! invoke_step "契约测试 (cargo test --test contracts)" \
        cargo test -p sz-orm-core --test contracts; then
        exit 7
    fi
fi

# ============================================================================
# 关卡 8: 禁止占位实现检查
# ============================================================================
write_step "禁止占位实现检查"
matches=$(grep -rn '\btodo!\|\bunimplemented!\|\bunreachable!' --include='*.rs' --exclude-dir=target .)
if [ -n "$matches" ]; then
    echo -e "${COLOR_RED}发现占位实现：${COLOR_RESET}"
    echo "$matches" | while IFS= read -r line; do
        echo "  $line"
    done
    exit 8
fi
write_ok "禁止占位实现检查通过"

# ============================================================================
# 关卡 9: SQL 注入扫描
# ============================================================================
write_step "SQL 注入扫描"
sql_found=0
while IFS= read -r pattern; do
    name=$(echo "$pattern" | cut -d'|' -f1)
    regex=$(echo "$pattern" | cut -d'|' -f2)
    m=$(grep -rn "$regex" --include='*.rs' --exclude-dir=target . 2>/dev/null)
    if [ -n "$m" ]; then
        echo -e "${COLOR_RED}[$name] 发现 SQL 拼接：${COLOR_RESET}"
        echo "$m" | while IFS= read -r line; do echo "  $line"; done
        sql_found=1
    fi
done << 'PATTERNS'
format! SQL 拼接|format!\s*\(\s*"[^"]*(?:SELECT|INSERT|UPDATE|DELETE|CREATE|DROP|ALTER|WHERE)[^"]*".*\{
字符串插值 SQL|"(?:[^"]*(?:SELECT|INSERT|UPDATE|DELETE|CREATE|DROP|ALTER|WHERE)[^"]*)\$\{?\w+\}?"
SQL 字符串拼接|\.to_string\(\s*\)\s*\+\s*"
raw SQL 参数插值|\.(?:execute|query|raw)\s*\(\s*format!
PATTERNS
if [ "$sql_found" -eq 1 ]; then
    write_fail "SQL 注入扫描未通过"
    exit 9
fi
write_ok "SQL 注入扫描通过"

# ============================================================================
# 关卡 10: Feature 全组合编译
# ============================================================================
if ! invoke_step "Feature 全组合编译 (cargo check --all-features)" \
    cargo check --workspace --all-targets --all-features; then
    exit 10
fi

# ============================================================================
# 关卡 11: ADR-0001 上游未修改检查
# ============================================================================
write_step "ADR-0001 上游未修改检查"
if ! bash "$SCRIPT_DIR/check-upstream-unmodified.sh"; then
    write_fail "ADR-0001 检查未通过"
    exit 11
fi

# ============================================================================
# 关卡 12: 文档一致性检查
# ============================================================================
write_step "文档一致性检查"
if ! python3 "$SCRIPT_DIR/check-doc-consistency.py"; then
    write_fail "文档一致性检查未通过"
    exit 12
fi

# ============================================================================
# 汇总
# ============================================================================
TOTAL_ELAPSED=$(($(date +%s) - START_TIME))

echo ""
echo -e "${COLOR_GREEN}========================================${COLOR_RESET}"
echo -e "${COLOR_GREEN}  门禁全部通过 ($STEP_COUNT 关)${COLOR_RESET}"
echo -e "${COLOR_GREEN}  总耗时: ${TOTAL_ELAPSED}s${COLOR_RESET}"
echo -e "${COLOR_GREEN}========================================${COLOR_RESET}"

exit 0
