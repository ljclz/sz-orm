#!/bin/bash
# run-szrsql-soak.sh — szrsql 自研存储引擎长期稳定性 soak
# （PG-wire 兼容服务 + WAL 崩溃恢复循环 + pgbench 负载 + 资源采样）
# 用法: run-szrsql-soak.sh [--duration 12h] [--skip-build] [--crash-interval 30m]
# 退出码: 0=无退化; 1=检测到退化; 2=构建/启动失败
set -uo pipefail

DURATION="12h"
CRASH_INTERVAL=1800        # 崩溃恢复演练间隔（秒），默认 30 分钟一次
SKIP_BUILD=0
SRC_DIR="/www/rust/szrsql_src"
BIN_DIR="/www/rust/szrsql_db/target/release"
BIN="$BIN_DIR/szrsql"
WORK_DIR="/www/rust/szrsql-soak"
DATA_DIR="/www/rust/szrsql-soak/soak-data"  # 隔离的 soak 专用数据目录（严禁指向生产 szrsql_data）
PORT=5433
REPORT_ROOT="/www/rust/soak-reports"
PG_USER="postgres"
PG_DB="szrsql_soak"

while [[ $# -gt 0 ]]; do
    case $1 in
        --duration) DURATION="$2"; shift 2 ;;
        --crash-interval) CRASH_INTERVAL="$2"; shift 2 ;;
        --skip-build) SKIP_BUILD=1; shift ;;
        *) echo "未知参数: $1"; exit 1 ;;
    esac
done
# 兼容纯数字秒
[[ "$CRASH_INTERVAL" =~ ^[0-9]+$ ]] || CRASH_INTERVAL=1800

TS="$(date +%Y%m%dT%H%M%S)"
DATE_DIR="$(date +%F)"
OUT_DIR="$REPORT_ROOT/$DATE_DIR"
mkdir -p "$OUT_DIR" "$WORK_DIR" "$DATA_DIR"
LOG="$OUT_DIR/szrsql-soak-${TS}.log"
CSV="$OUT_DIR/szrsql-soak-${TS}.csv"
PGP="PGPASSWORD=postgres"

exec >> "$LOG" 2>&1
echo "[szrsql-soak] ===== 开始 $TS duration=$DURATION crash_interval=${CRASH_INTERVAL}s ====="

start_server() {
    if [ -f "$WORK_DIR/szrsql.pid" ]; then
        oldpid=$(cat "$WORK_DIR/szrsql.pid" 2>/dev/null || true)
        [ -n "$oldpid" ] && kill -9 "$oldpid" 2>/dev/null
        rm -f "$WORK_DIR/szrsql.pid"
    fi
    fuser -k ${PORT}/tcp 2>/dev/null || true
    sleep 1
    nohup "$BIN" --port $PORT --data-dir "$DATA_DIR" \
        >> "$WORK_DIR/szrsql.log" 2>&1 &
    echo $! > "$WORK_DIR/szrsql.pid"
    for i in $(seq 1 30); do
        if PGPASSWORD=postgres psql -h 127.0.0.1 -p $PORT -U $PG_USER -d postgres \
            -c "SELECT 1" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    return 1
}

sample_metrics() {
    local pid="$1" tps="$2" lat="$3"
    local rss_kb=0 fds=0 cpu=0
    if [ -d "/proc/$pid" ]; then
        rss_kb=$(awk '/VmRSS/{print $2}' /proc/$pid/status 2>/dev/null || echo 0)
        fds=$(ls /proc/$pid/fd 2>/dev/null | wc -l)
        cpu=$(awk '{print int($14+$15)}' /proc/$pid/stat 2>/dev/null || echo 0)
    fi
    echo "$(date +%s),$rss_kb,$fds,$cpu,$tps,$lat" >> "$CSV"
}

# 1) 构建
cd "$SRC_DIR" || exit 2
if [ "$SKIP_BUILD" -eq 0 ]; then
    echo "[szrsql-soak] 构建 release..."
    if ! cargo build --release -p szrsql-bin > /tmp/szrsql-build.log 2>&1; then
        echo "[szrsql-soak] ❌ 构建失败"; tail -20 /tmp/szrsql-build.log; exit 2
    fi
fi
mkdir -p "$BIN_DIR"
cp -f "$SRC_DIR/target/release/szrsql" "$BIN" 2>/dev/null || { echo "❌ 二进制缺失"; exit 2; }

# 2) 启动 + 建库
start_server || { echo "❌ 服务启动失败"; exit 2; }
SPID=$(cat "$WORK_DIR/szrsql.pid")
echo "[szrsql-soak] 服务已启动 pid=$SPID"
PGPASSWORD=postgres psql -h 127.0.0.1 -p $PORT -U $PG_USER -d postgres \
    -c "CREATE DATABASE IF NOT EXISTS $PG_DB" 2>/dev/null || true

