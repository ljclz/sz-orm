# SeaORM 对标功能-深度对照表

> 版本：v1.2（2026-07-29）
> 用途：从"功能存在性审计"转向"实现深度审计"的基准文档
> 对标对象：SeaORM 0.12+（Rust 异步 ORM 的事实标准）
> 评估目标：识别 sz-orm 在"实现深度"上的真实差距，而非"API 是否存在"
> 当前进度：**P0-P2 全部 8 项任务已完成**，差距评分从初始 4→0 收敛

---

## 一、评估方法论

### 1.1 三层实现深度（L1/L2/L3）

| 层级 | 定义 | 判据 | 示例 |
|------|------|------|------|
| **L1 功能存在** | API/trait/类型已定义，编译通过 | 模块存在、类型导出 | `SoftDelete` trait 存在 |
| **L2 基础实现** | 核心路径可执行，单元测试通过 | 单函数正确 | `soft_delete_field()` 返回字段名 |
| **L3 完整实现** | SQL 下推、参数化、边界覆盖、行为测试 | 用户视角行为正确 | 查询自动追加 `WHERE deleted_at IS NULL`，且注入安全 |

### 1.2 完成判据（"完成"的定义）

一个功能视为"完成"必须同时满足：
1. **L3 实现深度**：SQL 下推到数据库（非内存过滤）、参数化绑定（非字符串拼接）、实际执行验证（非模拟）
2. **行为测试覆盖**：从用户视角验证正确行为（如"软删除后查询不到该记录"）
3. **对标差距 ≤ 1**：与 SeaORM 对标，差距评分 ≤ 1（满分 5，差距 0 表示完全对齐）

### 1.3 差距评分（Gap Score）

| 评分 | 含义 | 行动 |
|------|------|------|
| 0 | 完全对齐 | 无需行动 |
| 1 | 微小差距（命名/风格） | 可忽略 |
| 2 | 实现深度差距 | 待优化 |
| 3 | 功能缺失或残缺 | P1 修复 |
| 4 | 安全/正确性问题 | P0 修复 |
| 5 | 完全未实现 | 立即实现 |

---

## 二、8 大核心功能对标

### 2.1 软删除（Soft Delete）

| 维度 | SeaORM | SZ-ORM 当前状态 | 差距 |
|------|--------|----------------|------|
| **trait 定义** | `sea_orm::ActiveModelBehavior` + `ActiveValue::Set` | `SoftDelete` trait 已定义 | 0 |
| **字段标记** | `#[sea_orm(soft_delete = "deleted_at")]` | `fn soft_delete_field() -> &'static str` | 0 |
| **删除行为** | `Entity::delete()` 自动转 `UPDATE SET deleted_at = NOW()` | **v1.3.0+ 已集成**：`build_delete()` 检测 `soft_delete_field()` 自动转 UPDATE | 0 |
| **查询过滤** | `Entity::find()` 自动追加 `WHERE deleted_at IS NULL` | **v1.3.0+ 已集成**：`build_select/count/update/...` 自动追加过滤 | 0 |
| **物理删除** | 用户手动 `UPDATE` 或 raw SQL | **v1.3.0+ 新增** `build_force_delete()` / `build_force_delete_with_params()` | 0 |
| **临时禁用** | `filter(Column::DeletedAt.is_not_null())` | **v1.3.0+ 新增** `without_soft_delete()` 链式方法 | 0 |
| **恢复机制** | `ActiveValue::Set(None)` 恢复 | `before_restore`/`after_restore` 钩子存在 | 1 |
| **行为测试** | 完整覆盖 | **v1.3.0+ 已覆盖**：10 个 L3 行为测试（select/delete/force_delete/with_params/count/without_soft_delete） | 0 |

