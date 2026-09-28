# v9.2.0 覆盖率战役报告

> 采集日期：2026-09-28
> 采集环境：Linux 服务器（121.204.253.75），CentOS Stream 9，40 核 62GB
> 工具：cargo-llvm-cov 0.9.1
> Rust：1.98.1

## 1. 总览

| 维度 | v9.0.0 基线 | v9.2.0 实测 | 变化 |
|------|------------|------------|------|
| 行覆盖率 | 90.2% | **90.28%** | +0.08% |
| 分支覆盖率 | 100%（审计声明） | **88.45%** | — |
| 函数覆盖率 | — | **89.48%** | — |
| 未覆盖行 | ~30,000 | **29,767** | -233 |
| 新增测试 | 0 | **620** | +620 |

## 2. 采集命令

```bash
# 纯单元测试覆盖率（最终结果）
cd /www/rust/sz-orm-v910
CARGO_INCREMENTAL=0 RUST_MIN_STACK=33554432 \
CARGO_TARGET_DIR=/www/rust/cov-target6 \
cargo llvm-cov --workspace \
  --exclude sz-orm-python --exclude sz-orm-js --exclude sz-orm-wasm \
  --exclude sz-orm-cabi --exclude sz-orm-go --exclude sz-orm-java \
  --exclude sz-orm-cpp --exclude sz-orm-n1-lint \
  --summary-only
```

## 3. 新增测试明细

### M1: any.rs MySQL/PG 深度覆盖（61 测试，commit `ff20f975`）
- `packages/sz-orm-sqlx/tests/any_mysql_test.rs` — 20 tests（#[ignore]）
- `packages/sz-orm-sqlx/tests/any_pg_test.rs` — 41 tests（#[ignore]）

### M2: query.rs 复杂分支覆盖（38 测试，commit `911b3137`）
- query_join_test.rs(3) / query_having_test.rs(1) / query_keyset_test.rs(3)
- query_build_test.rs(20) / query_lock_test.rs(2) / query_aggregate_test.rs(6) / query_batch_test.rs(3)

### M3: dialect.rs 方言差异覆盖（60 测试，commit `2bd16299`）
- dialect_mysql_test.rs(13) / dialect_pg_test.rs(10) / dialect_sqlite_test.rs(6)
- dialect_oracle_test.rs(8) / dialect_mssql_test.rs(8) / dialect_exotic_test.rs(15)

### M4+M5: oracle/mssql 驱动覆盖（86 测试，commit `6a38fd18`）
- Oracle: oracle_conn_info_test.rs(4) / oracle_dialect_test.rs(18) / oracle_connection_test.rs(22,#[ignore]) / oracle_pool_test.rs(4,#[ignore])
- MSSQL: mssql_conn_info_test.rs(3) / mssql_dialect_test.rs(12) / mssql_connection_test.rs(20,#[ignore]) / mssql_pool_test.rs(3,#[ignore])

### M6+M7: pool/l2_cache 覆盖（49 测试，commit `6a38fd18`）
- Pool: 8 文件 25 tests
- L2_cache: 7 文件 24 tests

### M8: model+value+query-builder 覆盖（99 测试，commit `6a38fd18`）
- Model: 5 文件 18 tests
- Value: 6 文件 47 tests
- Query-builder: 7 文件 34 tests

### M9: 长尾模块纯单元测试补齐（227 测试，commit `07f1ed69`）
- schema_sync_test.rs: 32 tests（diff/DDL 生成器/SchemaSync）
- dynamic_sql_test.rs: 30 tests（DynamicSqlParser/SqlParams/标签）
- result_map_test.rs: 37 tests（ResultMap/Registry/apply_result_map）
- error_test.rs: 31 tests（DbError 变体/with_context/error_code）
- migration_test.rs: 27 tests（SchemaBuilder/ColumnDef/IndexDef/ForeignKeyDef）
- tenant_quota_rls_test.rs: 33 tests（feature-gated, QuotaEnforcer/QuotaResource）
- smart_eager_loader_test.rs: 15 tests（LoadStrategy/StrategyResolver）
- endpoint_test.rs: 5 tests（HealthEndpointConfig）
- log_pipeline_test.rs: 17 tests（LogRecord/过滤器）

## 4. 覆盖率缺口 TOP 10

| 文件 | 总行数 | 未覆盖 | 覆盖率 |
|------|--------|--------|--------|
| sz-orm-sqlx/src/any.rs | 3245 | 2518 | 22.40% |
| sz-orm-core/src/query.rs | 6137 | 1441 | 76.52% |
| sz-orm-core/src/dialect.rs | 5573 | 1199 | 78.49% |
| sz-orm-oracle/src/lib.rs | 2083 | 1164 | 44.12% |
| sz-orm-mssql/src/lib.rs | 1785 | 890 | 50.14% |
| sz-orm-core/src/pool.rs | 3368 | 636 | 81.12% |
| sz-orm-core/src/l2_cache.rs | 2766 | 613 | 77.84% |
| sz-orm-query-builder/src/lib.rs | 4485 | 511 | 88.61% |
| sz-orm-core/src/schema_sync.rs | 1203 | 301 | 74.98% |
| sz-orm-core/src/model.rs | 1898 | 337 | 82.24% |

## 5. 未达 95% 原因分析

1. **any.rs（2518 行未覆盖）**：MySQL/PG 实现需要真实 DB 连接，`#[ignore]` 测试在纯单元覆盖率采集中不运行
2. **oracle/lib.rs（1164 行未覆盖）**：Oracle Connection/Pool 需要真实 Oracle DB
3. **mssql/lib.rs（890 行未覆盖）**：MSSQL Connection/Pool 需要真实 MSSQL DB
4. **query.rs/dialect.rs（2640 行未覆盖）**：分支覆盖不足，部分分支需要复杂 SQL 构造
5. **pool.rs/l2_cache.rs（1249 行未覆盖）**：异步并发路径需要 tokio 运行时和真实连接

## 6. Git 提交记录

| Commit | 描述 | 测试数 |
|--------|------|--------|
| `ff20f975` | M1 any.rs MySQL/PG 深度覆盖 | 61 |
| `911b3137` | M2 query.rs 深度覆盖 | 38 |
| `2bd16299` | M3 dialect.rs 方言差异覆盖 | 60 |
| `6a38fd18` | M4~M8 oracle/mssql/pool/l2_cache/model/value/qb | 234 |
| `07f1ed69` | M9 长尾模块纯单元测试补齐 | 227 |
| **总计** | | **620** |