# v9.2.0 覆盖率战役报告

> 采集日期：2026-09-30（M17c 更新）
> 采集环境：Linux 服务器（121.204.253.75），CentOS Stream 9，40 核 62GB
> 工具：cargo-llvm-cov 0.9.1
> Rust：1.98.1

## 1. 总览

| 维度 | v9.0.0 基线 | v9.2.0 纯单元 | v9.2.0 含集成+Oracle | 变化 |
|------|------------|--------------|---------------------|------|
| 行覆盖率 | 90.2% | **90.63%** | **90.61%** | +0.43% / +0.41% |
| 分支覆盖率 | 100%（审计声明） | **88.74%** | **88.74%** | — |
| 函数覆盖率 | — | **89.81%** | **89.86%** | — |
| 未覆盖行 | ~30,000 | **28,695** | **28,774** | -1,305 / -1,226 |
| 新增测试 | 0 | **829** | **829** | +829 |

> **M16 新增**：query_misc_test 96 tests + qb_coverage_test 34 tests + schema_sync 15 tests = 145 tests
> **M17 Oracle XE 21c Docker**：ghcr.io/gvenzl/oracle-xe:21 运行在服务器 1521 端口
> **Oracle 集成测试**：26 passed（22 connection + 4 pool），测试文件已改为支持环境变量覆盖
> **M17c 覆盖率采集成功**：--include-ignored + --ignore-run-fail，Oracle lib.rs 从 46.00% 提升到 77.77%（未覆盖行从 1125 降到 463，减少 662 行）

## 2. 采集命令

```bash
# 纯单元测试覆盖率（M16）
cd /www/rust/sz-orm-v910
CARGO_INCREMENTAL=0 RUST_MIN_STACK=33554432 \
CARGO_TARGET_DIR=/www/rust/cov-target-m16 \
cargo llvm-cov --workspace \
  --exclude sz-orm-python --exclude sz-orm-js --exclude sz-orm-wasm \
  --exclude sz-orm-cabi --exclude sz-orm-go --exclude sz-orm-java \
  --exclude sz-orm-cpp --exclude sz-orm-n1-lint \
  --summary-only

# 含集成测试+Oracle 覆盖率（M17c，最终结果）
export CARGO_TARGET_DIR=/www/rust/cov-target-m17c
export RUST_MIN_STACK=33554432 CARGO_INCREMENTAL=0
export LD_LIBRARY_PATH="/usr/lib/oracle/23/client64/lib:$LD_LIBRARY_PATH"
export SZ_ORM_ORACLE_CONNECT_STRING="127.0.0.1:1521/XEPDB1"
export SZ_ORM_ORACLE_USER="sz_orm_test" SZ_ORM_ORACLE_PASSWORD="SzOrmTest2026"
export MYSQL_HOST=127.0.0.1 MYSQL_PORT=8802 MYSQL_USER=root MYSQL_PASSWORD=test123 MYSQL_DATABASE=sz_orm_test
export DATABASE_URL="mysql://root:test123@127.0.0.1:8802/sz_orm_test"
export PG_HOST=127.0.0.1 PG_PORT=5432 PG_USER=postgres PG_PASSWORD=test123 PG_DATABASE=sz_orm_test
cargo llvm-cov --workspace --ignore-run-fail \
  --exclude sz-orm-python --exclude sz-orm-js --exclude sz-orm-wasm \
  --exclude sz-orm-cabi --exclude sz-orm-go --exclude sz-orm-java \
  --exclude sz-orm-cpp --exclude sz-orm-n1-lint \
  --summary-only \
  -- --include-ignored --skip clickhouse --skip mssql --skip redis --skip gbase --skip mariadb
```

## 3. 新增测试明细

### M1~M9: 684 测试（commit `ff20f975` ~ `07f1ed69`）
- M1: any.rs MySQL/PG 61 tests
- M2: query.rs 38 tests
- M3: dialect.rs 60 tests
- M4+M5: oracle/mssql 86 tests
- M6+M7: pool/l2_cache 49 tests
- M8: model+value+query-builder 99 tests
- M9: 长尾模块 227 tests

### M12~M15: 139 测试
- M12: dialect.rs 高级分支 38 tests（commit `17d10fb3`）
- M13: query.rs where 条件变体 26 tests（commit `98ce2b53`）
- M14: query.rs 参数化方法 36 tests（commit `c88615fc`）
- M15: l2_cache/pool 高级分支 39 tests（commit `6f92facc`）

### M16: 145 测试
- query_misc_test.rs: 96 tests（commit `57f6f63a`）
  - build_max/min/sum/avg, validate*, select_quoted/expr, sql_insert/update/delete
  - select_exclude, column_as, with_tenant_id, cache_ttl, without_soft_delete
  - lock_shared, insert_or_ignore, clone_for_count, build_force_delete
  - having 全变体（CountStar/Sum/Avg/Max/Min x Gt/Ge/Lt/Eq/Ne/Le）
- qb_coverage_test.rs: 34 tests（commit `bf295cb4`）
  - all_columns, into_parts, paginate, with_cte/recursive_cte, window_function
  - row_number/rank/dense_rank, join_param/join_on, or_where_*, union
- schema_sync_test.rs: 15 tests（commit `576ea170`）
  - has_destructive_changes, diff_against, is_empty, get_column

