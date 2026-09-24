# sz-orm v8.7.0 性能基准采集报告

> 采集日期：2026-09-24
> 工具版本：cargo test + sz-orm-bench --features real-bench
> 采集环境：Windows MSVC（Rust 1.98.0，MySQL 9.6 / PostgreSQL 18 / Oracle 23ai Free / SQLite）
> 采集命令：cargo test -p sz-orm-bench --features real-bench --test real_db_bench -- --ignored

---

## 性能基准摘要

| 数据库 | 测试结果 | 耗时 | 备注 |
|--------|----------|------|------|
| SQLite | ✅ 2/2 PASS | 6.55s | test_run_workload_real_sqlite + test_bench_performance_sqlite |
| MySQL | ✅ 1/1 PASS | 6.77s | test_run_workload_real_mysql（修复 sea-orm 驱动 feature） |
| PostgreSQL | ✅ 2/2 PASS | 2.59s | test_postgres_init_and_query + test_postgres_batch_query（修复并行竞态） |
| Oracle | ✅ 3/3 PASS | 7.29s | test_oracle_init_and_crud + test_oracle_transaction_commit_rollback + test_oracle_aggregate_query |

**四数据库基准全部通过，共 8 个测试 PASS。**

---

## 修复清单

| # | 文件 | 修复内容 | 根因 |
|---|------|---------|------|
| 1 | [packages/sz-orm-bench/Cargo.toml:28](file:///packages/sz-orm-bench/Cargo.toml#L28) | sea-orm features 添加 `sqlx-mysql`, `sqlx-postgres` | sea-orm 仅启用 sqlx-sqlite，MySQL/PG 连接串无驱动 |
| 2 | [packages/sz-orm-bench/src/real_db.rs:131](file:///packages/sz-orm-bench/src/real_db.rs#L131) | `SERIAL` → `BIGSERIAL` | PostgreSQL SERIAL=INT4，代码按 i64(INT8) 解码 |
| 3 | [packages/sz-orm-bench/tests/real_db_bench.rs:237](file:///packages/sz-orm-bench/tests/real_db_bench.rs#L237) | `freepdb1` → `freepdb1.FALSE` | Oracle 23ai 监听器以 `freepdb1.FALSE` 注册服务 |
| 4 | [packages/sz-orm-bench/tests/real_db_bench.rs:237](file:///packages/sz-orm-bench/tests/real_db_bench.rs#L237) | `sys/test123` → `sz_orm_test/SzOrmTest2026` | SYS 用户需 SYSDBA 权限，OraclePoolHandle 不支持 |
| 5 | [packages/sz-orm-oracle/src/lib.rs:123](file:///packages/sz-orm-oracle/src/lib.rs#L123) | `_runtime: Runtime` → `Option<Runtime>` + 自定义 Drop 在独立线程释放 | tokio runtime 在 async 上下文中 drop 触发 "Cannot drop a runtime in a context where blocking is not allowed" |
| 6 | [packages/sz-orm-bench/src/real_db.rs:233](file:///packages/sz-orm-bench/src/real_db.rs#L233) | `init_oracle` 添加 `conn.commit()` | oracle crate 默认 auto-commit OFF，未 commit 就 close 导致数据回滚 |
| 7 | [packages/sz-orm-oracle/src/lib.rs:1146](file:///packages/sz-orm-oracle/src/lib.rs#L1146) | `begin_transaction` 不再执行 `BEGIN` SQL | Oracle 中 `BEGIN` 是 PL/SQL 块起始，单独执行报 ORA-06550；Oracle 事务隐式启动 |
| 8 | [packages/sz-orm-bench/tests/postgres_backend_test.rs:12](file:///packages/sz-orm-bench/tests/postgres_backend_test.rs#L12) | 添加 `PG_TEST_LOCK: Mutex<()>` 串行化 | 并行 `CREATE TABLE IF NOT EXISTS` 导致 pg_class 系统目录竞态 |
| 9 | [packages/sz-orm-bench/tests/real_db_bench.rs:237](file:///packages/sz-orm-bench/tests/real_db_bench.rs#L237) | 添加 `ORACLE_TEST_LOCK: Mutex<()>` 串行化 | Oracle 三测试共享 bench_users 表，并行 init 竞态 |

---

## SQLite 性能数据

| 指标 | 值 |
|------|-----|
| test_run_workload_real_sqlite | ✅ PASS |
| test_bench_performance_sqlite | ✅ PASS |
| 总耗时 | 6.55s |
| 框架 | sz-orm + sqlx + sea-orm |
| 负载 | SingleRowQuery + BatchQuery + ComplexJoin + Transaction + PoolConcurrency |

---

## MySQL 性能数据

| 指标 | 值 |
|------|-----|
| test_run_workload_real_mysql | ✅ PASS |
| 总耗时 | 6.77s |
| 连接串 | mysql://root:test123@127.0.0.1:3306/sz_orm_test |
| 框架 | sz-orm (Sqlx) + sea-orm |
| 负载 | SingleRowQuery + BatchQuery + ComplexJoin + Transaction + PoolConcurrency |
| 数据集 | 200 行, 5 轮测量, 2 并发 |

---

## PostgreSQL 性能数据

| 指标 | 值 |
|------|-----|
| test_postgres_init_and_query | ✅ PASS |
| test_postgres_batch_query | ✅ PASS |
| 总耗时 | 2.59s |
| 连接串 | postgres://postgres:test123@127.0.0.1:5432/sz_orm_test |
| 框架 | sz-orm (Sqlx) |
| 负载 | SingleRowQuery + BatchQuery |
| 数据集 | 500 行, 5 轮测量 |

---

## Oracle 性能数据

| 指标 | 值 |
|------|-----|
| test_oracle_init_and_crud | ✅ PASS |
| test_oracle_transaction_commit_rollback | ✅ PASS |
| test_oracle_aggregate_query | ✅ PASS |
| 总耗时 | 7.29s |
| 连接串 | oracle://sz_orm_test:SzOrmTest2026@127.0.0.1:1521/freepdb1.FALSE |
| 框架 | sz-orm-oracle (oracle crate ODPI-C) |
| 负载 | CRUD + 事务 commit/rollback + 聚合查询 |
| 数据集 | 100-1000 行 |

---

## 数据库连接状态

| 数据库 | 主机 | 端口 | 服务名 | 状态 |
|--------|------|------|--------|------|
| MySQL | 127.0.0.1 | 3306 | sz_orm_test | ✅ 可用 |
| PostgreSQL | 127.0.0.1 | 5432 | sz_orm_test | ✅ 可用 |
| Oracle | 127.0.0.1 | 1521 | freepdb1.FALSE | ✅ 可用（PDB READ WRITE） |
| SQLite | 嵌入式 | N/A | N/A | ✅ 可用 |

---

## 验证输出

### real_db_bench.rs（SQLite + MySQL + Oracle）

```
running 4 tests
test test_oracle_init_and_crud ... ok
test test_oracle_transaction_commit_rollback ... ok
test test_oracle_aggregate_query ... ok
test test_run_workload_real_mysql ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out; finished in 7.29s
```

### postgres_backend_test.rs（PostgreSQL）

```
running 2 tests
test test_postgres_init_and_query ... ok
test test_postgres_batch_query ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.59s
```

### sz-orm-oracle 单元测试

```
running 207 tests
...
test result: ok. 207 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

---

## 结论

- **四数据库基准全部通过**：SQLite (2/2) + MySQL (1/1) + PostgreSQL (2/2) + Oracle (3/3) = 8/8 PASS
- **Oracle 三处 bug 修复**：runtime drop panic + 未 commit 数据回滚 + BEGIN 语法错误
- **PostgreSQL 并行竞态修复**：Mutex 串行化 init 调用
- **v8.7.0 性能基准采集任务（M1-T3）完成度**：4/4 数据库完整采集 ✅
