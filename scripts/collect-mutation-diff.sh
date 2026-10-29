#!/usr/bin/env bash
# ============================================================================
# 门禁 20 增量变异测试（审计改动行子集）— collect-mutation-diff.sh
#
# 用途：对指定 diff（默认 git diff HEAD 工作区改动）生成变异体并运行测试，
#       计算杀率。相比全量变异（2352+ 变异体），`--in-diff` 只覆盖改动行，
#       可在本地/CI 分钟级完成，作为日常门禁；全量变异（collect-mutation-linux.sh）
#       作为发布前门禁分层执行。
#
# 跨包测试支持：sz-orm-core 的连接池/事务等核心逻辑，针对性测试可能位于
#       sz-orm-sqlx（如 test_known_good_fast_path_e2e）。cargo-mutants 默认只
#       运行 --package 指定包的测试，需 --test-package 指定测试包（风险 3 修复）。
#
# Windows/Git Bash 注意：不要使用 --in-place（强制单 worker 串行），本脚本
#       默认 --jobs 并行 + 临时 build 目录，避免 os error 206 与串行瓶颈。
#
# 用法：
#   bash scripts/collect-mutation-diff.sh [--diff <diff文件>] [--package <pkg>]
#       [--test-package <pkg>] [--features <features>] [--jobs <N>]
#       [--fail-under <百分比>] [--include-ignored]
#
# 默认值：package=sz-orm-core, features=tenant-quota-rls-enhanced,auto-prewarm,
#       jobs=8, fail-under=70, diff=git diff HEAD
#
# 退出码：0 = 杀率达标；1 = 杀率不达标或运行失败
# ============================================================================

set -uo pipefail

PACKAGE="sz-orm-core"
TEST_PACKAGE=""
FEATURES="tenant-quota-rls-enhanced,auto-prewarm"
JOBS=8
FAIL_UNDER=70
DIFF_FILE=""
INCLUDE_IGNORED=0
OUT_DIR="mutants-diff-out"

while [ $# -gt 0 ]; do
    case "$1" in
        --diff) DIFF_FILE="$2"; shift 2 ;;
        --package) PACKAGE="$2"; shift 2 ;;
        --test-package) TEST_PACKAGE="$2"; shift 2 ;;
        --features) FEATURES="$2"; shift 2 ;;
        --jobs) JOBS="$2"; shift 2 ;;
        --fail-under) FAIL_UNDER="$2"; shift 2 ;;
        --include-ignored) INCLUDE_IGNORED=1; shift ;;
        *) echo "未知参数: $1"; exit 1 ;;
    esac
done

if ! command -v cargo >/dev/null 2>&1; then
    # 自动定位 cargo（Windows Git Bash 的 cargo 可能不在 PATH）
    for cand in "$HOME/.cargo/bin" "$HOME/.cargo/bin/cargo" "/c/Users/Administrator/.cargo/bin"; do
        if [ -d "$cand" ]; then
            export PATH="$cand:$PATH"
            break
        fi
    done
fi
if ! command -v cargo >/dev/null 2>&1; then
    echo "ERROR: 未找到 cargo，请将 cargo 加入 PATH"
    exit 1
fi
if ! cargo mutants --version >/dev/null 2>&1; then
    echo "ERROR: cargo-mutants 未安装 (cargo install cargo-mutants)"
    exit 1
fi

# 生成 diff 文件（如果未指定）
TMP_DIFF=""
if [ -z "$DIFF_FILE" ]; then
    TMP_DIFF="$(mktemp /tmp/mutants-diff-XXXXXX.diff)"
    git diff HEAD > "$TMP_DIFF"
    DIFF_FILE="$TMP_DIFF"
    echo "已生成工作区 diff: $(wc -l < "$TMP_DIFF") 行"
fi
if [ ! -s "$DIFF_FILE" ]; then
    echo "diff 文件为空或不存在（$DIFF_FILE）→ 跳过变异测试"
    rm -f "$TMP_DIFF"
    exit 1
fi

echo "=== 增量变异测试 ==="
echo "  package:      $PACKAGE"
echo "  test-package: ${TEST_PACKAGE:-（同 package）}"
echo "  features:     $FEATURES"
echo "  jobs:         $JOBS"
echo "  diff:         $DIFF_FILE"
echo "  include-ignored: $INCLUDE_IGNORED"
echo "  fail-under:   $FAIL_UNDER%"
echo ""

# 清理旧输出
rm -rf "$OUT_DIR"

MUTANT_CMD=(cargo mutants --package "$PACKAGE")
[ -n "$TEST_PACKAGE" ] && MUTANT_CMD+=(--test-package "$TEST_PACKAGE")
MUTANT_CMD+=(--in-diff "$DIFF_FILE" --features "$FEATURES" -j "$JOBS" -o "$OUT_DIR" --)
# --include-ignored 需要放在 cargo test 的 `--` 之后（第二个 `--`），否则 cargo test 顶层报错
if [ "$INCLUDE_IGNORED" -eq 1 ]; then
    MUTANT_CMD+=(-- --include-ignored --test-threads=1)
else
    MUTANT_CMD+=(-- --test-threads=1)
fi

# 显示将执行的命令（安全转义，仅供日志参考；实际执行使用下方数组展开，不经 shell 解析）
printf '  $'
printf ' %q' "${MUTANT_CMD[@]}"
printf '\n'
"${MUTANT_CMD[@]}" > /tmp/mutants-diff-run.log 2>&1
MUTANTS_EXIT=$?

# 解析结果
caught=$(grep -c . "$OUT_DIR/mutants.out/caught.txt" 2>/dev/null || echo 0)
missed=$(grep -c . "$OUT_DIR/mutants.out/missed.txt" 2>/dev/null || echo 0)
timeout=$(grep -c . "$OUT_DIR/mutants.out/timeout.txt" 2>/dev/null || echo 0)
unviable=$(grep -c . "$OUT_DIR/mutants.out/unviable.txt" 2>/dev/null || echo 0)

total=$((caught + missed + timeout))
echo ""
echo "=== 变异结果 ==="
echo "  caught:   $caught"
echo "  missed:   $missed"
echo "  timeout:  $timeout"
echo "  unviable: $unviable"
if [ "$total" -eq 0 ]; then
    if [ "$MUTANTS_EXIT" -eq 0 ]; then
        echo "  无有效变异体（改动行未命中可变异代码）→ 视为通过"
        rm -f "$TMP_DIFF"
        exit 0
    else
        echo "❌ cargo-mutants 运行失败（MUTANTS_EXIT=$MUTANTS_EXIT）"
        tail -20 /tmp/mutants-diff-run.log
        rm -f "$TMP_DIFF"
        exit 1
    fi
fi

kill_rate=$(python3 -c "print(f'{$caught / $total * 100:.1f}')")
echo "  杀率: $kill_rate% ($caught/$total)"
echo ""

if awk "BEGIN{exit !($kill_rate >= $FAIL_UNDER)}"; then
    echo "✅ 门禁 20（增量）通过：杀率 $kill_rate% ≥ $FAIL_UNDER%"
    rm -f "$TMP_DIFF"
    exit 0
else
    echo "❌ 门禁 20（增量）未通过：杀率 $kill_rate% < $FAIL_UNDER%"
    echo "   存活变异体见 $OUT_DIR/mutants.out/missed.txt"
    tail -20 /tmp/mutants-diff-run.log
    rm -f "$TMP_DIFF"
    exit 1
fi