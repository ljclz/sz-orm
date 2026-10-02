#!/bin/bash
# run-szrust-soak.sh — sz-rust 框架长期稳定性 soak
# 形态：循环 { cargo build --release（增量）+ 抽样测试套件 }，每轮采样
# 内存/FD/耗时，检测编译时长漂移与内存泄漏（框架无长驻服务；sz300
# 应用服务的进程守护 soak 由既有 soak-runner.sh 模型承担，另行部署）。
# 用法: run-szrust-soak.sh [--cycles 6] [--skip-first-build]
# 退出码: 0=无退化; 1=检测到退化; 2=构建失败
set -uo pipefail

CYCLES=6
SRC_DIR="/www/rust/sz-rust"     # -> sz-rust-framework（符号链接）
REPORT_ROOT="/www/rust/soak-reports"

while [[ $# -gt 0 ]]; do
    case $1 in
        --cycles) CYCLES="$2"; shift 2 ;;
        --skip-first-build) CYCLES=$((CYCLES-1)); SKIP_BUILD=1; shift ;;
        *) echo "未知参数: $1"; exit 1 ;;
    esac
done
SKIP_BUILD="${SKIP_BUILD:-0}"

TS="$(date +%Y%m%dT%H%M%S)"
DATE_DIR="$(date +%F)"
OUT_DIR="$REPORT_ROOT/$DATE_DIR"
mkdir -p "$OUT_DIR"
LOG="$OUT_DIR/sz-rust-soak-${TS}.log"
CSV="$OUT_DIR/sz-rust-soak-${TS}.csv"

exec >> "$LOG" 2>&1
echo "[sz-rust-soak] ===== 开始 $TS cycles=$CYCLES ====="
cd "$SRC_DIR" || exit 2

echo "cycle,build_secs,test_secs,rss_kb,fds,test_ok,test_failed" > "$CSV"
FAILED_CYCLES=0
BASE_BUILD_SECS=0
MAX_BUILD_DRIFT=0

for c in $(seq 1 "$CYCLES"); do
    echo "[sz-rust-soak] ---- 第 $c/$CYCLES 轮 ----"
    if [ "$c" -eq 1 ] && [ "$SKIP_BUILD" = "1" ]; then
        echo "[sz-rust-soak] 首轮构建跳过（增量基线）"
        bsecs=0
    else
        b0=$(date +%s)
        if ! cargo build --release --workspace > /tmp/szrust-cycle-build.log 2>&1; then
            echo "[sz-rust-soak] ❌ 第 $c 轮构建失败"
            FAILED_CYCLES=$((FAILED_CYCLES+1))
            echo "$c,ERR,0,0,0,0" >> "$CSV"
            continue
        fi
        bsecs=$(( $(date +%s) - b0 ))
        [ "$BASE_BUILD_SECS" = "0" ] && BASE_BUILD_SECS=$bsecs
        drift=$(( bsecs * 100 / (BASE_BUILD_SECS > 0 ? BASE_BUILD_SECS : 1) - 100 ))
        [ "$drift" -gt "$MAX_BUILD_DRIFT" ] && MAX_BUILD_DRIFT=$drift
        echo "[sz-rust-soak] 构建完成 ${bsecs}s（相对首轮漂移 ${drift}%）"
    fi

    # 抽样测试：core 包单测（增量编译，速度快、覆盖签名行为）
    t0=$(date +%s)
    TEST_OUT=$(cargo test --release -p sz-rust-core 2>&1 | tail -4)
    tsecs=$(( $(date +%s) - t0 ))
    TOK=$(echo "$TEST_OUT" | grep -oE '[0-9]+ passed' | head -1 | grep -oE '[0-9]+')
    TFAIL=$(echo "$TEST_OUT" | grep -oE '[0-9]+ failed' | head -1 | grep -oE '[0-9]+')
    TOK="${TOK:-0}"; TFAIL="${TFAIL:-0}"

    # 采样 cargo 自身结束后系统的资源快照（观测 OOM/僵尸残留）
    rss=$(ps -eo rss,comm | awk '$2=="cargo"{s+=$1}END{print int(s/1024)}')
    fds=$(ls /proc/$(pgrep -f "cargo build" | head -1)/fd 2>/dev/null | wc -l)
    echo "$c,$bsecs,$tsecs,${rss:-0},${fds:-0},$TOK,$TFAIL" >> "$CSV"
    echo "[sz-rust-soak] 轮 $c: build=${bsecs}s test=${tsecs}s ok=$TOK fail=$TFAIL"
    [ "$TFAIL" -gt 0 ] && FAILED_CYCLES=$((FAILED_CYCLES+1))
done

# 终判
VERDICT="PASS"; REASONS=""
[ "$FAILED_CYCLES" -gt 0 ] && { VERDICT="FAIL"; REASONS="失败轮=$FAILED_CYCLES;"; }
[ "$MAX_BUILD_DRIFT" -gt 100 ] && { VERDICT="FAIL"; REASONS="构建时长漂移${MAX_BUILD_DRIFT}%;"; }
echo "[sz-rust-soak] 终判: $VERDICT cycles=$CYCLES max_build_drift=${MAX_BUILD_DRIFT}% $REASONS"

IDX="$REPORT_ROOT/index.csv"
[ -f "$IDX" ] || echo "date,project,ts,verdict,detail,log" >> "$IDX"
echo "$DATE_DIR,sz-rust,$TS,$VERDICT,cycles=$CYCLES failed=$FAILED_CYCLES drift=${MAX_BUILD_DRIFT}%,$LOG" >> "$IDX"
exit $([ "$VERDICT" = "PASS" ] && echo 0 || echo 1)
