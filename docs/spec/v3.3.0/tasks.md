# sz-orm v3.3.0 编码任务规划文档

> 版本：v3.3.0（分布式缓存一致性 + GraphQL 查询支持 + 多租户与数据隔离 + AI 自然语言查询增强）
> 基线：v3.2.0（已完成：零拷贝序列化 + SIMD 加速 + 连接池预热增强 + 查询计划缓存）
> 日期：2026-08-08
> 文档定位：编码任务规划（What to do），对应需求规格 `docs/spec/v3.3.0/spec.md`（22 条 EARS 需求）与技术设计 `docs/spec/v3.3.0/design.md`
> 任务粒度：每个任务可在 1-2 小时内完成
> 工程化铁律：禁止占位实现 / unsafe 零容忍 / 参数化查询 / API 向后兼容 / Feature 隔离 / 五方言行为一致 / ADR-0001 不改上游

---

# 1. 任务总览

## 1.1 任务统计

| 里程碑 | 方向 | 主任务数 | 子任务数 | 关联需求 |
|--------|------|---------|---------|---------|
| M1 多租户与数据隔离 | 方向 3 | 13 | 38 | REQ-MT-001~006 |
| M2 分布式缓存一致性 | 方向 1 | 16 | 42 | REQ-DC-001~006 |
| M3 GraphQL 查询支持 | 方向 2 | 14 | 36 | REQ-GQL-001~005 |
| M4 AI 自然语言查询增强 | 方向 4 | 11 | 28 | REQ-AI-001~006 |
| M5 集成验证与发布 | 全方向 | 9 | 18 | AC-ALL-1~8 |
| **合计** | — | **63** | **162** | **22 条 REQ + 8 条 AC-ALL** |

## 1.2 里程碑分布

```
M1 多租户与数据隔离 (2 周)  ──┐
                              ├──→ M5 集成验证与发布 (1 周)
M2 分布式缓存一致性 (2 周)  ──┤
M3 GraphQL 查询支持 (2 周)  ──┤    （M2/M3/M4 可与 M1 并行）
M4 AI 自然语言查询增强 (2 周) ┘
```

- **关键路径**：M1 → M5（M1 为多租户与缓存共用 `tenant_context`，需先交付）
- **并行机会**：M2 / M3 / M4 三者独立（不同包/不同模块），可与 M1 并行开发
- **总周期**：3 周（关键路径 M1 2 周 + M5 1 周，并行压缩）

## 1.3 Feature Gate 矩阵

| Feature | 所属包 | 默认 | 依赖 | 关联里程碑 |
|---------|--------|------|------|-----------|
| `dist-cache` | sz-orm-core | 关闭 | bloomfilter, rand, sz-orm-crypto | M2 |
| `multi-tenant-enhanced` | sz-orm-core | 关闭 | 无新增（复用 tokio/parking_lot/audit/masking） | M1 |
| `graphql-n1` | sz-orm-graphql | 关闭 | async-graphql（real feature） | M3 |
| `graphql-schema-gen` | sz-orm-graphql | 关闭 | sz-orm-macros | M3 |
| `graphql-complexity` | sz-orm-graphql | 关闭 | 无 | M3 |
| `ai-nl2sql-enhanced` | sz-orm-ai | 关闭 | 无（复用 real feature） | M4 |
| `ai-index-advisor` | sz-orm-ai | 关闭 | sqlparser | M4 |
| `ai-rewrite-advisor` | sz-orm-ai | 关闭 | sqlparser | M4 |

---

# 2. M1 多租户与数据隔离（REQ-MT-001~006）

> **目标**：在 sz-orm-core 内增强多租户能力，新增租户上下文自动注入、Schema 隔离、连接池隔离、行级安全、列级脱敏、多租户审计，通过 feature gate `multi-tenant-enhanced` 隔离，既有 `with_tenant_id` / `without_tenant` / `tenant_field` API 保持完全向后兼容。
> **周期**：2 周
> **关联设计**：design.md §2.3
> **关联验收**：AC-MT-1~6（spec §9.3）

## 2.1 M1-T1：Feature gate 配置与模块骨架

- [ ] **M1-T1.1** 在 `packages/sz-orm-core/Cargo.toml` 的 `[features]` 新增 `multi-tenant-enhanced = []`，确认无新增外部依赖（复用既有 tokio + parking_lot + sz-orm-audit + sz-orm-masking）
  - 关联需求：REQ-MT-001~006（feature 隔离基础）
  - 关联设计：design.md §2.3.5
  - 验收：`cargo check -p sz-orm-core` 通过，feature 默认关闭
  - 依赖：无

- [ ] **M1-T1.2** 创建 `packages/sz-orm-core/src/tenant_context.rs` 与 `packages/sz-orm-core/src/tenant_security.rs` 模块文件，添加模块文档注释与 `#[cfg(feature = "multi-tenant-enhanced")]` 条件编译守卫
  - 关联需求：REQ-MT-001~006
  - 关联设计：design.md §2.3.2
  - 验收：模块文件存在，未启用 feature 时零代码体积
  - 依赖：M1-T1.1

- [ ] **M1-T1.3** 在 `packages/sz-orm-core/src/lib.rs` 添加 `#[cfg(feature = "multi-tenant-enhanced")] pub mod tenant_context;` 与 `pub mod tenant_security;` 条件导出
  - 关联需求：REQ-MT-001~006
  - 关联设计：design.md §2.3.6（lib.rs 导出集成点）
  - 验收：`cargo doc -p sz-orm-core --features multi-tenant-enhanced` 模块可见
  - 依赖：M1-T1.2

## 2.2 M1-T2：租户上下文与 RAII 守卫（REQ-MT-001）

- [ ] **M1-T2.1** 在 `tenant_context.rs` 定义 `IsolationStrategy` 枚举（`RowLevel` / `SchemaIsolation`），派生 `Debug/Clone/Copy/PartialEq/Eq`
  - 关联需求：REQ-MT-001
  - 关联设计：design.md §2.3.3（IsolationStrategy）
  - 验收：枚举可用，`Copy` 派生确保值语义
  - 依赖：M1-T1.2

- [ ] **M1-T2.2** 定义 `TenantPermissions` 结构体（`row_level_policies: Vec<RowLevelSecurityPolicy>` + `column_masking_rules: Vec<ColumnMaskingRule>` + `roles: Vec<String>`），前置声明 `RowLevelSecurityPolicy` 与 `ColumnMaskingRule`
  - 关联需求：REQ-MT-001
  - 关联设计：design.md §2.3.3
  - 验收：结构体字段完整，可构造
  - 依赖：M1-T2.1

- [ ] **M1-T2.3** 定义 `TenantContext` 结构体（`tenant_id: i64` + `isolation_strategy: IsolationStrategy` + `permissions: TenantPermissions`），提供 `new(tenant_id, strategy)` 构造函数与 `enter() -> TenantContextGuard` 方法
  - 关联需求：REQ-MT-001
  - 关联设计：design.md §2.3.3（TenantContext）
  - 验收：`tenant_id` 必填 i64（禁止字符串避免注入，spec §6.3）；不可被客户端篡改
  - 依赖：M1-T2.2

- [ ] **M1-T2.4** 定义 `tokio::task_local! { static TENANT_CONTEXT: RefCell<Option<TenantContext>> }` task-local 存储，实现 `TenantContextGuard`（实现 `Drop` trait 在作用域结束自动清理 task-local）
  - 关联需求：REQ-MT-001
  - 关联设计：design.md §2.3.3（TenantContextGuard，task-local 异步上下文隔离）
  - 验收：RAII 守卫作用域内上下文不变；Drop 时自动清理；异步任务边界隔离正确
  - 依赖：M1-T2.3

- [ ] **M1-T2.5** 提供 `TenantContext::current() -> Option<TenantContext>` 静态方法（从 task-local 读取当前上下文），未设置时返回 `None`
  - 关联需求：REQ-MT-001
  - 关联设计：design.md §2.3.3
  - 验收：在 `TenantContextGuard` 作用域内返回 `Some`，作用域外返回 `None`
  - 依赖：M1-T2.4

## 2.3 M1-T3：Schema 隔离路由器（REQ-MT-002）

- [ ] **M1-T3.1** 在 `tenant_context.rs` 定义 `SchemaIsolationRouter`，实现 `rewrite_table(table: &str, tenant_id: i64) -> String` 返回 `format!("tenant_{}_{}", tenant_id, table)`
  - 关联需求：REQ-MT-002
  - 关联设计：design.md §2.3.3（SchemaIsolationRouter）
  - 验收：命名遵循 `tenant_{id}_{table}` 格式（spec §6.3）；禁止用户自定义命名
  - 依赖：M1-T2.1

- [ ] **M1-T3.2** 在 `packages/sz-orm-core/src/query.rs` 的 `QueryBuilder::table()` 方法（query.rs:36）添加 `#[cfg(feature = "multi-tenant-enhanced")]` 分支：当隔离策略为 `SchemaIsolation` 时调用 `SchemaIsolationRouter::rewrite_table` 重写表名
  - 关联需求：REQ-MT-002
  - 关联设计：design.md §2.3.6（QueryBuilder 表名集成点）
  - 验收：行级隔离策略下表名不变；Schema 隔离策略下路由到 `tenant_{id}_{table}`
  - 依赖：M1-T3.1

## 2.4 M1-T4：build_tenant_condition 自动注入（REQ-MT-001）

- [ ] **M1-T4.1** 在 `packages/sz-orm-core/src/query.rs` 的 `build_tenant_condition` 方法（query.rs:488）添加 `#[cfg(feature = "multi-tenant-enhanced")]` 分支：当 `tenant_id_value` 为 `None` 时从 `TenantContext::current()` 读取 tenant_id
  - 关联需求：REQ-MT-001
  - 关联设计：design.md §2.3.6（QueryBuilder 租户条件集成点）
  - 验收：显式 `with_tenant_id` 优先于上下文自动注入（兼容既有）；未显式调用时从上下文读取
  - 依赖：M1-T2.5

- [ ] **M1-T4.2** 在 `build_tenant_condition` 添加上下文必填校验：当 `multi-tenant-enhanced` feature 启用且 `tenant_id_value` 为 `None` 且 `TenantContext::current()` 为 `None` 且 `tenant_disabled` 为 `false` 时，返回错误 `TenantContextRequired`
  - 关联需求：REQ-MT-006（禁止跨租户数据泄漏）
  - 关联设计：design.md §2.3.4（上下文必填校验）
  - 验收：未设置租户上下文拒绝执行（spec §5.3.3 异常场景1）
  - 依赖：M1-T4.1

## 2.5 M1-T5：租户连接池注册表（REQ-MT-003）

- [ ] **M1-T5.1** 在 `tenant_context.rs` 定义 `TenantPoolRegistry` 结构体（`pools: parking_lot::RwLock<HashMap<i64, Arc<Pool>>>` + `pool_config: PoolConfig`）
  - 关联需求：REQ-MT-003
  - 关联设计：design.md §2.3.3（TenantPoolRegistry）
  - 验收：按 tenant_id 维护独立 Pool；共享 PoolConfig
  - 依赖：M1-T1.2

- [ ] **M1-T5.2** 实现 `TenantPoolRegistry::get_or_create(tenant_id: i64) -> Arc<Pool>` 方法，使用 CAS（compare-and-swap）防并发重复创建
  - 关联需求：REQ-MT-003
  - 关联设计：design.md §2.3.3
  - 验收：Pool 已存在时返回既有 Pool；不存在时创建新 Pool 并插入 HashMap
  - 依赖：M1-T5.1

- [ ] **M1-T5.3** 实现 `TenantPoolRegistry::switch(tenant_id: i64) -> TenantPoolGuard` 方法，返回 RAII 守卫在 Drop 时切换回原租户
  - 关联需求：REQ-MT-003
  - 关联设计：design.md §2.3.4（租户切换与连接池隔离流程）
  - 验收：租户切换原子（切换中查询不跨租户泄漏）；路由开销 ≤ 50μs（HashMap 查找 + Arc clone）
  - 依赖：M1-T5.2

## 2.6 M1-T6：行级安全策略（REQ-MT-004）