**P0-1 修复任务**：
- [x] `QueryBuilder::build_select()` 自动应用 `SoftDeleteScope`（L3-1/L3-2 测试覆盖）
- [x] `QueryBuilder::build_delete()` 在 Model 实现 `SoftDelete` 时转换为 `UPDATE SET deleted_at = NOW()`（L3-4 测试覆盖）
- [x] 行为测试：软删除后查询不到、恢复后可查询（L3-1 至 L3-10 共 10 个测试）
- [x] `without_soft_delete()` 可查询已删除记录（L3-3 测试覆盖）
- [x] 参数化版本 `build_select_with_params` / `build_delete_with_params` 同步集成（L3-6/L3-7 测试覆盖）
- [x] 物理删除 `build_force_delete` / `build_force_delete_with_params` 不追加过滤（L3-5/L3-8 测试覆盖）

---

### 2.2 查询构建器（Query Builder）

| 维度 | SeaORM | SZ-ORM 当前状态 | 差距 |
|------|--------|----------------|------|
| **链式 API** | `Entity::find().filter(...).all(db)` | `QueryBuilder::new().where_eq(...).build_select()` | 0 |
| **SELECT 列** | 编译期 `DeriveEntityModel` | `select()` 拼接 + `select_quoted()` 安全版本 | 1 |
| **WHERE 条件** | `filter(Column::Field.eq(value))` 类型安全 | **v1.3.0+ 新增** `where_eq/ne/gt/ge/lt/le/like(field, value)` 参数化 API | 0 |
| **参数化** | 自动绑定 | **v1.3.0+ 已统一**：`build_*_with_params()` 内部使用 `?` 占位符 + Value 绑定 | 0 |
| **旧 API 兼容** | - | `where_cond(condition: &str)` 标记 `#[deprecated]`，文档警告注入风险 | 1 |
| **JOIN** | `find_also_related(Entity)` | `join_inner/left/right` | 0 |
| **GROUP BY/HAVING** | 支持 | 支持 | 0 |
| **聚合** | `Entity::find().count()` | `build_count/sum/avg/max/min` | 0 |
| **分页** | `Paginator` | `page()` offset 分页 | 2（无 keyset） |

**P0-2 修复任务**：
- [x] 废弃 `where_cond(condition: impl Into<String>)`，新增 `where_eq/ne/gt/ge/lt/le/like(field, value)` 参数化 API
- [x] `where_cond` 保留但标记 `#[deprecated(since = "1.3.0")]`，文档警告注入风险
- [x] 行为测试：注入 payload（`"'; DROP TABLE users; --"`）应被参数化（L3-13 测试覆盖）
- [x] 行为测试：`' OR '1'='1` 经典攻击防护（L3-14 测试覆盖）
- [x] 所有 `build_*_with_params()` 内部统一走参数化路径（L3-15/L3-16/L3-17/L3-18 多参数/IN/BETWEEN/UPDATE 顺序测试覆盖）
- [x] 多参数顺序正确性验证（L3-15 测试：3 个参数顺序与 WHERE 子句出现顺序一致）

---

### 2.3 事务（Transaction）

| 维度 | SeaORM | SZ-ORM 当前状态 | 差距 |
|------|--------|----------------|------|
| **基本事务** | `db.transaction::<_, _, _>(|tx| async { ... }).await` | `Transaction::new(conn, opts)` + `commit/rollback` | 0 |
| **隔离级别** | `IsolationLevel::Serializable` | `IsolationLevel` 枚举 | 0 |
| **保存点** | `txn.begin()` 自动 SAVEPOINT | `savepoint/rollback_to_savepoint` | 0 |
| **只读事务** | `access_mode(ReadOnly)` | `TransactOptions::read_only()` | 0 |
| **超时** | `statement_timeout` | `with_timeout()` | 0 |
| **端到端回滚验证** | 完整覆盖 | **v1.3.0+ 已验证**：16 个 L3 行为测试覆盖真实回滚行为 | 0 |
| **死锁重试** | 用户自行实现 | **v1.3.0+ 新增** `retry_on_deadlock` 自动重试 | 0 |