# 3) 初始化压测表
PGPASSWORD=postgres psql -h 127.0.0.1 -p $PORT -U $PG_USER -d $PG_DB \
    -c "CREATE TABLE IF NOT EXISTS soak_kv (k BIGINT PRIMARY KEY, v TEXT, updated_at BIGINT)" 2>/dev/null || true
PGPASSWORD=postgres psql -h 127.0.0.1 -p $PORT -U $PG_USER -d $PG_DB \
    -c "CREATE TABLE IF NOT EXISTS soak_ao (id BIGSERIAL PRIMARY KEY, payload TEXT)" 2>/dev/null || true

echo "time,rss_kb,fds,cpu_ticks,tps,latency_ms" > "$CSV"
BASE_ROWS=$(PGPASSWORD=postgres psql -h 127.0.0.1 -p $PORT -U $PG_USER -d $PG_DB -tAc \
    "SELECT count(*) FROM soak_kv" 2>/dev/null || echo 0)
BASE_MD5=$( { a=$(PGPASSWORD=postgres psql -h 127.0.0.1 -p $PORT -U $PG_USER -d $PG_DB -tAc "SELECT sum(k) FROM soak_kv" 2>/dev/null); c=$(PGPASSWORD=postgres psql -h 127.0.0.1 -p $PORT -U $PG_USER -d $PG_DB -tAc "SELECT count(*) FROM soak_kv" 2>/dev/null); echo $(( ${a:-0} + ${c:-0} )); })

echo "[szrsql-soak] 基线: rows=$BASE_ROWS checksum=$BASE_MD5"

# 4) pgbench 负载脚本（读写混合）
BENCH_SQL="$WORK_DIR/pgbench-soak.sql"
cat > "$BENCH_SQL" <<'EOSQL'
\set k random(1, 100000)
\set v random(1, 1000000000)
INSERT INTO soak_kv (k, v, updated_at) VALUES (:k, :v::text, extract(epoch from now())::bigint)
    ON CONFLICT (k) DO UPDATE SET v = excluded.v, updated_at = excluded.updated_at;
SELECT count(*) FROM soak_kv WHERE k < :k;
UPDATE soak_kv SET v = :v::text WHERE k = :k;
SELECT count(*) FROM soak_kv WHERE k > :k - 100;
EOSQL

DURATION_SECS=$( { echo "$DURATION" | sed 's/h/*3600/; s/m/*60/; s/s//' | bc; } 2>/dev/null || echo 43200 )
END_AT=$(( $(date +%s) + DURATION_SECS ))
NEXT_CRASH=$(( $(date +%s) + CRASH_INTERVAL ))
LAST_SAMPLE=0
RECOVERY_PASS=0
RECOVERY_FAIL=0
CRASH_COUNT=0
MIN_TPS=999999999
FIRST_TPS=0