## 4. 覆盖率缺口 TOP 10（M17c 实测）

| 文件 | 总行数 | 未覆盖 | 覆盖率 | 说明 |
|------|--------|--------|--------|------|
| sz-orm-sqlx/src/any.rs | 3245 | 2065 | 36.36% | 需 SQLite/Oracle/MSSQL 真实 DB |
| sz-orm-core/src/query.rs | 6137 | 1355 | 77.92% | M16 已补 96 tests |
| sz-orm-mssql/src/lib.rs | 1785 | 1151 | 35.52% | 需真实 MSSQL DB |
| sz-orm-core/src/dialect.rs | 5573 | 738 | 86.76% | M12 已补 38 tests |
| sz-orm-core/src/l2_cache.rs | 2766 | 672 | 75.70% | M15 已补 25 tests |
| sz-orm-core/src/pool.rs | 3368 | 603 | 82.10% | M15 已补 14 tests |
| sz-orm-query-builder/src/lib.rs | 4485 | 495 | 88.96% | M16 已补 34 tests |
| sz-orm-oracle/src/lib.rs | 2083 | 463 | 77.77% | M17 Oracle 集成测试已大幅提升 |
| sz-orm-core/src/model.rs | 1898 | 337 | 82.24% | 方法已全覆盖，分支未覆盖 |
| sz-orm-core/src/accessors.rs | 1274 | 332 | 73.94% | 访问器分支未覆盖 |

## 5. 未达 95% 原因分析

1. **any.rs（2065 行未覆盖）**：MySQL/PG/Oracle 集成测试已运行，但 SQLite/MSSQL 后端实现仍需对应 DB
2. **mssql/lib.rs（1151 行未覆盖）**：MSSQL Connection/Pool 需要真实 MSSQL DB（腾讯云 MSSQL 已知但未接入覆盖率采集）
3. **query.rs（1355 行未覆盖）**：复杂 SQL 构造分支（CTE/窗口函数/递归等）部分已由 M16 覆盖
4. **l2_cache.rs/pool.rs（1275 行未覆盖）**：异步并发路径需要 tokio 运行时和真实连接
5. **dialect.rs（738 行未覆盖）**：多数据库方言差异分支

> **结论**：TOP 2 缺口（any.rs + mssql = 3216 行）需要真实 DB 连接，纯单元测试无法覆盖。
> 要达到 95% 需覆盖额外 ~13,455 行，需大量集成测试 + 稳定的 DB 环境（MSSQL + SQLite）。

## 6. Git 提交记录

| Commit | 描述 | 测试数 |
|--------|------|--------|
| `ff20f975` | M1 any.rs MySQL/PG 深度覆盖 | 61 |
| `911b3137` | M2 query.rs 深度覆盖 | 38 |
| `2bd16299` | M3 dialect.rs 方言差异覆盖 | 60 |
| `6a38fd18` | M4~M8 oracle/mssql/pool/l2_cache/model/value/qb | 234 |
| `07f1ed69` | M9 长尾模块纯单元测试补齐 | 227 |
| `ef43d88e` | M11 any_test 环境变量支持 | 0 |
| `17d10fb3` | M12 dialect.rs 高级分支覆盖 | 38 |
| `98ce2b53` | M13 query.rs where 条件变体覆盖 | 26 |
| `c88615fc` | M14 query.rs 参数化方法覆盖 | 36 |
| `6f92facc` | M15 l2_cache/pool 高级分支覆盖 | 39 |
| `57f6f63a` | M16 query.rs 未覆盖方法补齐 | 96 |
| `bf295cb4` | M16 query-builder 未覆盖方法补齐 | 34 |
| `576ea170` | M16 schema_sync 未覆盖方法补齐 | 15 |
| **总计** | | **829** |

## 7. Oracle 集成测试状态

- **服务器 Docker**：Oracle XE 21c（ghcr.io/gvenzl/oracle-xe:21），端口 1521，PDB XEPDB1
- **测试用户**：sz_orm_test / SzOrmTest2026（APP_USER 自动创建）
- **连接字符串**：`SZ_ORM_ORACLE_CONNECT_STRING=127.0.0.1:1521/XEPDB1`
- **测试结果**：26 passed（22 connection + 4 pool），0 failed
- **测试文件修改**：oracle_connection_test/oracle_pool_test/integration_test 添加 `SZ_ORM_ORACLE_*` 环境变量支持
- **M17c 覆盖率采集成功**：用 `--include-ignored` 替代 `--ignored`，Oracle lib.rs 从 46.00% 提升到 77.77%（减少 662 行未覆盖）
- **本机**：Oracle 23ai Free 127.0.0.1:1521/freepdb1.FALSE，26 tests passed

## 8. Oracle 包覆盖率提升明细（M17c vs M16）

| 文件 | M16 覆盖率 | M17c 覆盖率 | 变化 |
|------|-----------|------------|------|
| oracle/src/lib.rs | 46.00% | **77.77%** | +31.77% |
| oracle/src/bulk_operations.rs | — | 96.17% | — |
| oracle/src/cursor_manager.rs | — | 95.71% | — |
| oracle/src/pool_config.rs | — | 83.15% | — |
| oracle/src/stored_procedure.rs | — | 97.12% | — |
| oracle/src/transaction_isolation.rs | — | 94.26% | — |
| oracle/src/type_mapping.rs | — | 91.44% | — |