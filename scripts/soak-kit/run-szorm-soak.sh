#!/bin/bash
# run-szorm-soak.sh — sz-orm 长期稳定性 soak（独立于 GitHub，全本机闭环）
# 用法: run-szorm-soak.sh [--duration 6h] [--skip-build]
#
# 流程: 构建 test profile → SOAK_DURATION=6h 跑 soak.rs（连接池长稳 +
#       CSV 采样 + 内建 6 规则退化检测）→ 归档日志/CSV → 追加 index.csv
# 退出码: 0=无退化; 1=检测到退化; 2=构建失败
set -uo pipefail

DURATION="6h"
SKIP_BUILD=0
PROJ_DIR="/www/rust/sz-orm"
REPORT_ROOT="/www/rust/soak-reports"
BUILD_LOG="/tmp/szorm-soak-build.log"

while [[ $# -gt 0 ]]; do
    case $1 in
        --duration) DURATION="$2"; shift 2 ;;
        --skip-build) SKIP_BUILD=1; shift ;;
        *) echo "未知参数: $1"; exit 1 ;;
    esac
done

TS="$(date +%Y%m%dT%H%M%S)"
DATE_DIR="$(date +%F)"
OUT_DIR="$REPORT_ROOT/$DATE_DIR"
mkdir -p "$OUT_DIR"
LOG="$OUT_DIR/sz-orm-soak-${TS}.log"
CSV_OUT="$OUT_DIR/sz-orm-soak-${TS}.csv"

echo "[sz-orm-soak] 开始: duration=$DURATION ts=$TS"
cd "$PROJ_DIR" || exit 2

# 1) 构建（仅 sz-orm-core 的 soak 测试目标，test profile）
if [ "$SKIP_BUILD" -eq 0 ]; then
    echo "[sz-orm-soak] 构建 soak 测试..."
    if ! cargo test --release -p sz-orm-core --test soak --no-run > "$BUILD_LOG" 2>&1; then
        echo "[sz-orm-soak] ❌ 构建失败（见 $BUILD_LOG 尾部）"
        tail -20 "$BUILD_LOG"
        exit 2
    fi
    echo "[sz-orm-soak] 构建完成"
fi

# 2) 运行 soak（内建退化检测；MockConnection 无需外部 DB）
export SOAK_DURATION="$DURATION"
set +e
cargo test --release -p sz-orm-core --test soak -- --ignored --nocapture \
    > "$LOG" 2>&1
RC=$?
set -e

# 3) 归档 CSV（soak.rs 导出到 target/soak-report.csv）
if [ -f "$PROJ_DIR/target/soak-report.csv" ]; then
    cp "$PROJ_DIR/target/soak-report.csv" "$CSV_OUT"
    echo "[sz-orm-soak] CSV 已归档: $CSV_OUT"
fi

# 4) 判定
REGRESSIONS=$(grep -oE '检测到 [0-9]+ 项退化' "$LOG" | grep -oE '[0-9]+' | head -1)
REGRESSIONS="${REGRESSIONS:-0}"
OPS=$(grep -oE '总操作 [0-9]+ 次' "$LOG" | grep -oE '[0-9]+' | head -1)
OPS="${OPS:-0}"
if [ "$RC" -ne 0 ] || [ "$REGRESSIONS" -gt 0 ]; then
    VERDICT="FAIL"
else
    VERDICT="PASS"
fi
echo "[sz-orm-soak] 结果: $VERDICT rc=$RC regressions=$REGRESSIONS ops=$OPS"

# 5) 追加索引
IDX="$REPORT_ROOT/index.csv"
[ -f "$IDX" ] || echo "date,project,ts,verdict,regressions,ops,duration,log" >> "$IDX"
echo "$DATE_DIR,sz-orm,$TS,$VERDICT,$REGRESSIONS,$OPS,$DURATION,$LOG" >> "$IDX"
exit $([ "$VERDICT" = "PASS" ] && echo 0 || echo 1)