echo "[szrsql-soak] 进入负载循环: 结束时间戳=$END_AT"
while [ "$(date +%s)" -lt "$END_AT" ]; do
    # pgbench 60 秒一轮（8 客户端，6 线程）
    OUT=$(PGPASSWORD=postgres pgbench -h 127.0.0.1 -p $PORT -U $PG_USER -d $PG_DB \
        -c 8 -j 4 -T 60 -f "$BENCH_SQL" -n 2>&1 | tail -6)
    TPS=$(echo "$OUT" | grep -oE 'number of transactions actually processed: [0-9]+' | grep -oE '[0-9]+$')
    TPS=$(( ${TPS:-0} / 60 ))
    LAT=$(echo "$OUT" | grep -oE 'latency average = [0-9.]+' | grep -oE '[0-9.]+$')
    LAT="${LAT:-0}"
    [ "$FIRST_TPS" = "0" ] && [ "$TPS" -gt 0 ] && FIRST_TPS=$TPS
    [ "$TPS" -gt 0 ] && [ "$TPS" -lt "$MIN_TPS" ] && MIN_TPS=$TPS
    sample_metrics "$SPID" "$TPS" "$LAT"
    echo "[szrsql-soak] tps=$TPS lat=${LAT}ms"

    # 崩溃恢复演练：记录行数/校验和 → kill -9 → 重启 → 校验一致
    NOW=$(date +%s)
    if [ "$NOW" -ge "$NEXT_CRASH" ]; then
        echo "[szrsql-soak] 崩溃恢复演练 #$((CRASH_COUNT+1))"
        ROWS_BEFORE=$(PGPASSWORD=postgres psql -h 127.0.0.1 -p $PORT -U $PG_USER -d $PG_DB -tAc \
            "SELECT count(*) FROM soak_kv" 2>/dev/null || echo "-1")
        a=$(PGPASSWORD=postgres psql -h 127.0.0.1 -p $PORT -U $PG_USER -d $PG_DB -tAc "SELECT sum(k) FROM soak_kv" 2>/dev/null)
        c=$(PGPASSWORD=postgres psql -h 127.0.0.1 -p $PORT -U $PG_USER -d $PG_DB -tAc "SELECT count(*) FROM soak_kv" 2>/dev/null)
        MD5_BEFORE=$(( ${a:--1} + ${c:--1} ))
        kill -9 "$SPID" 2>/dev/null
        sleep 2
        CRASH_COUNT=$((CRASH_COUNT+1))
        start_server
        RC=$?
        if [ "$RC" -ne 0 ]; then
            echo "[szrsql-soak] ❌ 崩溃后服务未恢复"
            RECOVERY_FAIL=$((RECOVERY_FAIL+1))
        else
            SPID=$(cat "$WORK_DIR/szrsql.pid")
            ROWS_AFTER=$(PGPASSWORD=postgres psql -h 127.0.0.1 -p $PORT -U $PG_USER -d $PG_DB -tAc \
                "SELECT count(*) FROM soak_kv" 2>/dev/null || echo "-2")
            a2=$(PGPASSWORD=postgres psql -h 127.0.0.1 -p $PORT -U $PG_USER -d $PG_DB -tAc "SELECT sum(k) FROM soak_kv" 2>/dev/null)
            c2=$(PGPASSWORD=postgres psql -h 127.0.0.1 -p $PORT -U $PG_USER -d $PG_DB -tAc "SELECT count(*) FROM soak_kv" 2>/dev/null)
            MD5_AFTER=$(( ${a2:--2} + ${c2:--2} ))
            # kill -9 前 in-flight 事务允许丢最后一笔（未 checkpoint），行数允许 <= before
            if [ "$ROWS_AFTER" -le "$ROWS_BEFORE" ] && [ "$MD5_AFTER" = "$MD5_BEFORE" ]; then
                echo "[szrsql-soak] ✅ WAL 恢复一致 (rows=$ROWS_AFTER md5=$MD5_AFTER)"
                RECOVERY_PASS=$((RECOVERY_PASS+1))
            elif [ "$MD5_AFTER" != "$MD5_BEFORE" ]; then
                echo "[szrsql-soak] ❌ WAL 恢复数据不一致! before=$MD5_BEFORE after=$MD5_AFTER"
                RECOVERY_FAIL=$((RECOVERY_FAIL+1))
            else
                echo "[szrsql-soak] ✅ WAL 恢复通过（行数 $ROWS_BEFORE→$ROWS_AFTER）"
                RECOVERY_PASS=$((RECOVERY_PASS+1))
            fi
        fi
        NEXT_CRASH=$(( $(date +%s) + CRASH_INTERVAL ))
    fi
    SPID=$(cat "$WORK_DIR/szrsql.pid" 2>/dev/null || echo "$SPID")
done

# 5) 终判
FINAL_ROWS=$(PGPASSWORD=postgres psql -h 127.0.0.1 -p $PORT -U $PG_USER -d $PG_DB -tAc \
    "SELECT count(*) FROM soak_kv" 2>/dev/null || echo 0)
RSS_MAX=$(awk -F, 'NR>1{if($2>m)m=$2}END{print m}' "$CSV")
RSS_FIRST=$(awk -F, 'NR==2{print $2}' "$CSV")
FD_MAX=$(awk -F, 'NR>1{if($3>m)m=$3}END{print m}' "$CSV")
RSS_GROW=$(( (RSS_MAX - ${RSS_FIRST:-0}) / 1024 ))
VERDICT="PASS"; REASONS=""
[ "$RECOVERY_FAIL" -gt 0 ] && { VERDICT="FAIL"; REASONS="$REASONS 恢复失败x$RECOVERY_FAIL;"; }
[ "$RSS_GROW" -gt 200 ] && { VERDICT="FAIL"; REASONS="$REASONS RSS增长${RSS_GROW}MB;"; }
[ "$FD_MAX" -gt 500 ] && { VERDICT="FAIL"; REASONS="$REASONS FD泄漏${FD_MAX};"; }
if [ "$FIRST_TPS" -gt 0 ] && [ "$MIN_TPS" -lt $(( FIRST_TPS * 7 / 10 )) ]; then
    VERDICT="FAIL"; REASONS="$REASONS 吞吐衰减(首${FIRST_TPS}/低${MIN_TPS});"
fi
echo "[szrsql-soak] 终判: $VERDICT crashes=$CRASH_COUNT recovery(pass=$RECOVERY_PASS,fail=$RECOVERY_FAIL) rows=$FINAL_ROWS rss_max=${RSS_MAX}KB fd_max=$FD_MAX $REASONS"

IDX="$REPORT_ROOT/index.csv"
[ -f "$IDX" ] || echo "date,project,ts,verdict,detail,log" >> "$IDX"
echo "$DATE_DIR,szrsql,$TS,$VERDICT,crashes=$CRASH_COUNT rec_fail=$RECOVERY_FAIL rss=${RSS_GROW}MB tps=$FIRST_TPS→$MIN_TPS,$LOG" >> "$IDX"

pgrep -f "$BIN" | head -1 | xargs -r kill -9 2>/dev/null || true
exit $([ "$VERDICT" = "PASS" ] && echo 0 || echo 1)