- [ ] **M1-T6.1** 在 `tenant_security.rs` 定义 `ParameterizedCondition` 结构体（`sql_fragment: String` 含占位符 `$1` + `params: Vec<Value>`），确保参数化绑定（禁止 SQL 字符串拼接）
  - 关联需求：REQ-MT-004
  - 关联设计：design.md §2.3.3（ParameterizedCondition）
  - 验收：参数化查询铁律（spec §4.3 C-03）
  - 依赖：M1-T1.2

- [ ] **M1-T6.2** 定义 `Principal` 结构体（`tenant_id: i64` + `roles: Vec<String>`）与 `RowLevelSecurityPolicy` 结构体（`table: String` + `filter_condition: ParameterizedCondition` + `principal: Principal`）
  - 关联需求：REQ-MT-004
  - 关联设计：design.md §2.3.3（RowLevelSecurityPolicy）
  - 验收：策略由服务端定义不可被客户端篡改（spec §4.3）；超出 tenant_id 的细粒度权限（部门级、角色级）
  - 依赖：M1-T6.1

- [ ] **M1-T6.3** 在 `packages/sz-orm-core/src/access_control.rs`（access_control.rs:9）扩展 `AccessRule` 集成 `RowLevelSecurityPolicy`，新增 `apply_row_level_security(ctx: &TenantContext, table: &str) -> Option<ParameterizedCondition>` 方法
  - 关联需求：REQ-MT-004
  - 关联设计：design.md §2.3.6（AccessRule 行级安全集成点）
  - 验收：既有 `AccessRule` 不变；新增方法与 `TenantContext` 集成
  - 依赖：M1-T6.2

## 2.7 M1-T7：列级脱敏规则（REQ-MT-004）

- [ ] **M1-T7.1** 在 `tenant_security.rs` 定义 `MaskingFunction` 枚举（复用既有 `sz-orm-masking::MaskingRule`）与 `PermissionPredicate` 结构体（描述未授权租户/角色条件）
  - 关联需求：REQ-MT-004
  - 关联设计：design.md §2.3.3（ColumnMaskingRule）
  - 验收：复用既有 `MaskingRule` 枚举（Phone/Email/IdCard/BankCard/Name/Address/Ip/Imei/Plate/Custom）
  - 依赖：M1-T1.2

- [ ] **M1-T7.2** 定义 `ColumnMaskingRule` 结构体（`table: String` + `column: String` + `masking_function: MaskingFunction` + `applicable_permissions: PermissionPredicate`）
  - 关联需求：REQ-MT-004
  - 关联设计：design.md §2.3.3
  - 验收：ORM 层强制执行不可绕过（spec §4.3）；未配置脱敏规则的敏感列默认拒绝读取（安全优先，spec §5.3.3 异常场景5）
  - 依赖：M1-T7.1

- [ ] **M1-T7.3** 实现 ORM 层脱敏拦截器：在 `QueryBuilder` 结果反序列化后（result_map 路径或执行后）按列应用 `ColumnMaskingRule`，调用 `sz-orm-masking::DataMasker::apply` 执行脱敏
  - 关联需求：REQ-MT-004
  - 关联设计：design.md §2.3.6（sz-orm-masking 脱敏集成点）
  - 验收：既有 `DataMasker::apply` 不变；绕过 ORM 的原生 SQL 在 Connection 层拦截或文档明确约束
  - 依赖：M1-T7.2

## 2.8 M1-T8：多租户审计（REQ-MT-005）

- [ ] **M1-T8.1** 在 `tenant_security.rs` 定义 `TenantAuditOperation` 枚举（`ContextSet` / `ContextSwitch` / `CrossTenantDenied` / `RowLevelFiltered` / `ColumnMasked`）与 `AuditResult` 枚举（`Success` / `Denied`）
  - 关联需求：REQ-MT-005
  - 关联设计：design.md §2.3.3（TenantAuditOperation）
  - 验收：覆盖 spec §6.3 全部操作类型
  - 依赖：M1-T1.2

- [ ] **M1-T8.2** 定义 `TenantAuditContext` 结构体（`tenant_id: i64` + `operation: TenantAuditOperation` + `timestamp: i64` + `result: AuditResult` + `detail: String`），实现 `log_to(auditor: &SqlAuditor)` 方法调用既有 `SqlAuditor::log` 记录
  - 关联需求：REQ-MT-005
  - 关联设计：design.md §2.3.6（sz-orm-audit 审计集成点）
  - 验收：审计日志含租户 ID + 操作 + 时间 + 结果（spec §6.3）；日志不可篡改（追加写入或持久化）
  - 依赖：M1-T8.1

- [ ] **M1-T8.3** 在租户上下文设置、租户切换、跨租户拒绝、行级过滤、列级脱敏执行路径埋点调用 `TenantAuditContext::log_to`
  - 关联需求：REQ-MT-005
  - 关联设计：design.md §2.3.4
  - 验收：跨租户访问尝试被拒绝并审计；全部操作均有审计记录
  - 依赖：M1-T8.2

## 2.9 M1-T9：单元测试

- [ ] **M1-T9.1** 编写 `TenantContext` + `TenantContextGuard` 单元测试：上下文设置/读取/清理、RAII Drop 自动清理、task-local 异步边界隔离
  - 关联需求：REQ-MT-001
  - 关联设计：design.md §2.3.3
  - 验收：测试覆盖 RAII 生命周期与异步隔离
  - 依赖：M1-T2.5

- [ ] **M1-T9.2** 编写 `SchemaIsolationRouter` 单元测试：表名重写格式正确、不同 tenant_id 路由到不同 Schema
  - 关联需求：REQ-MT-002
  - 关联设计：design.md §2.3.3
  - 验收：`rewrite_table("users", 42) == "tenant_42_users"`
  - 依赖：M1-T3.1

- [ ] **M1-T9.3** 编写 `TenantPoolRegistry` 单元测试：get_or_create 幂等、switch 原子切换、路由开销基准（≤ 50μs）
  - 关联需求：REQ-MT-003
  - 关联设计：design.md §2.3.3
  - 验收：路由开销基准测试通过（spec §4.1 性能）
  - 依赖：M1-T5.3

- [ ] **M1-T9.4** 编写 `RowLevelSecurityPolicy` + `ColumnMaskingRule` 单元测试：参数化条件构造、脱敏规则应用、权限判定
  - 关联需求：REQ-MT-004
  - 关联设计：design.md §2.3.3
  - 验收：参数化绑定正确；脱敏值与规则一致
  - 依赖：M1-T6.3, M1-T7.3

- [ ] **M1-T9.5** 编写 `build_tenant_condition` 自动注入单元测试：显式 `with_tenant_id` 优先、上下文自动注入、上下文未设置拒绝、既有 API 兼容
  - 关联需求：REQ-MT-001, REQ-MT-006
  - 关联设计：design.md §2.3.4
  - 验收：既有 `with_tenant_id` 行为不变；自动注入正确；未设置上下文返回 `TenantContextRequired`
  - 依赖：M1-T4.2

## 2.10 M1-T10：集成测试（五方言）

- [ ] **M1-T10.1** 编写行级隔离集成测试：MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言下租户 A 查询仅返回租户 A 数据，租户 B 查询仅返回租户 B 数据
  - 关联需求：REQ-MT-001
  - 关联设计：design.md §2.3.4
  - 验收：五方言行为一致（spec §4.5 兼容性 C-07）
  - 依赖：M1-T9.5

- [ ] **M1-T10.2** 编写 Schema 隔离集成测试：五方言下租户 A 查询路由到 `tenant_a.{table}`，租户 B 路由到 `tenant_b.{table}`，两租户数据物理隔离
  - 关联需求：REQ-MT-002
  - 关联设计：design.md §2.3.4
  - 验收：五方言 Schema 创建 DDL 差异处理；物理隔离验证
  - 依赖：M1-T10.1

- [ ] **M1-T10.3** 编写行级安全 + 列级脱敏集成测试：部门级行级安全过滤、薪资列脱敏、绕过 ORM 的原生 SQL 仍受约束
  - 关联需求：REQ-MT-004
  - 关联设计：design.md §2.3.6
  - 验收：未授权租户读到脱敏值而非原始值；ORM 层强制执行不可绕过
  - 依赖：M1-T9.4

## 2.11 M1-T11：竞态与跨租户泄漏测试（REQ-MT-006）

- [ ] **M1-T11.1** 编写租户切换竞态测试：并发请求中租户上下文切换竞态，验证每请求读到正确租户上下文，无跨租户泄漏
  - 关联需求：REQ-MT-006
  - 关联设计：design.md §2.3.4（竞态测试）
  - 验收：切换原子无泄漏（spec §4.2 可靠性）
  - 依赖：M1-T10.1

- [ ] **M1-T11.2** 编写查询重写全覆盖测试：所有查询路径（select/insert/update/delete）均追加隔离条件，无遗漏
  - 关联需求：REQ-MT-006
  - 关联设计：design.md §2.3.4
  - 验收：查询重写全覆盖（spec §4.3 安全性 C-10）
  - 依赖：M1-T11.1

- [ ] **M1-T11.3** 编写审计日志完整性测试：租户切换/跨租户拒绝/行级过滤/列脱敏全部操作审计记录，含租户 ID/操作/时间/结果，日志不可篡改
  - 关联需求：REQ-MT-005
  - 关联设计：design.md §2.3.3
  - 验收：审计记录完整（spec §6.3）
  - 依赖：M1-T8.3

## 2.12 M1-T12：门禁验证

- [ ] **M1-T12.1** 运行 `cargo fmt --all -- --check` + `cargo check -p sz-orm-core --features multi-tenant-enhanced` + `cargo clippy -p sz-orm-core --all-targets --features multi-tenant-enhanced -- -D warnings`
  - 关联需求：REQ-MT-001~006
  - 关联设计：design.md §2.3.5
  - 验收：fmt 通过、编译通过、clippy 零警告
  - 依赖：M1-T11.3

- [ ] **M1-T12.2** 运行 `cargo test -p sz-orm-core --features multi-tenant-enhanced` 全部通过，扫描 `todo!` / `unimplemented!` / `unreachable!` / `unsafe` 零容忍
  - 关联需求：REQ-MT-001~006
  - 关联设计：design.md §2.3.5
  - 验收：测试全通过；禁止占位实现；unsafe 零容忍
  - 依赖：M1-T12.1

## 2.13 M1-T13：API 兼容性验证

- [ ] **M1-T13.1** 编写既有 API 兼容性测试：`with_tenant_id` / `without_tenant` / `tenant_field` / `build_tenant_condition` 既有行为不变（feature 未启用时零行为变更）
  - 关联需求：REQ-MT-001（兼容性）
  - 关联设计：design.md §2.3.6
  - 验收：AC-MT-1 既有 API 行为不变；feature 默认关闭零行为变更
  - 依赖：M1-T12.2

---

# 3. M2 分布式缓存一致性（REQ-DC-001~006）

> **目标**：在 sz-orm-core 内扩展分布式缓存一致性模块 `dist_cache`，新增跨实例失效协议（Redis Pub/Sub + Gossip）、一致性保证选项、Write-behind 异步批量写入、缓存击穿/雪崩防护，通过 feature gate `dist-cache` 隔离。
> **周期**：2 周
> **关联设计**：design.md §2.1
> **关联验收**：AC-DC-1~6（spec §9.1）
> **依赖**：M1（复用 `tenant_context` 缓存可按租户隔离）

## 3.1 M2-T1：Feature gate 配置与模块骨架

- [ ] **M2-T1.1** 在 `packages/sz-orm-core/Cargo.toml` 的 `[features]` 新增 `dist-cache = ["dep:bloomfilter", "dep:rand"]`，在 `[dependencies]` 新增 `bloomfilter = { version = "1.0", optional = true }` 与 `rand = { version = "0.8", optional = true }`
  - 关联需求：REQ-DC-001~006
  - 关联设计：design.md §2.1.5
  - 验收：`cargo check -p sz-orm-core` 默认不引入新依赖；`--features dist-cache` 引入 bloomfilter + rand
  - 依赖：M1-T1.1

- [ ] **M2-T1.2** 创建 `packages/sz-orm-core/src/dist_cache.rs` 模块文件，添加模块文档注释与 `#[cfg(feature = "dist-cache")]` 条件编译守卫
  - 关联需求：REQ-DC-001~006
  - 关联设计：design.md §2.1.2
  - 验收：模块文件存在，未启用 feature 时零代码体积
  - 依赖：M2-T1.1

