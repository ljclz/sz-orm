# sz-orm v3.7.0 编码任务规划文档

> 版本：v3.7.0（真实数据库端到端测试体系 + 对比分析重评估与文档同步 + v3.6.0 探索能力成熟化 + 方言扩展延续 + 云数仓真实验证 + 工程规范化）
> 基线：v3.6.0（已完成 M1-M5：15 新表达式 + 195 missing_docs 全补齐 + QueryBuilder 渐进合并 47 测试 + Snowflake/Redshift 20 种方言 32 测试 + async trait 重评估保持方案 C；workspace.package.version = "3.6.0"）
> 日期：2026-08-10
> 文档定位：编码任务规划（What to do），对应需求规格 `docs/spec/v3.7.0/spec.md`（6 方向 / 28 条 EARS 需求 / 6 组 REQ-E2E/REQ-REEVAL/REQ-MAT/REQ-DIALECT/REQ-CLOUD/REQ-ENG）与技术设计 `docs/spec/v3.7.0/design.md`（6 里程碑 + 5 新增 feature gate + 4 既有复用 + 成熟化标注）
> 任务粒度：每个子任务可在 0.5-4 小时内完成，单个任务不超过 500 行代码变更
> 任务统计：52 主任务 / 130 子任务 / 6 里程碑
> 工程化铁律：禁止占位实现（todo!/unimplemented!/unreachable!）/ unsafe 零容忍 / 参数化查询 / API 向后兼容 / Feature 隔离 / 五方言行为一致 / ADR-0001 不改上游 / 审计合规铁律（每结论附 file:line 证据）/ 严禁 PowerShell 替换操作（用 Node.js 脚本）/ 编译时 `$env:RUST_MIN_STACK="67108864"` + `$env:CARGO_INCREMENTAL=0` / 测试 `cargo test --workspace -j 2 --no-fail-fast`

---

# 1. 任务总览

## 1.1 任务统计

| 里程碑 | 方向 | 主任务数 | 子任务数 | 关联需求 | 周期 | 优先级 | 风险 |
|--------|------|---------|---------|---------|------|--------|------|
| M1 真实 DB e2e 测试体系 | 方向 1 | 13 | 30 | REQ-E2E-001~006 | 3 周 | 最高 | 中 |
| M2 对比分析重评估与文档同步 | 方向 2 | 7 | 18 | REQ-REEVAL-001~005 | 1 周 | 高 | 低 |
| M3 v3.6.0 探索能力成熟化 | 方向 3 | 11 | 26 | REQ-MAT-001~005 | 2 周 | 高 | 低 |
| M4 方言扩展延续 | 方向 4 | 9 | 22 | REQ-DIALECT-001~005 | 2 周 | 中 | 中 |
| M5 Snowflake/Redshift 真实云数据库验证 | 方向 5 | 7 | 16 | REQ-CLOUD-001~003 | 1 周 | 中 | 中 |
| M6 工程规范化 | 方向 6 | 5 | 18 | REQ-ENG-001~004 | 1 周 | 低但必须 | 低 |
| **合计** | — | **52** | **130** | **28 条 REQ** | **5 周（关键路径）** | — | — |

## 1.2 里程碑分布

```
M1 真实 DB e2e 测试体系 (3 周, 最高优先级, 中风险)
    │
    ├──→ M2 对比分析重评估 (1 周, 高优先级, 低风险)  [关键路径]
    ├──→ M3 v3.6.0 探索能力成熟化 (2 周, 高优先级, 低风险)
    ├──→ M4 方言扩展延续 (2 周, 中优先级, 中风险)
    ├──→ M5 Snowflake/Redshift 真实云验证 (1 周, 中优先级, 中风险)
    └──→ M6 工程规范化 (1 周, 低但必须, 低风险)  [关键路径终点]
```

- **关键路径**：M1 → M2 → M6（串行 5 周，M1 真实 DB e2e 测试就绪后评分更准确，M2 文档更新后入库）
- **并行机会**：
  - M1 完成后 M3/M4/M5 可并行（feature gate 体系就绪，2 周并行）
  - M1 内部：8 类核心路径测试可部分并行（不同业务路径）
  - M3 内部：typed relation + 自定义诊断 + proc-macro SQL 验证可并行成熟化
  - M4 内部：Informix + SAP HANA + Firebird 评估可并行
  - M5 内部：Snowflake + Redshift 验证可并行
- **总周期**：关键路径 5 周；并行开发下可压缩至 4-5 周

## 1.3 Feature Gate 矩阵

### 1.3.1 5 个新增 Feature gate

| 新增 Feature | 所属包 | 默认 | 依赖 | 关联里程碑 |
|---------|--------|------|------|-----------|
| `e2e-real-db` | sz-orm-core | 关闭 | 无（复用既有 tests/common/ adapter + 本机 DB 实例） | M1 |
| `custom-diagnostic` | sz-orm-macros | 关闭 | 无（复用既有 diagnostic.rs） | M3 |
| `dialect-informix` | sz-orm-core | 关闭 | 无（独立实现 Dialect trait 或仅 SQL 生成） | M4 |
| `dialect-saphana` | sz-orm-core | 关闭 | 无（独立实现 Dialect trait 或仅 SQL 生成） | M4 |
| `dialect-firebird` | sz-orm-core | 关闭 | 无（独立实现 Dialect trait 或仅 SQL 生成） | M4 |

### 1.3.2 4 个既有 Feature 复用 + 成熟化标注

| 既有 Feature | 所属包 | 默认 | 关联里程碑 | v3.7.0 变更 |
|---------|--------|------|-----------|------------|
| `typed-relation` | sz-orm-core | 关闭 | M3 | 标注 stable + 补齐测试 ≥10 + 文档完整 |
| `sql-verify-proc` | sz-orm-core | 关闭 | M3 | 标注 stable + 扩展连真 DB EXPLAIN 覆盖所有路径 + 补齐测试 ≥10 |
| `dialect-snowflake` | sz-orm-core | 关闭 | M5 | 真实云验证（或缺口报告） |
| `dialect-redshift` | sz-orm-core | 关闭 | M5 | 真实云验证（或缺口报告） |

---

# 2. M1 真实数据库端到端测试体系（REQ-E2E-001~006）

> **目标**：在 v3.6.0 既有 96 e2e 测试（InMemoryDb）+ 63 integration 测试（全 `#[ignore]`）基础上，新增连真实 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 的 e2e 测试套件，覆盖 8 类核心业务路径（CRUD/事务/预加载/分页/软删除/多租户/缓存/方言行为），通过 `e2e-real-db` feature gate 隔离（默认关闭，CI 启用），复用既有 tests/common/ adapter，既有 96 e2e + 63 integration 测试保留不替换。
> **周期**：3 周
> **优先级**：最高（中风险高收益，补 v3.6.0 最大缺口，用户最关心）
> **关联设计**：design.md §5.1.1
> **关联验收**：spec §9.1

## 2.1 M1-T1：新增 e2e-real-db feature gate

- [ ] **M1-T1.1** 在 `packages/sz-orm-core/Cargo.toml` `[features]` 段新增 `e2e-real-db = []` feature 定义（默认关闭，不加入 `default = ["redis"]`），附注释 `# v3.7.0: 真实 DB e2e 测试套件（连真实 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 验证 8 类核心路径）`
  - 关联需求：REQ-E2E-003
  - 关联设计：design.md §3.1 / §5.1.1 M1-T1
  - 输入：既有 25+ feature（[Cargo.toml:13](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L13)）
  - 输出：`e2e-real-db` feature 定义就绪，`cargo check --no-default-features` 通过（无 DB 环境不编译失败）
  - 涉及文件：`packages/sz-orm-core/Cargo.toml`
  - 工作量：S（0.5h）
  - 依赖：无

- [ ] **M1-T1.2** 验证默认 feature 零行为变更：执行 `cargo check --workspace --all-targets` 确认默认 feature 行为与 v3.6.0 一致（无新增代码编译），`cargo test --workspace -j 2 --no-fail-fast` 确认既有测试不回退
  - 关联需求：REQ-E2E-003
  - 关联设计：design.md §3.3 默认 Feature 零行为变更保证
  - 输入：M1-T1.1 的 feature 定义
  - 输出：默认 feature 编译产物大小 + 运行时开销 + 既有测试与 v3.6.0 一致
  - 涉及文件：无（验证任务）
  - 工作量：S（0.5h）
  - 依赖：M1-T1.1

## 2.2 M1-T2：实现 e2e_real_db_crud.rs（CRUD 测试套件）

