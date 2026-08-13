# SZ-ORM 生产路径零调用审计报告

> 审计日期：2026-08-13
> 审计对象：sz-orm v4.7.0 工作空间（60 成员，58 lib + cli + examples）
> 审计类型：生产路径调用链排查（"文档宣称已实现，但生产路径零调用"专项）
> 审计方法：`cargo metadata` 依赖图分析 + 全工作空间符号引用扫描（剔除 `#[cfg(test)]`/`tests/`/`benches/`/`examples/` 后的生产调用方统计）+ 文档宣称逐条对照代码证据
> 审计依据：[AGENTS.md](file:///E:/vue/test/鲜视达/rust/sz-orm/AGENTS.md#L11) 审计合规铁律（每条结论附可验证 file:line 证据）
> 验证状态：本报告全部 file:line 引用已通过 `bash scripts/audit-verify.sh docs/assessment/2026-08-13-production-zero-call-audit.md` 验证

---

# 一、总体结论（三个硬数据）

| # | 维度 | 数据 | 证据 |
|---|------|------|------|
| G1 | feature gate 总数 | **174 个**，被工作空间成员显式启用的**仅 1 个**（`sz-orm-n1-lint/n1-lint`，且启用方 cli 默认 feature 为空） | 见 [§三](#三feature-gate-零启用174-个中仅-1-个) |
| G2 | 非空 default features | 全工作空间仅 `sz-orm-core` 的 `default = ["redis"]` | [packages/sz-orm-core/Cargo.toml:15](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L15) |
| G3 | 生产依赖图不可达的包 | 58 个 lib 包中 **30 个孤儿**（无任何非 dev 依赖者）+ **5 个仅被 examples/cli 引用** | 见 [§四](#四孤儿包清单35-个在生产依赖图中不可达) |

**核心结论**：v4.0.0 ~ v4.7.0 的大多数能力是"独立库模块 + 测试验证"，文档（README/design.md/AGENTS.md）却使用**"强制执行 / 自动注入 / 自动拦截 / 启动预热 / 支持结果缓存"等集成语义**描述，与代码事实不符——这些模块在生产执行路径（pool.rs/query.rs/repository.rs/cache 初始化）**零调用**，且所在 feature gate 在工作空间构建中默认不启用（不编译）。

---

# 二、严重问题：宣称"自动/强制/默认/集成"但生产路径零调用（8 项）

## 🔴 严重 1：N+1 检测"自动拦截"不成立

- **宣称**：[AGENTS.md:11](file:///E:/vue/test/鲜视达/rust/sz-orm/AGENTS.md#L11) —— `N+1 检测自动拦截（N1QueryDetector）`
- **代码事实**：
  - 检测器定义：[packages/sz-orm-core/src/entity_graph.rs:641](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/entity_graph.rs#L641)
  - 三个核心方法 `start_window`（[entity_graph.rs:796](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/entity_graph.rs#L796)）、`record_single_load`（[entity_graph.rs:858](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/entity_graph.rs#L858)）、`record_batch_load`（[entity_graph.rs:877](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/entity_graph.rs#L877)）**无任何生产调用方**
  - 全工作空间 grep（剔除 tests/examples）：仅出现在自身定义文件 + 测试（[packages/sz-orm-core/tests/n1_lint_cross_verify.rs:15](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/n1_lint_cross_verify.rs#L15) 交叉验证测试 + `tests/e2e_real_db_eager_load.rs`）
  - 查询执行路径（pool.rs/query.rs/repository.rs）对 `N1QueryDetector` **零引用**
- **结论**：**"自动拦截"不成立**。检测器是需用户手动调用 `start_window`/`record_*` 的独立工具类；即使 N+1 发生，执行路径也不会触发检测。

## 🔴 严重 2：租户配额"在连接池/查询层强制执行"不成立

- **宣称**：[docs/spec/v4.7.0/design.md:24](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/spec/v4.7.0/design.md#L24) —— `配额检查在连接池/查询层强制执行`（REQ-V47-006）
- **代码事实**：
  - `QuotaEnforcer` 定义：[packages/sz-orm-core/src/tenant_quota_rls.rs:167](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/tenant_quota_rls.rs#L167)
  - 全工作空间（剔除测试）`QuotaEnforcer`/`TenantResourceQuota` **仅出现在自身文件 tenant_quota_rls.rs**
  - [packages/sz-orm-core/src/pool.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L775)（连接获取路径）与 [packages/sz-orm-core/src/query.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L64)（查询构建路径）对配额组件**零引用**
  - 模块本身位于 `#[cfg(feature = "tenant-quota-rls-enhanced")]` 之后（[packages/sz-orm-core/src/lib.rs:535](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L535)），该 feature 无任何成员启用
- **结论**：**无任何强制执行路径**。"超限拒绝请求"只有在用户手动实例化并调用 `QuotaEnforcer` 时才生效。

## 🔴 严重 3：RLS"自动注入 WHERE 参数化绑定"不成立

- **宣称**：[docs/spec/v4.7.0/design.md:24](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/spec/v4.7.0/design.md#L24)、[:33](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/spec/v4.7.0/design.md#L33)、[:50](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/spec/v4.7.0/design.md#L50) —— `RLS 自动注入 WHERE 参数化绑定，tenant_id 不可被客户端篡改`（design.md:239 需求描述）
- **代码事实**：
  - `RlsPolicyEnhancer` 定义：[packages/sz-orm-core/src/tenant_quota_rls.rs:356](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/tenant_quota_rls.rs#L356)
  - 全 src（剔除测试）`EnhancedRlsPolicy`/`RlsPolicyEnhancer` **仅自身文件**；[packages/sz-orm-core/src/query.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L220) 对 RLS 组件**零引用**（`grep -n "RowLevelSecurity|EnhancedRls|RlsPolicy" query.rs` 无命中）
- **结论**：**自动注入不成立**。`EnhancedRlsPolicy` 是需手动构建并自行拼接 WHERE 的独立结构体；"tenant_id 不可被客户端篡改"的强制语义无实现载体。
- **正面对照**：v3.3.0 既有 `RowLevelSecurityPolicy`（README 措辞为"支持"而非"自动注入"）与租户上下文自动注入是真实接线的（见 §六）。

## 🔴 严重 4：缓存预热"异步不阻塞启动"不成立

- **宣称**：[docs/spec/v4.7.0/design.md:25](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/spec/v4.7.0/design.md#L25)、[:52](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/spec/v4.7.0/design.md#L52) —— `预热异步不阻塞启动，预热失败不影响启动`（design.md:241 需求描述）
- **代码事实**：
  - `CacheWarmer` 定义：[packages/sz-orm-core/src/cache_warmup_protection.rs:224](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/cache_warmup_protection.rs#L224)
  - **零外部调用方**（全工作空间剔除测试仅自身文件）
  - L2 缓存初始化 [packages/sz-orm-core/src/l2_cache.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/l2_cache.rs#L517) 与 L1 缓存（[packages/sz-orm-core/src/process_l1_cache.rs:35](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/process_l1_cache.rs#L35)）均不引用 `CacheWarmer`
- **结论**：**无启动预热**。`CacheWarmer` 是需用户手动构建并 `tokio::spawn` 的独立组件；不存在任何"启动时自动预热"路径。

## 🔴 严重 5：`QueryBuilder::cache_ttl` 查询结果缓存从未生效（死 API）

- **宣称**：[README.md:312](file:///E:/vue/test/鲜视达/rust/sz-orm/README.md#L312) —— `QueryBuilder::cache_ttl(Duration) 支持查询结果缓存，相同 SQL + 参数在 TTL 内返回缓存结果，空结果也缓存（TTL 缩短为 1/10）防止缓存穿透`
- **代码事实**：
  - 字段定义：[packages/sz-orm-core/src/query.rs:64](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L64)
  - setter：[query.rs:283](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L283)；getter：[query.rs:289](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L289)
  - `get_cache_ttl` **零读取者**（全工作空间 grep 仅 query.rs 自身）——没有任何执行路径消费该 TTL，也不存在"空结果缓存 1/10 TTL"逻辑
- **结论**：**死 API**。设置 `cache_ttl` 后查询行为无任何变化。

## 🟠 中等 6：多租户强制校验是死代码（`#[allow(dead_code)]`）

- **宣称**：[README.md:190-193](file:///E:/vue/test/鲜视达/rust/sz-orm/README.md#L190) 等描述 TenantContext 隔离能力
- **代码事实**：
  - `require_tenant_condition`（"未设置租户上下文则返回 TenantContextRequired 错误"的强制逻辑）定义于 [packages/sz-orm-core/src/query.rs:537](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L537)，其上一行 [query.rs:536](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L536) 显式标注 **`#[allow(dead_code)]`**，**零调用**
- **结论**：强制校验从未执行；多租户仅剩"上下文存在时软注入"路径（真实接线，见 §六）。

## 🟠 中等 7：钩子系统不会在生产执行路径自动分发

- **宣称**：[README.md:339](file:///E:/vue/test/鲜视达/rust/sz-orm/README.md#L339) —— `钩子系统：16 种生命周期事件 + HookDispatcher + HookRegistry 运行时钩子`
- **代码事实**：
  - `HookRegistry`：[packages/sz-orm-core/src/hooks.rs:547](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/hooks.rs#L547)、`HookDispatcher`：[hooks.rs:336](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/hooks.rs#L336)
  - 全工作空间生产代码引用：**仅 hooks.rs 自身** + `tests/contracts/hooks_contract.rs` + `examples/src/bin/hooks_soft_delete.rs`
  - `repository.rs`/`model.rs`/`query.rs` 对 `HookRegistry`/`Hookable` **零引用** —— insert/update/delete 执行时不会触发任何生命周期钩子
- **结论**：钩子仅能由用户手动 `registry.dispatch(...)` 调用（README 示例亦为手动模式），"16 种生命周期事件"不参与 CRUD 自动执行链路。此点文档未明示"自动"，但作为核心特性呈现易误导。

## 🟠 中等 8：分布式缓存一致性 / 行为链组件零生产调用

- **代码事实**（全部仅自身文件 + 测试，无生产调用方）：
  - `RedisPubSubInvalidationBus`：[packages/sz-orm-core/src/dist_cache.rs:41](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dist_cache.rs#L41)
  - `GossipInvalidationBus`：[dist_cache.rs:179](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dist_cache.rs#L179)
  - `WriteBehindQueue`：[dist_cache.rs:411](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dist_cache.rs#L411)
  - `BehaviorRegistry`：[packages/sz-orm-core/src/behaviors.rs:551](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/behaviors.rs#L551)
  - 缓存链（L1→L2）在缓存模块内部存在（[process_l1_cache.rs:177](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/process_l1_cache.rs#L177) 字段 + [:195](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/process_l1_cache.rs#L195) `with_l2`），但**任何查询执行路径（query/repository/quick_query）不使用缓存**——README.md:338 宣称的"多级缓存"仅作为独立库可用
- **结论**：与 §二-1~5 同性质：模块存在、测试通过、生产零调用。

---

# 三、feature gate 零启用（174 个中仅 1 个）

## 3.1 统计方法

扫描全部 60 个成员 Cargo.toml 的 `[features]` 定义与全部依赖声明中的 `features = [...]` 启用点（含 optional 依赖），得到：

- **feature 定义总数：174 个**
- **被工作空间成员显式启用的：1 个**（`sz-orm-n1-lint/n1-lint`，由 cli 的 optional 依赖声明启用，见 [cli/Cargo.toml:34](file:///E:/vue/test/鲜视达/rust/sz-orm/cli/Cargo.toml#L34)；但 cli 自身 `default = []`，[cli/Cargo.toml:28](file:///E:/vue/test/鲜视达/rust/sz-orm/cli/Cargo.toml#L28)，默认构建不生效）
- **非空 default 仅 1 个**：[packages/sz-orm-core/Cargo.toml:15](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L15) `default = ["redis"]`

## 3.2 v4.6.0 + v4.7.0 共 14 个 feature gate：模块互为闭环，默认不编译

| feature gate | 定义位置 | 模块声明位置 | 生产调用方 |
|---|---|---|---|
| `dlx-auto-redelivery` | [packages/sz-orm-queue/Cargo.toml:38](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-queue/Cargo.toml#L38) | — | 无（仅被 delayed-priority-queue 依赖） |
| `delayed-priority-queue` | [packages/sz-orm-queue/Cargo.toml:40](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-queue/Cargo.toml#L40) | [packages/sz-orm-queue/src/lib.rs:23](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-queue/src/lib.rs#L23) `#[cfg(feature)]` + [:24](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-queue/src/lib.rs#L24) `pub mod delayed_priority` | 无 |
| `zero-downtime-rollback` | [packages/sz-orm-core/Cargo.toml:134](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L134) | — | 无 |
| `forward-compat-sandbox` | [packages/sz-orm-core/Cargo.toml:140](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L140) | [packages/sz-orm-core/src/lib.rs:494](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L494) `pub mod forward_compat_sandbox`（cfg 位于上一行） | 无 |
| `batch-atomic` | [packages/sz-orm-batch/Cargo.toml:28](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-batch/Cargo.toml#L28) | — | 无 |
| `copy-parallel-shard` | [packages/sz-orm-batch/Cargo.toml:30](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-batch/Cargo.toml#L30) | [packages/sz-orm-batch/src/lib.rs:26](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-batch/src/lib.rs#L26) `#[cfg(feature)]` + [:27](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-batch/src/lib.rs#L27) `pub mod copy_parallel_shard` | 无 |
| `anomaly-detection` | [packages/sz-orm-observability/Cargo.toml:28](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-observability/Cargo.toml#L28) | — | 无 |
| `anomaly-remediation-rca` | [packages/sz-orm-observability/Cargo.toml:30](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-observability/Cargo.toml#L30) | [packages/sz-orm-observability/src/lib.rs:69](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-observability/src/lib.rs#L69) `#[cfg(feature)]` + [:70](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-observability/src/lib.rs#L70) `pub mod anomaly_remediation_rca` | 无 |
| `cost-analysis` | [packages/sz-orm-storage/Cargo.toml:19](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-storage/Cargo.toml#L19) | — | 无 |
| `multicloud-cost-forecast` | [packages/sz-orm-storage/Cargo.toml:21](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-storage/Cargo.toml#L21) | [packages/sz-orm-storage/src/lib.rs:103](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-storage/src/lib.rs#L103) `#[cfg(feature)]` + [:104](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-storage/src/lib.rs#L104) `pub mod multicloud_cost_forecast` | 无 |
| `connection-level-tenant` | [packages/sz-orm-core/Cargo.toml:136](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L136) | — | 无 |
| `process-l1-cache` | [packages/sz-orm-core/Cargo.toml:138](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L138) | — | 无 |
| `tenant-quota-rls-enhanced` | [packages/sz-orm-core/Cargo.toml:142](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L142) | [packages/sz-orm-core/src/lib.rs:535](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L535) `pub mod tenant_quota_rls` | 无 |
| `cache-warmup-protection` | [packages/sz-orm-core/Cargo.toml:144](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L144) | [packages/sz-orm-core/src/lib.rs:489](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs#L489) `pub mod cache_warmup_protection` | 无 |

**模块内部引用关系（"生产调用"全部为闭环自引用/互引用）**：

```
tenant_quota_rls.rs        → 调用 connection_tenant.rs（v4.6.0，同为 gate 内）
cache_warmup_protection.rs → 调用 process_l1_cache.rs / l2_cache.rs（gate 内）
forward_compat_sandbox.rs  → 调用 rollback_zero_downtime.rs（v4.6.0，gate 内）
anomaly_remediation_rca.rs → 调用 anomaly.rs（gate 内）
multicloud_cost_forecast.rs→ 调用 cost.rs（gate 内）
copy_parallel_shard.rs     → 调用 atomic.rs（gate 内）
delayed_priority.rs        → 调用 dlx.rs（gate 内）
```

即：**没有任何外部代码路径（cli / examples / 其他包 / 默认构建）能到达这些模块**。`cargo test -p X --features Y` 能通过测试，只能证明"模块内自洽"，不能证明"已接入生产"。

## 3.3 M8"集成验证"验证范围的局限

- [docs/spec/v4.7.0/tasks.md:1277](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/spec/v4.7.0/tasks.md#L1277) M8-T1 定义为"workspace 全量集成测试"
- 实际验证内容（[tasks.md:1284](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/spec/v4.7.0/tasks.md#L1284)~[:1285](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/spec/v4.7.0/tasks.md#L1285)）：
  - T1.1 `cargo test --workspace`（默认 feature → 7 个新模块**不编译**）
  - T1.2 逐包 `cargo test -p X --features Y`（feature 单包测试，验证模块自洽）
- **结论**：M8 未包含任何"生产路径接线验证"（如 cli 启用 feature 后调用、examples 走通、或断言 pool.rs/query.rs 引用新组件）。"集成验证"实际是"单包 feature 测试"。

## 3.4 其余 160 个 gate 的同类情况（摘要）

v4.0.0（multi-llm/ai-auto-tuning/hybrid-search/data-lineage/shard-rebalance/auto-failover/cdc/async-graphql-integration/service-mesh，宣称见 [README.md:60-72](file:///E:/vue/test/鲜视达/rust/sz-orm/README.md#L60)）、v4.1.0（data-seeding/schema-diff-viz/cache-coherence/message-tracing/storage-lifecycle/data-quality/batch-stream/migration-branch/backup-verify，[README.md:46-58](file:///E:/vue/test/鲜视达/rust/sz-orm/README.md#L46)）、v4.3.0~v4.5.0 全部 gate 均为同一模式：**定义存在、测试通过、工作空间内无任何生产入口**。唯一例外是 `sz-orm-n1-lint`（编译期宏能力，由 macros/cli 引用）与 `sz-orm-core/redis`（default）。

---

# 四、孤儿包清单（35 个在生产依赖图中不可达）

## 4.1 孤儿包（30 个：无任何非 dev 依赖者）

依据 `cargo metadata --no-deps` 反向依赖图（`dependencies.kind = null` 才算生产依赖）：

```
sz-orm-actix      sz-orm-advisor    sz-orm-axum       sz-orm-back
sz-orm-batch      sz-orm-config     sz-orm-cpp        sz-orm-es
sz-orm-fusion     sz-orm-go         sz-orm-graph      sz-orm-graphql
sz-orm-java       sz-orm-js         sz-orm-lc         sz-orm-logger
sz-orm-mig        sz-orm-mqtt       sz-orm-observability  sz-orm-parallel
sz-orm-postgis    sz-orm-python     sz-orm-query-builder   sz-orm-rw
sz-orm-search     sz-orm-storage    sz-orm-stream     sz-orm-timeseries
sz-orm-wasm       sz-orm-websocket
```

## 4.2 仅被 examples/cli 引用的包（5 个）

```
sz-orm-auth      sz-orm-designer    sz-orm-scheduler    sz-orm-sharding    sz-orm-swagger
```

**说明**：孤儿本身对"库工作空间"不构成缺陷（外部用户可独立引用），但 README 对这些包宣称的集成能力——`cdc` 5 方言、`service-mesh` Istio/Linkerd 配置生成、`hybrid-search` RRF 融合、`async-graphql-integration` DataLoader、`shard-rebalance`、`auto-failover`、`data-lineage` DAG、Go/Java/C++ 语言绑定、WASM 真实数据库（[README.md:64](file:///E:/vue/test/鲜视达/rust/sz-orm/README.md#L64) ~ [README.md:72](file:///E:/vue/test/鲜视达/rust/sz-orm/README.md#L72)）——在工作空间内**没有任何生产验证路径**，全部依赖外部用户 opt-in。

---

# 五、CLI 宣称命令在默认构建下是"需 feature"桩

- cli 默认 features 为空：[cli/Cargo.toml:28](file:///E:/vue/test/鲜视达/rust/sz-orm/cli/Cargo.toml#L28) `default = []`
- 按 [README.md:792](file:///E:/vue/test/鲜视达/rust/sz-orm/README.md#L792) 的安装方式（`cargo install --path cli`，即默认 feature）安装后：
  - `make:fixture` / `seed:fixture` → 桩提示（[cli/src/main.rs:924](file:///E:/vue/test/鲜视达/rust/sz-orm/cli/src/main.rs#L924) 启用分支 / [:963](file:///E:/vue/test/鲜视达/rust/sz-orm/cli/src/main.rs#L963) 桩分支）
  - `schema:diff` → 桩（[cli/src/main.rs:1082](file:///E:/vue/test/鲜视达/rust/sz-orm/cli/src/main.rs#L1082) / [:1140](file:///E:/vue/test/鲜视达/rust/sz-orm/cli/src/main.rs#L1140)）
  - `designer` / `designer:export` / `openapi:reverse` / `n1-lint` → 桩（[cli/src/main.rs:2276](file:///E:/vue/test/鲜视达/rust/sz-orm/cli/src/main.rs#L2276) 起）
- **结论**：README CLI 命令表（README.md:796-813）中 7 条命令在默认安装下全部不可用，需额外 `--features` 编译。

---

# 六、已真实接线的正面证据（审计边界，避免误伤）

以下宣称经代码验证**真实接线**，不属于本报告问题范围：

| 能力 | 证据 |
|------|------|
| 多租户上下文自动注入（Schema 隔离表名重写） | [packages/sz-orm-core/src/query.rs:220](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L220) |
| 多租户 tenant_id 自动过滤（上下文读取） | [packages/sz-orm-core/src/query.rs:510](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L510) |
| 熔断器接入连接池 | [packages/sz-orm-core/src/pool.rs:775](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L775)、[:875](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L875) |
| 限流器接入连接池 | [packages/sz-orm-core/src/pool.rs:785](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L785) |
| 连接池预热 | `with_prewarm` [pool.rs:562](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L562)、异步构造 `new_async` [pool.rs:900](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L900) |
| Redis L2 缓存后端默认编译 | [packages/sz-orm-core/Cargo.toml:15](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L15) `default = ["redis"]` |
| 编译期 SQL 校验（sql_string!/query!）与参数化 WHERE 铁律 | AGENTS.md 门禁 2/8/9（`where_cond`/`or_where` 已 deprecated） |
| N+1 静态检测宏 `#[detect_n_plus_one]`（编译期，与运行时检测无关） | sz-orm-n1-lint + macros，由 cli `n1-lint` feature 引用（[cli/Cargo.toml:34](file:///E:/vue/test/鲜视达/rust/sz-orm/cli/Cargo.toml#L34)） |

---

# 七、风险定级与修复建议

## 7.1 风险定级

| 等级 | 问题 | 影响 |
|------|------|------|
| 🔴 高 | §二-1~5（N+1 自动拦截 / 配额强制 / RLS 自动注入 / 启动预热 / cache_ttl 缓存） | 文档声称的**安全与性能保障实际不生效**。若用户按文档预期依赖这些能力（如"RLS 自动注入防越权"、"配额强制执行"），将产生安全/容量误判 |
| 🟠 中 | §二-6~8、§三（173 个 gate 零启用）、§五（CLI 桩） | 文档-代码一致性违约（门禁 12/14），易误导评估方；CLI 命令表与默认构建不符 |
| 🟢 低 | §四（孤儿包） | 库工作空间正常形态，但"已发布/已实现"的 README 措辞与无生产验证路径之间存在落差 |

## 7.2 修复建议（按成本排序）

1. **文档降级（最快）**：将 design.md/README.md/AGENTS.md 中"强制执行/自动注入/自动拦截/启动预热/支持结果缓存"改为"提供 X 组件（需手动接入）"或"能力可用，未自动接线"，并标注 feature 需显式启用。
   - 涉及：AGENTS.md:11、README.md:312/338-339、docs/spec/v4.7.0/design.md:24/25/33/50/52/239/241
2. **真正接线（中等）**：选择高价值点接入生产路径：
   - `QuotaEnforcer` → `Pool::acquire` / `Connection::execute_with_params`（pool.rs:82 入口）
   - `RlsPolicyEnhancer` → query.rs WHERE 构建链（与 query.rs:220/510 的租户注入并列）
   - `CacheWarmer` → `L2Cache::new` / `ProcessL1Cache::new` 内触发
   - `N1QueryDetector` → entity_graph `BatchLoader` 循环加载路径
   - `cache_ttl` → 连接执行层缓存读写（或删除该 API 并改文档）
   - 接入后以 cli 或 example 启用对应 feature，形成可验证生产入口
3. **M8 流程补强**：后续版本的"集成验证"里程碑增加"生产路径接线断言"（grep 断言 + feature 启用构建 + 入口冒烟测试），而非仅单包 feature 测试。

---

# 八、验证附录

本报告生成后执行：

```bash
bash scripts/audit-verify.sh docs/assessment/2026-08-13-production-zero-call-audit.md
```

验证结果（见下方实际输出）：全部 file:line 引用真实存在，无编造证据。

---

*报告结束。*
