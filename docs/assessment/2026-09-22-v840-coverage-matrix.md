# sz-orm v8.4.0 端到端测试覆盖矩阵（13×3=39 格）

> 日期：2026-09-22 | 版本：v8.4.0
> v8.3.0 基线：33 已覆盖 / 1 N/A / 5 跳过
> v8.4.0 结果：38 已覆盖 / 1 N/A / 0 跳过（Oracle 5 格缺口已全部补齐）

## 覆盖矩阵

| 功能 | MySQL | PostgreSQL | Oracle |
|------|-------|------------|--------|
| CRUD | ✅ test_e2e_mysql_crud_full_lifecycle_test_db | ✅ test_e2e_pg_crud_full_lifecycle | ✅ test_e2e_oracle_crud_full_lifecycle |
| 连接池 | ✅ test_e2e_mysql_pool_acquire_release | ✅ test_e2e_pg_pool_concurrent | ✅ test_e2e_oracle_pool_exhaustion |
| 事务 | ✅ test_e2e_mysql_tx_commit | ✅ test_e2e_pg_tx_commit | ✅ test_e2e_oracle_tx_commit |
| 迁移 | ✅ test_e2e_mysql_migration_create_table | ✅ test_e2e_pg_migration_alter_table | ✅ test_e2e_oracle_migration_create_table |
| 查询构建 | ✅ test_e2e_mysql_query_where_conditions | ✅ test_e2e_pg_query_where_conditions | ✅ test_e2e_oracle_query_where_conditions |
| 多租户 | ✅ test_e2e_mysql_tenant_isolation | ✅ test_e2e_pg_tenant_isolation | ✅ test_e2e_oracle_tenant_isolation（v8.4.0 新增） |
| 缓存 | ✅ test_e2e_mysql_cache_hit | ✅ test_e2e_pg_cache_invalidation | ✅ test_e2e_oracle_cache_hit（v8.4.0 新增） |
| 软删除 | ✅ test_e2e_mysql_soft_delete_full | ✅ test_e2e_pg_soft_delete_full | ✅ test_e2e_oracle_soft_delete_full |
| 分页 | ✅ test_e2e_mysql_pagination_limit_offset | ✅ test_e2e_pg_pagination_keyset | ✅ test_e2e_oracle_pagination_offset_fetch |
| 预加载 | ✅ test_e2e_mysql_eager_load | ✅ test_e2e_pg_eager_load | ✅ test_e2e_oracle_eager_load（v8.4.0 新增） |
| 方言行为 | ✅ test_e2e_mysql_dialect_upsert | ✅ test_e2e_pg_dialect_upsert | ✅ test_e2e_oracle_dialect_identifier |
| 批量操作 | ✅ test_e2e_mysql_crud_batch_insert | ✅ test_e2e_pg_crud_batch_insert | ✅ test_e2e_oracle_crud_batch_insert（v8.4.0 新增） |
| RETURNING | N/A（MySQL 不支持） | ✅ test_e2e_pg_crud_returning | ✅ test_e2e_oracle_crud_returning（v8.4.0 新增） |

## 统计

- **总格子数**：39
- **已覆盖**：38
- **N/A**：1（MySQL RETURNING，MySQL 不支持 RETURNING 子句）
- **跳过**：0

## v8.4.0 新增 Oracle 补齐测试

| 格 | 测试函数 | 文件 | 通过状态 |
|----|----------|------|----------|
| (多租户, Oracle) | test_e2e_oracle_tenant_isolation | `packages/sz-orm-core/tests/e2e_real_db_multi_tenant.rs` | ✅ passed |
| (缓存, Oracle) | test_e2e_oracle_cache_hit | `packages/sz-orm-core/tests/e2e_real_db_cache.rs` | ✅ passed |
| (预加载, Oracle) | test_e2e_oracle_eager_load | `packages/sz-orm-core/tests/e2e_real_db_eager_load.rs` | ✅ passed |
| (批量操作, Oracle) | test_e2e_oracle_crud_batch_insert | `packages/sz-orm-core/tests/e2e_real_db_crud.rs` | ✅ passed |
| (RETURNING, Oracle) | test_e2e_oracle_crud_returning | `packages/sz-orm-core/tests/e2e_real_db_crud.rs` | ✅ passed |

## 与 v8.3.0 对比

| 维度 | v8.3.0 | v8.4.0 | 变化 |
|------|--------|--------|------|
| 已覆盖 | 33 | 38 | +5 |
| N/A | 1 | 1 | 0 |
| 跳过 | 5 | 0 | -5 |
| 总格 | 39 | 39 | 0 |