- [ ] **M1-T2.1** 在 `packages/sz-orm-core/tests/e2e_real_db_crud.rs` 新增测试文件骨架：`#![cfg(feature = "e2e-real-db")]` 条件编译隔离，导入 `common::{SqlxPgAdapter, SqlxMysqlAdapter, RusqliteAdapter, schema_builder}`，定义 `fn get_database_url() -> Option<String>` 从环境变量读取 DATABASE_URL（未设置返回 None，测试跳过）
  - 关联需求：REQ-E2E-001/005
  - 关联设计：design.md §5.1.1 M1-T2 / §1.1.3 模块 A
  - 输入：既有 tests/common/ adapter（[tests/common/mod.rs:7](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/common/mod.rs#L7)）
  - 输出：测试文件骨架就绪，无 DB 环境编译通过
  - 涉及文件：`packages/sz-orm-core/tests/e2e_real_db_crud.rs` + `packages/sz-orm-core/Cargo.toml`（注册 `[[test]] e2e_real_db_crud required-features=["e2e-real-db"]`）
  - 工作量：S（1h）
  - 依赖：M1-T1.1

- [ ] **M1-T2.2** 实现 CRUD insert 测试：连真实 DB（MySQL/PostgreSQL/SQLite），建表 `e2e_test_crud_<uuid>` + 准备数据 + 执行 `Query::insert()` + 断言行数 + 断言内容 + 清理（DROP TABLE IF EXISTS），≥3 测试用例（单行插入/批量插入/返回主键）
  - 关联需求：REQ-E2E-001/002/004
  - 关联设计：design.md §5.1.1 M1-T2 / §2.2.2.1
  - 输入：M1-T2.1 的测试骨架 + 既有 SqlxPgAdapter/SqlxMysqlAdapter
  - 输出：insert 测试通过，`cargo test --features e2e-real-db --test e2e_real_db_crud` 通过
  - 涉及文件：`packages/sz-orm-core/tests/e2e_real_db_crud.rs`
  - 工作量：M（2h）
  - 依赖：M1-T2.1 + M1-T10.1

- [ ] **M1-T2.3** 实现 CRUD select/update/delete 测试：覆盖 `Query::select()` + `Query::update()` + `Query::delete()` + where 条件参数化 + 断言结果 + 清理，≥7 测试用例（精确查询/范围查询/更新单行/更新多行/删除单行/删除多行/软删除字段）
  - 关联需求：REQ-E2E-001/002/004
  - 关联设计：design.md §5.1.1 M1-T2
  - 输入：M1-T2.2 的 insert 测试
  - 输出：select/update/delete 测试通过，CRUD 套件 ≥10 测试用例
  - 涉及文件：`packages/sz-orm-core/tests/e2e_real_db_crud.rs`
  - 工作量：M（3h）
  - 依赖：M1-T2.2

## 2.3 M1-T3：实现 e2e_real_db_transaction.rs（事务测试套件）

- [ ] **M1-T3.1** 在 `packages/sz-orm-core/tests/e2e_real_db_transaction.rs` 实现事务 commit/rollback 测试：连真实 DB，验证 `Transaction::begin()` + `commit()` + `rollback()` 真实行为，≥4 测试用例（commit 持久化/rollback 回滚/嵌套事务/事务内异常回滚）
  - 关联需求：REQ-E2E-002/004
  - 关联设计：design.md §5.1.1 M1-T3 / §2.2.2.1
  - 输入：既有 TransactionalConnection（[tests/common/mod.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/common/mod.rs)）
  - 输出：commit/rollback 测试通过
  - 涉及文件：`packages/sz-orm-core/tests/e2e_real_db_transaction.rs` + `Cargo.toml`
  - 工作量：M（2h）
  - 依赖：M1-T1.1 + M1-T10.1

- [ ] **M1-T3.2** 实现 savepoint 测试：验证 `Transaction::savepoint()` + `release_savepoint()` + `rollback_to_savepoint()` 真实行为，≥4 测试用例（创建 savepoint/回滚到 savepoint/释放 savepoint/多层嵌套 savepoint）
  - 关联需求：REQ-E2E-002/004
  - 关联设计：design.md §5.1.1 M1-T3
  - 输入：M1-T3.1 的 commit/rollback 测试
  - 输出：savepoint 测试通过，事务套件 ≥8 测试用例
  - 涉及文件：`packages/sz-orm-core/tests/e2e_real_db_transaction.rs`
  - 工作量：M（2h）
  - 依赖：M1-T3.1

## 2.4 M1-T4：实现 e2e_real_db_eager_load.rs（预加载测试套件）

- [ ] **M1-T4.1** 实现 BelongsTo/HasMany/HasOne 预加载测试：连真实 DB，建父子表 + 外键 + 数据，验证 `EagerLoader::load_belongs_to()` + `load_has_many()` + `load_has_one()` 真实行为 + N+1 检测，≥6 测试用例（BelongsTo/HasMany/HasOne/嵌套预加载/N+1 检测告警/批量预加载）
  - 关联需求：REQ-E2E-002/004
  - 关联设计：design.md §5.1.1 M1-T4 / §2.2.2.1
  - 输入：既有 EagerLoader + 既有 typed_relation.rs
  - 输出：预加载测试通过
  - 涉及文件：`packages/sz-orm-core/tests/e2e_real_db_eager_load.rs` + `Cargo.toml`
  - 工作量：M（3h）
  - 依赖：M1-T1.1 + M1-T10.1

- [ ] **M1-T4.2** 实现复杂关联回退测试：验证多对多（通过中间表）+ 自引用关联 + 深层嵌套（3 层以上）真实行为，≥2 测试用例
  - 关联需求：REQ-E2E-002/004
  - 关联设计：design.md §5.1.1 M1-T4
  - 输入：M1-T4.1 的基础预加载测试
  - 输出：复杂关联测试通过，预加载套件 ≥8 测试用例
  - 涉及文件：`packages/sz-orm-core/tests/e2e_real_db_eager_load.rs`
  - 工作量：M（1.5h）
  - 依赖：M1-T4.1

## 2.5 M1-T5：实现 e2e_real_db_pagination.rs（分页测试套件）

- [ ] **M1-T5.1** 实现 offset/limit 分页测试：连真实 DB，插入 100 行数据，验证 `Query::limit().offset()` 真实行为 + 总数计算 + 页数计算，≥3 测试用例（第一页/中间页/最后一页）
  - 关联需求：REQ-E2E-002/004
  - 关联设计：design.md §5.1.1 M1-T5 / §2.2.2.1
  - 输入：既有 Query::limit/offset
  - 输出：offset/limit 分页测试通过
  - 涉及文件：`packages/sz-orm-core/tests/e2e_real_db_pagination.rs` + `Cargo.toml`
  - 工作量：M（1.5h）
  - 依赖：M1-T1.1 + M1-T10.1

- [ ] **M1-T5.2** 实现 keyset 分页测试：验证 keyset 分页（基于上一页最后一行的主键/排序键）真实行为 + 性能对比（keyset vs offset/limit），≥3 测试用例（keyset 第一页/keyset 中间页/keyset 性能优于 offset）
  - 关联需求：REQ-E2E-002/004
  - 关联设计：design.md §5.1.1 M1-T5
  - 输入：M1-T5.1 的 offset/limit 测试
  - 输出：keyset 测试通过，分页套件 ≥6 测试用例
  - 涉及文件：`packages/sz-orm-core/tests/e2e_real_db_pagination.rs`
  - 工作量：M（1.5h）
  - 依赖：M1-T5.1

## 2.6 M1-T6：实现 e2e_real_db_soft_delete.rs（软删除测试套件）

- [ ] **M1-T6.1** 实现软删除测试：连真实 DB，建表含 `deleted_at` 字段，验证软删除（设置 deleted_at）+ 过滤（自动排除已删除）+ 恢复（清除 deleted_at）真实行为，≥5 测试用例（软删除单行/软删除多行/查询自动过滤/恢复单行/查询包含已删除）
  - 关联需求：REQ-E2E-002/004
  - 关联设计：design.md §5.1.1 M1-T6 / §2.2.2.1
  - 输入：既有软删除支持
  - 输出：软删除测试通过，套件 ≥5 测试用例
  - 涉及文件：`packages/sz-orm-core/tests/e2e_real_db_soft_delete.rs` + `Cargo.toml`
  - 工作量：M（2h）
  - 依赖：M1-T1.1 + M1-T10.1

## 2.7 M1-T7：实现 e2e_real_db_multi_tenant.rs（多租户测试套件）

- [ ] **M1-T7.1** 实现多租户隔离测试：连真实 DB，建表含 `tenant_id` 字段，验证 `tenant_context` 隔离 + 行级安全 + 跨租户查询拦截真实行为，≥5 测试用例（设置 tenant 上下文/查询自动过滤 tenant_id/跨租户查询拦截/切换 tenant/无 tenant 上下文报错）
  - 关联需求：REQ-E2E-002/004
  - 关联设计：design.md §5.1.1 M1-T7 / §2.2.2.1
  - 输入：既有 multi-tenant-enhanced feature + tenant_context
  - 输出：多租户测试通过，套件 ≥5 测试用例
  - 涉及文件：`packages/sz-orm-core/tests/e2e_real_db_multi_tenant.rs` + `Cargo.toml`
  - 工作量：M（2h）
  - 依赖：M1-T1.1 + M1-T10.1

## 2.8 M1-T8：实现 e2e_real_db_cache.rs（缓存测试套件）

- [ ] **M1-T8.1** 实现 L1 缓存测试：连真实 DB + 启用 L1 缓存，验证缓存命中/失效/一致性真实行为，≥3 测试用例（首次查询 miss/二次查询 hit/数据变更后 invalidation）
  - 关联需求：REQ-E2E-002/004
  - 关联设计：design.md §5.1.1 M1-T8 / §2.2.2.1
  - 输入：既有 l1-cache feature
  - 输出：L1 缓存测试通过
  - 涉及文件：`packages/sz-orm-core/tests/e2e_real_db_cache.rs` + `Cargo.toml`
  - 工作量：M（2h）
  - 依赖：M1-T1.1 + M1-T10.1

- [ ] **M1-T8.2** 实现 L2 缓存测试：连真实 DB + 启用 L2 缓存（Redis），验证 L2 缓存命中/失效/一致性 + L1/L2 协作真实行为，≥3 测试用例（L2 miss/L2 hit/L1 miss + L2 hit）
  - 关联需求：REQ-E2E-002/004
  - 关联设计：design.md §5.1.1 M1-T8
  - 输入：M1-T8.1 的 L1 缓存测试 + 既有 l2_cache
  - 输出：L2 缓存测试通过，缓存套件 ≥6 测试用例
  - 涉及文件：`packages/sz-orm-core/tests/e2e_real_db_cache.rs`
  - 工作量：M（2h）
  - 依赖：M1-T8.1

## 2.9 M1-T9：实现 e2e_real_db_dialect_behavior.rs（方言行为一致性测试套件）

- [ ] **M1-T9.1** 实现 UPSERT 行为一致性测试：连真实 DB（MySQL/PostgreSQL/SQLite），验证 `Query::upsert()` / `ON CONFLICT` / `ON DUPLICATE KEY UPDATE` 各方言真实行为一致，≥3 测试用例（MySQL UPSERT/PG ON CONFLICT/SQLite UPSERT）
  - 关联需求：REQ-E2E-002/004
  - 关联设计：design.md §5.1.1 M1-T9 / §2.2.2.1
  - 输入：既有 Dialect trait + UPSERT 支持
  - 输出：UPSERT 行为一致性测试通过
  - 涉及文件：`packages/sz-orm-core/tests/e2e_real_db_dialect_behavior.rs` + `Cargo.toml`
  - 工作量：M（2h）
  - 依赖：M1-T1.1 + M1-T10.1

- [ ] **M1-T9.2** 实现行锁 + 标识符引用 + 方言特有语法测试：验证 `SELECT ... FOR UPDATE` 行锁 + 标识符引用（quote）+ 方言特有语法（PG RETURNING/MySQL LIMIT/SQLite 特有函数）真实行为，≥5 测试用例（PG FOR UPDATE/MySQL FOR UPDATE/标识符引用/PG RETURNING/方言特有函数）
  - 关联需求：REQ-E2E-002/004
  - 关联设计：design.md §5.1.1 M1-T9
  - 输入：M1-T9.1 的 UPSERT 测试
  - 输出：行锁 + 标识符 + 方言特有语法测试通过，方言行为套件 ≥8 测试用例
  - 涉及文件：`packages/sz-orm-core/tests/e2e_real_db_dialect_behavior.rs`
  - 工作量：M（2h）
  - 依赖：M1-T9.1

## 2.10 M1-T10：实现测试清理机制

- [ ] **M1-T10.1** 在 `packages/sz-orm-core/tests/common/cleanup.rs` 实现测试清理工具：`fn cleanup_table(conn, table_name)` 执行 `DROP TABLE IF EXISTS`，`fn unique_table_name(prefix)` 生成独立表名（`<prefix>_<uuid>`），`fn cleanup_with_warning(conn, table_name)` 清理失败时输出警告不影响测试判定
  - 关联需求：REQ-E2E-004
  - 关联设计：design.md §5.1.1 M1-T10 / §2.3.2 数据模型
  - 输入：既有 schema_builder（[tests/common/schema_builder.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/common/schema_builder.rs)）
  - 输出：清理工具就绪，独立表名 + DROP TABLE IF EXISTS + 清理失败警告
  - 涉及文件：`packages/sz-orm-core/tests/common/cleanup.rs` + `packages/sz-orm-core/tests/common/mod.rs`
  - 工作量：M（2h）
  - 依赖：M1-T1.1

- [ ] **M1-T10.2** 实现事务回滚清理策略：`fn cleanup_with_rollback(conn, test_fn)` 在事务内执行测试，测试完成后 rollback 保证无残留，作为独立表名的替代策略
  - 关联需求：REQ-E2E-004
  - 关联设计：design.md §5.1.1 M1-T10
  - 输入：M1-T10.1 的清理工具
  - 输出：事务回滚清理策略就绪
  - 涉及文件：`packages/sz-orm-core/tests/common/cleanup.rs`
  - 工作量：S（1h）
  - 依赖：M1-T10.1

## 2.11 M1-T11：实现测试超时机制

- [ ] **M1-T11.1** 在 `packages/sz-orm-core/tests/common/timeout.rs` 实现测试超时工具：`async fn run_with_timeout(test_fn, timeout)` 使用 `tokio::time::timeout` 包装测试，超时标记失败并输出卡点（测试名 + 耗时），单方言默认 60s，全方言默认 300s
  - 关联需求：REQ-E2E-001
  - 关联设计：design.md §5.1.1 M1-T11 / §4.1.1 性能
  - 输入：tokio 既有依赖
  - 输出：超时工具就绪，超时标记失败 + 输出卡点
  - 涉及文件：`packages/sz-orm-core/tests/common/timeout.rs` + `packages/sz-orm-core/tests/common/mod.rs`
  - 工作量：S（1h）
  - 依赖：M1-T1.1

- [ ] **M1-T11.2** 验证单方言 ≤60s + 全方言 ≤300s：执行 `cargo test --features e2e-real-db` 测量各方言 e2e 套件执行时间，确认在阈值内
  - 关联需求：REQ-E2E-001
  - 关联设计：design.md §5.1.1 M1-T11
  - 输入：M1-T11.1 的超时工具 + M1-T2~M1-T9 的测试套件
  - 输出：各方言 e2e 套件执行时间在阈值内
  - 涉及文件：无（验证任务）
  - 工作量：S（1h）
  - 依赖：M1-T11.1 + M1-T2.3 + M1-T3.2 + M1-T4.2 + M1-T5.2 + M1-T6.1 + M1-T7.1 + M1-T8.2 + M1-T9.2

## 2.12 M1-T12：DATABASE_URL 不硬编码验证

- [ ] **M1-T12.1** 审查所有 e2e_real_db_*.rs 文件，确认 DATABASE_URL 仅通过 `std::env::var("DATABASE_URL")` 读取，无硬编码连接串，测试库为 `sz_orm_test`（非生产库），在测试文件头部注释中标注示例 `# Example: DATABASE_URL=mysql://root:test123@127.0.0.1:3306/sz_orm_test`
  - 关联需求：REQ-E2E-006
  - 关联设计：design.md §5.1.1 M1-T12 / §4.3 安全性
  - 输入：M1-T2~M1-T9 的测试套件
  - 输出：所有测试文件 DATABASE_URL 不硬编码，测试库为 sz_orm_test
  - 涉及文件：`packages/sz-orm-core/tests/e2e_real_db_*.rs`
  - 工作量：S（1h）
  - 依赖：M1-T2.3 + M1-T3.2 + M1-T4.2 + M1-T5.2 + M1-T6.1 + M1-T7.1 + M1-T8.2 + M1-T9.2

## 2.13 M1-T13：既有测试不回退验证

- [ ] **M1-T13.1** 执行 `cargo test --workspace -j 2 --no-fail-fast`（默认 feature）验证既有 96 e2e + 63 integration + 单元/集成测试全通过，与 v3.6.0 基线对比不回退
  - 关联需求：REQ-E2E-005
  - 关联设计：design.md §5.1.1 M1-T13 / §4.3 v3.6.0 测试基线不回退
  - 输入：M1-T1~M1-T12 全部完成
  - 输出：既有测试全通过，v3.6.0 基线不回退
  - 涉及文件：无（验证任务）
  - 工作量：S（1h）
  - 依赖：M1-T1.2 + M1-T12.1

- [ ] **M1-T13.2** 执行 `cargo test --features e2e-real-db -j 2 --no-fail-fast` 验证真实 DB e2e 测试全通过（需本机 DB 实例），确认 8 类核心路径 ≥56 测试用例全通过
  - 关联需求：REQ-E2E-001/002
  - 关联设计：design.md §5.1.1 M1-T13 / §6.1.1
  - 输入：M1-T13.1 的既有测试验证
  - 输出：真实 DB e2e 测试全通过，≥56 测试用例
  - 涉及文件：无（验证任务）
  - 工作量：S（1h）
  - 依赖：M1-T13.1 + M1-T11.2

---

# 3. M2 对比分析重评估与文档同步（REQ-REEVAL-001~005）

> **目标**：将停留 v3.5.0 基线的对比分析文档 `docs/sz-orm与同类产品对比分析.md` 更新到 v3.6.0 基线，重新逐维度评分（13 维度），每条变更附 v3.6.0 file:line 证据，§6 已知不足标注 v3.6.0 改进状态，§7 结论与建议更新，§5 独特优势更新，纯文档工作不改变代码行为。
> **周期**：1 周
> **优先级**：高（低风险高收益，纯文档工作，用户明确要求）
> **关联设计**：design.md §5.1.2
> **关联验收**：spec §9.2

## 3.1 M2-T1：更新文档头部到 v3.6.0 基线

- [ ] **M2-T1.1** 更新 `docs/sz-orm与同类产品对比分析.md` 头部：`版本：v3.5.0 | 日期：2026-08-09` → `版本：v3.6.0 | 日期：2026-08-10`，代码基线 `Cargo.toml workspace.package.version = "3.5.0"` → `"3.6.0"`
  - 关联需求：REQ-REEVAL-001
  - 关联设计：design.md §5.1.2 M2-T1 / §1.1.2
  - 输入：既有 [docs/sz-orm与同类产品对比分析.md:3](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/sz-orm与同类产品对比分析.md#L3)（v3.5.0 基线）
  - 输出：文档头部更新到 v3.6.0 基线
  - 涉及文件：`docs/sz-orm与同类产品对比分析.md`
  - 工作量：S（0.5h）
  - 依赖：M1-T13.2（M1 完成后评分更准确）

## 3.2 M2-T2：13 维度重新评分

- [ ] **M2-T2.1** 重新评分类型安全维度：v3.5.0 评分 7/10 → v3.6.0 评分 9/10（+2，新增 15 表达式 + typed relation + 自定义诊断），附证据 [typed_ast.rs:397](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_ast.rs#L397)（61 种表达式）+ [typed_relation.rs:35](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_relation.rs#L35)（typed relation）+ [diagnostic.rs:10](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/src/diagnostic.rs#L10)（自定义诊断）
  - 关联需求：REQ-REEVAL-002
  - 关联设计：design.md §5.1.2 M2-T2 / §1.1.2
  - 输入：v3.6.0 代码库
  - 输出：类型安全维度评分更新 + 证据
  - 涉及文件：`docs/sz-orm与同类产品对比分析.md`
  - 工作量：S（1h）
  - 依赖：M2-T1.1

- [ ] **M2-T2.2** 重新评分方言维度：v3.5.0 评分 7/10 → v3.6.0 评分 9/10（+2，新增 Snowflake + Redshift，方言数 18 → 20），附证据 [dialect.rs:1567](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1567)（SnowflakeDialect）+ [dialect.rs:1794](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1794)（RedshiftDialect）
  - 关联需求：REQ-REEVAL-002
  - 关联设计：design.md §5.1.2 M2-T2
  - 输入：v3.6.0 代码库
  - 输出：方言维度评分更新 + 证据
  - 涉及文件：`docs/sz-orm与同类产品对比分析.md`
  - 工作量：S（1h）
  - 依赖：M2-T2.1

- [ ] **M2-T2.3** 重新评分文档生态维度：v3.5.0 评分 6/10 → v3.6.0 评分 8/10（+2，195 missing_docs 全补齐 + 全局 `#![warn(missing_docs)]` 启用），附证据 [lib.rs:404](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L404)（`#![warn(missing_docs)]`）
  - 关联需求：REQ-REEVAL-002
  - 关联设计：design.md §5.1.2 M2-T2
  - 输入：v3.6.0 代码库
  - 输出：文档生态维度评分更新 + 证据
  - 涉及文件：`docs/sz-orm与同类产品对比分析.md`
  - 工作量：S（1h）
  - 依赖：M2-T2.2

- [ ] **M2-T2.4** 重新评分宏维度：v3.5.0 评分 7/10 → v3.6.0 评分 8/10（+1，proc-macro SQL 验证 + query! 宏 db-verify feature），附证据 [sql_verify.rs:22](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/sql_verify.rs#L22)（VerifyResult）+ [sz-orm-macros/src/lib.rs:464](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/src/lib.rs#L464)（query! 宏）
  - 关联需求：REQ-REEVAL-002
  - 关联设计：design.md §5.1.2 M2-T2
  - 输入：v3.6.0 代码库
  - 输出：宏维度评分更新 + 证据
  - 涉及文件：`docs/sz-orm与同类产品对比分析.md`
  - 工作量：S（1h）
  - 依赖：M2-T2.3

- [ ] **M2-T2.5** 重新评分其余 9 维度（异步/连接池/查询API/事务/缓存/N+1/安全/性能/生产就绪）：逐维度基于 v3.6.0 实际能力重新评分，无变化标注"无变化"，有变化附 file:line 证据
  - 关联需求：REQ-REEVAL-002
  - 关联设计：design.md §5.1.2 M2-T2
  - 输入：v3.6.0 代码库
  - 输出：9 维度评分更新 + 证据（如变更）
  - 涉及文件：`docs/sz-orm与同类产品对比分析.md`
  - 工作量：L（4h）
  - 依赖：M2-T2.4

## 3.3 M2-T3：更新 §6 已知不足

- [ ] **M2-T3.1** 更新 §6.1 生态成熟度：标注"⚠️ 部分改进（v3.6.0 已补齐 195 missing_docs，但社区规模未扩展）"，附证据 [lib.rs:404](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L404)
  - 关联需求：REQ-REEVAL-003
  - 关联设计：design.md §5.1.2 M2-T3 / §1.1.2
  - 输入：M2-T2.3 的文档生态评分
  - 输出：§6.1 更新 + 改进状态标注
  - 涉及文件：`docs/sz-orm与同类产品对比分析.md`
  - 工作量：S（1h）
  - 依赖：M2-T2.5

- [ ] **M2-T3.2** 更新 §6.2 文档完整度：标注"✅ 已改进（v3.6.0 已补齐 195 missing_docs，文档完整度已对齐竞品）"，附证据 [lib.rs:404](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L404)
  - 关联需求：REQ-REEVAL-003
  - 关联设计：design.md §5.1.2 M2-T3
  - 输入：M2-T3.1 的 §6.1 更新
  - 输出：§6.2 更新 + 改进状态标注
  - 涉及文件：`docs/sz-orm与同类产品对比分析.md`
  - 工作量：S（0.5h）
  - 依赖：M2-T3.1

- [ ] **M2-T3.3** 更新 §6 其余子节（端到端测试/对比分析基线/探索能力成熟度/方言数量/云数仓验证）：标注 v3.6.0 改进状态（✅/⚠️/❌）+ 证据，如端到端测试标注"⚠️ 部分改进（v3.6.0 仍用 InMemoryDb，v3.7.0 改进中）"
  - 关联需求：REQ-REEVAL-003
  - 关联设计：design.md §5.1.2 M2-T3
  - 输入：M2-T3.2 的 §6.2 更新
  - 输出：§6 其余子节更新 + 改进状态标注
  - 涉及文件：`docs/sz-orm与同类产品对比分析.md`
  - 工作量：M（2.5h）
  - 依赖：M2-T3.2

## 3.4 M2-T4：更新 §7 结论与建议

- [ ] **M2-T4.1** 更新 §7 综合结论：反映 v3.6.0 后真实状态（类型安全 9/10 + 方言 9/10 + 文档生态 8/10 + 宏 8/10），更新定位建议（"sz-orm 在类型安全/方言覆盖度/文档完整度已对齐或超越竞品，社区规模/生产案例待扩展"）
  - 关联需求：REQ-REEVAL-004
  - 关联设计：design.md §5.1.2 M2-T4
  - 输入：M2-T2.5 的 13 维度评分 + M2-T3.3 的 §6 更新
  - 输出：§7 综合结论 + 定位建议更新
  - 涉及文件：`docs/sz-orm与同类产品对比分析.md`
  - 工作量：S（1h）
  - 依赖：M2-T3.3

- [ ] **M2-T4.2** 更新 §7 改进建议：反映 v3.7.0+ 方向（真实 DB e2e 测试体系/探索能力成熟化/方言扩展延续/云数仓真实验证/工程规范化/社区规模扩展/生产案例扩展）
  - 关联需求：REQ-REEVAL-004
  - 关联设计：design.md §5.1.2 M2-T4
  - 输入：M2-T4.1 的综合结论
  - 输出：§7 改进建议更新
  - 涉及文件：`docs/sz-orm与同类产品对比分析.md`
  - 工作量：S（1h）
  - 依赖：M2-T4.1

## 3.5 M2-T5：更新 §5 独特优势

- [ ] **M2-T5.1** 在 §5 新增 v3.6.0 独特优势项：15 新表达式（CTE 3 + Window Frame 6 + JSON 6）+ typed relation（编译期类型安全关联查询）+ 自定义编译期诊断 + proc-macro SQL 验证 + Snowflake/Redshift 方言 + QueryBuilder 迁移工具，每项附 file:line 证据
  - 关联需求：REQ-REEVAL-004
  - 关联设计：design.md §5.1.2 M2-T5 / §1.1.2
  - 输入：v3.6.0 代码库
  - 输出：§5 独特优势更新，v3.6.0 新增项 + 证据
  - 涉及文件：`docs/sz-orm与同类产品对比分析.md`
  - 工作量：M（2h）
  - 依赖：M2-T4.2

## 3.6 M2-T6：审计验证脚本运行

- [ ] **M2-T6.1** 运行 `bash scripts/audit-verify.sh docs/sz-orm与同类产品对比分析.md`（或 `.\scripts\audit-verify.ps1`）验证所有 file:line 证据真实存在，逐项确认证据文件存在且行号在范围内
  - 关联需求：REQ-REEVAL-002
  - 关联设计：design.md §5.1.2 M2-T6 / §6.1.2
  - 输入：M2-T5.1 的 §5 更新
  - 输出：审计验证脚本通过，所有证据真实存在
  - 涉及文件：无（验证任务）
  - 工作量：S（1h）
  - 依赖：M2-T5.1

- [ ] **M2-T6.2** 修正证据验证失败的引用：如有 file:line 证据验证失败（文件不存在或行号超出范围），修正引用到正确的 file:line
  - 关联需求：REQ-REEVAL-002
  - 关联设计：design.md §5.1.2 M2-T6 / §5.2.3 异常场景
  - 输入：M2-T6.1 的审计验证结果
  - 输出：所有证据验证通过
  - 涉及文件：`docs/sz-orm与同类产品对比分析.md`
  - 工作量：S（1h）
  - 依赖：M2-T6.1

## 3.7 M2-T7：纯文档变更验证

- [ ] **M2-T7.1** 执行 `git diff --name-only` 确认仅文档变更（`docs/sz-orm与同类产品对比分析.md`），无 .rs 代码变更
  - 关联需求：REQ-REEVAL-005
  - 关联设计：design.md §5.1.2 M2-T7 / §5.2.1 规则 5
  - 输入：M2-T6.2 的证据修正
  - 输出：git diff 确认纯文档变更
  - 涉及文件：无（验证任务）
  - 工作量：S（0.5h）
  - 依赖：M2-T6.2

---

# 4. M3 v3.6.0 探索能力成熟化（REQ-MAT-001~005）

> **目标**：将 v3.6.0 以"探索"性质实现的能力（typed relation / 自定义编译期诊断 / proc-macro SQL 验证）转为正式 feature，补齐测试覆盖至 ≥10 用例 + 文档完整 + 稳定性标注（Cargo.toml feature 注释 "stable"），既有 API 完全向后兼容，无运行时开销（均为编译期工作）。
> **周期**：2 周
> **优先级**：高（低风险高收益，既有探索实现仅补齐测试与文档）
> **关联设计**：design.md §5.1.3
> **关联验收**：spec §9.3

## 4.1 M3-T1：typed relation 补齐测试

- [ ] **M3-T1.1** 在 `packages/sz-orm-core/tests/typed_relation_test.rs` 新增 typed relation 测试文件：`#![cfg(feature = "typed-relation")]` 条件编译隔离，导入 `typed_relation::{TypedTable, Relation, BelongsTo, HasMany, HasOne, RelationKind}`
  - 关联需求：REQ-MAT-001
  - 关联设计：design.md §5.1.3 M3-T1 / §1.1.3 模块 C
  - 输入：既有 [typed_relation.rs:35](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/typed_relation.rs#L35)（TypedTable trait）
  - 输出：测试文件骨架就绪
  - 涉及文件：`packages/sz-orm-core/tests/typed_relation_test.rs` + `Cargo.toml`（注册 `[[test]] typed_relation_test required-features=["typed-relation"]`）
  - 工作量：S（0.5h）
  - 依赖：M1-T1.1

- [ ] **M3-T1.2** 实现编译期外键类型校验测试：定义 `User: TypedTable<PrimaryKey = i64, ForeignKey = i64>` + `Post: TypedTable<PrimaryKey = i64, ForeignKey = i64>`，验证 `BelongsTo<Post, User, i64>` 编译通过（外键类型匹配），≥3 测试用例（外键匹配/外键不匹配编译失败/不同表外键编译失败）
  - 关联需求：REQ-MAT-001
  - 关联设计：design.md §5.1.3 M3-T1 / §1.2.4
  - 输入：M3-T1.1 的测试骨架
  - 输出：编译期外键校验测试通过
  - 涉及文件：`packages/sz-orm-core/tests/typed_relation_test.rs`
  - 工作量：M（2h）
  - 依赖：M3-T1.1

- [ ] **M3-T1.3** 实现运行时关联查询测试 + 表归属校验 + 与 EagerLoader 协作 + escape hatch：≥7 测试用例（BelongsTo 查询/HasMany 查询/HasOne 查询/表归属校验/与 EagerLoader 协作/escape hatch 回退/复杂关联回退），typed relation 套件 ≥10 测试用例
  - 关联需求：REQ-MAT-001
  - 关联设计：design.md §5.1.3 M3-T1 / §1.2.4
  - 输入：M3-T1.2 的编译期校验测试
  - 输出：typed relation 套件 ≥10 测试用例全通过
  - 涉及文件：`packages/sz-orm-core/tests/typed_relation_test.rs`
  - 工作量：M（3.5h）
  - 依赖：M3-T1.2

## 4.2 M3-T2：typed relation 补齐文档

- [ ] **M3-T2.1** 在 `docs/typed-relation-guide.md` 新增 typed relation 迁移指南：从 EagerLoader 到 typed relation 的迁移路径 + 适用场景（编译期类型安全需求）+ escape hatch 说明（复杂关联回退 EagerLoader）+ 稳定性标注（v3.7.0 stable）
  - 关联需求：REQ-MAT-001
  - 关联设计：design.md §5.1.3 M3-T2
  - 输入：M3-T1.3 的测试覆盖
  - 输出：typed relation 文档完整
  - 涉及文件：`docs/typed-relation-guide.md`
  - 工作量：M（2h）
  - 依赖：M3-T1.3

- [ ] **M3-T2.2** 在 `packages/sz-orm-core/src/typed_relation.rs` 模块级文档注释补齐：模块说明 + 适用场景 + 示例代码 + escape hatch 说明 + 稳定性标注
  - 关联需求：REQ-MAT-001
  - 关联设计：design.md §5.1.3 M3-T2
  - 输入：M3-T2.1 的迁移指南
  - 输出：模块级文档注释完整
  - 涉及文件：`packages/sz-orm-core/src/typed_relation.rs`
  - 工作量：S（1h）
  - 依赖：M3-T2.1

## 4.3 M3-T3：typed relation Cargo.toml 标注 stable

- [ ] **M3-T3.1** 更新 `packages/sz-orm-core/Cargo.toml` typed-relation feature 注释：`typed-relation = []  # v3.7.0: stable（类型安全关联查询，编译期外键校验）`
  - 关联需求：REQ-MAT-001
  - 关联设计：design.md §5.1.3 M3-T3 / §3.1
  - 输入：M3-T2.2 的文档补齐
  - 输出：Cargo.toml feature 注释标注 stable
  - 涉及文件：`packages/sz-orm-core/Cargo.toml`
  - 工作量：S（0.5h）
  - 依赖：M3-T2.2

## 4.4 M3-T4：自定义诊断独立 feature gate

- [ ] **M3-T4.1** 在 `packages/sz-orm-macros/Cargo.toml` `[features]` 段新增 `custom-diagnostic = []` feature 定义（默认关闭），附注释 `# v3.7.0: stable（自定义编译期诊断信息）`
  - 关联需求：REQ-MAT-002
  - 关联设计：design.md §5.1.3 M3-T4 / §3.1
  - 输入：既有 [sz-orm-macros/Cargo.toml:37](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/Cargo.toml#L37)（typed-dsl feature）
  - 输出：custom-diagnostic feature 定义就绪
  - 涉及文件：`packages/sz-orm-macros/Cargo.toml`
  - 工作量：S（0.5h）
  - 依赖：M1-T1.1

- [ ] **M3-T4.2** 在 `packages/sz-orm-macros/src/diagnostic.rs` 添加 `#[cfg(feature = "custom-diagnostic")]` 条件编译隔离，保持既有 `typed-dsl` feature 兼容（自定义诊断在 typed-dsl + custom-diagnostic 下均可用，向后兼容）
  - 关联需求：REQ-MAT-002
  - 关联设计：design.md §5.1.3 M3-T4 / §1.2.5
  - 输入：M3-T4.1 的 feature 定义
  - 输出：自定义诊断独立 feature gate 隔离，既有 typed-dsl 兼容
  - 涉及文件：`packages/sz-orm-macros/src/diagnostic.rs` + `packages/sz-orm-macros/src/lib.rs`
  - 工作量：S（1h）
  - 依赖：M3-T4.1

## 4.5 M3-T5：自定义诊断补齐测试

- [ ] **M3-T5.1** 在 `packages/sz-orm-macros/tests/custom_diagnostic_test.rs` 新增测试文件：`#![cfg(feature = "custom-diagnostic")]` 条件编译隔离，覆盖错误位置 + 期望类型 + 实际类型 + 修复建议字段验证，≥5 测试用例（位置正确/期望类型正确/实际类型正确/修复建议正确/format_error 输出格式）
  - 关联需求：REQ-MAT-002
  - 关联设计：design.md §5.1.3 M3-T5 / §1.1.3 模块 C
  - 输入：既有 [diagnostic.rs:10](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-macros/src/diagnostic.rs#L10)（TypeMismatchDiagnostic）
  - 输出：基础字段测试通过
  - 涉及文件：`packages/sz-orm-macros/tests/custom_diagnostic_test.rs` + `Cargo.toml`
  - 工作量：M（2h）
  - 依赖：M3-T4.2

- [ ] **M3-T5.2** 实现各诊断场景测试：覆盖 TYPE_MISMATCH_EQ / NON_BOOLEAN_LOGIC / CROSS_TABLE_REFERENCE 场景，≥5 测试用例（Eq 类型不匹配/And 非 boolean/Or 非 boolean/filter 跨表/复杂表达式），自定义诊断套件 ≥10 测试用例
  - 关联需求：REQ-MAT-002
  - 关联设计：design.md §5.1.3 M3-T5 / §1.2.5
  - 输入：M3-T5.1 的基础字段测试
  - 输出：自定义诊断套件 ≥10 测试用例全通过
  - 涉及文件：`packages/sz-orm-macros/tests/custom_diagnostic_test.rs`
  - 工作量：M（2h）
  - 依赖：M3-T5.1

## 4.6 M3-T6：自定义诊断补齐文档

- [ ] **M3-T6.1** 在 `docs/custom-diagnostic-guide.md` 新增自定义诊断文档：启用方式（`--features custom-diagnostic`）+ 诊断场景（Eq/And/Or/filter 跨表）+ 迁移指南（从 typed-dsl 到 custom-diagnostic）+ 稳定性标注（v3.7.0 stable）
  - 关联需求：REQ-MAT-002
  - 关联设计：design.md §5.1.3 M3-T6
  - 输入：M3-T5.2 的测试覆盖
  - 输出：自定义诊断文档完整
  - 涉及文件：`docs/custom-diagnostic-guide.md`
  - 工作量：S（1.5h）
  - 依赖：M3-T5.2

- [ ] **M3-T6.2** 在 `packages/sz-orm-macros/src/diagnostic.rs` 模块级文档注释补齐：模块说明 + 启用方式 + 诊断场景 + 示例代码 + 稳定性标注
  - 关联需求：REQ-MAT-002
  - 关联设计：design.md §5.1.3 M3-T6
  - 输入：M3-T6.1 的文档
  - 输出：模块级文档注释完整
  - 涉及文件：`packages/sz-orm-macros/src/diagnostic.rs`
  - 工作量：S（0.5h）
  - 依赖：M3-T6.1

## 4.7 M3-T7：proc-macro SQL 验证扩展连真 DB EXPLAIN

- [ ] **M3-T7.1** 在 `packages/sz-orm-core/src/sql_verify.rs` 扩展连真 DB EXPLAIN 覆盖所有 QueryBuilder 路径：SELECT/INSERT/UPDATE/DELETE 基础路径 + JOIN（INNER/LEFT/RIGHT/FULL）+ 子查询（WHERE/SELECT/FROM）+ CTE（WITH/WITH RECURSIVE）+ 窗口函数（OVER/PARTITION BY/FRAME），复用既有 db-verify feature 连真 DB 逻辑
  - 关联需求：REQ-MAT-003
  - 关联设计：design.md §5.1.3 M3-T7 / §1.2.6
  - 输入：既有 [sql_verify.rs:22](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/sql_verify.rs#L22)（VerifyResult）+ 既有 db-verify feature
  - 输出：连真 DB EXPLAIN 覆盖所有 QueryBuilder 路径
  - 涉及文件：`packages/sz-orm-core/src/sql_verify.rs`
  - 工作量：L（4h）
  - 依赖：M1-T1.1

- [ ] **M3-T7.2** 实现降级模式：DATABASE_URL 未设置或 DB 不可达时回退到仅语法校验（不连真 DB），输出降级警告 `warning: sql-verify-proc degraded to syntax-only (DATABASE_URL not set)`
  - 关联需求：REQ-MAT-003
  - 关联设计：design.md §5.1.3 M3-T7 / §5.3.3 异常场景
  - 输入：M3-T7.1 的 EXPLAIN 扩展
  - 输出：降级模式就绪，DATABASE_URL 未设置时回退到语法校验
  - 涉及文件：`packages/sz-orm-core/src/sql_verify.rs`
  - 工作量：M（2h）
  - 依赖：M3-T7.1

## 4.8 M3-T8：proc-macro SQL 验证补齐测试

- [ ] **M3-T8.1** 在 `packages/sz-orm-core/tests/sql_verify_proc_test.rs` 新增测试文件：`#![cfg(feature = "sql-verify-proc")]` 条件编译隔离，覆盖 SQL 解析 + 表/列存在性 + 类型匹配 + EXPLAIN only + 缓存，≥6 测试用例（SELECT 解析/INSERT 表存在性/UPDATE 列存在性/DELETE 类型匹配/EXPLAIN only/缓存命中）
  - 关联需求：REQ-MAT-003
  - 关联设计：design.md §5.1.3 M3-T8 / §1.1.3 模块 C
  - 输入：M3-T7.2 的 EXPLAIN 扩展 + 降级模式
  - 输出：基础测试通过
  - 涉及文件：`packages/sz-orm-core/tests/sql_verify_proc_test.rs` + `Cargo.toml`
  - 工作量：M（2.5h）
  - 依赖：M3-T7.2

- [ ] **M3-T8.2** 实现降级模式 + 所有 QueryBuilder 路径测试：覆盖降级模式（DATABASE_URL 未设置）+ JOIN/子查询/CTE/窗口函数路径，≥4 测试用例（降级模式/JOIN EXPLAIN/CTE EXPLAIN/窗口函数 EXPLAIN），proc-macro SQL 验证套件 ≥10 测试用例
  - 关联需求：REQ-MAT-003
  - 关联设计：design.md §5.1.3 M3-T8
  - 输入：M3-T8.1 的基础测试
  - 输出：proc-macro SQL 验证套件 ≥10 测试用例全通过
  - 涉及文件：`packages/sz-orm-core/tests/sql_verify_proc_test.rs`
  - 工作量：M（1.5h）
  - 依赖：M3-T8.1

## 4.9 M3-T9：proc-macro SQL 验证补齐文档

- [ ] **M3-T9.1** 在 `docs/sql-verify-proc-guide.md` 新增 proc-macro SQL 验证文档：启用方式（`--features sql-verify-proc` + `DATABASE_URL` + `SZ_ORM_QUERY_VERIFY=1`）+ 覆盖路径（SELECT/INSERT/UPDATE/DELETE/JOIN/子查询/CTE/窗口）+ 降级模式说明 + 缓存机制 + 稳定性标注（v3.7.0 stable）
  - 关联需求：REQ-MAT-003
  - 关联设计：design.md §5.1.3 M3-T9
  - 输入：M3-T8.2 的测试覆盖
  - 输出：proc-macro SQL 验证文档完整
  - 涉及文件：`docs/sql-verify-proc-guide.md`
  - 工作量：M（1.5h）
  - 依赖：M3-T8.2

- [ ] **M3-T9.2** 更新 `packages/sz-orm-core/Cargo.toml` sql-verify-proc feature 注释：`sql-verify-proc = ["dep:sqlparser", "dep:xxhash-rust"]  # v3.7.0: stable（proc-macro 编译期 SQL 验证）`
  - 关联需求：REQ-MAT-003
  - 关联设计：design.md §5.1.3 M3-T9 / §3.1
  - 输入：M3-T9.1 的文档
  - 输出：Cargo.toml feature 注释标注 stable
  - 涉及文件：`packages/sz-orm-core/Cargo.toml`
  - 工作量：S（0.5h）
  - 依赖：M3-T9.1

## 4.10 M3-T10：v3.6.0 既有 API 向后兼容验证

- [ ] **M3-T10.1** 验证既有 typed-relation API 向后兼容：执行 `cargo test --features typed-relation` 确认 v3.6.0 既有 typed-relation 调用编译运行通过，无 breaking change
  - 关联需求：REQ-MAT-004
  - 关联设计：design.md §5.1.3 M3-T10 / §4.2 兼容性
  - 输入：M3-T3.1 的 stable 标注
  - 输出：typed-relation 既有 API 向后兼容
  - 涉及文件：无（验证任务）
  - 工作量：S（1h）
  - 依赖：M3-T3.1

- [ ] **M3-T10.2** 验证既有 sql-verify-proc API 向后兼容：执行 `cargo test --features sql-verify-proc` 确认 v3.6.0 既有 sql-verify 调用编译运行通过，无 breaking change
  - 关联需求：REQ-MAT-004
  - 关联设计：design.md §5.1.3 M3-T10
  - 输入：M3-T9.2 的 stable 标注
  - 输出：sql-verify-proc 既有 API 向后兼容
  - 涉及文件：无（验证任务）
  - 工作量：S（1h）
  - 依赖：M3-T9.2

## 4.11 M3-T11：无运行时开销验证

- [ ] **M3-T11.1** 执行性能基准对比：`cargo bench --features typed-relation,sql-verify-proc,custom-diagnostic` vs `cargo bench`（默认 feature），确认启用/禁用 feature 运行时性能无差异（均为编译期工作）
  - 关联需求：REQ-MAT-005
  - 关联设计：design.md §5.1.3 M3-T11 / §5.3.1 规则 5
  - 输入：M3-T10.2 的向后兼容验证
  - 输出：性能基准对比结果，无运行时开销
  - 涉及文件：无（验证任务）
  - 工作量：S（1h）
  - 依赖：M3-T10.2

- [ ] **M3-T11.2** 验证既有测试不回退：执行 `cargo test --workspace -j 2 --no-fail-fast` 确认既有测试全通过，v3.6.0 基线不回退
  - 关联需求：REQ-MAT-004
  - 关联设计：design.md §5.1.3 M3-T11 / §4.3
  - 输入：M3-T11.1 的性能基准
  - 输出：既有测试全通过，v3.6.0 基线不回退
  - 涉及文件：无（验证任务）
  - 工作量：S（1h）
  - 依赖：M3-T11.1

---

# 5. M4 方言扩展延续（REQ-DIALECT-001~005）

> **目标**：按需实现 Informix/SAP HANA/Firebird 方言（v3.5.0 路线图 v3.7.0 候选），通过 `dialect-informix`/`dialect-saphana`/`dialect-firebird` feature gate 隔离，新增 DbType 变体 + 路线图更新，既有 20 种方言测试不回退。
> **周期**：2 周
> **优先级**：中（中风险中收益，需 Rust 驱动成熟 + 用户需求）
> **关联设计**：design.md §5.1.4
> **关联验收**：spec §9.4

## 5.1 M4-T1：评估 Rust Informix 驱动成熟度

- [ ] **M4-T1.1** 调研 Rust Informix 驱动：搜索 crates.io Informix 驱动（如 `informix-rs` 等），评估驱动成熟度（最新版本/下载量/issue 数/是否维护/是否支持 SERIAL/ROW 类型/PUT 语句），输出评估结论
  - 关联需求：REQ-DIALECT-001
  - 关联设计：design.md §5.1.4 M4-T1 / §5.4.3 异常场景
  - 输入：crates.io + Informix 驱动调研
  - 输出：Informix 驱动成熟度评估结论（成熟/不成熟）
  - 涉及文件：`docs/dialect-evaluation.md`（评估文档）
  - 工作量：S（1.5h）
  - 依赖：M1-T1.1

- [ ] **M4-T1.2** 调研 Informix 用户需求：评估是否有用户提出 Informix 方言需求（GitHub issue/社区反馈），输出需求评估结论
  - 关联需求：REQ-DIALECT-001
  - 关联设计：design.md §5.1.4 M4-T1
  - 输入：GitHub issue + 社区反馈
  - 输出：Informix 用户需求评估结论（有需求/无需求）
  - 涉及文件：`docs/dialect-evaluation.md`
  - 工作量：S（0.5h）
  - 依赖：M4-T1.1

## 5.2 M4-T2：InformixDialect 实现（按需）

- [ ] **M4-T2.1** 在 `packages/sz-orm-core/Cargo.toml` 新增 `dialect-informix = []` feature 定义（默认关闭），附注释 `# v3.7.0: Informix 方言（按需实现，SERIAL/ROW 类型 + PUT 语句）`
  - 关联需求：REQ-DIALECT-001
  - 关联设计：design.md §5.1.4 M4-T2 / §3.1
  - 输入：M4-T1.2 的需求评估
  - 输出：dialect-informix feature 定义就绪
  - 涉及文件：`packages/sz-orm-core/Cargo.toml`
  - 工作量：S（0.5h）
  - 依赖：M4-T1.2

- [ ] **M4-T2.2** 在 `packages/sz-orm-core/src/dialect.rs` 实现 InformixDialect：`#[cfg(feature = "dialect-informix")] pub struct InformixDialect;` + 实现 Dialect trait（quote/escape_string/build_pagination/SERIAL/ROW 类型/PUT 语句），如驱动不成熟则仅实现 SQL 生成方言（标注"SQL generation only, no real DB driver"）
  - 关联需求：REQ-DIALECT-001
  - 关联设计：design.md §5.1.4 M4-T2 / §2.2.2.3
  - 输入：M4-T2.1 的 feature 定义 + 既有 Dialect trait（[dialect.rs:23](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L23)）
  - 输出：InformixDialect 实现就绪
  - 涉及文件：`packages/sz-orm-core/src/dialect.rs`
  - 工作量：L（4h）
  - 依赖：M4-T2.1

- [ ] **M4-T2.3** 编写 InformixDialect 测试 `tests/dialect_informix_test.rs`：`#![cfg(feature = "dialect-informix")]`，覆盖 Dialect trait 方法 + SERIAL/ROW 类型 + PUT 语句 SQL 生成，≥5 测试用例
  - 关联需求：REQ-DIALECT-001
  - 关联设计：design.md §5.1.4 M4-T2 / §6.1.4
  - 输入：M4-T2.2 的 InformixDialect 实现
  - 输出：InformixDialect 测试通过
  - 涉及文件：`packages/sz-orm-core/tests/dialect_informix_test.rs` + `Cargo.toml`
  - 工作量：M（2h）
  - 依赖：M4-T2.2

## 5.3 M4-T3：评估 Rust SAP HANA 驱动成熟度 + 企业需求

- [ ] **M4-T3.1** 调研 Rust SAP HANA 驱动 + 企业需求：搜索 crates.io SAP HANA 驱动，评估驱动成熟度 + 企业需求（GitHub issue/社区反馈/企业用户），输出评估结论
  - 关联需求：REQ-DIALECT-002
  - 关联设计：design.md §5.1.4 M4-T3 / §5.4.3
  - 输入：crates.io + SAP HANA 驱动调研 + 企业需求
  - 输出：SAP HANA 驱动成熟度 + 企业需求评估结论
  - 涉及文件：`docs/dialect-evaluation.md`
  - 工作量：S（1.5h）
  - 依赖：M1-T1.1

## 5.4 M4-T4：SapHanaDialect 实现（按需）

- [ ] **M4-T4.1** 如 SAP HANA 驱动成熟 + 企业需求出现，在 `packages/sz-orm-core/src/dialect.rs` 实现 SapHanaDialect（Dialect trait + 计算列 + CE 函数）+ `Cargo.toml` 新增 `dialect-saphana = []` feature + 测试 `tests/dialect_saphana_test.rs`；如不成熟或无需求则标注暂缓
  - 关联需求：REQ-DIALECT-002
  - 关联设计：design.md §5.1.4 M4-T4 / §2.2.2.3
  - 输入：M4-T3.1 的评估结论
  - 输出：SapHanaDialect 实现（或暂缓标注）
  - 涉及文件：`packages/sz-orm-core/src/dialect.rs` + `Cargo.toml` + `tests/dialect_saphana_test.rs`
  - 工作量：L（4h）
  - 依赖：M4-T3.1

## 5.5 M4-T5：评估 Firebird 用户需求

- [ ] **M4-T5.1** 调研 Firebird 用户需求：评估是否有用户提出 Firebird 方言需求（GitHub issue/社区反馈），输出需求评估结论
  - 关联需求：REQ-DIALECT-003
  - 关联设计：design.md §5.1.4 M4-T5 / §5.4.3
  - 输入：GitHub issue + 社区反馈
  - 输出：Firebird 用户需求评估结论
  - 涉及文件：`docs/dialect-evaluation.md`
  - 工作量：S（1h）
  - 依赖：M1-T1.1

## 5.6 M4-T6：FirebirdDialect 实现（按需）

- [ ] **M4-T6.1** 如 Firebird 用户需求出现，在 `packages/sz-orm-core/src/dialect.rs` 实现 FirebirdDialect（Dialect trait + GENERATOR/SEQUENCE + EXECUTE BLOCK）+ `Cargo.toml` 新增 `dialect-firebird = []` feature + 测试 `tests/dialect_firebird_test.rs`；如无需求则标注暂缓
  - 关联需求：REQ-DIALECT-003
  - 关联设计：design.md §5.1.4 M4-T6 / §2.2.2.3
  - 输入：M4-T5.1 的需求评估
  - 输出：FirebirdDialect 实现（或暂缓标注）
  - 涉及文件：`packages/sz-orm-core/src/dialect.rs` + `Cargo.toml` + `tests/dialect_firebird_test.rs`
  - 工作量：L（3h）
  - 依赖：M4-T5.1

## 5.7 M4-T7：DbType 新增变体 + 路线图更新

- [ ] **M4-T7.1** 在 `packages/sz-orm-core/src/db_type.rs` DbType 枚举新增变体（如实现）：`#[cfg(feature = "dialect-informix")] Informix,` + `#[cfg(feature = "dialect-saphana")] SapHana,` + `#[cfg(feature = "dialect-firebird")] Firebird,`，保持 `#[non_exhaustive]`，更新 `as_str`/`from_str`/`default_port` 方法
  - 关联需求：REQ-DIALECT-005
  - 关联设计：design.md §5.1.4 M4-T7 / §1.1.1
  - 输入：M4-T2.2 + M4-T4.1 + M4-T6.1 的方言实现
  - 输出：DbType 枚举新增变体 + 方法更新
  - 涉及文件：`packages/sz-orm-core/src/db_type.rs`
  - 工作量：M（1.5h）
  - 依赖：M4-T2.3 + M4-T4.1 + M4-T6.1

- [ ] **M4-T7.2** 更新方言扩展路线图：在 `docs/spec/v3.7.0/spec.md` §10.5 路线图标注 v3.7.0 已实现/暂缓状态，对比分析文档方言数量更新（20 + 新增数）
  - 关联需求：REQ-DIALECT-005
  - 关联设计：design.md §5.1.4 M4-T7
  - 输入：M4-T7.1 的 DbType 变体
  - 输出：路线图更新 + 对比分析文档方言数量更新
  - 涉及文件：`docs/spec/v3.7.0/spec.md` + `docs/sz-orm与同类产品对比分析.md`
  - 工作量：S（0.5h）
  - 依赖：M4-T7.1

## 5.8 M4-T8：既有 20 种方言不回退验证

- [ ] **M4-T8.1** 执行 `cargo test --workspace -j 2 --no-fail-fast`（默认 feature）验证既有 20 种方言测试全通过，与 v3.6.0 基线对比不回退
  - 关联需求：REQ-DIALECT-004
  - 关联设计：design.md §5.1.4 M4-T8 / §4.3
  - 输入：M4-T7.2 的路线图更新
  - 输出：既有 20 种方言测试全通过，v3.6.0 基线不回退
  - 涉及文件：无（验证任务）
  - 工作量：S（1h）
  - 依赖：M4-T7.2

- [ ] **M4-T8.2** 执行 `cargo test --features dialect-informix,dialect-saphana,dialect-firebird` 验证新方言测试通过（如实现），既有方言 SQL 生成快照与 v3.6.0 一致
  - 关联需求：REQ-DIALECT-004
  - 关联设计：design.md §5.1.4 M4-T8 / §6.1.4
  - 输入：M4-T8.1 的既有方言验证
  - 输出：新方言测试通过，既有方言 SQL 生成快照一致
  - 涉及文件：无（验证任务）
  - 工作量：S（1h）
  - 依赖：M4-T8.1

## 5.9 M4-T9：Cargo.toml feature gate 验证

- [ ] **M4-T9.1** 验证 Cargo.toml 新增 dialect-informix/saphana/firebird feature 定义正确 + `#[cfg(feature)]` 条件编译隔离正确 + 默认 feature 零行为变更（`cargo check --no-default-features` 通过）
  - 关联需求：REQ-DIALECT-001/002/003
  - 关联设计：design.md §5.1.4 M4-T9 / §3.3
  - 输入：M4-T8.2 的方言测试
  - 输出：feature gate 隔离正确，默认 feature 零行为变更
  - 涉及文件：无（验证任务）
  - 工作量：S（0.5h）
  - 依赖：M4-T8.2

---

# 6. M5 Snowflake/Redshift 真实云数据库验证（REQ-CLOUD-001~003）

> **目标**：补齐 v3.6.0 Snowflake/Redshift 方言仅 SQL 生成测试的缺口，连真实 Snowflake/Redshift 云实例验证 SQL 行为一致性（UPSERT/TIME TRAVEL/VARIANT/COPY/UNLOAD/PG 兼容性），或评估无可用云实例时输出验证缺口报告 + 替代方案。
> **周期**：1 周
> **优先级**：中（中风险中收益，需云实例可用，无实例时输出缺口报告）
> **关联设计**：design.md §5.1.5
> **关联验收**：spec §9.5

## 6.1 M5-T1：评估 Snowflake 云实例可用性

- [ ] **M5-T1.1** 调研 Snowflake 云实例可用性：检查是否有 Snowflake 云账号/实例可达（如 AWS Snowflake/Azure Snowflake），评估免费试用账号/共享实例/本地 Snowflake 模拟（如 Snowflake Developer Edition），输出可用性评估结论
  - 关联需求：REQ-CLOUD-001
  - 关联设计：design.md §5.1.5 M5-T1 / §5.5.3 异常场景
  - 输入：Snowflake 云账号/实例调研
  - 输出：Snowflake 云实例可用性评估结论（可用/不可用）
  - 涉及文件：`docs/snowflake-cloud-verification.md`（验证报告）
  - 工作量：S（1.5h）
  - 依赖：M1-T1.1

- [ ] **M5-T1.2** 如 Snowflake 云实例可用，准备验证环境：配置连接串 + 创建测试库 + 准备测试数据，如不可用则跳过
  - 关联需求：REQ-CLOUD-001
  - 关联设计：design.md §5.1.5 M5-T1
  - 输入：M5-T1.1 的可用性评估
  - 输出：Snowflake 验证环境就绪（如可用）
  - 涉及文件：无（环境准备）
  - 工作量：S（0.5h）
  - 依赖：M5-T1.1

## 6.2 M5-T2：Snowflake 真实云验证（如可用）

- [ ] **M5-T2.1** 如 Snowflake 云实例可用，连真实 Snowflake 验证 UPSERT/TIME TRAVEL/VARIANT 类型行为一致性：执行 SnowflakeDialect 生成的 SQL + 断言结果与预期一致 + 与 SQL 生成测试对比差异，≥3 验证用例（UPSERT/TIME TRAVEL/VARIANT）；如不可用则跳过
  - 关联需求：REQ-CLOUD-001
  - 关联设计：design.md §5.1.5 M5-T2 / §2.2.2.5
  - 输入：M5-T1.2 的验证环境 + 既有 SnowflakeDialect（[dialect.rs:1567](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1567)）
  - 输出：Snowflake 真实云验证结果（如可用）
  - 涉及文件：`docs/snowflake-cloud-verification.md`
  - 工作量：L（3h）
  - 依赖：M5-T1.2

## 6.3 M5-T3：Snowflake 验证缺口报告（如不可用）

- [ ] **M5-T3.1** 如 Snowflake 云实例不可用，在 `docs/snowflake-cloud-verification.md` 输出验证缺口报告：标注"cloud verification pending: no accessible instance" + 替代方案（本地 Snowflake 模拟/SQL 生成 + 人工审核）+ 现有 SQL 生成测试通过证据
  - 关联需求：REQ-CLOUD-001
  - 关联设计：design.md §5.1.5 M5-T3 / §5.5.3
  - 输入：M5-T1.1 的可用性评估（不可用）
  - 输出：Snowflake 验证缺口报告 + 替代方案
  - 涉及文件：`docs/snowflake-cloud-verification.md`
  - 工作量：S（1h）
  - 依赖：M5-T1.1

## 6.4 M5-T4：评估 Redshift 云实例可用性 + 真实云验证

- [ ] **M5-T4.1** 调研 Redshift 云实例可用性：检查是否有 AWS Redshift Serverless/Provisioned 实例可达，输出可用性评估结论
  - 关联需求：REQ-CLOUD-002
  - 关联设计：design.md §5.1.5 M5-T4 / §5.5.3
  - 输入：AWS Redshift 云账号/实例调研
  - 输出：Redshift 云实例可用性评估结论
  - 涉及文件：`docs/redshift-cloud-verification.md`
  - 工作量：S（1.5h）
  - 依赖：M1-T1.1

- [ ] **M5-T4.2** 如 Redshift 云实例可用，连真实 Redshift 验证 COPY/UNLOAD/PG 兼容性行为一致性，≥3 验证用例（COPY/UNLOAD/PG 兼容性）；如不可用则输出验证缺口报告 + 替代方案
  - 关联需求：REQ-CLOUD-002
  - 关联设计：design.md §5.1.5 M5-T4 / §2.2.2.5
  - 输入：M5-T4.1 的可用性评估 + 既有 RedshiftDialect（[dialect.rs:1794](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L1794)）
  - 输出：Redshift 真实云验证结果（如可用）或缺口报告
  - 涉及文件：`docs/redshift-cloud-verification.md`
  - 工作量：L（3h）
  - 依赖：M5-T4.1

## 6.5 M5-T5：Redshift 验证缺口报告（如不可用）

- [ ] **M5-T5.1** 如 Redshift 云实例不可用，在 `docs/redshift-cloud-verification.md` 输出验证缺口报告：标注"cloud verification pending: no accessible instance" + 替代方案 + 现有 SQL 生成测试通过证据
  - 关联需求：REQ-CLOUD-002
  - 关联设计：design.md §5.1.5 M5-T5 / §5.5.3
  - 输入：M5-T4.1 的可用性评估（不可用）
  - 输出：Redshift 验证缺口报告 + 替代方案
  - 涉及文件：`docs/redshift-cloud-verification.md`
  - 工作量：S（1h）
  - 依赖：M5-T4.1

## 6.6 M5-T6：验证报告文档化

- [ ] **M5-T6.1** 在 `docs/snowflake-cloud-verification.md` + `docs/redshift-cloud-verification.md` 文档化验证报告：验证环境 + 验证用例 + 验证结果 + 与 SQL 生成测试的差异分析（如有）+ 替代方案（如不可用）
  - 关联需求：REQ-CLOUD-003
  - 关联设计：design.md §5.1.5 M5-T6 / §2.2.2.5
  - 输入：M5-T2.1 + M5-T3.1 + M5-T4.2 + M5-T5.1 的验证结果/缺口报告
  - 输出：验证报告文档化完成
  - 涉及文件：`docs/snowflake-cloud-verification.md` + `docs/redshift-cloud-verification.md`
  - 工作量：S（1.5h）
  - 依赖：M5-T2.1 + M5-T3.1 + M5-T4.2 + M5-T5.1

## 6.7 M5-T7：验证报告审查

- [ ] **M5-T7.1** 审查验证报告完整性：确认报告含验证环境/用例/结果/差异分析/替代方案（如不可用），验证结果与 SQL 生成测试对比一致（如可用）
  - 关联需求：REQ-CLOUD-003
  - 关联设计：design.md §5.1.5 M5-T7 / §6.1.5
  - 输入：M5-T6.1 的验证报告
  - 输出：验证报告审查通过
  - 涉及文件：无（审查任务）
  - 工作量：S（0.5h）
  - 依赖：M5-T6.1

---

# 7. M6 工程规范化（REQ-ENG-001~004）

> **目标**：将 v3.6.0 未提交工作（213 文件）按里程碑分组提交 git，Prisma 方言评估结论落地文档，14 道门禁全通过，无占位实现，sz-pay 升级到 3.7.0 零回归验证。
> **周期**：1 周
> **优先级**：低但必须（低风险，git 入库 + Prisma 评估落地 + 门禁）
> **关联设计**：design.md §5.1.6
> **关联验收**：spec §9.6

## 7.1 M6-T1：v3.6.0 未提交工作入库（按里程碑分组）

- [ ] **M6-T1.1** 提交 v3.6.0 M1 编译期类型安全工作：`git add` M1 相关文件（typed_ast.rs 新表达式 + typed_relation.rs + sql_verify.rs + diagnostic.rs + 测试），`git commit -m "feat(v3.6.0): M1 编译期类型安全深入优化（15 新表达式 + typed relation + 自定义诊断 + proc-macro SQL 验证）"`，提交后运行 `cargo check --workspace` 验证
  - 关联需求：REQ-ENG-001
  - 关联设计：design.md §5.1.6 M6-T1 / §1.1.2
  - 输入：v3.6.0 M1 未提交文件
  - 输出：M1 工作入库，提交后编译通过
  - 涉及文件：v3.6.0 M1 相关文件
  - 工作量：S（1h）
  - 依赖：M1-T13.2 + M2-T7.1 + M3-T11.2 + M4-T9.1 + M5-T7.1

- [ ] **M6-T1.2** 提交 v3.6.0 M2 文档补齐工作：`git add` M2 相关文件（195 missing_docs 补齐 + lib.rs `#![warn(missing_docs)]`），`git commit -m "docs(v3.6.0): M2 313 pub API 文档补齐（195 missing_docs 全补齐）"`，提交后运行门禁验证
  - 关联需求：REQ-ENG-001
  - 关联设计：design.md §5.1.6 M6-T1
  - 输入：v3.6.0 M2 未提交文件
  - 输出：M2 工作入库
  - 涉及文件：v3.6.0 M2 相关文件
  - 工作量：S（1h）
  - 依赖：M6-T1.1

- [ ] **M6-T1.3** 提交 v3.6.0 M3 QueryBuilder 渐进合并工作：`git add` M3 相关文件（lint + fix + 差分测试），`git commit -m "feat(v3.6.0): M3 QueryBuilder 渐进合并（lint + fix + 差分测试 47 测试）"`，提交后运行门禁验证
  - 关联需求：REQ-ENG-001
  - 关联设计：design.md §5.1.6 M6-T1
  - 输入：v3.6.0 M3 未提交文件
  - 输出：M3 工作入库
  - 涉及文件：v3.6.0 M3 相关文件
  - 工作量：S（1h）
  - 依赖：M6-T1.2

- [ ] **M6-T1.4** 提交 v3.6.0 M4 方言扩展工作：`git add` M4 相关文件（SnowflakeDialect + RedshiftDialect + DbType 变体 + 测试），`git commit -m "feat(v3.6.0): M4 方言扩展（Snowflake + Redshift 20 种方言 32 测试）"`，提交后运行门禁验证
  - 关联需求：REQ-ENG-001
  - 关联设计：design.md §5.1.6 M6-T1
  - 输入：v3.6.0 M4 未提交文件
  - 输出：M4 工作入库
  - 涉及文件：v3.6.0 M4 相关文件
  - 工作量：S（1h）
  - 依赖：M6-T1.3

- [ ] **M6-T1.5** 提交 v3.6.0 M5 async trait 重评估工作 + v3.7.0 全部工作：`git add` M5 相关文件（async-trait-evaluation.md）+ v3.7.0 全部新增文件（e2e_real_db_*.rs + 对比分析文档更新 + 探索能力成熟化 + 方言扩展 + 云数仓验证报告），`git commit -m "feat(v3.6.0): M5 async trait 重评估（保持方案 C）"` + `git commit -m "feat(v3.7.0): 真实 DB e2e 测试体系 + 对比分析重评估 + 探索能力成熟化 + 方言扩展 + 云数仓验证"`，提交后运行门禁验证
  - 关联需求：REQ-ENG-001
  - 关联设计：design.md §5.1.6 M6-T1
  - 输入：v3.6.0 M5 + v3.7.0 全部未提交文件
  - 输出：v3.6.0 + v3.7.0 全部工作入库，`git status` 无未提交残留
  - 涉及文件：v3.6.0 M5 + v3.7.0 全部文件
  - 工作量：M（2h）
  - 依赖：M6-T1.4

## 7.2 M6-T2：Prisma 评估结论落地

- [ ] **M6-T2.1** 审查 `docs/prisma-dialect-evaluation.md` 评估结论：确认评估文档含可行性结论/推荐方案/实现计划或不可行理由，如结论为不可行则标注"不实施，跨生态兼容难度高收益低"+ 理由，如可行则输出实现计划
  - 关联需求：REQ-ENG-002
  - 关联设计：design.md §5.1.6 M6-T2 / §1.1.1
  - 输入：既有 [docs/prisma-dialect-evaluation.md:1](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/prisma-dialect-evaluation.md#L1)（v3.6.0 评估文档）
  - 输出：Prisma 评估结论落地标注
  - 涉及文件：`docs/prisma-dialect-evaluation.md`
  - 工作量：S（1h）
  - 依赖：M6-T1.5

- [ ] **M6-T2.2** 在 `docs/prisma-dialect-evaluation.md` 新增 v3.7.0 落地章节：标注正式落地结论（可行性/推荐方案/实现计划或不可行理由）+ v3.8.0 候选状态（如可行则标注 v3.8.0 候选，如不可行则标注不实施）
  - 关联需求：REQ-ENG-002
  - 关联设计：design.md §5.1.6 M6-T2
  - 输入：M6-T2.1 的评估结论
  - 输出：Prisma 评估结论落地文档完整
  - 涉及文件：`docs/prisma-dialect-evaluation.md`
  - 工作量：S（1h）
  - 依赖：M6-T2.1

## 7.3 M6-T3：14 道门禁运行

- [ ] **M6-T3.1** 运行门禁 1-4（fmt/check/clippy/test）：`cargo fmt --all -- --check` + `cargo check --workspace --all-targets` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace -j 2 --no-fail-fast`，全部通过
  - 关联需求：REQ-ENG-003
  - 关联设计：design.md §5.1.6 M6-T3 / §6.1.6
  - 输入：M6-T1.5 的全部工作入库
  - 输出：门禁 1-4 全通过
  - 涉及文件：无（门禁运行）
  - 工作量：S（1h）
  - 依赖：M6-T1.5

- [ ] **M6-T3.2** 运行门禁 5-7（doc/audit/integration）：`cargo doc --workspace --no-deps --all-features` + `cargo audit` + `cargo deny check` + `cargo test --workspace -- --ignored`，全部通过
  - 关联需求：REQ-ENG-003
  - 关联设计：design.md §5.1.6 M6-T3
  - 输入：M6-T3.1 的门禁 1-4
  - 输出：门禁 5-7 全通过
  - 涉及文件：无（门禁运行）
  - 工作量：S（1h）
  - 依赖：M6-T3.1

- [ ] **M6-T3.3** 运行门禁 8-10（占位检查/SQL 注入/feature 全组合）：`grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'`（无匹配）+ `scripts/check-sql-injection.ps1` + `cargo check --workspace --all-targets --all-features`，全部通过
  - 关联需求：REQ-ENG-003/004
  - 关联设计：design.md §5.1.6 M6-T3
  - 输入：M6-T3.2 的门禁 5-7
  - 输出：门禁 8-10 全通过
  - 涉及文件：无（门禁运行）
  - 工作量：S（1h）
  - 依赖：M6-T3.2

- [ ] **M6-T3.4** 运行门禁 11-14（上游未改/文档一致性/审计证据/文档同步）：`git diff --name-only HEAD`（ADR-0001）+ `python scripts/check-doc-consistency.py` + `bash scripts/audit-verify.sh <审计报告.md>` + `python scripts/check-doc-sync.py --diff HEAD`，全部通过
  - 关联需求：REQ-ENG-003
  - 关联设计：design.md §5.1.6 M6-T3
  - 输入：M6-T3.3 的门禁 8-10
  - 输出：门禁 11-14 全通过，14 道门禁全通过
  - 涉及文件：无（门禁运行）
  - 工作量：S（1h）
  - 依赖：M6-T3.3

## 7.4 M6-T4：占位实现检查

- [ ] **M6-T4.1** 执行 `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` 确认无匹配（无占位实现），如发现匹配则修复后重新检查
  - 关联需求：REQ-ENG-004
  - 关联设计：design.md §5.1.6 M6-T4 / §5.6.1 规则 4
  - 输入：M6-T3.4 的门禁通过
  - 输出：无占位实现
  - 涉及文件：无（检查任务）
  - 工作量：S（0.5h）
  - 依赖：M6-T3.4

## 7.5 M6-T5：sz-pay 零回归验证

- [ ] **M6-T5.1** 复制 sz-pay 项目到临时验证目录 `E:\vue\test\sz-pay-upgrade-verify\`，修改临时目录 `Cargo.toml` 将 sz-orm-* 版本号从 3.6.0 改为 3.7.0
  - 关联需求：REQ-ENG-003
  - 关联设计：design.md §5.1.6 M6-T5 / §6.3
  - 输入：sz-pay 项目（`E:\vue\test\sz-pay\server\sz-rust\`）
  - 输出：临时验证目录就绪，版本号改为 3.7.0
  - 涉及文件：`E:\vue\test\sz-pay-upgrade-verify\`（临时目录）
  - 工作量：S（0.5h）
  - 依赖：M6-T4.1

- [ ] **M6-T5.2** 在临时验证目录执行 `cargo check` + `cargo test -j 2 --no-fail-fast` 验证 sz-pay 升级到 3.7.0 零回归，设置 `$env:RUST_MIN_STACK="67108864"` + `$env:CARGO_INCREMENTAL=0`，验证结果记录到 `logs/sz-pay-upgrade-<timestamp>.log`
  - 关联需求：REQ-ENG-003
  - 关联设计：design.md §5.1.6 M6-T5 / §6.3
  - 输入：M6-T5.1 的临时验证目录
  - 输出：sz-pay cargo check + cargo test 零回归（0 failed）
  - 涉及文件：`logs/sz-pay-upgrade-<timestamp>.log`
  - 工作量：S（1h）
  - 依赖：M6-T5.1

- [ ] **M6-T5.3** 删除临时验证目录 `E:\vue\test\sz-pay-upgrade-verify\`，不提交任何 sz-pay 修改（ADR-0001 严禁修改下游仓库）
  - 关联需求：REQ-ENG-003
  - 关联设计：design.md §5.1.6 M6-T5 / §6.3
  - 输入：M6-T5.2 的零回归验证结果
  - 输出：临时目录删除，sz-pay 无修改
  - 涉及文件：无（清理任务）
  - 工作量：S（0.5h）
  - 依赖：M6-T5.2

---

# 8. 任务依赖关系图

## 8.1 里程碑间依赖

```plantuml
@startuml
!theme plain
title v3.7.0 里程碑依赖关系

rectangle "M1 真实 DB e2e 测试体系\n(3周, 最高优先级, 中风险)" as M1
rectangle "M2 对比分析重评估\n(1周, 高优先级, 低风险)" as M2
rectangle "M3 v3.6.0 探索能力成熟化\n(2周, 高优先级, 低风险)" as M3
rectangle "M4 方言扩展延续\n(2周, 中优先级, 中风险)" as M4
rectangle "M5 Snowflake/Redshift 真实云验证\n(1周, 中优先级, 中风险)" as M5
rectangle "M6 工程规范化\n(1周, 低但必须, 低风险)" as M6

M1 --> M2 : 真实 DB e2e 测试就绪后评分更准确
M1 --> M3 : feature gate 体系就绪
M1 --> M4 : feature gate 体系就绪
M1 --> M5 : feature gate 体系就绪
M2 --> M6 : M2 完成后入库
M3 --> M6 : M3 完成后入库
M4 --> M6 : M4 完成后入库
M5 --> M6 : M5 完成后入库

note bottom of M1
  **关键路径**：M1 → M2 → M6（串行 5 周）
  **并行机会**：
  - M1 完成后 M3/M4/M5 可并行（2 周并行）
  - M1 内部：8 类核心路径测试可部分并行
  - M3 内部：typed relation + 自定义诊断 + proc-macro SQL 验证可并行
  - M4 内部：Informix + SAP HANA + Firebird 评估可并行
  - M5 内部：Snowflake + Redshift 验证可并行
  **总周期**：关键路径 5 周；并行开发下可压缩至 4-5 周
end note

@enduml
```

## 8.2 M1 内部任务依赖

```plantuml
@startuml
!theme plain
title M1 真实 DB e2e 测试体系任务依赖

M1_T1_1 --> M1_T1_2 : 默认 feature 验证
M1_T1_1 --> M1_T10_1 : 清理机制
M1_T1_1 --> M1_T11_1 : 超时机制

M1_T10_1 --> M1_T10_2 : 事务回滚清理
M1_T11_1 --> M1_T11_2 : 超时验证

M1_T2_1 --> M1_T2_2 --> M1_T2_3
M1_T3_1 --> M1_T3_2
M1_T4_1 --> M1_T4_2
M1_T5_1 --> M1_T5_2
M1_T8_1 --> M1_T8_2
M1_T9_1 --> M1_T9_2

M1_T2_1 --> M1_T10_1
M1_T2_3 --> M1_T12_1
M1_T3_2 --> M1_T12_1
M1_T4_2 --> M1_T12_1
M1_T5_2 --> M1_T12_1
M1_T6_1 --> M1_T12_1
M1_T7_1 --> M1_T12_1
M1_T8_2 --> M1_T12_1
M1_T9_2 --> M1_T12_1

M1_T12_1 --> M1_T13_1
M1_T13_1 --> M1_T13_2
M1_T11_2 --> M1_T13_2

@enduml
```

## 8.3 M2 内部任务依赖

```plantuml
@startuml
!theme plain
title M2 对比分析重评估任务依赖

M1_T13_2 --> M2_T1_1 : M1 完成后开始
M2_T1_1 --> M2_T2_1 --> M2_T2_2 --> M2_T2_3 --> M2_T2_4 --> M2_T2_5
M2_T2_5 --> M2_T3_1 --> M2_T3_2 --> M2_T3_3
M2_T3_3 --> M2_T4_1 --> M2_T4_2
M2_T4_2 --> M2_T5_1
M2_T5_1 --> M2_T6_1 --> M2_T6_2
M2_T6_2 --> M2_T7_1

@enduml
```

## 8.4 M3/M4/M5 内部任务依赖

```plantuml
@startuml
!theme plain
title M3/M4/M5 任务依赖

package M3 {
  M1_T1_1 --> M3_T1_1
  M3_T1_1 --> M3_T1_2 --> M3_T1_3
  M3_T1_3 --> M3_T2_1 --> M3_T2_2
  M3_T2_2 --> M3_T3_1
  M3_T3_1 --> M3_T10_1
  
  M1_T1_1 --> M3_T4_1 --> M3_T4_2
  M3_T4_2 --> M3_T5_1 --> M3_T5_2
  M3_T5_2 --> M3_T6_1 --> M3_T6_2
  
  M1_T1_1 --> M3_T7_1 --> M3_T7_2
  M3_T7_2 --> M3_T8_1 --> M3_T8_2
  M3_T8_2 --> M3_T9_1 --> M3_T9_2
  M3_T9_2 --> M3_T10_2
  M3_T10_2 --> M3_T11_1 --> M3_T11_2
}

package M4 {
  M1_T1_1 --> M4_T1_1 --> M4_T1_2
  M4_T1_2 --> M4_T2_1 --> M4_T2_2 --> M4_T2_3
  
  M1_T1_1 --> M4_T3_1 --> M4_T4_1
  M1_T1_1 --> M4_T5_1 --> M4_T6_1
  
  M4_T2_3 --> M4_T7_1
  M4_T4_1 --> M4_T7_1
  M4_T6_1 --> M4_T7_1
  M4_T7_1 --> M4_T7_2 --> M4_T8_1 --> M4_T8_2 --> M4_T9_1
}

package M5 {
  M1_T1_1 --> M5_T1_1 --> M5_T1_2
  M5_T1_2 --> M5_T2_1
  M5_T1_1 --> M5_T3_1
  M5_T1_1 --> M5_T4_1 --> M5_T4_2
  M5_T4_1 --> M5_T5_1
  M5_T2_1 --> M5_T6_1
  M5_T3_1 --> M5_T6_1
  M5_T4_2 --> M5_T6_1
  M5_T5_1 --> M5_T6_1
  M5_T6_1 --> M5_T7_1
}

@enduml
```

## 8.5 M6 内部任务依赖

```plantuml
@startuml
!theme plain
title M6 工程规范化任务依赖

M1_T13_2 --> M6_T1_1
M2_T7_1 --> M6_T1_1
M3_T11_2 --> M6_T1_1
M4_T9_1 --> M6_T1_1
M5_T7_1 --> M6_T1_1

M6_T1_1 --> M6_T1_2 --> M6_T1_3 --> M6_T1_4 --> M6_T1_5
M6_T1_5 --> M6_T2_1 --> M6_T2_2
M6_T1_5 --> M6_T3_1 --> M6_T3_2 --> M6_T3_3 --> M6_T3_4
M6_T3_4 --> M6_T4_1
M6_T4_1 --> M6_T5_1 --> M6_T5_2 --> M6_T5_3

@enduml
```

---

# 9. 关键路径分析

## 9.1 关键路径

```
M1（3 周）→ M2（1 周）→ M6（1 周）= 5 周
```

**关键路径任务序列**：

1. M1-T1.1（e2e-real-db feature gate）→ M1-T1.2（默认 feature 验证）
2. M1-T2.1~M1-T2.3（CRUD 测试套件）
3. M1-T3.1~M1-T3.2（事务测试套件）
4. M1-T4.1~M1-T4.2（预加载测试套件）
5. M1-T5.1~M1-T5.2（分页测试套件）
6. M1-T6.1（软删除测试套件）
7. M1-T7.1（多租户测试套件）
8. M1-T8.1~M1-T8.2（缓存测试套件）
9. M1-T9.1~M1-T9.2（方言行为测试套件）
10. M1-T10.1~M1-T10.2（清理机制）
11. M1-T11.1~M1-T11.2（超时机制）
12. M1-T12.1（DATABASE_URL 验证）
13. M1-T13.1~M1-T13.2（既有测试不回退）
14. M2-T1.1（文档头部更新）→ M2-T2.1~M2-T2.5（13 维度重新评分）
15. M2-T3.1~M2-T3.3（§6 已知不足更新）→ M2-T4.1~M2-T4.2（§7 结论更新）
16. M2-T5.1（§5 独特优势更新）→ M2-T6.1~M2-T6.2（审计验证）→ M2-T7.1（纯文档验证）
17. M6-T1.1~M6-T1.5（v3.6.0 + v3.7.0 工作入库）
18. M6-T2.1~M6-T2.2（Prisma 评估落地）
19. M6-T3.1~M6-T3.4（14 道门禁）
20. M6-T4.1（占位实现检查）
21. M6-T5.1~M6-T5.3（sz-pay 零回归验证）

## 9.2 并行机会

| 并行段 | 可并行任务 | 周期 |
|--------|-----------|------|
| M1 内部 | 8 类核心路径测试（M1-T2~M1-T9）可部分并行 | 3 周（并行可压缩至 2 周） |
| M1 完成后 | M3/M4/M5 可并行（feature gate 体系就绪） | 2 周（并行） |
| M3 内部 | typed relation + 自定义诊断 + proc-macro SQL 验证可并行 | 2 周（并行可压缩至 1.5 周） |
| M4 内部 | Informix + SAP HANA + Firebird 评估可并行 | 2 周（并行可压缩至 1 周） |
| M5 内部 | Snowflake + Redshift 验证可并行 | 1 周（并行） |

## 9.3 总周期

- **关键路径**：M1 → M2 → M6 = 5 周
- **并行开发**：M1 完成后 M3/M4/M5 并行（2 周），总周期可压缩至 4-5 周
- **最乐观估计**：4 周（M1 内部 + M3/M4/M5 充分并行）
- **最保守估计**：5 周（关键路径串行）

---

# 10. 风险任务标注

## 10.1 高风险任务

| 任务 ID | 任务描述 | 风险 | 概率 | 影响 | 缓解措施 |
|---------|---------|------|------|------|---------|
| M1-T2.2 | CRUD insert 测试（连真实 DB） | 本机 DB 不可用 | 中 | 中 | feature gate 默认关闭缓解，CI 预置 DB 实例，本地无 DB 可跳过 |
| M1-T9.1 | UPSERT 行为一致性测试 | 方言行为差异 | 中 | 中 | 按方言分派，不支持的方言返回 Err，文档标注支持矩阵 |
| M4-T2.2 | InformixDialect 实现 | Rust Informix 驱动不成熟 | 高 | 中 | 仅实现 SQL 生成方言，标注"SQL generation only, no real DB driver" |
| M4-T4.1 | SapHanaDialect 实现 | Rust SAP HANA 驱动不成熟 + 企业需求未出现 | 高 | 中 | 评估不成熟或无需求则标注暂缓 |
| M5-T2.1 | Snowflake 真实云验证 | Snowflake 云实例不可用 | 高 | 中 | 输出验证缺口报告 + 替代方案，不阻断交付 |
| M5-T4.2 | Redshift 真实云验证 | Redshift 云实例不可用 | 高 | 中 | 输出验证缺口报告 + 替代方案，不阻断交付 |
| M6-T5.2 | sz-pay 零回归验证 | sz-pay 升级到 3.7.0 回归 | 低 | 高 | 回退 sz-pay 版本号，分析失败原因，修复后再升级或维持 3.6.0 |

## 10.2 中风险任务

| 任务 ID | 任务描述 | 风险 | 概率 | 影响 | 缓解措施 |
|---------|---------|------|------|------|---------|
| M1-T11.2 | 测试超时验证（单方言 ≤60s） | 真实 DB e2e 测试超时 | 低 | 中 | tokio::time::timeout 超时标记失败，输出卡点 |
| M3-T7.1 | proc-macro SQL 验证扩展连真 DB EXPLAIN | 编译时间显著增加 | 中 | 中 | 缓存验证结果（按 SQL 哈希缓存），仅 SQL 变更时重新验证，默认关闭 |
| M3-T1.2 | typed relation 编译期外键校验测试 | 编译期校验误报（合法关联被误判） | 低 | 中 | 提供 escape hatch（运行时关联回退 EagerLoader），补充回归测试 |
| M6-T3.4 | 门禁 11-14（上游未改/文档一致性/审计证据/文档同步） | 14 道门禁失败 | 中 | 高 | 输出失败门禁列表与修复建议，阻断交付，修复后重新运行 |

## 10.3 低风险任务

| 任务 ID | 任务描述 | 风险 | 概率 | 影响 | 缓解措施 |
|---------|---------|------|------|------|---------|
| M1-T10.1 | 测试清理机制 | DROP TABLE 权限不足 | 低 | 低 | 输出清理失败警告，不影响测试结果判定 |
| M2-T2.5 | 13 维度重新评分（9 维度无变化） | 评分与证据矛盾 | 低 | 中 | 审计验证脚本检测证据真实性，矛盾时修正评分或补充证据 |
| M2-T6.1 | 审计验证脚本运行 | v3.6.0 代码证据不存在 | 低 | 中 | 审计验证脚本检测，标注证据验证失败，修正引用 |
| M4-T8.1 | 既有 20 种方言不回退验证 | 新方言破坏既有方言 SQL 生成 | 低 | 中 | feature gate 隔离，既有 20 种方言测试不回退验证 |
| M6-T1.5 | v3.6.0 + v3.7.0 全部工作入库 | 213 文件合并冲突 | 低 | 中 | 按里程碑分组提交，每次提交后运行门禁验证 |

---

# 11. 验收标准映射：28 条 EARS 需求 → 任务映射

| 需求编号 | 需求描述 | 优先级 | 关联任务 | 验收条件 |
|---------|---------|--------|---------|---------|
| REQ-E2E-001 | 真实 DB e2e 测试连真实数据库 | high | M1-T2.1~M1-T2.3 + M1-T3.1~M1-T3.2 + M1-T4.1~M1-T4.2 + M1-T5.1~M1-T5.2 + M1-T6.1 + M1-T7.1 + M1-T8.1~M1-T8.2 + M1-T9.1~M1-T9.2 + M1-T11.1~M1-T11.2 + M1-T13.2 | 连真实 DB + 8 类核心路径 + ≥56 测试用例 + 超时控制 |
| REQ-E2E-002 | 覆盖 8 类核心业务路径 | high | M1-T2~M1-T9 | CRUD/事务/预加载/分页/软删除/多租户/缓存/方言行为 |
| REQ-E2E-003 | feature gate 隔离默认关闭 | high | M1-T1.1~M1-T1.2 | e2e-real-db feature gate 默认关闭 + 无 DB 环境编译通过 |
| REQ-E2E-004 | 幂等且隔离 | high | M1-T10.1~M1-T10.2 | DROP TABLE IF EXISTS + 独立表名 + 事务回滚 |
| REQ-E2E-005 | 复用既有基础设施 | medium | M1-T2.1 + M1-T13.1 | 复用 tests/common/ adapter + 既有测试不回退 |
| REQ-E2E-006 | 禁止连生产库 | high | M1-T12.1 | DATABASE_URL 不硬编码 + 测试库 sz_orm_test |
| REQ-REEVAL-001 | 文档更新到 v3.6.0 基线 | high | M2-T1.1 | 头部版本/日期/代码基线更新到 v3.6.0 |
| REQ-REEVAL-002 | 逐维度重新评分附证据 | high | M2-T2.1~M2-T2.5 + M2-T6.1~M2-T6.2 | 13 维度重新评分 + 每条变更附 file:line 证据 + 审计验证通过 |
| REQ-REEVAL-003 | 已知不足标注改进状态 | high | M2-T3.1~M2-T3.3 | §6 各子节标注 v3.6.0 改进状态（✅/⚠️/❌）+ 证据 |
| REQ-REEVAL-004 | 结论与建议更新 | medium | M2-T4.1~M2-T4.2 + M2-T5.1 | §7 结论与建议更新 + §5 独特优势更新 |
| REQ-REEVAL-005 | 纯文档不改变代码 | high | M2-T7.1 | git diff 确认仅文档变更，无 .rs 代码变更 |
| REQ-MAT-001 | typed relation 转正式 feature | high | M3-T1.1~M3-T1.3 + M3-T2.1~M3-T2.2 + M3-T3.1 | typed-relation stable + 测试 ≥10 + 文档完整 |
| REQ-MAT-002 | 自定义诊断转正式 feature | high | M3-T4.1~M3-T4.2 + M3-T5.1~M3-T5.2 + M3-T6.1~M3-T6.2 | custom-diagnostic stable + 测试 ≥10 + 文档完整 |
| REQ-MAT-003 | proc-macro SQL 验证转正式 feature | high | M3-T7.1~M3-T7.2 + M3-T8.1~M3-T8.2 + M3-T9.1~M3-T9.2 | sql-verify-proc stable + 连真 DB EXPLAIN 覆盖所有路径 + 测试 ≥10 |
| REQ-MAT-004 | 不破坏 v3.6.0 既有 API | high | M3-T10.1~M3-T10.2 + M3-T11.2 | 既有 API 向后兼容 + 既有测试不回退 |
| REQ-MAT-005 | 无运行时开销 | medium | M3-T11.1 | 性能基准对比，启用/禁用 feature 运行时性能无差异 |
| REQ-DIALECT-001 | Informix 方言按需实现 | medium | M4-T1.1~M4-T1.2 + M4-T2.1~M4-T2.3 | InformixDialect 实现（或标注暂缓）+ feature gate 隔离 |
| REQ-DIALECT-002 | SAP HANA 方言按需实现 | medium | M4-T3.1 + M4-T4.1 | SapHanaDialect 实现（或标注暂缓）+ feature gate 隔离 |
| REQ-DIALECT-003 | Firebird 方言按需实现 | medium | M4-T5.1 + M4-T6.1 | FirebirdDialect 实现（或标注暂缓）+ feature gate 隔离 |
| REQ-DIALECT-004 | 既有方言不回退 | high | M4-T8.1~M4-T8.2 | 既有 20 种方言测试全通过 + SQL 生成快照一致 |
| REQ-DIALECT-005 | 更新 DbType 与路线图 | medium | M4-T7.1~M4-T7.2 | DbType 新增变体 + 路线图标注 v3.7.0 状态 |
| REQ-CLOUD-001 | Snowflake 真实云验证 | medium | M5-T1.1~M5-T1.2 + M5-T2.1 + M5-T3.1 | Snowflake 真实云验证（或缺口报告 + 替代方案） |
| REQ-CLOUD-002 | Redshift 真实云验证 | medium | M5-T4.1~M5-T4.2 + M5-T5.1 | Redshift 真实云验证（或缺口报告） |
| REQ-CLOUD-003 | 验证结果文档化 | medium | M5-T6.1~M5-T7.1 | 验证报告附测试用例/结果/差异分析 |
| REQ-ENG-001 | v3.6.0 未提交工作入库 | high | M6-T1.1~M6-T1.5 | 213 文件按里程碑分组提交 + git status 无残留 |
| REQ-ENG-002 | Prisma 评估结论落地 | medium | M6-T2.1~M6-T2.2 | Prisma 评估结论落地文档（可行性/推荐方案/实现计划或不可行理由） |
| REQ-ENG-003 | 14 道门禁全通过 | high | M6-T3.1~M6-T3.4 + M6-T5.1~M6-T5.3 | 14 道门禁全通过 + sz-pay 零回归 |
| REQ-ENG-004 | 禁止占位实现 | high | M6-T4.1 | grep todo!/unimplemented!/unreachable! 无匹配 |

---

# 12. 工程化规范

## 12.1 14 道门禁（提交前必过）

| # | 门禁 | 命令 |
|---|------|------|
| 1 | fmt 格式检查 | `cargo fmt --all -- --check` |
| 2 | check 编译检查 | `cargo check --workspace --all-targets` |
| 3 | clippy 静态分析 | `cargo clippy --workspace --all-targets -- -D warnings` |
| 4 | test 单元/集成测试 | `cargo test --workspace -j 2 --no-fail-fast` |
| 5 | doc 文档构建 | `cargo doc --workspace --no-deps --all-features` |
| 6 | audit 安全审计 | `cargo audit` + `cargo deny check` |
| 7 | integration 真实服务集成 | `cargo test --workspace -- --ignored` |
| 8 | 禁止占位实现检查 | `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` |
| 9 | SQL 注入扫描 | `scripts/check-sql-injection.ps1` |
| 10 | Feature 全组合编译 | `cargo check --workspace --all-targets --all-features` |
| 11 | 上游仓库未修改检查 | `git diff --name-only HEAD`（ADR-0001） |
| 12 | 文档与代码一致性检查 | `python scripts/check-doc-consistency.py` |
| 13 | 审计证据验证 | `bash scripts/audit-verify.sh <审计报告.md>` |
| 14 | 文档同步更新检查 | `python scripts/check-doc-sync.py --diff HEAD` |

## 12.2 五维审查（每次 PR 必做）

正确性 → 可读性 → 架构 → 安全性 → 性能

## 12.3 AI 辅助开发 10 条硬约束

1. 禁止占位实现（todo!/unimplemented!/unreachable!）
2. 强制参数化查询（禁止 SQL 字符串拼接）
3. API 兼容性（签名变更必须同步更新所有调用方和测试）
4. 五维审查必过
5. unsafe 零容忍（必须有 // SAFETY: 注释）
6. 禁止 mock 逃逸
7. 门禁前置（主动运行 gate.ps1）
8. 跨平台意识
9. Feature 隔离
10. 教训记忆（阅读防御追溯表）

## 12.4 审计合规铁律

- 每条审计/审查结论必须附带可验证的 `file:line` 代码证据
- 修复后必须运行 `cargo test` 并附输出，禁止未验证即标记 ✅
- 多项修复必须逐项验证，禁止批量声称"全部通过"
- 审计后必须运行 `bash scripts/audit-verify.sh <审计报告.md>` 验证证据

## 12.5 编译环境

- 操作系统：Windows MSVC
- 必须设置：`$env:RUST_MIN_STACK="67108864"` + `$env:CARGO_INCREMENTAL=0`
- 测试命令：`cargo test --workspace -j 2 --no-fail-fast`
- 严禁 PowerShell 替换操作（用 Node.js 脚本）

## 12.6 ADR-0001

严禁修改上游 sz-orm / sz-rust 仓库的任何文件。任何改动必须通过 PR 贡献到上游。违反此原则会导致审计记录与事实不符，直接红牌拒绝入库。

## 12.7 本机数据库（用于真实 DB e2e 测试）

- MySQL 9.6：`mysql://root:test123@127.0.0.1:3306/sz_orm_test`
- PostgreSQL 18：`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`
- Oracle 23ai Free：`127.0.0.1:1521/freepdb1`（用户 sys，密码 test123，Sysdba 权限）
- SQLite：文件型（无需独立服务）
- MSSQL：待配置（或通过 Docker）

---

> 本文档为 sz-orm v3.7.0 编码任务规划文档，基于 v3.6.0 已验收基线（M1-M5 五个里程碑完成，workspace.package.version = "3.6.0"）+ v3.6.0 端到端测试缺口（96 e2e 用 InMemoryDb，63 真实 DB 全 ignore）+ 对比分析文档滞后（停留 v3.5.0）+ v3.6.0 探索能力成熟度不足 + v3.5.0 方言扩展路线图 v3.7.0 候选 + v3.6.0 Snowflake/Redshift 无真实云验证 + 213 文件未提交 git 生成。所有改进通过 feature gate 隔离，保证既有 API 完全向后兼容与测试基线不回退。
> 生成日期：2026-08-10
> 基线版本：v3.6.0（M1-M5 五个里程碑完成，workspace.package.version = "3.6.0"）
> 目标版本：v3.7.0
> 需求总数：28 条（REQ-E2E-001~006 + REQ-REEVAL-001~005 + REQ-MAT-001~005 + REQ-DIALECT-001~005 + REQ-CLOUD-001~003 + REQ-ENG-001~004）
> 设计方向：6 个（真实 DB e2e 测试体系 / 对比分析重评估 / 探索能力成熟化 / 方言扩展延续 / 云数仓真实验证 / 工程规范化）
> 里程碑：6 个（M1~M6，关键路径 5 周，并行开发可压缩至 4-5 周）
> 任务统计：52 主任务 / 130 子任务 / 6 里程碑
> Feature Gate：5 个新增（e2e-real-db/custom-diagnostic/dialect-informix/dialect-saphana/dialect-firebird）+ 4 个既有复用 + 成熟化标注（typed-relation/sql-verify-proc/dialect-snowflake/dialect-redshift）
> 真实 DB e2e 测试：8 类核心路径 ≥56 测试用例，复用既有 tests/common/ adapter
> 探索能力成熟化：3 个 feature 标注 stable（typed-relation/sql-verify-proc/custom-diagnostic），每个 ≥10 测试
> 方言扩展：Informix/SAP HANA/Firebird 按需实现，总计 20 + 新增数方言