- [ ] **M2-T1.3** 在 `packages/sz-orm-core/src/lib.rs` 添加 `#[cfg(feature = "dist-cache")] pub mod dist_cache;` 条件导出
  - 关联需求：REQ-DC-001~006
  - 关联设计：design.md §2.1.6（lib.rs 导出集成点）
  - 验收：`cargo doc -p sz-orm-core --features dist-cache` 模块可见
  - 依赖：M2-T1.2

## 3.2 M2-T2：一致性级别配置（REQ-DC-003）

- [ ] **M2-T2.1** 在 `dist_cache.rs` 定义 `ConsistencyLevel` 枚举（`Eventual` / `Strong`），派生 `Debug/Clone/Copy/PartialEq/Eq`，默认 `Eventual` 向后兼容
  - 关联需求：REQ-DC-003
  - 关联设计：design.md §2.1.3（ConsistencyLevel）
  - 验收：默认 Eventual 向后兼容；Strong 性能开销大于 Eventual
  - 依赖：M2-T1.2

- [ ] **M2-T2.2** 在 `packages/sz-orm-core/src/l2_cache.rs` 的 `L2CacheConfigBuilder` 新增 `consistency_level(ConsistencyLevel)` 方法，`L2Cache` 新增 `consistency_level` 字段（默认 `Eventual`）
  - 关联需求：REQ-DC-003
  - 关联设计：design.md §2.1.6（L2Cache 写路径集成点）
  - 验收：既有 `L2Cache` 公开 API 不变；新增字段默认 Eventual 行为不变
  - 依赖：M2-T2.1

## 3.3 M2-T3：Redis Pub/Sub 失效总线（REQ-DC-001）

- [ ] **M2-T3.1** 在 `dist_cache.rs` 定义 `RedisPubSubInvalidationBus` 结构体（`client: redis::aio::ConnectionManager` + `channel: String` + `local_buffer: parking_lot::Mutex<VecDeque<InvalidationMessage>>` + `instance_id: String`）
  - 关联需求：REQ-DC-001
  - 关联设计：design.md §2.1.3（RedisPubSubInvalidationBus）
  - 验收：复用既有 Redis 连接管理（自动重连）；Pub/Sub 专用连接
  - 依赖：M2-T1.2

- [ ] **M2-T3.2** 实现 `InvalidationBus` trait for `RedisPubSubInvalidationBus`：`publish(message)` 经 `PUBLISH` 命令广播（序列化 InvalidationMessage 为 ≤1KB JSON）；`subscribe()` drain local_buffer
  - 关联需求：REQ-DC-001
  - 关联设计：design.md §2.1.3
  - 验收：消息大小 ≤ 1KB（spec §6.1）；既有 `InvalidationBus` trait 不变
  - 依赖：M2-T3.1

- [ ] **M2-T3.3** 实现异步订阅循环：`tokio::spawn` 独立 Pub/Sub 连接 `SUBSCRIBE` 通道，收到消息反序列化后写入 local_buffer（跳过本实例 instance_id 避免自回环）
  - 关联需求：REQ-DC-001
  - 关联设计：design.md §2.1.4（跨实例失效协议主流程）
  - 验收：跨实例失效在 50ms 内同步（spec §4.1 性能）；避免自回环
  - 依赖：M2-T3.2

- [ ] **M2-T3.4** 实现 Redis Pub/Sub 连接失败降级：Redis 不可达时降级为本地失效（仅失效本实例缓存），记录告警日志，定期重连
  - 关联需求：REQ-DC-001（异常场景）
  - 关联设计：design.md §2.1.3 + spec §5.1.3 异常场景1
  - 验收：降级为本地失效；日志含 Redis 连接错误；定期重连
  - 依赖：M2-T3.3

## 3.4 M2-T4：Gossip 失效总线（REQ-DC-002）

- [ ] **M2-T4.1** 在 `dist_cache.rs` 定义 `GossipInvalidationBus` 结构体（`nodes: Vec<NodeAddr>` + `shared_secret: Vec<u8>` + `local_buffer` + `seen_messages: parking_lot::RwLock<HashSet<u64>>` + `instance_id: String`）
  - 关联需求：REQ-DC-002
  - 关联设计：design.md §2.1.3（GossipInvalidationBus）
  - 验收：去重避免重复传播；共享密钥认证
  - 依赖：M2-T1.2

- [ ] **M2-T4.2** 实现 `InvalidationBus` trait for `GossipInvalidationBus`：`publish(message)` 点对点发送到所有已知节点（并行 `tokio::join`）；`subscribe()` drain local_buffer
  - 关联需求：REQ-DC-002
  - 关联设计：design.md §2.1.3
  - 验收：≤10 实例 1s 收敛（spec §4.1 性能）
  - 依赖：M2-T4.1

- [ ] **M2-T4.3** 实现节点认证：每次通信附带 `HMAC(shared_secret, message)` 认证标签，未认证节点消息拒绝
  - 关联需求：REQ-DC-002
  - 关联设计：design.md §2.1.3 + spec §4.3 安全性
  - 验收：禁止未认证节点加入失效广播
  - 依赖：M2-T4.2

- [ ] **M2-T4.4** 实现反熵（anti-entropy）：节点重连后定期（默认 5s）与随机对端交换 seen_messages 摘要，补全缺失失效消息
  - 关联需求：REQ-DC-002
  - 关联设计：design.md §2.1.3 + spec §5.1.3 异常场景4
  - 验收：离线节点重连后缓存最终一致
  - 依赖：M2-T4.3

## 3.5 M2-T5：写路径分派（REQ-DC-003）

- [ ] **M2-T5.1** 在 `L2Cache` 写路径添加 `consistency_level` 分派：`Strong` 先失效所有实例缓存再写库；`Eventual` 写库后异步失效 + TTL 兜底
  - 关联需求：REQ-DC-003
  - 关联设计：design.md §2.1.4（跨实例失效协议主流程）
  - 验收：强一致写后读返回最新值；最终一致写后立即返回 + TTL 兜底
  - 依赖：M2-T2.2, M2-T3.2

## 3.6 M2-T6：Write-behind 配置与队列（REQ-DC-004）

- [ ] **M2-T6.1** 在 `dist_cache.rs` 定义 `WriteBehindConfig` 结构体（`batch_size: u32` 默认 100 + `flush_interval: Duration` 默认 100ms + `wal_path: PathBuf` + `encryption_key: Vec<u8>` + `fallback_to_sync: bool` 默认 true）
  - 关联需求：REQ-DC-004
  - 关联设计：design.md §2.1.3（WriteBehindConfig）
  - 验收：配置项完整，默认值合理
  - 依赖：M2-T1.2

- [ ] **M2-T6.2** 定义 `WriteOp` 结构体（`op_type: WriteOpType` Insert/Update/Delete + `table: String` + `pk: Value` + `data: Vec<(String, Value)>` + `timestamp: i64` + `sequence: u64`）与 `WriteOpType` 枚举
  - 关联需求：REQ-DC-004
  - 关联设计：design.md §2.1.3
  - 验收：WAL 条目完整性（spec §6.1）
  - 依赖：M2-T6.1

- [ ] **M2-T6.3** 定义 `WriteBehindQueue` 结构体（`wal: Mutex<WalFile>` + `pending: crossbeam_queue::ArrayQueue<WriteOp>` + `sequence: AtomicU64` + `config: WriteBehindConfig`）
  - 关联需求：REQ-DC-004
  - 关联设计：design.md §2.1.3（WriteBehindQueue）
  - 验收：内存待刷盘队列 + 单调递增序列号
  - 依赖：M2-T6.2

## 3.7 M2-T7：WAL 持久化与加密（REQ-DC-004）

- [ ] **M2-T7.1** 实现 WAL 文件格式：每条记录 = [4字节长度][加密载荷][8字节CRC]，加密复用 `sz-orm-crypto`，CRC 校验完整性
  - 关联需求：REQ-DC-004
  - 关联设计：design.md §2.1.3 + spec §4.3 安全性 C-02
  - 验收：WAL 加密存储（禁止明文持久化敏感数据）；CRC 校验完整性
  - 依赖：M2-T6.3

- [ ] **M2-T7.2** 实现 `WriteBehindQueue::enqueue(op) -> Result<()>`：写 WAL 持久化（加密 + CRC）→ 入内存 pending 队列 → 立即返回成功（≤ 1ms）
  - 关联需求：REQ-DC-004
  - 关联设计：design.md §2.1.4（Write-behind 流程）
  - 验收：WAL 持久化先于返回成功（宕机不丢数据）；单条写操作返回延迟 ≤ 1ms（spec §4.1 性能）
  - 依赖：M2-T7.1

## 3.8 M2-T8：后台批量刷盘与宕机回放（REQ-DC-004）

- [ ] **M2-T8.1** 实现后台刷盘任务（`tokio::spawn`）：等待 `flush_interval` 或 pending 达 `batch_size`，批量取出按 sequence 顺序执行批量 SQL（参数化）
  - 关联需求：REQ-DC-004
  - 关联设计：design.md §2.1.4
  - 验收：批量合并 + 异步刷盘；吞吐量较 write-through 提升 ≥ 3x（spec §4.1 性能）
  - 依赖：M2-T7.2

- [ ] **M2-T8.2** 实现刷盘失败处理：告警 + 回退同步写模式（write-through），保留 WAL 待重试
  - 关联需求：REQ-DC-004
  - 关联设计：design.md §2.1.4 + spec §5.1.3 异常场景2
  - 验收：刷盘失败告警并回退同步写；数据不丢失（WAL 保留）
  - 依赖：M2-T8.1

- [ ] **M2-T8.3** 实现 `WriteBehindQueue::replay() -> Result<()>`：宕机重启读取 WAL 文件，CRC 校验 + 解密，按 sequence 顺序回放未刷盘 WriteOp，回放完成后截断 WAL
  - 关联需求：REQ-DC-004
  - 关联设计：design.md §2.1.4
  - 验收：宕机重启 WAL 回放零数据丢失（spec §4.2 可靠性）
  - 依赖：M2-T8.2

## 3.9 M2-T9：布隆过滤器防护（REQ-DC-005）

- [ ] **M2-T9.1** 在 `dist_cache.rs` 定义 `BloomFilterGuard` 结构体（`filter: bloomfilter::Bloom<String>` + `capacity: usize` 默认 100000 + `false_positive_rate: f64` 默认 0.01）
  - 关联需求：REQ-DC-005
  - 关联设计：design.md §2.1.3（BloomFilterGuard）
  - 验收：假阳性率 ≤ 1% 可配置（spec §6.1）；容量可配置
  - 依赖：M2-T1.2

- [ ] **M2-T9.2** 实现 `BloomFilterGuard` 方法：`add(key: &str)` + `might_contain(key: &str) -> bool` + `rebuild(keys: impl Iterator)`（超容量时重建）
  - 关联需求：REQ-DC-005
  - 关联设计：design.md §2.1.3
  - 验收：超容量自动扩容或重建
  - 依赖：M2-T9.1

## 3.10 M2-T10：互斥锁防护（REQ-DC-005）

- [ ] **M2-T10.1** 在 `dist_cache.rs` 定义 `MutexGuard` 结构体（`mutexes: parking_lot::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>`），实现 `guard(key: &str) -> tokio::sync::MutexGuard` 方法
  - 关联需求：REQ-DC-005
  - 关联设计：design.md §2.1.3（MutexGuard）
  - 验收：互斥期间仅一个请求查库回填（穿透 ≤ 1，spec §4.1 性能）
  - 依赖：M2-T1.2

## 3.11 M2-T11：随机 TTL 雪崩防护（REQ-DC-005）

- [ ] **M2-T11.1** 在 `dist_cache.rs` 定义 `RandomTtlJitter`，实现 `jitter(base_ttl: Duration, jitter_range: f64) -> Duration` 方法（`base_ttl × (1 ± jitter_range × random)`，random 使用 rand crate 安全随机源）
  - 关联需求：REQ-DC-005
  - 关联设计：design.md §2.1.3（RandomTtlJitter）
  - 验收：抖动范围默认基础 TTL 的 ±20%（spec §6.1）；安全随机源（非伪随机）避免抖动可预测；标准差 ≥ 抖动范围（spec §4.2 可靠性）
  - 依赖：M2-T1.2