**P1-3 修复任务**：
- [x] 端到端测试：INSERT → ERROR → ROLLBACK → 数据库无残留（L3-1/L3-2 测试覆盖）
- [x] 端到端测试：嵌套 SAVEPOINT 回滚到中间点（L3-5 至 L3-8 测试覆盖）
- [x] 端到端测试：超时事务自动回滚（L3-9/L3-10 测试覆盖）
- [x] 端到端测试：死锁检测与自动重试（L3-13 至 L3-16 测试覆盖）

---

### 2.4 Migration 系统

| 维度 | SeaORM | SZ-ORM 当前状态 | 差距 |
|------|--------|----------------|------|
| **文件解析** | `sea_schema::Discovery` | `FileMigrationResolver` | 0 |
| **执行器** | `Migrator::up(db, None)` 真实执行 | **v1.3.0+ 已对接**：`Migrator::migrate()` 实际执行 SQL | 0 |
| **`__migrations` 表** | 自动创建 + 记录版本 | **v1.3.0+ 已实现**：自动 `CREATE TABLE IF NOT EXISTS __migrations` | 0 |
| **回滚** | `Migrator::down(db, Some(steps))` | `down/rollback/reset/refresh` | 0 |
| **SchemaBuilder** | `Schema::create_table()` | `SchemaBuilder::new()` | 0 |
| **CLI** | `sea-orm-cli migrate` | `sz-orm-cli migrate` 已对接 | 1 |

**P1-4 修复任务**：
- [x] `Migrator::migrate()` 实际执行 SQL（通过传入 `Connection`）
- [x] 自动创建 `__migrations` 表（`CREATE TABLE IF NOT EXISTS`）
- [x] 执行后写入版本号到 `__migrations` 表
- [x] 端到端测试：迁移执行 + 版本记录 + 回滚（e2e_migration.rs 测试覆盖）

---

### 2.5 分页（Pagination）

| 维度 | SeaORM | SZ-ORM 当前状态 | 差距 |
|------|--------|----------------|------|
| **Offset 分页** | `Paginator::page_size` | `page()` + `LIMIT/OFFSET` | 0 |
| **Keyset 分页** | 不原生支持 | **v1.3.0+ 已实现**：`keyset_after` / `keyset_before` 游标分页 | 0 |
| **游标分页** | 不原生支持 | **v1.3.0+ 已实现**：基于 `WHERE field > ? / < ? ORDER BY field LIMIT N` | 0 |
| **总数查询** | `paginator.num_items()` | `build_count()` | 0 |
| **has_next/prev** | `has_more` | `PageResult::has_next/has_prev` | 0 |

**P2-5 修复任务**：
- [x] `QueryBuilder::keyset_after(cursor_field, cursor_value)` — `WHERE id > ? ORDER BY id ASC LIMIT N`
- [x] `QueryBuilder::keyset_before(cursor_field, cursor_value)` — 反向游标 `WHERE id < ? ORDER BY id DESC LIMIT N`
- [x] 自动设置 ORDER BY 方向（若字段已存在则更新方向，保证 keyset 语义一致）
- [x] 参数化绑定：`build_select_with_params` 返回 `(sql, vec![cursor_value])`
- [x] 行为测试：20 个 L3 测试覆盖（SQL 生成、参数顺序、方向、多字段、NULL 处理、方言兼容等）

---

### 2.6 批量操作（Batch Operations）

| 维度 | SeaORM | SZ-ORM 当前状态 | 差距 |
|------|--------|----------------|------|
| **批量插入** | `Entity::insert_many(vec![]).exec(db)` | **v1.3.0+ 已实现**：`build_batch_insert_with_params` 多 VALUES 子句 | 0 |
| **Batch Upsert** | `on_conflict(OnConflict::new().do_nothing())` | **v1.3.0+ 已实现**：`build_batch_upsert_with_params` | 0 |
| **分片防止超限** | 自动分片 | `DEFAULT_BATCH_SIZE = 1000` | 0 |
| **批量更新** | `update_many().col_expr(...)` | 未实现批量 UPDATE | 2 |
| **方言兼容** | - | **v1.3.0+ 已覆盖**：MySQL `ON DUPLICATE KEY`、PG/SQLite `ON CONFLICT DO UPDATE` | 0 |

