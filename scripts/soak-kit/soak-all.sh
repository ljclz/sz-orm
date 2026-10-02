#!/bin/bash
# soak-all.sh — 三项目总调度入口（串行错峰，结果统一入 soak-reports/index.csv）
# 用法: soak-all.sh <project> [duration]   project ∈ {sz-orm, szrsql, sz-rust, all}
set -uo pipefail
TOOLKIT="/www/rust/soak-toolkit"
PROJECT="${1:-all}"
DURATION="${2:-}"

run_one() {
    local p="$1" d="$2"
    local args=""
    [ -n "$d" ] && args="--duration $d"
    echo "[$(date '+%F %T')] >>> 启动 $p soak $args"
    case "$p" in
        sz-orm)  "$TOOLKIT/run-szorm-soak.sh"  $args ;;
        szrsql)  "$TOOLKIT/run-szrsql-soak.sh" $args ;;
        sz-rust) "$TOOLKIT/run-szrust-soak.sh" $args ;;
        *) echo "未知项目: $p"; return 2 ;;
    esac
    local rc=$?
    echo "[$(date '+%F %T')] <<< $p soak 结束 rc=$rc"
    return $rc
}

case "$PROJECT" in
    all)
        rc_sum=0
        for p in sz-orm szrsql sz-rust; do
            run_one "$p" "$DURATION" || rc_sum=$((rc_sum+1))
        done
        echo "[$(date '+%F %T')] === all 完成，失败项: $rc_sum/3 ==="
        exit $rc_sum
        ;;
    sz-orm|szrsql|sz-rust)
        run_one "$PROJECT" "$DURATION"
        ;;
    *)
        echo "用法: soak-all.sh <sz-orm|szrsql|sz-rust|all> [duration]"; exit 2 ;;
esac