## 3.12 M2-T12：失效消息丢失兜底（REQ-DC-006）

- [ ] **M2-T12.1** 实现失效消息丢失兜底：最终一致性通过 TTL 到期自动失效；强一致性通过同步重试至所有实例确认失效
  - 关联需求：REQ-DC-006
  - 关联设计：design.md §2.1.4 + spec §5.1.1 规则6
  - 验收：网络分区/Redis 故障场景无实例持续读到过期数据
  - 依赖：M2-T5.1

## 3.13 M2-T13：telemetry 指标

- [ ] **M2-T13.1** 在 `packages/sz-orm-core/src/telemetry.rs`（telemetry.rs:83）新增失效协议指标（发布数/接收数/丢弃数/延迟）原子计数器
  - 关联需求：REQ-DC-001~006（可观测性）
  - 关联设计：design.md §2.1.6（telemetry 指标集成点）+ spec §4.4 可维护性
  - 验收：失效消息传播暴露指标；协议异常告警
  - 依赖：M2-T3.3

## 3.14 M2-T14：单元测试

- [ ] **M2-T14.1** 编写 `RedisPubSubInvalidationBus` 单元测试：publish/subscribe 基本功能、消息序列化 ≤1KB、自回环避免
  - 关联需求：REQ-DC-001
  - 关联设计：design.md §2.1.3
  - 验收：消息大小 ≤ 1KB；自回环跳过
  - 依赖：M2-T3.4

- [ ] **M2-T14.2** 编写 `GossipInvalidationBus` 单元测试：点对点传播、节点认证、反熵补全、去重
  - 关联需求：REQ-DC-002
  - 关联设计：design.md §2.1.3
  - 验收：未认证节点消息拒绝；重连后反熵补全
  - 依赖：M2-T4.4

- [ ] **M2-T14.3** 编写 `WriteBehindQueue` 单元测试：enqueue 立即返回（≤1ms）、WAL 持久化、批量刷盘、序列号单调递增
  - 关联需求：REQ-DC-004
  - 关联设计：design.md §2.1.3
  - 验收：单条写操作返回延迟 ≤ 1ms；WAL 持久化先于返回
  - 依赖：M2-T8.3

- [ ] **M2-T14.4** 编写 `BloomFilterGuard` + `MutexGuard` + `RandomTtlJitter` 单元测试：假阳性率、互斥锁、TTL 标准差
  - 关联需求：REQ-DC-005
  - 关联设计：design.md §2.1.3
  - 验收：假阳性率 ≤ 1%；TTL 标准差 ≥ 抖动范围
  - 依赖：M2-T9.2, M2-T10.1, M2-T11.1

## 3.15 M2-T15：集成测试（多实例 + Redis Pub/Sub）

- [ ] **M2-T15.1** 编写多实例 Redis Pub/Sub 集成测试：实例 A 失效 table_x 缓存 → 实例 B 及所有订阅实例的 table_x 缓存在 50ms 内同步失效
  - 关联需求：REQ-DC-001
  - 关联设计：design.md §2.1.4
  - 验收：AC-DC-1 50ms 内同步失效（多实例集成测试证据）
  - 依赖：M2-T14.1

- [ ] **M2-T15.2** 编写 Gossip 集群集成测试：≤10 实例 gossip 传播，1s 内收敛一致，共享密钥认证
  - 关联需求：REQ-DC-002
  - 关联设计：design.md §2.1.3
  - 验收：AC-DC-2 ≤10 实例 1s 收敛 + 认证
  - 依赖：M2-T14.2

- [ ] **M2-T15.3** 编写强一致/最终一致集成测试：强一致写后读返回最新值；最终一致写后立即返回 + TTL 兜底
  - 关联需求：REQ-DC-003
  - 关联设计：design.md §2.1.4
  - 验收：AC-DC-3 强一致/最终一致行为正确
  - 依赖：M2-T15.1

## 3.16 M2-T16：宕机回放与网络分区测试（REQ-DC-004, REQ-DC-006）

- [ ] **M2-T16.1** 编写 Write-behind 吞吐量基准测试：vs write-through 提升 ≥ 3x
  - 关联需求：REQ-DC-004
  - 关联设计：design.md §2.1.4
  - 验收：AC-DC-4 吞吐量 ≥ 3x（基准证据）
  - 依赖：M2-T14.3

- [ ] **M2-T16.2** 编写宕机重启 WAL 回放测试：写入 N 条 → 模拟宕机 → 重启回放 → 零数据丢失
  - 关联需求：REQ-DC-004
  - 关联设计：design.md §2.1.4
  - 验收：AC-DC-4 WAL 宕机回放零数据丢失
  - 依赖：M2-T16.1

- [ ] **M2-T16.3** 编写网络分区/Redis 故障测试：失效消息丢失场景由 TTL 兜底（最终一致）或同步重试（强一致）保证最终一致
  - 关联需求：REQ-DC-006
  - 关联设计：design.md §2.1.4
  - 验收：AC-DC-6 失效消息丢失兜底
  - 依赖：M2-T12.1

- [ ] **M2-T16.4** 编写缓存击穿/雪崩防护集成测试：高并发不存在 key 穿透请求数 ≤ 1；批量过期 TTL 标准差 ≥ 抖动范围
  - 关联需求：REQ-DC-005
  - 关联设计：design.md §2.1.4
  - 验收：AC-DC-5 击穿穿透 ≤ 1 + 雪崩防护
  - 依赖：M2-T14.4

## 3.17 M2-T17：门禁验证

- [ ] **M2-T17.1** 运行 `cargo fmt --all -- --check` + `cargo check -p sz-orm-core --features dist-cache` + `cargo clippy -p sz-orm-core --all-targets --features dist-cache -- -D warnings` + `cargo test -p sz-orm-core --features dist-cache`
  - 关联需求：REQ-DC-001~006
  - 关联设计：design.md §2.1.5
  - 验收：fmt 通过、编译通过、clippy 零警告、测试全通过；禁止占位实现；unsafe 零容忍
  - 依赖：M2-T16.4

---

# 4. M3 GraphQL 查询支持（REQ-GQL-001~005）

> **目标**：在 sz-orm-graphql 内扩展四项能力：查询解析为 IR、N+1 自动消除（DataLoader）、类型化 Schema 自动生成、查询复杂度限制，通过 feature gate `graphql-n1` / `graphql-schema-gen` / `graphql-complexity` 隔离。
> **周期**：2 周
> **关联设计**：design.md §2.2
> **关联验收**：AC-GQL-1~5（spec §9.2）
> **依赖**：无（与 M1/M2 独立，可并行）

## 4.1 M3-T1：Feature gate 配置与模块骨架

- [ ] **M3-T1.1** 在 `packages/sz-orm-graphql/Cargo.toml` 的 `[features]` 新增 `graphql-n1 = []` / `graphql-schema-gen = ["dep:sz-orm-macros"]` / `graphql-complexity = []`，在 `[dependencies]` 新增 `sz-orm-macros = { version = "2.1.0", path = "../sz-orm-macros", optional = true }`
  - 关联需求：REQ-GQL-001~005
  - 关联设计：design.md §2.2.5
  - 验收：默认不启用；`graphql-schema-gen` 需 sz-orm-macros optional 依赖
  - 依赖：无

- [ ] **M3-T1.2** 创建 `packages/sz-orm-graphql/src/{query_ir,dataloader,schema_gen,complexity}.rs` 模块文件，添加 `#[cfg(feature = "...")]` 条件编译守卫
  - 关联需求：REQ-GQL-001~005
  - 关联设计：design.md §2.2.2
  - 验收：模块文件存在，未启用 feature 时零代码体积
  - 依赖：M3-T1.1

- [ ] **M3-T1.3** 在 `packages/sz-orm-graphql/src/lib.rs` 添加条件导出：`#[cfg(any(feature = "graphql-n1", feature = "graphql-complexity"))] pub mod query_ir;` + `#[cfg(feature = "graphql-n1")] pub mod dataloader;` + `#[cfg(feature = "graphql-schema-gen")] pub mod schema_gen;` + `#[cfg(feature = "graphql-complexity")] pub mod complexity;`
  - 关联需求：REQ-GQL-001~005
  - 关联设计：design.md §2.2.6（lib.rs 导出集成点）
  - 验收：`query_ir` 由 `graphql-n1` 和 `graphql-complexity` 共用，任一启用即导出
  - 依赖：M3-T1.2

## 4.2 M3-T2：GraphQL IR 数据结构（REQ-GQL-001）

- [ ] **M3-T2.1** 在 `query_ir.rs` 定义 `GraphQLOperation` 枚举（`Query` / `Mutation` / `Subscription`）与 `GraphQLValue` 枚举（`Int(i64)` / `Float(f64)` / `String(String)` / `Boolean(bool)` / `Null` / `Enum(String)` / `List(Vec<GraphQLValue>)` / `Object(HashMap<String, GraphQLValue>)` / `Variable(String)`）
  - 关联需求：REQ-GQL-001
  - 关联设计：design.md §2.2.3（GraphQLIR）
  - 验收：GraphQLValue 覆盖所有 GraphQL 字面量类型 + 变量引用
  - 依赖：M3-T1.2

- [ ] **M3-T2.2** 定义 `GraphQLDirective` 结构体（`name: String` + `arguments: HashMap<String, GraphQLValue>`）与 `GraphQLSelection` 结构体（`name` + `alias: Option<String>` + `arguments: HashMap<String, GraphQLValue>` + `directives: Vec<GraphQLDirective>` + `selection_set: Vec<GraphQLSelection>`）
  - 关联需求：REQ-GQL-001
  - 关联设计：design.md §2.2.3
  - 验收：含完整选择集（字段名/别名/参数/指令/子选择集）
  - 依赖：M3-T2.1

- [ ] **M3-T2.3** 定义 `GraphQLIR` 结构体（`operation: GraphQLOperation` + `selection_set: Vec<GraphQLSelection>`）与 `GraphQLParseError` 结构体（含错误位置与原因）
  - 关联需求：REQ-GQL-001
  - 关联设计：design.md §2.2.3
  - 验收：IR 与原始查询文本语义等价（可往返解析，spec §6.2）
  - 依赖：M3-T2.2

## 4.3 M3-T3：查询解析函数（REQ-GQL-001）

- [ ] **M3-T3.1** 实现 `parse_query(query_text: &str, variables: Option<Value>) -> Result<GraphQLIR, GraphQLParseError>` 函数：复用 async-graphql 的 `async_graphql::parser::parse_query_token` 解析为 AST，再转换为内部 `GraphQLIR`
  - 关联需求：REQ-GQL-001
  - 关联设计：design.md §2.2.3（parse_query）
  - 验收：合法查询解析为 IR；非法查询返回 `GraphQLParseError`（含错误位置与原因，spec §5.2.3 异常场景1）
  - 依赖：M3-T2.3

## 4.4 M3-T4：BatchLoader trait（REQ-GQL-002）

- [ ] **M3-T4.1** 在 `dataloader.rs` 定义 `BatchLoader<K, V>` trait（`batch_load(keys: Vec<K>) -> BoxFuture<'_, Result<HashMap<K, V>, BatchLoadError>>`）与 `BatchLoadError` 枚举
  - 关联需求：REQ-GQL-002
  - 关联设计：design.md §2.2.3（BatchLoader trait）
  - 验收：调用方实现批量加载逻辑（如 `SELECT * FROM orders WHERE user_id IN (?, ?, ?)`）
  - 依赖：M3-T1.2

## 4.5 M3-T5：DataLoader 实现（REQ-GQL-002）

- [ ] **M3-T5.1** 定义 `DataLoader<K, V>` 结构体（`batch_loader: Arc<dyn BatchLoader<K, V>>` + `pending: parking_lot::Mutex<HashMap<K, Vec<oneshot::Sender<V>>>>` + `tick_handle: Option<tokio::task::JoinHandle>`）
  - 关联需求：REQ-GQL-002
  - 关联设计：design.md §2.2.3（DataLoader）
  - 验收：当前 tick 收集的请求 + 回填 channel
  - 依赖：M3-T4.1