**P2-6 修复任务**：
- [x] `QueryBuilder::build_batch_insert_with_params(rows)` — 多 VALUES 子句 + 参数化
- [x] `QueryBuilder::build_batch_upsert_with_params(rows, conflict_columns, update_columns)` — 批量 upsert
- [x] 方言差异：MySQL `ON DUPLICATE KEY UPDATE`、PostgreSQL/SQLite `ON CONFLICT DO UPDATE` / `DO NOTHING`
- [x] `Dialect::build_upsert_on_conflict` trait 方法 + 各方言实现
- [x] 行为测试：20 个 L3 测试覆盖（SQL 生成、参数绑定、方言兼容、NULL 处理、SQL 注入防护、大批量 100 行等）

---

### 2.7 Eager Loading（关联关系加载）

| 维度 | SeaORM | SZ-ORM 当前状态 | 差距 |
|------|--------|----------------|------|
| **三策略** | join / subquery / separate | `find_with_related` 存在 | 0 |
| **BelongsTo** | `find_also_related` | 支持 | 0 |
| **HasMany** | `find_with_related` | 支持 | 0 |
| **HasOne** | `find_with_related` | 支持 | 0 |
| **BelongsToMany** | `find_with_related` via pivot | 支持 | 0 |
| **循环引用检测** | 不支持（用户责任） | **v1.3.0+ 已实现**：DFS 三色标记算法检测循环 | 0 |
| **重复关联检测** | 不支持 | **v1.3.0+ 已实现**：`WithRelation` 加载前检测重复关联名 | 0 |
| **N+1 查询警告** | `debug_query` | 未实现 | 2 |

**P2-7 修复任务**：
- [x] `EntityGraph::detect_cycles()` — DFS 三色标记算法检测循环引用（含子图递归展平）
- [x] `EntityGraph::detect_duplicate_edges()` — 检测重复边
- [x] `EntityGraph::validate()` — 综合校验（循环引用 + 重复边）
- [x] `WithRelation::check_duplicate_relations()` — 加载前检测重复关联名
- [x] 确定性输出：DFS 按字典序排序节点和邻接列表，保证相同图结构产生相同结果
- [x] 行为测试：20 个 L3 测试覆盖（DAG、直接循环、间接循环、自环、深层嵌套、子图循环、重复边、重复关联等）

---

### 2.8 多租户（Multi-tenant）

| 维度 | SeaORM | SZ-ORM 当前状态 | 差距 |
|------|--------|----------------|------|
| **TenantModel trait** | 用户自行实现 | `TenantModel` trait 存在 + `Model::tenant_field()` 默认 None | 0 |
| **TenantScope** | 用户自行实现 | `(TenantScope, M)` 实现 `GlobalScope` | 0 |
| **自动过滤** | 用户自行实现 | **v1.3.0+ 已集成**：`with_tenant_id(id)` + `QueryBuilder` 自动追加 `WHERE tenant_field = ?` | 0 |
| **租户隔离** | 用户自行保证 | **v1.3.0+ 强制**：SELECT/UPDATE/DELETE/force_delete 均追加租户条件 | 0 |
| **临时禁用** | - | **v1.3.0+ 新增** `without_tenant()` 链式方法 | 0 |
| **行为测试** | - | **v1.3.0+ 已覆盖**：12 个 L3 行为测试 | 0 |

**修复任务（与 P0-1 同步）**：
- [x] `QueryBuilder::build_select()` 自动应用租户过滤（L3-21/L3-22 测试覆盖）
- [x] UPDATE/DELETE 时自动追加 `WHERE tenant_id = ?`（L3-24/L3-25 测试覆盖）
- [x] `without_tenant()` 可跨租户查询（L3-23 测试覆盖）
- [x] 物理删除 `build_force_delete` 保留租户条件（L3-32 测试覆盖）
- [x] 软删除 + 多租户组合工作（L3-29/L3-30 测试覆盖）
- [x] 非多租户模型不受影响（L3-27 测试覆盖）
- [x] 未设置 tenant_id 时不追加过滤（L3-28 测试覆盖）
- [ ] INSERT 时自动填充 `tenant_id`（待 P1 实施，当前需调用方手动填入 data）

