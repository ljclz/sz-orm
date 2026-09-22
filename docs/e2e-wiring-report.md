# sz-orm v8.3.0 生产接线验证报告

> 日期：2026-09-22 | 版本：v8.3.0
> 验证范围：13 项功能 × 3 数据库 = 39 格覆盖矩阵

## 1. 生产入口可达性

| # | 功能 | 生产入口 | file:line 证据 | e2e 测试 |
|---|------|----------|---------------|----------|
| 1 | CRUD | QueryBuilder INSERT/SELECT/UPDATE/DELETE | `packages/sz-orm-core/src/query.rs:804` (where_eq) | test_e2e_{mysql,pg,oracle}_crud_full_lifecycle |
| 2 | 连接池 | Pool::acquire/release | `packages/sz-orm-core/src/pool.rs:1607` (acquire) | test_e2e_{mysql,pg,oracle}_pool_acquire_release |
| 3 | 事务 | Transaction::begin/commit/rollback | `packages/sz-orm-core/src/transaction.rs:540` (begin) | test_e2e_{mysql,pg,oracle}_tx_commit/rollback |
| 4 | 迁移 | Migrator::run | `packages/sz-orm-core/src/migration.rs:588` (rollback) | test_e2e_{mysql,pg,oracle}_migration_create_table |
| 5 | 查询构建 | where_eq/order_by/limit/join_inner | `packages/sz-orm-core/src/query.rs:804` / `:1003` / `:1176` / `:1338` | test_e2e_{mysql,pg,oracle}_query_where/join/aggregate |
| 6 | 多租户 | TenantContext | `packages/sz-orm-core/src/tenant.rs` | test_e2e_{mysql,pg}_tenant_isolation |
| 7 | 缓存 | Cache::get/set/invalidate | `packages/sz-orm-core/src/cache.rs` | test_e2e_{mysql,pg}_cache_hit/invalidation |
| 8 | 软删除 | SoftDelete (deleted_at) | `packages/sz-orm-core/src/model.rs` | test_e2e_{mysql,pg,oracle}_soft_delete_full |
| 9 | 分页 | LIMIT/OFFSET + keyset | `packages/sz-orm-core/src/query.rs:1176` (limit) | test_e2e_{mysql,pg,oracle}_pagination |
| 10 | 预加载 | eager_load (JOIN) | `packages/sz-orm-core/src/query.rs:1338` (join_inner) | test_e2e_{mysql,pg}_eager_load |
| 11 | 方言行为 | get_dialect | `packages/sz-orm-core/src/dialect.rs:2610` | test_e2e_{mysql,pg,oracle}_dialect_upsert/identifier |
| 12 | 批量操作 | batch_insert/batch_update | `packages/sz-orm-core/src/query.rs` (batch) | test_e2e_{mysql,pg}_crud_batch_insert/update |
| 13 | RETURNING | RETURNING 子句 | `packages/sz-orm-core/src/query.rs` | test_e2e_pg_crud_returning (MySQL N/A) |

## 2. 接线路径完整性

每项功能从生产入口到数据库执行的完整路径：

```
用户代码 → QueryBuilder API → SQL 生成 → Dialect 适配 → Connection::execute → 数据库 → 结果映射 → 用户
```

- **CRUD**: QueryBuilder.build() → dialect.compile() → Connection.execute() → ResultSet
- **连接池**: Pool.acquire() → ArrayQueue.pop() → Notify.wake() → PooledConnection
- **事务**: TransactionManager.begin() → Connection.begin_transaction() → DB BEGIN
- **迁移**: Migrator.run() → VersionTracker → DDL 执行 → 版本记录
- **查询构建**: where_eq/order_by/limit → SQL 拼接 → 参数绑定 → execute
- **多租户**: TenantContext → SchemaIsolationRouter → 表名重写 → execute
- **缓存**: Cache.get() → L1 LRU → L2 Redis → DB fallback → cache set
- **软删除**: SoftDelete trait → deleted_at IS NULL 过滤 → UPDATE deleted_at
- **分页**: limit/offset → SQL LIMIT/OFFSET → 结果切片
- **预加载**: join_inner → SQL JOIN → 结果关联
- **方言行为**: get_dialect → Dialect.compile() → 方言特定 SQL
- **批量操作**: batch_insert → 多值 INSERT → execute
- **RETURNING**: RETURNING 子句 → 结果返回

## 3. 端到端测试覆盖矩阵

| 功能 | MySQL | PostgreSQL | Oracle |
|------|-------|------------|--------|
| CRUD | PASSED | PASSED | PASSED |
| 连接池 | PASSED | PASSED | PASSED |
| 事务 | PASSED | PASSED | PASSED |
| 迁移 | PASSED | PASSED | PASSED |
| 查询构建 | PASSED | PASSED | PASSED |
| 多租户 | PASSED | PASSED | SKIPPED |
| 缓存 | PASSED | PASSED | SKIPPED |
| 软删除 | PASSED | PASSED | PASSED |
| 分页 | PASSED | PASSED | PASSED |
| 预加载 | PASSED | PASSED | SKIPPED |
| 方言行为 | PASSED | PASSED | PASSED |
| 批量操作 | PASSED | PASSED | SKIPPED |
| RETURNING | N/A | PASSED | SKIPPED |

**统计**: 总 39 格 / 已覆盖 33 格 / N/A 1 格 / 跳过 5 格

## 4. 幻影交付扫描

- 门禁 15（幻影交付检查）：`python scripts/check-phantom-delivery.py` 退出码 0
- 每项宣称的能力附生产调用点证据（file:line）+ e2e 测试
- 零幻影交付

## 5. 测试环境

- MySQL: 121.204.253.75:8802（3 库 test/shop/njszjt）
- PostgreSQL: 121.204.253.75:5432（lewuli）
- Oracle: 127.0.0.1:1521/freepdb1（sys/Sysdba）
- 禁止使用 122.51.216.76（第一台服务器）
- 禁止使用本机 MySQL/PostgreSQL