- [ ] **M3-T5.2** 实现 `DataLoader::load(key: K) -> BoxFuture<'_, Result<V, BatchLoadError>>` 方法：收集请求到 pending，返回 oneshot Receiver
  - 关联需求：REQ-GQL-002
  - 关联设计：design.md §2.2.4
  - 验收：当前 tick 结束时触发 batch_load 并回填所有 pending
  - 依赖：M3-T5.1

- [ ] **M3-T5.3** 实现 tick 机制：`tokio::spawn` 在当前事件循环 tick 结束（`tokio::task::yield_now`）时触发 batch_load，合并当前 tick 内所有 load 请求为一次批量
  - 关联需求：REQ-GQL-002
  - 关联设计：design.md §2.2.4（DataLoader 批量加载与回填流程）
  - 验收：单 tick 内所有 load 合并为 1 次批量；键去重避免重复查询；按键映射回填保持顺序（spec §6.2）
  - 依赖：M3-T5.2

## 4.6 M3-T6：DataLoader 集成到 resolver 路径（REQ-GQL-002）

- [ ] **M3-T6.1** 在 `packages/sz-orm-graphql/src/resolver.rs`（resolver.rs:69）的 `DbResolver` 执行路径外批量收集关联字段访问，集成 DataLoader，不修改 `DbResolver` trait
  - 关联需求：REQ-GQL-002
  - 关联设计：design.md §2.2.6（DbResolver trait 集成点）
  - 验收：既有 `DbResolver` trait 与实现不变；N 个关联字段查询次数 ≤ 2
  - 依赖：M3-T5.3

## 4.7 M3-T7：SchemaGenerator 与类型映射（REQ-GQL-003）

- [ ] **M3-T7.1** 在 `schema_gen.rs` 定义 `TypeMapping`（Rust → GraphQL 映射表：`String → String` / `i32 → Int` / `i64 → BigInt` / `f64 → Float` / `bool → Boolean` / `Option<T> → T 可空` / `Vec<T> → [T] 列表` / `NaiveDate → Date` / `DateTime → DateTime` / `Uuid → ID`）
  - 关联需求：REQ-GQL-003
  - 关联设计：design.md §2.2.3（TypeMapping）
  - 验收：类型映射明确（spec §6.2）；不支持类型告警跳过
  - 依赖：M3-T1.2

- [ ] **M3-T7.2** 定义 `SchemaGenerator` 结构体，实现 `from_model<M: Model>() -> GraphQLSchema` 方法：通过 `M::table_name()` + `M::columns()` 获取表名 + 字段名 + 类型，按 `TypeMapping` 映射为 GraphQL 类型，调用 `GraphQLSchema::add_type` / `add_query` / `add_mutation` 构建
  - 关联需求：REQ-GQL-003
  - 关联设计：design.md §2.2.3（SchemaGenerator）
  - 验收：Rust 类型与 GraphQL 类型一致（字段名、类型映射、可空性）；生成的 Schema 可直接用于 GraphQL 查询执行
  - 依赖：M3-T7.1

- [ ] **M3-T7.3** 实现不支持类型处理：复杂嵌套枚举、泛型等告警跳过（spec §5.2.3 异常场景3），告警含不支持字段与类型
  - 关联需求：REQ-GQL-003
  - 关联设计：design.md §2.2.3
  - 验收：不支持类型告警跳过；用户可手动标注覆盖
  - 依赖：M3-T7.2

## 4.8 M3-T8：过程宏提取字段元数据（REQ-GQL-003）

- [ ] **M3-T8.1** 在 `packages/sz-orm-macros` 增强过程宏，从 `#[derive(Model)]` 结构体提取字段元数据（字段名 + Rust 类型 + 可空性），供 `SchemaGenerator` 使用
  - 关联需求：REQ-GQL-003
  - 关联设计：design.md §2.2.5 + 决策表（Schema 自动生成方式：过程宏编译时生成）
  - 验收：零运行时开销；复用 sz-orm-macros 过程宏能力
  - 依赖：M3-T7.2

## 4.9 M3-T9：复杂度配置与计算（REQ-GQL-004）

- [ ] **M3-T9.1** 在 `complexity.rs` 定义 `ComplexityConfig` 结构体（`max_depth: u32` 默认 10 + `max_fields: u32` 默认 100 + `max_cost: u64` 默认 1000 + `field_weights: HashMap<String, u64>` 默认 1）
  - 关联需求：REQ-GQL-004
  - 关联设计：design.md §2.2.3（ComplexityConfig）
  - 验收：深度/字段数/成本三项可独立配置（spec §6.2）；字段权重可按字段配置
  - 依赖：M3-T1.2

- [ ] **M3-T9.2** 定义 `ComplexityResult` 结构体（`depth: u32` + `field_count: u32` + `cost: u64` + `exceeded: Option<ComplexityError>`）与 `ComplexityError` 枚举（`DepthExceeded` / `FieldsExceeded` / `CostExceeded`）
  - 关联需求：REQ-GQL-004
  - 关联设计：design.md §2.2.3
  - 验收：超限错误含深度/字段数/成本超限详情
  - 依赖：M3-T9.1

- [ ] **M3-T9.3** 定义 `ComplexityCalculator`，实现 `calculate(ir: &GraphQLIR) -> ComplexityResult` 方法：深度 = 最大嵌套层级；字段数 = 所有选择集字段总数；成本 = Σ(字段权重 × 子树深度)（递归累加）
  - 关联需求：REQ-GQL-004
  - 关联设计：design.md §2.2.3（ComplexityCalculator）
  - 验收：计算开销 ≤ 查询执行总耗时的 5%（spec §4.1 性能）
  - 依赖：M3-T9.2

## 4.10 M3-T10：复杂度超限拒绝（REQ-GQL-004）

- [ ] **M3-T10.1** 在 GraphQL 查询执行前集成复杂度校验：`ComplexityCalculator::calculate(ir)` 返回 `exceeded` 时拒绝查询并返回 `ComplexityError`，合法查询正常执行
  - 关联需求：REQ-GQL-004
  - 关联设计：design.md §2.2.4（GraphQL 查询主流程）
  - 验收：深度/字段数/成本超限查询被拒绝；合法查询正常执行
  - 依赖：M3-T9.3

## 4.11 M3-T11：GraphQL 变量参数化绑定（REQ-GQL-005）

- [ ] **M3-T11.1** 确保 DataLoader 批量 SQL 参数化绑定：`SELECT * FROM orders WHERE user_id IN ($1, $2, $3)`，禁止将 GraphQL 变量字符串拼接到 SQL
  - 关联需求：REQ-GQL-005
  - 关联设计：design.md §2.2.4 + spec §4.3 安全性 C-03
  - 验收：GraphQL 变量参数化绑定到下游 SQL；注入载荷作为参数值而非语法
  - 依赖：M3-T6.1

## 4.12 M3-T12：单元测试

- [ ] **M3-T12.1** 编写 `parse_query` 单元测试：合法查询解析为 IR（选择集/字段/参数/指令完整）、非法查询返回解析错误
  - 关联需求：REQ-GQL-001
  - 关联设计：design.md §2.2.3
  - 验收：AC-GQL-1 IR 含完整选择集
  - 依赖：M3-T3.1

- [ ] **M3-T12.2** 编写 `DataLoader` 单元测试：单 tick 收集合并、键去重、按键映射回填保持顺序
  - 关联需求：REQ-GQL-002
  - 关联设计：design.md §2.2.4
  - 验收：批量键唯一；结果按键映射回各请求点，顺序与原始请求一致（spec §6.2）
  - 依赖：M3-T5.3

- [ ] **M3-T12.3** 编写 `SchemaGenerator` 单元测试：Rust 模型 → Schema 生成（类型/字段一一对应）、不支持类型告警跳过
  - 关联需求：REQ-GQL-003
  - 关联设计：design.md §2.2.3
  - 验收：AC-GQL-3 类型/字段一一对应
  - 依赖：M3-T7.3, M3-T8.1

- [ ] **M3-T12.4** 编写 `ComplexityCalculator` 单元测试：深度/字段数/成本计算正确、超限返回错误、合法查询正常
  - 关联需求：REQ-GQL-004
  - 关联设计：design.md §2.2.3
  - 验收：AC-GQL-4 超限拒绝 + 合法执行
  - 依赖：M3-T10.1

## 4.13 M3-T13：差分测试（批量 vs 逐条）

- [ ] **M3-T13.1** 编写 DataLoader 差分测试：N 个关联字段查询，批量加载结果与逐条加载完全一致（含关联字段顺序与数据）
  - 关联需求：REQ-GQL-002
  - 关联设计：design.md §2.2.4 + spec §4.2 可靠性
  - 验收：AC-GQL-2 结果与逐条一致；查询次数 ≤ 2；减少 ≥ 90%
  - 依赖：M3-T12.2

## 4.14 M3-T14：集成测试

- [ ] **M3-T14.1** 编写 N+1 消除集成测试：启用 `graphql-n1` 后 N 个关联字段查询次数 ≤ 2，未启用时 N+1 次
  - 关联需求：REQ-GQL-002
  - 关联设计：design.md §2.2.4
  - 验收：AC-GQL-2 查询次数 ≤ 2（基准证据）
  - 依赖：M3-T13.1

- [ ] **M3-T14.2** 编写 Schema 自动生成集成测试：生成的 Schema 可直接用于 GraphQL 查询执行
  - 关联需求：REQ-GQL-003
  - 关联设计：design.md §2.2.3
  - 验收：AC-GQL-3 生成 Schema 可用于查询执行
  - 依赖：M3-T12.3

- [ ] **M3-T14.3** 编写复杂度限制集成测试：深度上限 5 + 提交深度 6 查询 → 拒绝；字段数上限 100 + 提交 101 字段 → 拒绝；成本上限 1000 + 高成本查询 → 拒绝；复杂度计算开销 ≤ 5% 基准
  - 关联需求：REQ-GQL-004
  - 关联设计：design.md §2.2.3
  - 验收：AC-GQL-4 超限拒绝 + 开销 ≤ 5%
  - 依赖：M3-T12.4

- [ ] **M3-T14.4** 编写 GraphQL 变量注入防护集成测试：变量含注入载荷 → 参数化绑定，载荷作为参数值而非语法，下游 SQL 执行安全
  - 关联需求：REQ-GQL-005
  - 关联设计：design.md §2.2.4
  - 验收：AC-GQL-5 变量参数化无注入
  - 依赖：M3-T11.1

## 4.15 M3-T15：门禁验证

- [ ] **M3-T15.1** 运行 `cargo fmt --all -- --check` + `cargo check -p sz-orm-graphql --features graphql-n1,graphql-schema-gen,graphql-complexity` + `cargo clippy -p sz-orm-graphql --all-targets --features graphql-n1,graphql-schema-gen,graphql-complexity -- -D warnings` + `cargo test -p sz-orm-graphql --features graphql-n1,graphql-schema-gen,graphql-complexity`
  - 关联需求：REQ-GQL-001~005
  - 关联设计：design.md §2.2.5
  - 验收：fmt 通过、编译通过、clippy 零警告、测试全通过；禁止占位实现；unsafe 零容忍
  - 依赖：M3-T14.4

---

# 5. M4 AI 自然语言查询增强（REQ-AI-001~006）

> **目标**：在 sz-orm-ai 内扩展三项能力：NL2SQL 增强、查询意图分析、自动索引建议、查询重写建议，通过 feature gate `ai-nl2sql-enhanced` / `ai-index-advisor` / `ai-rewrite-advisor` 隔离。所有 AI 建议仅作建议展示，禁止自动执行 LLM 生成的 SQL/DDL。
> **周期**：2 周
> **关联设计**：design.md §2.4
> **关联验收**：AC-AI-1~6（spec §9.4）
> **依赖**：无（与 M1/M2/M3 独立，可并行）

## 5.1 M4-T1：Feature gate 配置与模块骨架

- [ ] **M4-T1.1** 在 `packages/sz-orm-ai/Cargo.toml` 的 `[features]` 新增 `ai-nl2sql-enhanced = []` / `ai-index-advisor = ["dep:sqlparser"]` / `ai-rewrite-advisor = ["dep:sqlparser"]`，在 `[dependencies]` 新增 `sqlparser = { workspace = true, optional = true }`
  - 关联需求：REQ-AI-001~006
  - 关联设计：design.md §2.4.5
  - 验收：默认不启用；`ai-index-advisor` 与 `ai-rewrite-advisor` 需 sqlparser optional 依赖
  - 依赖：无