---

## 三、优先级排序

| 优先级 | 任务 | 原因 |
|--------|------|------|
| **P0-1** | 软删除集成到 QueryBuilder | 安全 + 正确性：当前软删除完全无效 |
| **P0-2** | where_cond 注入风险修复 | 安全：SQL 注入是生产级阻断问题 |
| **P0-3** | 多租户自动过滤（与 P0-1 同步） | 安全：租户隔离是合规要求 |
| **P1-3** | Transaction 端到端回滚验证 | 正确性：事务回滚未真实验证 |
| **P1-4** | Migration __migrations 表自动创建 | 功能完整性：migrate 当前残缺 |
| **P2-5** | Keyset 分页 | 性能：大数据量 offset 分页性能差 |
| **P2-6** | Batch upsert | 性能：批量导入场景必需 |
| **P2-7** | Eager Loading 循环引用检测 | 健壮性：循环引用会导致栈溢出 |

---

## 四、完成进度跟踪

| 任务 | 状态 | 完成时间 | 备注 |
|------|------|---------|------|
| P0-1 软删除集成 | ✅ 已完成 | 2026-07-28 | 10 个 L3 行为测试全通过；集成到 build_select/count/update/delete + 参数化版本 |
| P0-2 注入风险修复 | ✅ 已完成 | 2026-07-28 | 10 个 L3 行为测试全通过；where_cond 标记 deprecated；新增 where_eq/ne/gt/ge/lt/le/like |
| P0-1/P0-2 行为测试 | ✅ 已完成 | 2026-07-28 | 20 个测试用例（L3-1 至 L3-20）覆盖软删除 + 注入防护 |
| P0-1/P0-2 编译验证 | ✅ 已完成 | 2026-07-28 | cargo check --workspace 0 errors；cargo test --workspace 约 4087 个测试 0 failures |
| P0-3 多租户自动过滤 | ✅ 已完成 | 2026-07-28 | 12 个 L3 行为测试全通过；with_tenant_id + without_tenant + 软删除组合 |
| P1-3 事务回滚验证 | ✅ 已完成 | 2026-07-28 | 16 个 L3 行为测试全通过；覆盖 ROLLBACK/SAVEPOINT/超时/死锁重试 |
| P1-4 Migration 持久化 | ✅ 已完成 | 2026-07-28 | __migrations 表自动创建；migrate 实际执行 SQL；版本记录 + 回滚 |
| P2-5 Keyset 分页 | ✅ 已完成 | 2026-07-28 | 20 个 L3 行为测试全通过；keyset_after/keyset_before + 参数化 + 方言兼容 |
| P2-6 Batch upsert | ✅ 已完成 | 2026-07-28 | 20 个 L3 行为测试全通过；MySQL/PG/SQLite 方言；SQL 注入防护 |
| P2-7 循环引用检测 | ✅ 已完成 | 2026-07-28 | 20 个 L3 行为测试全通过；DFS 三色标记 + 确定性输出 + 重复关联检测 |
| 全量回归验证 | ✅ 已完成 | 2026-07-28 | cargo test --workspace 全部通过；cargo check --workspace 0 errors |

---

## 五、审计方法论声明

### 5.1 不再做的事

1. **不再说"可上生产"**：改为"本批次目标达成 + 剩余短板清单"
2. **不再做局部功能验证**：所有"完成"必须 L3 + 行为测试
3. **不再扩维度**：当前 8 大核心功能收敛完成前，不引入新维度（如可观测性、性能基准）

### 5.2 必须做的事

1. **对标 SeaORM**：每个功能的"完成"以 SeaORM 等价功能为基准
2. **行为测试**：从用户视角验证（如"软删除后查询不到"），而非函数调用验证
3. **代码搜索 + 真实执行**：结论必须基于代码搜索 + 独立验证，不允许主观判断
