#!/usr/bin/env bash
# 真库测试统一入口（2026-10-30）
#
# 背景：仓库内存在两族凭据入口——
#   * 旧集成套件：SZ_ORM_PG_URL / SZ_ORM_MYSQL_URL / SZ_ORM_ORACLE_PASSWORD / SZ_ORM_MSSQL_PASSWORD
#   * e2e 套件：  SZ_ORM_E2E_PG_URL / SZ_ORM_E2E_MYSQL_URL_{TEST,SHOP,NJSZJT} / SZ_ORM_E2E_ORACLE_*
# 本脚本是两者的唯一入口：读同一份 .env.real-db（已被 .gitignore 覆盖），
# 先校验连通性前置条件，再按套件运行；未配置的套件显式 SKIP 并记账。
#
# 用法：
#   cp docs/env.real-db.example .env.real-db   # 填入真实连接串（.env.* 已被 .gitignore 覆盖）
#   bash scripts/run-real-db-tests.sh pg mysql e2e
#   bash scripts/run-real-db-tests.sh all
#
# 退出码：0 = 所有被请求的套件都跑了且通过
#         1 = 有套件失败
#         2 = 所有套件都 SKIP（什么都没验证，禁止当作通过）
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT" || exit 1

ENV_FILE=${SZ_ORM_REAL_DB_ENV:-$ROOT/.env.real-db}
if [ -f "$ENV_FILE" ]; then
    # shellcheck disable=SC1090
    set -a; . "$ENV_FILE"; set +a
    echo "[env] 已加载 $ENV_FILE"
else
    echo "[env] 未找到 $ENV_FILE，仅使用已导出的环境变量"
fi

OUT=${SZ_ORM_REAL_DB_OUT:-$ROOT/docs/assessment/real-db-run-$(date -u +%Y%m%dT%H%M%SZ).log}
: > "$OUT"

say() { echo "$*" | tee -a "$OUT"; }

FAILED=0
RAN=0
SKIPPED=0

need() { # need <描述> <变量名...>
    local desc=$1; shift
    local v
    for v in "$@"; do
        if [ -n "${!v:-}" ]; then return 0; fi
    done
    say "SKIP[$desc]: 未设置 $*（见 docs/e2e-env-config.md 变量总表）"
    SKIPPED=$((SKIPPED + 1))
    return 1
}

run_suite() { # run_suite <套件名> <命令...>
    local name=$1; shift
    say ""
    say "============================================================"
    say "  $name"
    say "============================================================"
    "$@" 2>&1 | tee -a "$OUT"
    local rc=${PIPESTATUS[0]}
    say "[$name] rc=$rc"
    RAN=$((RAN + 1))
    if [ "$rc" -ne 0 ]; then FAILED=$((FAILED + 1)); fi
}

suite_pg() {
    need postgres SZ_ORM_PG_URL || return 0
    run_suite "pg: sqlx real_db_pool_tests" \
        cargo test -p sz-orm-sqlx --test real_db_pool_tests -- --ignored pg_
    run_suite "pg: core prewarm_integration" \
        cargo test -p sz-orm-core --test prewarm_integration -- --ignored pg
    run_suite "pg: core smart_eager_integration_pg" \
        cargo test -p sz-orm-core --test smart_eager_integration_pg -- --ignored
}

suite_mysql() {
    need mysql SZ_ORM_MYSQL_URL || return 0
    run_suite "mysql: sqlx real_db_pool_tests" \
        cargo test -p sz-orm-sqlx --test real_db_pool_tests -- --ignored mysql_
    run_suite "mysql: core prewarm_integration" \
        cargo test -p sz-orm-core --test prewarm_integration -- --ignored mysql
    run_suite "mysql: core smart_eager_integration_mysql" \
        cargo test -p sz-orm-core --test smart_eager_integration_mysql -- --ignored
}

suite_e2e() {
    need e2e SZ_ORM_E2E_MYSQL_URL_TEST SZ_ORM_E2E_PG_URL SZ_ORM_PG_URL SZ_ORM_MYSQL_URL || return 0
    local f name
    for f in packages/sz-orm-core/tests/e2e_*.rs; do
        [ -e "$f" ] || continue
        name=$(basename "$f" .rs)
        run_suite "e2e: $name" \
            cargo test -p sz-orm-core --test "$name" -- --ignored
    done
}

suite_oracle() {
    need oracle SZ_ORM_ORACLE_PASSWORD || return 0
    run_suite "oracle: sz-orm-oracle" \
        cargo test -p sz-orm-oracle --test integration_test -- --ignored
}

WANT=${*:-all}
[ "$WANT" = "all" ] && WANT="pg mysql e2e oracle"

for s in $WANT; do
    case "$s" in
        pg) suite_pg ;;
        mysql) suite_mysql ;;
        e2e) suite_e2e ;;
        oracle) suite_oracle ;;
        *) say "未知套件: $s（可选 pg/mysql/e2e/oracle/all）"; FAILED=$((FAILED + 1)) ;;
    esac
done

say ""
say "============================================================"
say "  汇总: 运行 $RAN 组 / 失败 $FAILED / 跳过 $SKIPPED 套件"
say "  日志: $OUT"
say "============================================================"

if [ "$FAILED" -ne 0 ]; then exit 1; fi
if [ "$RAN" -eq 0 ]; then
    say "全部套件 SKIP：未验证任何真库行为，不视为通过。"
    exit 2
fi
exit 0