- [ ] **M4-T1.2** 创建 `packages/sz-orm-ai/src/{intent_analysis,index_advisor,rewrite_advisor}.rs` 模块文件，添加 `#[cfg(feature = "...")]` 条件编译守卫
  - 关联需求：REQ-AI-001~006
  - 关联设计：design.md §2.4.2
  - 验收：模块文件存在，未启用 feature 时零代码体积
  - 依赖：M4-T1.1

- [ ] **M4-T1.3** 在 `packages/sz-orm-ai/src/lib.rs` 添加条件导出：`#[cfg(feature = "ai-nl2sql-enhanced")] pub mod intent_analysis;` + `#[cfg(feature = "ai-index-advisor")] pub mod index_advisor;` + `#[cfg(feature = "ai-rewrite-advisor")] pub mod rewrite_advisor;`
  - 关联需求：REQ-AI-001~006
  - 关联设计：design.md §2.4.6（lib.rs 导出集成点）
  - 验收：条件导出正确
  - 依赖：M4-T1.2

## 5.2 M4-T2：NL2SQL prompt 增强（REQ-AI-001）

- [ ] **M4-T2.1** 在 `packages/sz-orm-ai/src/nl2sql.rs`（nl2sql.rs:80）增强 `OpenAINl2SqlEngine` 的 LLM prompt：含完整 schema（表名 + 列名 + 类型）+ 关系信息（外键 + JOIN 关系），支持多表 JOIN + 聚合 + 子查询 + 排序 + 分页
  - 关联需求：REQ-AI-001
  - 关联设计：design.md §2.4.6（Nl2SqlEngine 集成点）
  - 验收：既有 `Nl2SqlEngine` trait 与 `SimpleNl2SqlEngine` 不变；增强 prompt 支持复杂查询
  - 依赖：M4-T1.2

- [ ] **M4-T2.2** 确保 NL2SQL 输出经 `safety::validate_select_only` + `safety::validate_no_injection` 安全验证（仅 SELECT + 注入检测），LLM 请求经 `sql_sanitizer` 脱敏输入
  - 关联需求：REQ-AI-001
  - 关联设计：design.md §2.4.6（safety + sql_sanitizer 集成点）
  - 验收：生成 SQL 仅 SELECT；LLM 请求内容敏感字面量已脱敏；安全验证失败返回 `SqlSafetyCheckFailed`
  - 依赖：M4-T2.1

## 5.3 M4-T3：查询意图分析（REQ-AI-002）

- [ ] **M4-T3.1** 在 `intent_analysis.rs` 定义 `QueryIntent` 枚举（`Select` / `Insert` / `Update` / `Delete`）与 `RiskLevel` 枚举（`Low` / `Medium` / `High`）
  - 关联需求：REQ-AI-002
  - 关联设计：design.md §2.4.3（QueryIntent）
  - 验收：写操作（Insert/Update/Delete）标注高风险（spec §5.4.1 规则2）
  - 依赖：M4-T1.2

- [ ] **M4-T3.2** 定义 `IntentAnalysisResult` 结构体（`intent: QueryIntent` + `table: String` + `conditions: Vec<ParameterizedCondition>` + `ordering: Vec<OrderField>` + `pagination: Option<Pagination>` + `update_fields: Vec<(String, Value)>` + `risk_level: RiskLevel` + `confidence: f32` + `candidates: Vec<IntentAnalysisResult>`）
  - 关联需求：REQ-AI-002
  - 关联设计：design.md §2.4.3（IntentAnalysisResult）
  - 验收：意图模糊时多候选 + 置信度（spec §5.4.3 异常场景3）
  - 依赖：M4-T3.1

- [ ] **M4-T3.3** 定义 `IntentAnalyzer` 结构体，实现 `analyze(natural_language: &str, schema: &SchemaContext) -> BoxFuture<'_, Result<IntentAnalysisResult, IntentError>>` 方法：构造 LLM prompt（含 schema + 自然语言），调用 LLM 识别意图 + 提取参数；输入经 `sql_sanitizer` 脱敏；写操作标注 High 风险
  - 关联需求：REQ-AI-002
  - 关联设计：design.md §2.4.3（IntentAnalyzer）
  - 验收：延迟 ≤ 5s P95（spec §4.1 性能）；LLM 请求内容脱敏；不阻塞业务查询主路径
  - 依赖：M4-T3.2

## 5.4 M4-T4：自动索引建议（REQ-AI-003）

- [ ] **M4-T4.1** 在 `index_advisor.rs` 定义 `IndexType` 枚举（`BTree` / `Hash` / `GIN` / `BRIN` 等，按方言选择）与 `QueryPattern` 结构体（`sql_template: String` + `frequency: u64` + `columns_accessed: Vec<String>`）与 `SlowQueryLog` 结构体
  - 关联需求：REQ-AI-003
  - 关联设计：design.md §2.4.3
  - 验收：索引类型覆盖五方言常见类型
  - 依赖：M4-T1.2

- [ ] **M4-T4.2** 定义 `BenefitEstimate` 结构体（`speedup_ratio: f64` + `confidence: f32` + `uncertain: bool`）与 `IndexSuggestion` 结构体（`index_columns: Vec<String>` + `index_type: IndexType` + `ddl_text: String` + `expected_benefit: BenefitEstimate` + `evidence: Vec<QueryPattern>`）
  - 关联需求：REQ-AI-003
  - 关联设计：design.md §2.4.3（IndexSuggestion）
  - 验收：建议含索引列/类型/DDL/收益/证据；收益不确定标注（spec §5.4.3 异常场景4）
  - 依赖：M4-T4.1

- [ ] **M4-T4.3** 定义 `IndexAdvisor` 结构体，实现 `suggest(query_patterns: &[QueryPattern], slow_queries: &[SlowQueryLog]) -> BoxFuture<'_, Result<Vec<IndexSuggestion>, IndexError>>` 方法：sqlparser 解析查询（WHERE/JOIN/ORDER BY 列）+ 识别高频查询模式 + 规则型分析（列组合 + 选择性）+ 可选 LLM 建议
  - 关联需求：REQ-AI-003
  - 关联设计：design.md §2.4.3（IndexAdvisor）
  - 验收：DDL 不自动执行（spec §4.3 安全性 C-09）；建议附查询模式证据；延迟 ≤ 10s P95
  - 依赖：M4-T4.2

## 5.5 M4-T5：查询重写建议（REQ-AI-004）

- [ ] **M4-T5.1** 在 `rewrite_advisor.rs` 定义 `TransformType` 枚举（`PredicatePushdown` / `SubqueryFlattening` / `JoinReorder` / `RedundantElimination`）与 `EquivalenceProof` 结构体（`proof_text: String` + `verified: bool` + `unverified: bool`）
  - 关联需求：REQ-AI-004
  - 关联设计：design.md §2.4.3
  - 验收：等价性未验证标注（spec §5.4.3 异常场景5）
  - 依赖：M4-T1.2

- [ ] **M4-T5.2** 定义 `RewriteSuggestion` 结构体（`original_sql: String` + `rewritten_sql: String` + `transform_type: TransformType` + `equivalence_proof: EquivalenceProof` + `expected_benefit: BenefitEstimate`）
  - 关联需求：REQ-AI-004
  - 关联设计：design.md §2.4.3（RewriteSuggestion）
  - 验收：建议含原始/重写/变换/论证/收益
  - 依赖：M4-T5.1

- [ ] **M4-T5.3** 定义 `RewriteAdvisor` 结构体，实现 `suggest(sql: &str, schema: &SchemaContext) -> BoxFuture<'_, Result<Vec<RewriteSuggestion>, RewriteError>>` 方法：sqlparser 解析 SQL 为 AST，识别可优化模式（谓词下推/子查询展开/JOIN 顺序/冗余条件）+ 规则型分析 + 可选 LLM 建议
  - 关联需求：REQ-AI-004
  - 关联设计：design.md §2.4.3（RewriteAdvisor）
  - 验收：不自动重写（spec §4.3 安全性 C-09）；支持谓词下推 + 子查询展开（spec §5.4.1 规则4）
  - 依赖：M4-T5.2

## 5.6 M4-T6：AI 建议审计记录（REQ-AI-005）

- [ ] **M4-T6.1** 定义 `AdviceSource` 枚举（`Rule` / `Llm`）与 `AdviceType` 枚举（`Nl2Sql` / `Intent` / `Index` / `Rewrite`）与 `AiAdviceAuditRecord` 结构体（`source_engine: AdviceSource` + `llm_model: Option<String>` + `confidence: f32` + `advice_type: AdviceType` + `timestamp: i64`）
  - 关联需求：REQ-AI-005
  - 关联设计：design.md §2.4.3（AiAdviceAuditRecord）
  - 验收：每条建议记录来源/模型/置信度/类型（spec §4.4 可维护性）
  - 依赖：M4-T1.2

- [ ] **M4-T6.2** 在 NL2SQL/意图分析/索引建议/重写建议生成路径埋点记录 `AiAdviceAuditRecord`
  - 关联需求：REQ-AI-005
  - 关联设计：design.md §2.4.4
  - 验收：所有 AI 建议均有审计记录；LLM 请求内容脱敏
  - 依赖：M4-T6.1

## 5.7 M4-T7：LLM 降级处理（REQ-AI-001~004）

- [ ] **M4-T7.1** 实现 LLM 服务不可用降级：返回 `LlmServiceUnavailable` 错误，降级为规则型建议（如适用，索引/重写有规则型路径），不阻塞业务查询主路径
  - 关联需求：REQ-AI-001~004
  - 关联设计：design.md §2.4.4 + spec §5.4.3 异常场景1
  - 验收：LLM 不可用降级；不阻塞业务主路径
  - 依赖：M4-T3.3, M4-T4.3, M4-T5.3

## 5.8 M4-T8：单元测试

- [ ] **M4-T8.1** 编写 NL2SQL 增强单元测试：多表 JOIN + 聚合 + 分页自然语言查询生成正确参数化 SQL，安全验证仅 SELECT，LLM 请求脱敏
  - 关联需求：REQ-AI-001
  - 关联设计：design.md §2.4.3
  - 验收：AC-AI-1 多表 JOIN + 仅 SELECT + 脱敏
  - 依赖：M4-T2.2

- [ ] **M4-T8.2** 编写意图分析单元测试：SELECT/INSERT/UPDATE/DELETE 意图识别 + 参数提取 + 写操作高风险标注
  - 关联需求：REQ-AI-002
  - 关联设计：design.md §2.4.3
  - 验收：AC-AI-2 意图识别 + 写操作高风险
  - 依赖：M4-T3.3

- [ ] **M4-T8.3** 编写索引建议单元测试：查询模式分析 + 索引建议含列/类型/DDL/收益/证据 + DDL 不自动执行
  - 关联需求：REQ-AI-003
  - 关联设计：design.md §2.4.3
  - 验收：AC-AI-3 索引建议 + 不执行
  - 依赖：M4-T4.3

- [ ] **M4-T8.4** 编写重写建议单元测试：谓词下推/子查询展开建议 + 等价性论证 + 不自动重写
  - 关联需求：REQ-AI-004
  - 关联设计：design.md §2.4.3
  - 验收：AC-AI-4 重写建议 + 不重写
  - 依赖：M4-T5.3

## 5.9 M4-T9：集成测试与安全验证

- [ ] **M4-T9.1** 编写 AI 建议零数据库执行测试：mock DB 断言无 execute 调用，AI 建议路径仅返回建议文本
  - 关联需求：REQ-AI-006
  - 关联设计：design.md §2.4.4
  - 验收：AC-AI-6 零数据库执行（安全铁律验证）
  - 依赖：M4-T8.4

- [ ] **M4-T9.2** 编写 AI 建议审计记录集成测试：所有 AI 建议记录来源/模型/置信度/类型，LLM 请求脱敏
  - 关联需求：REQ-AI-005
  - 关联设计：design.md §2.4.4
  - 验收：AC-AI-5 审计记录 + 脱敏
  - 依赖：M4-T6.2

- [ ] **M4-T9.3** 编写 NL2SQL 安全验证集成测试：注入载荷测试集，安全验证拦截非 SELECT + 注入风险
  - 关联需求：REQ-AI-001
  - 关联设计：design.md §2.4.6
  - 验收：安全验证 100% 覆盖；注入载荷被拦截
  - 依赖：M4-T8.1

## 5.10 M4-T10：延迟基准测试

- [ ] **M4-T10.1** 编写 NL2SQL 延迟基准测试：单条转换延迟 ≤ 10s P95（含 LLM 调用）
  - 关联需求：REQ-AI-001
  - 关联设计：design.md §2.4.3
  - 验收：AC-AI-1 延迟 ≤ 10s P95
  - 依赖：M4-T9.3

- [ ] **M4-T10.2** 编写意图分析延迟基准测试：延迟 ≤ 5s P95
  - 关联需求：REQ-AI-002
  - 关联设计：design.md §2.4.3
  - 验收：AC-AI-2 延迟 ≤ 5s P95
  - 依赖：M4-T10.1

- [ ] **M4-T10.3** 编写索引/重写建议延迟基准测试：延迟 ≤ 10s P95
  - 关联需求：REQ-AI-003, REQ-AI-004
  - 关联设计：design.md §2.4.3
  - 验收：AC-AI-3/4 延迟 ≤ 10s P95
  - 依赖：M4-T10.2

## 5.11 M4-T11：门禁验证

- [ ] **M4-T11.1** 运行 `cargo fmt --all -- --check` + `cargo check -p sz-orm-ai --features ai-nl2sql-enhanced,ai-index-advisor,ai-rewrite-advisor` + `cargo clippy -p sz-orm-ai --all-targets --features ai-nl2sql-enhanced,ai-index-advisor,ai-rewrite-advisor -- -D warnings` + `cargo test -p sz-orm-ai --features ai-nl2sql-enhanced,ai-index-advisor,ai-rewrite-advisor`
  - 关联需求：REQ-AI-001~006
  - 关联设计：design.md §2.4.5
  - 验收：fmt 通过、编译通过、clippy 零警告、测试全通过；禁止占位实现；unsafe 零容忍
  - 依赖：M4-T10.3

---

# 6. M5 集成验证与发布

> **目标**：8 feature 全组合编译 + 五方言集成测试 + sz-pay/sz-rust 零回归验证 + 性能基准不回退验证 + 22 条 REQ 验收 + 文档更新 + 版本发布。
> **周期**：1 周
> **关联设计**：design.md §2.5（里程碑规划）+ §2.7（验收标准映射）
> **关联验收**：AC-ALL-1~8（spec §9.5）
> **依赖**：M1 + M2 + M3 + M4 全方向就绪

## 6.1 M5-T1：8 Feature 全组合编译

- [ ] **M5-T1.1** 运行 `cargo check --workspace --all-targets --all-features` 验证 8 个新 feature 与既有 feature 全组合编译通过
  - 关联需求：AC-ALL-4
  - 关联设计：design.md §2.5.2（M5 交付物）+ spec §4.4 可维护性 C-07
  - 验收：全组合编译通过（门禁 10）；feature 正交性验证
  - 依赖：M1-T12.2, M2-T17.1, M3-T15.1, M4-T11.1

- [ ] **M5-T1.2** 运行 `cargo clippy --workspace --all-targets --all-features -- -D warnings` 验证全组合 clippy 零警告
  - 关联需求：AC-ALL-3
  - 关联设计：design.md §2.5.2
  - 验收：clippy 零警告
  - 依赖：M5-T1.1

## 6.2 M5-T2：五方言集成测试

- [ ] **M5-T2.1** 运行五方言集成测试：`cargo test --workspace -- --ignored`（MySQL/PostgreSQL/SQLite/Oracle/MSSQL 真实服务集成测试）
  - 关联需求：AC-ALL-7
  - 关联设计：design.md §2.5.2 + spec §4.5 兼容性 C-07
  - 验收：五方言行为一致；多租户 Schema 隔离在各方言支持差异处理
  - 依赖：M5-T1.2

- [ ] **M5-T2.2** 验证五方言下四项能力行为一致：分布式缓存（Redis 后端）、GraphQL 查询、多租户隔离、AI 建议
  - 关联需求：AC-ALL-7
  - 关联设计：design.md §2.7.5
  - 验收：五方言下四项能力行为一致
  - 依赖：M5-T2.1

## 6.3 M5-T3：sz-pay/sz-rust 零回归验证

- [ ] **M5-T3.1** 在 sz-pay 项目（`E:\vue\test\sz-pay\server\sz-rust`）升级 sz-orm-core/sz-orm-sqlx 等依赖到 v3.3.0，运行 5139 测试基线验证零回归
  - 关联需求：AC-ALL-5
  - 关联设计：design.md §2.5.2 + spec §4.5 兼容性 C-04
  - 验收：sz-pay 5139 测试零回归；feature gate 默认关闭确保默认零行为变更
  - 依赖：M5-T2.2

- [ ] **M5-T3.2** 在 sz-rust 项目验证 v3.3.0 升级零回归
  - 关联需求：AC-ALL-5
  - 关联设计：design.md §2.5.2
  - 验收：sz-rust 测试零回归
  - 依赖：M5-T3.1

## 6.4 M5-T4：性能基准不回退验证

- [ ] **M5-T4.1** 运行 v3.2.0 性能基准对比测试：冷启动 P95 ≤ 20ms、查询计划缓存命中率 ≥ 80%、零拷贝反序列化分配减少 ≥ 50%、SIMD 批量解码吞吐量 ≥ 2x
  - 关联需求：AC-ALL-6
  - 关联设计：design.md §2.7.5 + spec §4.1 性能 8
  - 验收：v3.2.0 性能基准不回退
  - 依赖：M5-T3.2

- [ ] **M5-T4.2** 验证四项新能力性能指标：跨实例失效延迟 ≤ 50ms（Pub/Sub）/ ≤ 1s（Gossip）、Write-behind 吞吐量 ≥ 3x、GraphQL N+1 查询次数 ≤ 2、复杂度计算开销 ≤ 5%、多租户隔离开销 ≤ 5μs（行级）/ ≤ 50μs（Schema）、AI 建议延迟 ≤ 10s/5s P95
  - 关联需求：REQ-DC-001/004, REQ-GQL-002/004, REQ-MT-001/003, REQ-AI-001/002/003
  - 关联设计：design.md §2.7.1~2.7.4
  - 验收：四项能力性能指标达标
  - 依赖：M5-T4.1

## 6.5 M5-T5：22 条 REQ 验收

- [ ] **M5-T5.1** 验收 REQ-DC-001~006（分布式缓存一致性）：对照 spec §7 需求追溯矩阵逐条验证，附 file:line 证据 + 测试输出
  - 关联需求：REQ-DC-001~006
  - 关联设计：design.md §2.7.1
  - 验收：6 条 REQ 全部满足，附代码证据
  - 依赖：M5-T4.2

- [ ] **M5-T5.2** 验收 REQ-GQL-001~005（GraphQL 查询支持）：对照 spec §7 需求追溯矩阵逐条验证
  - 关联需求：REQ-GQL-001~005
  - 关联设计：design.md §2.7.2
  - 验收：5 条 REQ 全部满足
  - 依赖：M5-T5.1

- [ ] **M5-T5.3** 验收 REQ-MT-001~006（多租户与数据隔离）：对照 spec §7 需求追溯矩阵逐条验证
  - 关联需求：REQ-MT-001~006
  - 关联设计：design.md §2.7.3
  - 验收：6 条 REQ 全部满足
  - 依赖：M5-T5.2

- [ ] **M5-T5.4** 验收 REQ-AI-001~006（AI 自然语言查询增强）：对照 spec §7 需求追溯矩阵逐条验证
  - 关联需求：REQ-AI-001~006
  - 关联设计：design.md §2.7.4
  - 验收：6 条 REQ 全部满足
  - 依赖：M5-T5.3

## 6.6 M5-T6：API 兼容性验证

- [ ] **M5-T6.1** 验证无 Breaking Change：对比 v3.2.0 与 v3.3.0 公开 API 签名（`L2Cache` / `QueryBuilder` / `GraphQLSchema` / `Nl2SqlEngine` 等），确认全部保持向后兼容
  - 关联需求：AC-ALL-1
  - 关联设计：design.md §2.7.5 + spec §4.5 兼容性 C-05
  - 验收：无 Breaking Change；既有公开 API 签名不变
  - 依赖：M5-T5.4

- [ ] **M5-T6.2** 验证 feature gate 隔离：`cargo check --workspace`（默认 feature）零新依赖；`cargo check --workspace --all-features` 全组合编译
  - 关联需求：AC-ALL-4
  - 关联设计：design.md §2.7.5
  - 验收：默认 feature 不引入额外依赖与行为变更
  - 依赖：M5-T6.1

## 6.7 M5-T7：10 道门禁全通过

- [ ] **M5-T7.1** 运行 10 道门禁全通过：fmt + check + clippy + test + doc + audit + integration + 禁止占位实现检查 + SQL 注入扫描 + Feature 全组合编译 + ADR-0001 上游未修改检查
  - 关联需求：AC-ALL-2, AC-ALL-3
  - 关联设计：AGENTS.md §10 道门禁
  - 验收：10 道门禁全通过；`git diff --name-only HEAD` 零上游修改（ADR-0001）
  - 依赖：M5-T6.2

- [ ] **M5-T7.2** 运行 `cargo audit` + `cargo deny check` 安全审计
  - 关联需求：AC-ALL-2
  - 关联设计：AGENTS.md §10 道门禁 6
  - 验收：安全审计通过
  - 依赖：M5-T7.1

## 6.8 M5-T8：文档更新

- [ ] **M5-T8.1** 更新 `CHANGELOG.md`：新增 v3.3.0 版本条目，列出四项能力扩展（分布式缓存一致性 + GraphQL 查询支持 + 多租户与数据隔离 + AI 自然语言查询增强）+ 8 个 feature gate + 22 条 REQ
  - 关联需求：AC-ALL-8
  - 关联设计：design.md §2.5.2
  - 验收：CHANGELOG 含 v3.3.0 完整变更记录
  - 依赖：M5-T7.2

- [ ] **M5-T8.2** 更新 `README.md`：新增 v3.3.0 能力介绍 + 8 个 feature gate 启用方式 + 使用示例
  - 关联需求：AC-ALL-8
  - 关联设计：design.md §2.5.2
  - 验收：README 含 v3.3.0 能力介绍与启用方式
  - 依赖：M5-T8.1

- [ ] **M5-T8.3** 更新 `docs/sz-orm-engineering-practices.md`：新增 v3.3.0 工程化审查要点 + 8 feature 组合矩阵 + 五方言集成测试指南
  - 关联需求：AC-ALL-7
  - 关联设计：AGENTS.md §工程化审查规范
  - 验收：工程化实践文档含 v3.3.0 审查要点
  - 依赖：M5-T8.2

- [ ] **M5-T8.4** 编写 sz-pay/sz-rust 升级指南文档：v3.3.0 升级步骤 + feature gate 启用建议 + 注意事项
  - 关联需求：AC-ALL-5
  - 关联设计：design.md §2.6（R-14 缓解措施）
  - 验收：升级指南文档完整
  - 依赖：M5-T8.3

## 6.9 M5-T9：版本发布

- [ ] **M5-T9.1** 更新 `Cargo.toml` workspace.package.version 从 1.2.2 到 1.3.0（语义化版本，新增能力 minor 版本）
  - 关联需求：AC-ALL-1
  - 关联设计：design.md §2.5.2
  - 验收：workspace 版本统一更新
  - 依赖：M5-T8.4

- [ ] **M5-T9.2** 运行 `cargo publish --dry-run` 验证发布就绪，确认 crates.io 发布元数据正确
  - 关联需求：AC-ALL-1
  - 关联设计：design.md §2.5.2
  - 验收：dry-run 通过；发布元数据正确
  - 依赖：M5-T9.1

---

# 7. 依赖关系图

## 7.1 里程碑间依赖

```
M1 多租户与数据隔离 ──→ M2 分布式缓存一致性（复用 tenant_context）
                      ──→ M5 集成验证与发布
M2 分布式缓存一致性 ──→ M5
M3 GraphQL 查询支持 ──→ M5（与 M1/M2/M4 独立，可并行）
M4 AI 自然语言查询增强 ──→ M5（与 M1/M2/M3 独立，可并行）
```

## 7.2 M1 内部依赖

```
M1-T1 (feature 配置) ──→ M1-T2 (TenantContext) ──→ M1-T4 (自动注入)
                      ──→ M1-T3 (Schema 隔离)
                      ──→ M1-T5 (连接池)
                      ──→ M1-T6 (行级安全)
                      ──→ M1-T7 (列级脱敏)
                      ──→ M1-T8 (审计)
M1-T2~T8 ──→ M1-T9 (单元测试) ──→ M1-T10 (集成测试) ──→ M1-T11 (竞态测试) ──→ M1-T12 (门禁) ──→ M1-T13 (兼容性)
```

## 7.3 M2 内部依赖

```
M2-T1 (feature 配置) ──→ M2-T2 (一致性级别) ──→ M2-T5 (写路径分派)
                      ──→ M2-T3 (Redis Pub/Sub) ──→ M2-T5
                      ──→ M2-T4 (Gossip)
                      ──→ M2-T6 (Write-behind 配置) ──→ M2-T7 (WAL) ──→ M2-T8 (刷盘回放)
                      ──→ M2-T9 (布隆过滤器)
                      ──→ M2-T10 (互斥锁)
                      ──→ M2-T11 (随机 TTL)
M2-T5 ──→ M2-T12 (失效兜底)
M2-T3 ──→ M2-T13 (telemetry)
M2-T3~T13 ──→ M2-T14 (单元测试) ──→ M2-T15 (集成测试) ──→ M2-T16 (宕机网络分区测试) ──→ M2-T17 (门禁)
```

## 7.4 M3 内部依赖

```
M3-T1 (feature 配置) ──→ M3-T2 (IR 数据结构) ──→ M3-T3 (parse_query)
                      ──→ M3-T4 (BatchLoader) ──→ M3-T5 (DataLoader) ──→ M3-T6 (集成 resolver)
                      ──→ M3-T7 (SchemaGenerator) ──→ M3-T8 (过程宏)
                      ──→ M3-T9 (复杂度配置) ──→ M3-T10 (超限拒绝)
M3-T6 ──→ M3-T11 (变量参数化)
M3-T3~T11 ──→ M3-T12 (单元测试) ──→ M3-T13 (差分测试) ──→ M3-T14 (集成测试) ──→ M3-T15 (门禁)
```

## 7.5 M4 内部依赖

```
M4-T1 (feature 配置) ──→ M4-T2 (NL2SQL 增强)
                      ──→ M4-T3 (意图分析)
                      ──→ M4-T4 (索引建议)
                      ──→ M4-T5 (重写建议)
                      ──→ M4-T6 (审计记录)
M4-T3~T5 ──→ M4-T7 (LLM 降级)
M4-T2~T7 ──→ M4-T8 (单元测试) ──→ M4-T9 (集成安全测试) ──→ M4-T10 (延迟基准) ──→ M4-T11 (门禁)
```

## 7.6 M5 内部依赖

```
M1~M4 全就绪 ──→ M5-T1 (全组合编译) ──→ M5-T2 (五方言集成) ──→ M5-T3 (下游零回归) ──→ M5-T4 (性能基准) ──→ M5-T5 (22 REQ 验收) ──→ M5-T6 (API 兼容性) ──→ M5-T7 (10 门禁) ──→ M5-T8 (文档更新) ──→ M5-T9 (版本发布)
```

---

# 8. 验收标准映射

## 8.1 方向 1 验收标准映射（分布式缓存一致性）

| 验收标准 | 对应 REQ | 关联任务 | 验证方式 |
|---------|---------|---------|---------|
| AC-DC-1：Redis Pub/Sub 50ms 内同步 | REQ-DC-001 | M2-T3, M2-T15.1 | 多实例集成测试 |
| AC-DC-2：Gossip ≤10 实例 1s 收敛 + 认证 | REQ-DC-002 | M2-T4, M2-T15.2 | 10 实例 gossip 集群测试 |
| AC-DC-3：强一致/最终一致可选 | REQ-DC-003 | M2-T2, M2-T5, M2-T15.3 | 强一致写后读 + 最终一致 TTL 兜底测试 |
| AC-DC-4：Write-behind 3x 吞吐 + WAL 不丢数据 | REQ-DC-004 | M2-T6~T8, M2-T16.1~T16.2 | 吞吐量基准 + 宕机回放测试 |
| AC-DC-5：击穿穿透 ≤1 + 雪崩 TTL 标准差 | REQ-DC-005 | M2-T9~T11, M2-T16.4 | 高并发穿透测试 + TTL 标准差测试 |
| AC-DC-6：失效消息丢失兜底 | REQ-DC-006 | M2-T12, M2-T16.3 | 网络分区/Redis 故障测试 |

## 8.2 方向 2 验收标准映射（GraphQL 查询支持）

| 验收标准 | 对应 REQ | 关联任务 | 验证方式 |
|---------|---------|---------|---------|
| AC-GQL-1：IR 解析含完整选择集 | REQ-GQL-001 | M3-T2~T3, M3-T12.1 | 合法/非法查询解析测试 |
| AC-GQL-2：N+1 消除查询次数 ≤2 | REQ-GQL-002 | M3-T4~T6, M3-T13.1, M3-T14.1 | 差分测试 + 查询次数基准 |
| AC-GQL-3：Schema 自动生成类型一致 | REQ-GQL-003 | M3-T7~T8, M3-T12.3, M3-T14.2 | Rust 模型 → Schema 生成测试 |
| AC-GQL-4：复杂度限制 + 开销 ≤5% | REQ-GQL-004 | M3-T9~T10, M3-T12.4, M3-T14.3 | 超限拒绝 + 开销基准测试 |
| AC-GQL-5：变量参数化无注入 | REQ-GQL-005 | M3-T11, M3-T14.4 | 注入载荷测试 |

## 8.3 方向 3 验收标准映射（多租户与数据隔离）

| 验收标准 | 对应 REQ | 关联任务 | 验证方式 |
|---------|---------|---------|---------|
| AC-MT-1：上下文自动注入 + 兼容 | REQ-MT-001 | M1-T2, M1-T4, M1-T9.5, M1-T13.1 | 自动注入 + 既有 API 兼容测试 |
| AC-MT-2：Schema 隔离路由 + 物理隔离 | REQ-MT-002 | M1-T3, M1-T9.2, M1-T10.2 | 租户 A/B 路由测试 |
| AC-MT-3：连接池原子切换 + 路由 ≤50μs | REQ-MT-003 | M1-T5, M1-T9.3 | 原子切换 + 路由开销基准 |
| AC-MT-4：行级安全 + 列脱敏不可绕过 | REQ-MT-004 | M1-T6, M1-T7, M1-T9.4, M1-T10.3 | 部门级过滤 + 薪资脱敏测试 |
| AC-MT-5：审计日志完整 | REQ-MT-005 | M1-T8, M1-T11.3 | 审计记录完整性测试 |
| AC-MT-6：上下文必填 + 无泄漏 | REQ-MT-006 | M1-T4.2, M1-T11.1~T11.2 | 竞态 + 全覆盖测试 |

## 8.4 方向 4 验收标准映射（AI 自然语言查询增强）

| 验收标准 | 对应 REQ | 关联任务 | 验证方式 |
|---------|---------|---------|---------|
| AC-AI-1：NL2SQL 多表 JOIN + 仅 SELECT + 脱敏 + ≤10s | REQ-AI-001 | M4-T2, M4-T8.1, M4-T9.3, M4-T10.1 | 多表 JOIN + 安全验证 + 延迟基准 |
| AC-AI-2：意图分析 + 写操作高风险 + ≤5s | REQ-AI-002 | M4-T3, M4-T8.2, M4-T10.2 | 意图识别 + 风险标注 + 延迟基准 |
| AC-AI-3：索引建议 + 不执行 + ≤10s | REQ-AI-003 | M4-T4, M4-T8.3, M4-T10.3 | 索引建议 + DDL 不执行 + 延迟基准 |
| AC-AI-4：重写建议 + 不重写 | REQ-AI-004 | M4-T5, M4-T8.4, M4-T10.3 | 谓词下推/子查询展开 + 不重写 |
| AC-AI-5：建议不执行 + 审计 + 脱敏 | REQ-AI-005 | M4-T6, M4-T9.2 | 零执行 + 审计记录 + 脱敏 |
| AC-AI-6：AI 生成 SQL/DDL 零执行 | REQ-AI-006 | M4-T9.1 | mock DB 断言无 execute |

## 8.5 总体验收标准映射

| 验收标准 | 关联任务 | 验证方式 |
|---------|---------|---------|
| AC-ALL-1：无 Breaking Change | M5-T6.1, M5-T9.1 | API 签名对比测试 |
| AC-ALL-2：cargo test 全通过 | M5-T7.1 | `cargo test --workspace` 全通过 |
| AC-ALL-3：clippy 零警告 | M5-T1.2, M5-T7.1 | `cargo clippy -- -D warnings` 零警告 |
| AC-ALL-4：feature gate 隔离 | M5-T1.1, M5-T6.2 | 默认 feature 零新依赖 + 全组合编译 |
| AC-ALL-5：sz-pay/sz-rust 零回归 | M5-T3.1~T3.2 | 5139 测试 + sz-rust 测试回归 |
| AC-ALL-6：性能基准不回退 | M5-T4.1 | v3.2.0 基准对比 |
| AC-ALL-7：五方言行为一致 | M5-T2.1~T2.2 | 五方言集成测试 |
| AC-ALL-8：22 条 REQ 全满足 | M5-T5.1~T5.4 | 需求追溯矩阵逐条验收 |

---

# 9. 风险与缓解措施

| 编号 | 风险 | 等级 | 关联任务 | 缓解措施 |
|------|------|------|---------|---------|
| R-01 | 跨实例失效协议网络分区失效消息丢失 | 高 | M2-T12, M2-T16.3 | TTL 兜底 + 同步重试 + gossip 反熵；网络分区测试覆盖 |
| R-02 | Write-behind 宕机丢数据 | 高 | M2-T7, M2-T8, M2-T16.2 | WAL 持久化先于返回 + 重启回放 + 刷盘失败回退同步写 |
| R-03 | 布隆过滤器假阳性导致穿透 | 中 | M2-T9, M2-T16.4 | 假阳性率 ≤ 1% 可配置 + 互斥锁兜底 + 空值标记回填 |
| R-04 | DataLoader 批量加载顺序不一致 | 中 | M3-T5, M3-T13.1 | 差分测试覆盖 + 按键映射回填保持顺序 |
| R-05 | Schema 自动生成复杂类型支持不足 | 中 | M3-T7, M3-T8 | 不支持类型告警跳过 + 用户可手动标注 |
| R-06 | 复杂度限制误拒合法查询 | 中 | M3-T9, M3-T10 | 上限可独立配置 + 边界测试 |
| R-07 | 多租户上下文竞态跨租户泄漏 | 高 | M1-T2, M1-T11.1 | tokio task-local + RAII 守卫 + 竞态测试 |
| R-08 | Schema 隔离 Schema 数量膨胀 | 中 | M1-T3 | 大规模租户用行级隔离 + 小规模用 Schema 隔离 |
| R-09 | 列级脱敏规则配置遗漏 | 高 | M1-T7 | 默认拒绝读取未配置脱敏规则的敏感列 + 告警 |
| R-10 | LLM 生成 SQL 安全验证遗漏 | 高 | M4-T2, M4-T9.3 | 安全验证强制 + 注入载荷测试集 |
| R-11 | AI 建议被误自动执行 | 高 | M4-T9.1 | 安全铁律 + mock DB 断言无 execute |
| R-12 | LLM 服务不可用 | 中 | M4-T7 | 降级规则型建议 + 不阻塞业务主路径 |
| R-13 | feature 组合矩阵膨胀 | 低 | M5-T1.1 | 纳入门禁 10 全组合编译 + feature 正交性设计 |
| R-14 | 下游 sz-pay 升级回归 | 中 | M5-T3.1~T3.2 | feature gate 默认关闭 + 实际回归验证 |
| R-15 | 五方言行为差异 | 中 | M5-T2.1~T2.2 | Schema 隔离在 core 层统一抽象 + 五方言集成测试 |

---

> **文档结束**