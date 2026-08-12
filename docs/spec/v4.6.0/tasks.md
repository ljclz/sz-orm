# sz-orm v4.6.0 编码任务规划

> 版本：v4.6.0（消息死信队列自动重投递 + 迁移回滚自动化 + 批量事务原子性保证 + 异常检测 + 存储成本分析 + 连接级多租户隔离 + 进程级 L1 缓存）
> 基线：v4.5.0（并行查询执行器 + 批量 INSERT/UPDATE/DELETE 优化 + 异步流式结果集，3 项需求 REQ-V45-001~003 全部通过 feature gate 隔离，27 任务 / 148 子任务全部已完成并提交，6,900+ 个测试通过，14 道门禁全通过）
> 日期：2026-08-12
> 文档定位：编码任务规划（How to execute），对应需求规格 `spec.md`（What to build，1139 行）+ 技术设计 `design.md`（How to build，2242 行）
> 任务约束：无 Breaking Change，7 个新 feature gate 隔离，默认全关闭；优先复用既有能力 + 五方言覆盖 + 每项任务附 file:line 代码证据 + unsafe 零容忍 + 禁止占位实现（todo!/unimplemented!/unreachable!）+ 参数化查询强制
> 审计合规铁律：每项任务结论须附真实存在的 file:line 证据，修复后必须运行 `cargo test` 并附输出，禁止未验证即标记 ✅
> 实施顺序：按 design.md §7.2 依赖关系，M0（P0 文档基线，立即）→ M1~M7（7 项需求并行开发，主体独立）→ M8（最终集成验证与文档同步，全部完成后）
> 与 v4.5.0 零重叠：v4.5.0 是"执行优化"层（并行执行/批量执行/流式执行），v4.6.0 是"可靠性 + 运维智能化"层（消息可靠/迁移安全/批量原子/异常自检/成本自优/租户隔离/缓存升级），新增范围全部落在既有包扩展（sz-orm-queue / sz-orm-core / sz-orm-batch / sz-orm-observability / sz-orm-storage），不新增包

---

# 一、任务总览

## 1.1 里程碑 × 任务数 × 预期工作量

| 里程碑 | 名称 | 对应需求 | 优先级 | 任务数 | 子任务数 | 预期工作量 | 启动时机 |
|--------|------|---------|--------|--------|----------|-----------|---------|
| M0 | 文档基线与准备 | — | P0 | 3 | 12 | 0.5 天 | 立即（v4.5.0 已完成） |
| M1 | 消息死信队列自动重投递 | REQ-V46-001 | P1 | 6 | 36 | 2 天 | 立即（独立扩展 sz-orm-queue） |
| M2 | 迁移回滚自动化 | REQ-V46-002 | P1 | 6 | 38 | 2.5 天 | 立即（独立扩展 sz-orm-core migration） |
| M3 | 批量事务原子性保证 | REQ-V46-003 | P1 | 6 | 34 | 2 天 | 立即（独立扩展 sz-orm-batch + sz-orm-dtx） |
| M4 | 连接级多租户隔离 | REQ-V46-006 | P1 | 6 | 36 | 2 天 | 立即（独立扩展 sz-orm-core pool/tenant） |
| M5 | 进程级 L1 缓存 | REQ-V46-007 | P1 | 6 | 34 | 2 天 | 立即（独立扩展 sz-orm-core cache） |
| M6 | 异常检测 | REQ-V46-004 | P2 | 5 | 28 | 1.5 天 | 立即（独立扩展 sz-orm-observability） |
| M7 | 存储成本分析与优化建议 | REQ-V46-005 | P2 | 5 | 30 | 2 天 | 立即（独立扩展 sz-orm-storage） |
| M8 | 集成验证与文档同步 | 全局 | P0 | 3 | 16 | 0.5 天 | M1~M7 全部完成后 |
| **合计** | — | **7 项全覆盖** | — | **46** | **264** | **15 天** | — |

## 1.2 任务编号约定

- 主任务：`M{里程碑号}-T{任务序号}`（如 M1-T1）
- 子任务：`M{里程碑号}-T{任务序号}.{子任务序号}`（如 M1-T2.1）
- 集成验证任务：每个里程碑末尾固定一个集成测试与门禁验证任务（如 M1-T6）
- 里程碑内需求按 REQ-V46-xxx 序号顺序编排任务

## 1.3 全局约束（适用于所有任务）

1. **feature gate 隔离**：7 个新 feature（`dlx-auto-redelivery` / `zero-downtime-rollback` / `batch-atomic` / `anomaly-detection` / `cost-analysis` / `connection-level-tenant` / `process-l1-cache`），默认全关闭，默认 feature 行为不变
2. **既有 API 不变**：既有公开 API 签名完全向后兼容，sz-pay 既有代码不受影响（sz-pay 从 crates.io 拉取 sz-orm-* 6 个包）
3. **禁止占位实现**：禁止 `todo!`/`unimplemented!`/`unreachable!`
4. **unsafe 零容忍**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释
5. **五方言覆盖**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL（迁移回滚/批量原子性/连接级多租户按方言能力适配，如 `SET app.tenant_id` 仅 PG/MySQL 支持）
6. **参数化查询强制**：任何 WHERE 条件必须参数化绑定，禁止 SQL 字符串拼接（复用既有 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82`）
7. **审计证据**：每项任务结论附真实存在的 file:line 证据
8. **测试基线不回退**：v4.5.0 已验收测试基线（6,900+ 个测试）不回退，v4.6.0 仅增不减
9. **复用优先**：优先复用既有能力，不重复实现（整体复用率 57.3%，见 design.md §3.8）
10. **不新增包**：所有新能力通过既有包扩展实现（sz-orm-queue / sz-orm-core / sz-orm-batch / sz-orm-observability / sz-orm-storage），workspace 成员保持 60 个
11. **Windows MSVC 编译环境**：RUST_MIN_STACK=134217728, CARGO_INCREMENTAL=0
12. **测试命令**：`cargo test --workspace -j 2 --no-fail-fast`；feature 包测试：`cargo test -p <package> --features <feature>`

## 1.4 里程碑依赖关系

```
M0（P0，文档基线，立即）
M1（P1，DLX 自动重投递，独立扩展 sz-orm-queue）
  - REQ-V46-001 复用既有 MessageQueue/InMemoryQueue/dead_letters/nack/reject/requeue_dead_letter
M2（P1，零停机回滚，独立扩展 sz-orm-core migration）
  - REQ-V46-002 复用既有 Migration/rollback/down/MigrationResolver/FileMigrationResolver
M3（P1，批量事务原子性，独立扩展 sz-orm-batch + sz-orm-dtx）
  - REQ-V46-003 复用既有 BatchExecutor + Saga/SagaStep/DistributedTransaction
M4（P1，连接级多租户，独立扩展 sz-orm-core pool/tenant_context）
  - REQ-V46-006 复用既有 Pool/Connection/TenantContext/IsolationStrategy/RowLevelSecurityPolicy
M5（P1，进程级 L1 缓存，独立扩展 sz-orm-core l1_cache/l2_cache/cache_coherence）
  - REQ-V46-007 复用既有 L1Cache/L2Cache/CacheKey/CacheCoherenceProtocol
M6（P2，异常检测，独立扩展 sz-orm-observability）
  - REQ-V46-004 复用既有 MetricsRegistry/SloMonitor/QueryLogger
M7（P2，存储成本分析，独立扩展 sz-orm-storage）
  - REQ-V46-005 复用既有 Storage/StorageProvider/BucketLifecycle/LifecycleRule
M8（P0，集成验证与文档同步，M1~M7 全部完成后）
  - 依赖 M0~M7 全部完成
```

> **依赖关系说明**：M0 立即启动；M1~M7 七项需求主体相互独立，可并行开发（design.md §7.2 明确声明）；M8 必须在 M1~M7 全部完成后执行。七项需求无强依赖，可并行推进。

## 1.5 feature gate 定义与测试命令

| feature gate | 所属包 | 依赖 | 测试命令 | 默认 |
|-------------|--------|------|---------|------|
| `dlx-auto-redelivery` | sz-orm-queue（扩展） | tokio（optional） | `cargo test -p sz-orm-queue --features dlx-auto-redelivery` | 关闭 |
| `zero-downtime-rollback` | sz-orm-core（扩展） | tokio（optional） | `cargo test -p sz-orm-core --features zero-downtime-rollback` | 关闭 |
| `batch-atomic` | sz-orm-batch（扩展）+ sz-orm-dtx（只读复用） | sz-orm-dtx（optional） | `cargo test -p sz-orm-batch --features batch-atomic` | 关闭 |
| `anomaly-detection` | sz-orm-observability（扩展） | 无额外依赖 | `cargo test -p sz-orm-observability --features anomaly-detection` | 关闭 |
| `cost-analysis` | sz-orm-storage（扩展） | 无额外依赖 | `cargo test -p sz-orm-storage --features cost-analysis` | 关闭 |
| `connection-level-tenant` | sz-orm-core（扩展） | multi-tenant-enhanced（既有） | `cargo test -p sz-orm-core --features connection-level-tenant` | 关闭 |
| `process-l1-cache` | sz-orm-core（扩展） | l1-cache + cache-coherence（既有） | `cargo test -p sz-orm-core --features process-l1-cache` | 关闭 |

---

# 二、M0：文档基线与准备（P0，0.5 天）

**目标**：锁定 v4.5.0 已验收基线，准备 v4.6.0 开发环境（7 个 feature gate 骨架 + 版本号升级），不新增包。
**对应需求**：—（文档基线与环境准备，非功能需求）
**预期工作量**：0.5 天
**依赖**：无（v4.5.0 已全部完成并提交）

## M0-T1：v4.5.0 完成总结与基线锁定

**任务描述**：总结 v4.5.0 交付成果（27 任务 / 148 子任务全部完成），锁定测试基线（6,900+ 个测试通过 + 14 道门禁全通过），作为 v4.6.0 开发的基准。

**涉及文件**：`docs/spec/v4.5.0/tasks.md`（既有，确认全部 `[x]`）、`docs/spec/v4.5.0/spec.md`（既有，688 行）、`docs/spec/v4.5.0/design.md`（既有，1676 行）

**子任务**：
- [ ] M0-T1.1 确认 `docs/spec/v4.5.0/tasks.md` 27 任务 / 148 子任务全部标记 `[x]`（v4.5.0 已完成）
- [ ] M0-T1.2 运行 `cargo test --workspace -j 2 --no-fail-fast` 记录 v4.5.0 测试基线（约 6,900+ 个测试通过）
- [ ] M0-T1.3 运行 14 道门禁全量验证，记录基线通过状态（fmt/check/clippy/test/doc/audit/integration/占位/SQL注入/feature组合/ADR-0001/文档一致性/审计证据/文档同步）
- [ ] M0-T1.4 确认 v4.5.0 3 个 feature gate（`parallel-query`/`batch-v2`/`stream-resultset`）默认全关闭，行为不变

**验收标准**：v4.5.0 基线锁定（测试数 + 门禁通过状态 + feature gate 状态），每项附 file:line 或命令输出证据

**依赖**：无

## M0-T2：v4.6.0 开发环境准备

**任务描述**：在 5 个既有包新增 7 个 feature gate 占位（默认关闭），升级版本号 4.5.0 → 4.6.0，验证 workspace 编译通过，不新增包。

**涉及文件**：
- `Cargo.toml`（workspace.package.version 4.5.0 → 4.6.0）
- `packages/sz-orm-queue/Cargo.toml`（扩展：新增 `dlx-auto-redelivery` feature 占位）
- `packages/sz-orm-core/Cargo.toml`（扩展：新增 `zero-downtime-rollback` / `connection-level-tenant` / `process-l1-cache` feature 占位）
- `packages/sz-orm-batch/Cargo.toml`（扩展：新增 `batch-atomic` feature 占位）
- `packages/sz-orm-observability/Cargo.toml`（扩展：新增 `anomaly-detection` feature 占位）
- `packages/sz-orm-storage/Cargo.toml`（扩展：新增 `cost-analysis` feature 占位）

**复用标注**：既有 workspace 结构 `Cargo.toml`（60 个成员）；既有 feature gate 模式 `packages/sz-orm-core/Cargo.toml`（40+ feature）；既有 `packages/sz-orm-queue/Cargo.toml`（`cdc`/`message-tracing` feature）

**子任务**：
- [ ] M0-T2.1 `packages/sz-orm-queue/Cargo.toml` 新增 `dlx-auto-redelivery = ["dep:tokio"]` feature（默认关闭，tokio optional）
- [ ] M0-T2.2 `packages/sz-orm-core/Cargo.toml` 新增 `zero-downtime-rollback = ["dep:tokio"]` feature（默认关闭）
- [ ] M0-T2.3 `packages/sz-orm-core/Cargo.toml` 新增 `connection-level-tenant = ["multi-tenant-enhanced"]` feature（默认关闭，依赖既有 `multi-tenant-enhanced`）
- [ ] M0-T2.4 `packages/sz-orm-core/Cargo.toml` 新增 `process-l1-cache = ["l1-cache", "cache-coherence"]` feature（默认关闭，依赖既有 `l1-cache` + `cache-coherence`）
- [ ] M0-T2.5 `packages/sz-orm-batch/Cargo.toml` 新增 `batch-atomic = ["dep:sz-orm-dtx"]` feature（默认关闭，sz-orm-dtx optional）
- [ ] M0-T2.6 `packages/sz-orm-observability/Cargo.toml` 新增 `anomaly-detection = []` feature（默认关闭）
- [ ] M0-T2.7 `packages/sz-orm-storage/Cargo.toml` 新增 `cost-analysis = []` feature（默认关闭）
- [ ] M0-T2.8 `Cargo.toml` workspace.package.version 从 `4.5.0` 升级为 `4.6.0`
- [ ] M0-T2.9 验证 `cargo check --workspace` 编译通过（7 个 feature gate 占位不影响既有编译，workspace 成员仍 60）
- [ ] M0-T2.10 验证默认 feature 行为与 v4.5.0 一致（`cargo build --workspace` 行为不变）

**验收标准**：7 个 feature gate 占位创建成功，workspace 成员仍 60，版本号 4.6.0，workspace 编译通过，默认 feature 行为不变

**依赖**：M0-T1

## M0-T3：基线验证

**任务描述**：运行文档一致性、审计证据、文档同步三道门禁，验证 v4.5.0 基线可被工具消费，v4.6.0 骨架不破坏既有基线。

**涉及文件**：`scripts/check-doc-consistency.py`、`scripts/audit-verify.sh`、`scripts/check-doc-sync.py`

**子任务**：
- [ ] M0-T3.1 运行 `python scripts/check-doc-consistency.py`（门禁 12），验证文档与代码一致
- [ ] M0-T3.2 运行 `bash scripts/audit-verify.sh docs/spec/v4.5.0/tasks.md`（门禁 13），验证 v4.5.0 tasks.md 所有 file:line 引用真实存在
- [ ] M0-T3.3 运行 `python scripts/check-doc-sync.py --diff HEAD`（门禁 14），验证文档与 HEAD 同步

**验收标准**：三道门禁全部通过；v4.5.0 tasks.md 所有 file:line 引用经 audit-verify 验证真实存在

**依赖**：M0-T2

---

# 三、M1：消息死信队列自动重投递（REQ-V46-001，P1，2 天）

**目标**：扩展既有 `sz-orm-queue` 包，新增 `RedeliveryScheduler` 自动重投递调度器，死信消息按 `BackoffPolicy` 退避策略自动调度重投递（无需手动调用 `requeue_dead_letter`），支持 `DlxRoutingStrategy` 四种路由策略，重投递次数上限保护，复用既有 `InMemoryQueue.dead_letters` / `Message.retry_count` / `max_retries` / `nack` / `reject` / `requeue_dead_letter`。
**对应需求**：REQ-V46-001（spec.md §5.1，design.md §2.1.3.1 + §2.2.2.1）
**预期工作量**：2 天
**依赖**：无（M1 为 P1 独立需求，复用既有 sz-orm-queue MessageQueue/InMemoryQueue，扩展包可与 M2~M7 并行）

## M1-T1：dlx-auto-redelivery feature gate + DlxConfig 配置

**任务描述**：在 `sz-orm-queue` 完善 `dlx-auto-redelivery` feature gate 隔离（M0-T2 已创建占位），定义 `DlxConfig` DLX 配置结构 + `BackoffPolicy` 退避策略枚举 + `DlxRoutingStrategy` 路由策略枚举，作为 DLX 自动重投递的数据模型。

**涉及文件**：
- `packages/sz-orm-queue/Cargo.toml`（完善：`dlx-auto-redelivery` feature 依赖 tokio optional）
- `packages/sz-orm-queue/src/lib.rs`（扩展：模块声明 `mod dlx;`，`#[cfg(feature = "dlx-auto-redelivery")]` 门控）
- `packages/sz-orm-queue/src/dlx.rs`（新建，DLX 配置 + 退避策略 + 路由策略）

**复用标注**：既有 `MessageQueue` trait `packages/sz-orm-queue/src/queue.rs:18`；既有 `Message` `packages/sz-orm-queue/src/queue.rs:57`；既有 `InMemoryQueue` `packages/sz-orm-queue/src/queue.rs:339`；既有 `dead_letters` `packages/sz-orm-queue/src/queue.rs:364`；既有 `max_retries` `packages/sz-orm-queue/src/queue.rs:366`；既有 `DEFAULT_MAX_RETRIES` `packages/sz-orm-queue/src/queue.rs:377`

**feature gate 隔离**：`dlx-auto-redelivery = ["dep:tokio"]`，默认关闭

**子任务**：
- [ ] M1-T1.1 `packages/sz-orm-queue/Cargo.toml` 完善 `dlx-auto-redelivery = ["dep:tokio"]` feature，新增 `tokio`（workspace, optional）依赖
- [ ] M1-T1.2 `src/lib.rs` 声明 `mod dlx;`，`#[cfg(feature = "dlx-auto-redelivery")]` 门控
- [ ] M1-T1.3 `src/dlx.rs` 定义 `pub enum BackoffPolicy { Fixed, Exponential, Linear, RandomJitter }`（四种退避策略，`Serialize + Deserialize`）
- [ ] M1-T1.4 `src/dlx.rs` 定义 `pub enum DlxRoutingStrategy { RequeueToOriginal, ForwardToDlxTopic, ForwardToDlxQueue, Drop }`（四种路由策略，`Serialize + Deserialize`）
- [ ] M1-T1.5 `src/dlx.rs` 定义 `pub struct DlxConfig { enabled, backoff_policy, initial_backoff_ms, max_backoff_ms, max_redelivery_count, routing_strategy, dlx_topic, dlx_queue }`（DLX 配置）
- [ ] M1-T1.6 实现 `impl DlxConfig { pub fn new() -> Self }`（默认：enabled false, Exponential, initial 1000ms, max 60000ms, max_redelivery 10, RequeueToOriginal）
- [ ] M1-T1.7 实现链式配置方法：`with_backoff_policy` / `with_initial_backoff_ms` / `with_max_backoff_ms` / `with_max_redelivery_count` / `with_routing_strategy` / `with_dlx_topic` / `with_dlx_queue`
- [ ] M1-T1.8 实现 `impl BackoffPolicy { pub fn calculate(&self, retry_count: u32, initial_ms: u64, max_ms: u64) -> u64 }`（Fixed=initial；Exponential=initial×2^retry 不超 max；Linear=initial×retry 不超 max；RandomJitter=initial×random(0.5~1.5)）
- [ ] M1-T1.9 单元测试：`DlxConfig::new()` 默认值正确（Exponential, 1000ms, 60000ms, 10 次, RequeueToOriginal）
- [ ] M1-T1.10 单元测试：`BackoffPolicy::Exponential` 计算 1s→2s→4s→8s，不超过 max 60s
- [ ] M1-T1.11 单元测试：`BackoffPolicy::Fixed` 始终返回 initial_ms
- [ ] M1-T1.12 边界测试：`BackoffPolicy::Exponential` retry_count=20 指数溢出 → 降级为 max_ms，不 panic
- [ ] M1-T1.13 验证 `cargo check -p sz-orm-queue --features dlx-auto-redelivery` 编译通过

**验收标准**：feature gate 定义；DlxConfig/BackoffPolicy/DlxRoutingStrategy 定义完整；退避策略计算正确；链式配置方法正确；边界处理不 panic

**依赖**：M0-T2

## M1-T2：DlxEntry 死信条目 + RedeliveryOutcome 重投递结果

**任务描述**：定义 `DlxEntry` 死信条目结构 + `RedeliveryOutcome` 重投递结果枚举，作为自动重投递调度器的中间数据结构。

**涉及文件**：
- `packages/sz-orm-queue/src/dlx.rs`（扩展：死信条目 + 重投递结果）

**复用标注**：既有 `Message` `packages/sz-orm-queue/src/queue.rs:57`（死信条目复用消息结构）；既有 `Message.retry_count` `packages/sz-orm-queue/src/queue.rs:67`

**子任务**：
- [ ] M1-T2.1 定义 `pub struct DlxEntry { message: Message, redelivery_count: u32, last_redelivery_at: u64, next_redelivery_at: u64 }`（死信条目，复用既有 `Message`）
- [ ] M1-T2.2 定义 `pub enum RedeliveryOutcome { Requeued, ForwardedToDlxTopic, ForwardedToDlxQueue, Dropped, LimitReached, Skipped(String) }`（重投递结果）
- [ ] M1-T2.3 实现 `impl DlxEntry { pub fn new(message: Message) -> Self }`（redelivery_count 初始 0，next_redelivery_at 初始 now）
- [ ] M1-T2.4 实现 `impl DlxEntry { pub fn should_redeliver(&self, max_count: u32) -> bool }`（redelivery_count < max_count 时返回 true）
- [ ] M1-T2.5 单元测试：`DlxEntry::new` 初始 redelivery_count 为 0
- [ ] M1-T2.6 单元测试：`should_redeliver` 在 redelivery_count < max_count 时返回 true，达到上限返回 false
- [ ] M1-T2.7 单元测试：`RedeliveryOutcome` 各变体序列化/反序列化往返一致

**验收标准**：DlxEntry/RedeliveryOutcome 定义完整；复用既有 `Message`；边界处理正确

**依赖**：M1-T1

## M1-T3：RedeliveryScheduler 自动重投递调度器

**任务描述**：实现 `RedeliveryScheduler` 自动重投递调度器，定期检查死信队列并按退避策略调度重投递，复用既有 `InMemoryQueue.dead_letters` / `requeue_dead_letter`。

**涉及文件**：
- `packages/sz-orm-queue/src/dlx.rs`（扩展：重投递调度器核心）

**复用标注**：既有 `InMemoryQueue` `packages/sz-orm-queue/src/queue.rs:339`；既有 `InMemoryQueue.dead_letters` `packages/sz-orm-queue/src/queue.rs:364`；既有 `InMemoryQueue::requeue_dead_letter` `packages/sz-orm-queue/src/queue.rs:484`（RequeueToOriginal 路由策略复用）；既有 `MessageQueue::publish` `packages/sz-orm-queue/src/queue.rs:24`（ForwardToDlxTopic/Queue 路由策略复用）；tokio 异步运行时

**子任务**：
- [ ] M1-T3.1 定义 `pub struct RedeliveryScheduler { queue: Arc<InMemoryQueue>, config: DlxConfig, running: Arc<AtomicBool> }`（调度器，持有队列 + 配置 + 运行标志）
- [ ] M1-T3.2 实现 `impl RedeliveryScheduler { pub fn new(queue: Arc<InMemoryQueue>, config: DlxConfig) -> Self }`
- [ ] M1-T3.3 实现 `pub async fn start(&self) -> Result<(), MqError>`：启动调度循环，`running` 设为 true，定期检查死信队列
- [ ] M1-T3.4 实现 `pub async fn stop(&self)`：停止调度循环，`running` 设为 false
- [ ] M1-T3.5 实现 `async fn schedule_redelivery(&self, entry: &DlxEntry) -> Result<RedeliveryOutcome, MqError>`：计算退避时间 → 等待 → 执行路由
- [ ] M1-T3.6 实现 `fn calculate_backoff(&self, retry_count: u32) -> u64`：委托 `BackoffPolicy::calculate`（M1-T1.8），不超过 `max_backoff_ms`
- [ ] M1-T3.7 退避等待：`tokio::time::sleep(backoff_time)` 等待退避时间后执行重投递
- [ ] M1-T3.8 重投递次数上限检查：`entry.should_redeliver(max_redelivery_count)` 为 false 时返回 `RedeliveryOutcome::LimitReached`，记录日志"redelivery limit reached for message X"
- [ ] M1-T3.9 单元测试：死信消息进入后，调度器按 Exponential 退避 1s→2s→4s 自动调度重投递
- [ ] M1-T3.10 单元测试：重投递次数达到上限（默认 10 次）→ 返回 `LimitReached`，不再重投递
- [ ] M1-T3.11 单元测试：`stop` 后调度器停止，不再调度重投递
- [ ] M1-T3.12 边界测试：死信队列为空 → 调度器空转，不 panic
- [ ] M1-T3.13 性能测试：调度器检查开销 ≤ 1ms/次（含退避计算 + 调度判定）

**验收标准**：RedeliveryScheduler 自动调度重投递；复用既有 `InMemoryQueue.dead_letters`；退避策略正确；重投递次数上限保护；性能 ≤ 1ms/次

**依赖**：M1-T1、M1-T2

## M1-T4：DLX 路由执行 + 四种路由策略

**任务描述**：实现 `execute_routing` 方法，按 `DlxRoutingStrategy` 处理死信消息（RequeueToOriginal 复用既有 `requeue_dead_letter` / ForwardToDlxTopic 转发到死信 topic / ForwardToDlxQueue 转发到死信 queue / Drop 丢弃）。

**涉及文件**：
- `packages/sz-orm-queue/src/dlx.rs`（扩展：路由执行逻辑）

**复用标注**：既有 `InMemoryQueue::requeue_dead_letter` `packages/sz-orm-queue/src/queue.rs:484`（RequeueToOriginal 复用）；既有 `MessageQueue::publish` `packages/sz-orm-queue/src/queue.rs:24`（ForwardToDlxTopic/Queue 复用）

**子任务**：
- [ ] M1-T4.1 实现 `async fn execute_routing(&self, message: &Message) -> Result<RedeliveryOutcome, MqError>`：按 `config.routing_strategy` 分发
- [ ] M1-T4.2 策略 RequeueToOriginal：复用既有 `requeue_dead_letter` `packages/sz-orm-queue/src/queue.rs:484`，消息重回原队列，返回 `Requeued`
- [ ] M1-T4.3 策略 ForwardToDlxTopic：通过 `publish(dlx_topic, message)` 转发到死信 topic，返回 `ForwardedToDlxTopic`
- [ ] M1-T4.4 策略 ForwardToDlxQueue：通过 `publish(dlx_queue, message)` 转发到死信 queue，返回 `ForwardedToDlxQueue`
- [ ] M1-T4.5 策略 Drop：从死信队列移除，记录日志，返回 `Dropped`
- [ ] M1-T4.6 前置条件校验：`ForwardToDlxTopic` 时 `dlx_topic` 必填，`ForwardToDlxQueue` 时 `dlx_queue` 必填，缺失返回错误
- [ ] M1-T4.7 单元测试：RequeueToOriginal → 复用 `requeue_dead_letter`，消息重回原队列
- [ ] M1-T4.8 单元测试：ForwardToDlxTopic + dlx_topic="orders.dlx" → 消息转发到 "orders.dlx" topic
- [ ] M1-T4.9 单元测试：ForwardToDlxQueue + dlx_queue="orders.dlx.queue" → 消息转发到死信 queue
- [ ] M1-T4.10 单元测试：Drop → 死信消息丢弃不重投递
- [ ] M1-T4.11 边界测试：ForwardToDlxTopic 但 dlx_topic=None → 返回错误"dlx_topic required but not configured"

**验收标准**：四种路由策略实现正确；复用既有 `requeue_dead_letter`/`publish`；前置条件校验

**依赖**：M1-T3

## M1-T5：重投递日志 + 异常处理

**任务描述**：实现重投递日志记录（消息 ID + 重投递次数 + 退避时间 + 路由策略 + 结果），处理三种异常场景（重投递消息不存在 / 目标队列不可用 / 退避策略计算异常）。

**涉及文件**：
- `packages/sz-orm-queue/src/dlx.rs`（扩展：日志记录 + 异常处理）

**复用标注**：既有 `message_tracing` 模块 `packages/sz-orm-queue/src/message_tracing.rs`（feature gate `message-tracing`，重投递日志复用）

**子任务**：
- [ ] M1-T5.1 重投递日志记录：每次重投递记录（消息 ID + 重投递次数 + 退避时间 + 路由策略 + 结果），复用既有 `message_tracing` 模块
- [ ] M1-T5.2 异常处理 1：重投递消息不存在（已被手动消费或删除）→ 跳过，返回 `RedeliveryOutcome::Skipped("message not found")`，记录日志"dead letter message X not found, skipped"
- [ ] M1-T5.3 异常处理 2：重投递目标队列不可用（连接断开）→ 按退避策略重试，重试超限后保持死信，记录日志"redelivery failed, target unavailable, retried N times"
- [ ] M1-T5.4 异常处理 3：退避策略计算溢出（Exponential 指数过大）→ 降级为 `max_backoff_ms`（60 秒），记录日志"backoff calculation overflow, fallback to max"
- [ ] M1-T5.5 单元测试：消息重投递 3 次 → 记录 3 条重投递日志，含消息 ID + 次数 + 退避时间 + 结果
- [ ] M1-T5.6 单元测试：重投递消息不存在 → 返回 `Skipped`，日志标注"message not found"
- [ ] M1-T5.7 单元测试：退避策略计算溢出 → 降级为 max_backoff_ms，日志标注"backoff fallback to max 60s"
- [ ] M1-T5.8 边界测试：重投递目标队列持续不可用 → 按退避策略重试至上限，保持死信，不无限重试

**验收标准**：重投递日志可追溯；三种异常场景处理正确；不丢失消息不无限重试

**依赖**：M1-T3、M1-T4

## M1-T6：M1 集成测试与门禁验证

**任务描述**：M1 里程碑集成测试与门禁验证，确保 REQ-V46-001 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M1-T6.1 集成测试：`RedeliveryScheduler::start` 完整流程（消息 nack 达 max_retries → 进死信 → 自动重投递 → 退避策略 → 路由策略 → 日志记录）
- [ ] M1-T6.2 集成测试：四种退避策略（Fixed/Exponential/Linear/RandomJitter）完整验证
- [ ] M1-T6.3 集成测试：四种路由策略（RequeueToOriginal/ForwardToDlxTopic/ForwardToDlxQueue/Drop）完整验证
- [ ] M1-T6.4 集成测试：复用既有 `InMemoryQueue.dead_letters` `packages/sz-orm-queue/src/queue.rs:364` + `requeue_dead_letter` `packages/sz-orm-queue/src/queue.rs:484`，不新建死信存储
- [ ] M1-T6.5 运行 `cargo test -p sz-orm-queue --features dlx-auto-redelivery`（全部通过）
- [ ] M1-T6.6 `cargo clippy -p sz-orm-queue --features dlx-auto-redelivery -- -D warnings`（clippy 静态分析）
- [ ] M1-T6.7 `cargo fmt -p sz-orm-queue -- --check`（fmt 格式检查）
- [ ] M1-T6.8 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-queue/src/dlx.rs` 无占位实现
- [ ] M1-T6.9 扫描 `grep -rn 'unsafe' packages/sz-orm-queue/src/dlx.rs` 无 unsafe 块
- [ ] M1-T6.10 验证默认 feature 行为与 v4.5.0 一致（`cargo build -p sz-orm-queue` 无 DLX 自动重投递，既有 `requeue_dead_letter` 手动调用仍可用）
- [ ] M1-T6.11 验证 `dlx-auto-redelivery` 与既有 feature（`cdc`/`message-tracing`）组合编译通过

**验收标准**：M1 集成测试通过；clippy/fmt/占位/unsafe 检查通过；默认 feature 行为不变；RedeliveryScheduler + 四种退避 + 四种路由 + 重投递上限 + 日志可追溯全部验证

**依赖**：M1-T1、M1-T2、M1-T3、M1-T4、M1-T5

---

# 四、M2：迁移回滚自动化（REQ-V46-002，P1，2.5 天）

**目标**：扩展既有 `sz-orm-core` 迁移管理，新增 `ZeroDowntimeRollbackStrategy` 三种零停机回滚策略（ShadowTable/ReverseMigration/BlueGreen），`AutoRollbackTrigger` 自动回滚触发器（健康检查连续失败 N 次在回滚窗口内触发回滚），`RollbackExecutor` 回滚执行器（复用既有 `MigrationContext::rollback`/`down`），回滚数据一致性校验，复用既有 `Migration`/`MigrationResolver`/`FileMigrationResolver`/`MigrationContext`。
**对应需求**：REQ-V46-002（spec.md §5.2，design.md §2.1.3.2 + §2.2.2.2）
**预期工作量**：2.5 天
**依赖**：无（M2 为 P1 独立需求，复用既有 sz-orm-core migration，扩展包可与 M1/M3~M7 并行）

## M2-T1：zero-downtime-rollback feature gate + ZeroDowntimeRollbackConfig 配置

**任务描述**：在 `sz-orm-core` 完善 `zero-downtime-rollback` feature gate 隔离（M0-T2 已创建占位），定义 `ZeroDowntimeRollbackConfig` 配置 + `ZeroDowntimeRollbackStrategy` 策略枚举 + `RollbackPlan` 回滚计划 + `RollbackWindow` 回滚窗口。

**涉及文件**：
- `packages/sz-orm-core/Cargo.toml`（完善：`zero-downtime-rollback` feature 依赖 tokio optional）
- `packages/sz-orm-core/src/lib.rs`（扩展：模块声明 `mod rollback_zero_downtime;`，`#[cfg(feature = "zero-downtime-rollback")]` 门控）
- `packages/sz-orm-core/src/rollback_zero_downtime.rs`（新建，零停机回滚配置 + 策略 + 计划 + 窗口）

**复用标注**：既有 `Migration` `packages/sz-orm-core/src/migration.rs:10`；既有 `Migration.sql_down` `packages/sz-orm-core/src/migration.rs:18`；既有 `MigrationResolver` `packages/sz-orm-core/src/migration.rs:62`；既有 `FileMigrationResolver` `packages/sz-orm-core/src/migration.rs:68`；既有 `MigrationContext` `packages/sz-orm-core/src/migration.rs:193`

**子任务**：
- [ ] M2-T1.1 `packages/sz-orm-core/Cargo.toml` 完善 `zero-downtime-rollback = ["dep:tokio"]` feature，新增 `tokio`（workspace, optional）依赖
- [ ] M2-T1.2 `src/lib.rs` 声明 `mod rollback_zero_downtime;`，`#[cfg(feature = "zero-downtime-rollback")]` 门控
- [ ] M2-T1.3 `src/rollback_zero_downtime.rs` 定义 `pub enum ZeroDowntimeRollbackStrategy { ShadowTable, ReverseMigration, BlueGreen }`（三种策略，`Serialize + Deserialize`）
- [ ] M2-T1.4 定义 `pub struct ZeroDowntimeRollbackConfig { strategy, rollback_window_ms, health_check_interval_ms, health_check_failure_threshold, error_rate_threshold, response_time_threshold_ms }`（配置）
- [ ] M2-T1.5 实现 `impl ZeroDowntimeRollbackConfig { pub fn new() -> Self }`（默认：ShadowTable, window 300000ms, interval 10000ms, threshold 3, error_rate 0.05, response_time 5000ms）
- [ ] M2-T1.6 实现链式配置方法：`with_strategy` / `with_rollback_window_ms` / `with_health_check_failure_threshold` / `with_error_rate_threshold` / `with_response_time_threshold_ms`
- [ ] M2-T1.7 定义 `pub struct RollbackPlan { target_version: String, strategy: ZeroDowntimeRollbackStrategy, migrations_to_rollback: Vec<Migration> }`（回滚计划，复用既有 `Migration`）
- [ ] M2-T1.8 定义 `pub struct RollbackWindow { deployed_at: u64, window_ms: u64 }` + `impl RollbackWindow { pub fn new(window_ms: u64) -> Self; pub fn is_within_window(&self) -> bool }`（回滚窗口）
- [ ] M2-T1.9 单元测试：`ZeroDowntimeRollbackConfig::new()` 默认值正确（ShadowTable, 300000ms, 3 次, 0.05, 5000ms）
- [ ] M2-T1.10 单元测试：`RollbackWindow::is_within_window` 在窗口内返回 true，超时返回 false
- [ ] M2-T1.11 边界测试：`rollback_window_ms = 0` → `is_within_window` 始终返回 false（无窗口）
- [ ] M2-T1.12 验证 `cargo check -p sz-orm-core --features zero-downtime-rollback` 编译通过

**验收标准**：feature gate 定义；配置/策略/计划/窗口定义完整；复用既有 `Migration`；链式配置方法正确

**依赖**：M0-T2

## M2-T2：HealthCheck 健康检查 + HealthStatus

**任务描述**：实现 `HealthCheck` 健康检查器，检查错误率与响应时间，连续失败计数，达到阈值后触发回滚。

**涉及文件**：
- `packages/sz-orm-core/src/rollback_zero_downtime.rs`（扩展：健康检查）

**子任务**：
- [ ] M2-T2.1 定义 `pub enum HealthStatus { Healthy, Unhealthy { error_rate: f64, response_time_ms: u64 } }`（健康状态）
- [ ] M2-T2.2 定义 `pub struct HealthCheck { error_rate_threshold: f64, response_time_threshold_ms: u64, consecutive_failures: u32, failure_threshold: u32 }`（健康检查器）
- [ ] M2-T2.3 实现 `impl HealthCheck { pub fn new(config: &ZeroDowntimeRollbackConfig) -> Self }`（从配置初始化，consecutive_failures 初始 0）
- [ ] M2-T2.4 实现 `pub async fn check(&mut self) -> Result<HealthStatus, RollbackError>`：检查错误率与响应时间，健康时重置 consecutive_failures，不健康时 consecutive_failures + 1
- [ ] M2-T2.5 实现 `pub fn should_trigger_rollback(&self) -> bool`：consecutive_failures >= failure_threshold 时返回 true
- [ ] M2-T2.6 单元测试：健康检查通过 → 返回 `Healthy`，consecutive_failures 重置为 0
- [ ] M2-T2.7 单元测试：健康检查失败 → 返回 `Unhealthy`，consecutive_failures + 1
- [ ] M2-T2.8 单元测试：连续失败 3 次（默认阈值）→ `should_trigger_rollback` 返回 true
- [ ] M2-T2.9 边界测试：单次失败后立即恢复（抖动）→ consecutive_failures 重置，不触发回滚
- [ ] M2-T2.10 边界测试：`error_rate_threshold = 0.0` → 任何错误率都触发不健康

**验收标准**：健康检查器正确；连续失败计数；抖动过滤；阈值触发

**依赖**：M2-T1

## M2-T3：RollbackExecutor 回滚执行器 + 三种策略

**任务描述**：实现 `RollbackExecutor` 回滚执行器，按 `ZeroDowntimeRollbackStrategy` 执行回滚，复用既有 `MigrationContext::rollback`/`down`，ShadowTable 模式校验数据一致性后切换流量。

**涉及文件**：
- `packages/sz-orm-core/src/rollback_zero_downtime.rs`（扩展：回滚执行器）

**复用标注**：既有 `MigrationContext::rollback` `packages/sz-orm-core/src/migration.rs:587`（ReverseMigration 策略复用）；既有 `MigrationContext::down` `packages/sz-orm-core/src/migration.rs:677`（ShadowTable 策略复用）；既有 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82`（参数化绑定执行 down SQL）

**子任务**：
- [ ] M2-T3.1 定义 `pub struct RollbackExecutor { migration_context: MigrationContext }`（回滚执行器，复用既有 `MigrationContext`）
- [ ] M2-T3.2 实现 `impl RollbackExecutor { pub fn new(migration_context: MigrationContext) -> Self }`
- [ ] M2-T3.3 实现 `pub async fn execute(&mut self, plan: &RollbackPlan) -> Result<RollbackResult, RollbackError>`：按 `plan.strategy` 分发到具体策略
- [ ] M2-T3.4 实现 `async fn execute_shadow_table(&mut self, plan: &RollbackPlan) -> Result<RollbackResult, RollbackError>`：在 shadow table 执行 down SQL（复用既有 `MigrationContext::down` `migration.rs:677`）→ 校验数据一致性 → 切换流量
- [ ] M2-T3.5 实现 `async fn execute_reverse_migration(&mut self, plan: &RollbackPlan) -> Result<RollbackResult, RollbackError>`：复用既有 `MigrationContext::rollback` `migration.rs:587` 直接执行 down SQL
- [ ] M2-T3.6 实现 `async fn execute_blue_green(&mut self, plan: &RollbackPlan) -> Result<RollbackResult, RollbackError>`：切换到旧版本（需蓝绿部署支持）
- [ ] M2-T3.7 实现 `async fn verify_consistency(&self, shadow_table: &str, original_table: &str) -> Result<bool, RollbackError>`：校验 shadow table 与原表数据一致性
- [ ] M2-T3.8 定义 `pub struct RollbackResult { version: String, strategy: ZeroDowntimeRollbackStrategy, elapsed_ms: u64, success: bool }`（回滚结果）
- [ ] M2-T3.9 单元测试：ShadowTable 策略 → 在 shadow table 执行 down SQL + 校验一致性 + 切换流量
- [ ] M2-T3.10 单元测试：ReverseMigration 策略 → 复用既有 `rollback` 执行 down SQL
- [ ] M2-T3.11 单元测试：ShadowTable + 数据一致性校验失败 → 保持原状态不切换流量，返回错误"consistency check failed"
- [ ] M2-T3.12 边界测试：回滚 SQL 执行失败 → 中止回滚，保持原状态，返回错误"rollback SQL execution failed"
- [ ] M2-T3.13 性能测试：零停机回滚切换时间 ≤ 5 秒（ShadowTable 模式，含数据校验 + 切换 + 连接刷新）

**验收标准**：三种回滚策略实现正确；复用既有 `MigrationContext::rollback`/`down`；数据一致性校验；零停机切换 ≤ 5 秒

**依赖**：M2-T1

## M2-T4：AutoRollbackTrigger 自动回滚触发器

**任务描述**：实现 `AutoRollbackTrigger` 自动回滚触发器，持续健康检查，连续失败 N 次在回滚窗口内自动触发回滚，无需人工干预。

**涉及文件**：
- `packages/sz-orm-core/src/rollback_zero_downtime.rs`（扩展：自动回滚触发器）

**子任务**：
- [ ] M2-T4.1 定义 `pub struct AutoRollbackTrigger { config: ZeroDowntimeRollbackConfig, health_check: HealthCheck, window: RollbackWindow, executor: RollbackExecutor }`（自动回滚触发器）
- [ ] M2-T4.2 实现 `impl AutoRollbackTrigger { pub fn new(config: ZeroDowntimeRollbackConfig, executor: RollbackExecutor) -> Self }`
- [ ] M2-T4.3 实现 `pub async fn start(&mut self) -> Result<(), RollbackError>`：启动持续健康检查循环
- [ ] M2-T4.4 实现 `async fn evaluate_and_trigger(&mut self) -> Result<(), RollbackError>`：健康检查 → 连续失败判定 → 窗口判定 → 触发回滚
- [ ] M2-T4.5 回滚窗口判定：`window.is_within_window()` 为 false 时拒绝自动回滚，返回错误"rollback window expired, manual rollback required"
- [ ] M2-T4.6 健康检查间隔：`tokio::time::sleep(config.health_check_interval_ms)` 控制检查频率
- [ ] M2-T4.7 单元测试：健康检查连续失败 3 次 + 在回滚窗口内 → 自动触发零停机回滚，记录日志"auto rollback triggered by health check failure"
- [ ] M2-T4.8 单元测试：健康检查连续失败 3 次 + 超过回滚窗口 → 拒绝自动回滚，返回错误"rollback window expired"
- [ ] M2-T4.9 单元测试：健康检查失败 2 次 + 恢复 → consecutive_failures 重置，不触发回滚
- [ ] M2-T4.10 边界测试：`failure_threshold = 1` → 单次失败即触发回滚

**验收标准**：自动回滚触发器正确；窗口保护；连续失败过滤抖动；无需人工干预

**依赖**：M2-T2、M2-T3

## M2-T5：回滚日志 + 数据一致性校验

**任务描述**：实现回滚日志记录（版本 + 策略 + 触发原因 + 耗时 + 结果），完善 ShadowTable 数据一致性校验逻辑。

**涉及文件**：
- `packages/sz-orm-core/src/rollback_zero_downtime.rs`（扩展：日志 + 一致性校验）

**子任务**：
- [ ] M2-T5.1 回滚日志记录：每次回滚记录（版本 + 策略 + 触发原因 + 耗时 + 结果），供审计追溯
- [ ] M2-T5.2 ShadowTable 数据一致性校验：校验 shadow table 与原表行数 + 校验关键字段值一致
- [ ] M2-T5.3 校验失败处理：保持原状态不切换流量，记录错误"rollback data consistency check failed"
- [ ] M2-T5.4 回滚 SQL 参数化：down SQL 通过 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82` 参数化绑定，禁止 SQL 字符串拼接
- [ ] M2-T5.5 单元测试：自动回滚触发 → 记录回滚日志，含版本 + 策略 + 触发原因 + 耗时 + 结果
- [ ] M2-T5.6 单元测试：ShadowTable 数据一致性校验通过 → 切换流量，记录成功
- [ ] M2-T5.7 单元测试：ShadowTable 数据一致性校验失败 → 保持原状态，记录错误"consistency check failed"
- [ ] M2-T5.8 边界测试：回滚日志在回滚失败时也记录（含失败原因）

**验收标准**：回滚日志可追溯；数据一致性校验正确；参数化查询强制

**依赖**：M2-T3、M2-T4

## M2-T6：M2 集成测试与门禁验证

**任务描述**：M2 里程碑集成测试与门禁验证，确保 REQ-V46-002 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M2-T6.1 集成测试：`AutoRollbackTrigger::start` 完整流程（部署 → 健康检查 → 连续失败 → 窗口判定 → 回滚执行 → 一致性校验 → 流量切换 → 日志记录）
- [ ] M2-T6.2 集成测试：三种回滚策略（ShadowTable/ReverseMigration/BlueGreen）完整验证
- [ ] M2-T6.3 集成测试：复用既有 `MigrationContext::rollback` `packages/sz-orm-core/src/migration.rs:587` + `down` `packages/sz-orm-core/src/migration.rs:677`，不新建回滚执行逻辑
- [ ] M2-T6.4 集成测试：回滚窗口超时保护 + 健康检查抖动过滤
- [ ] M2-T6.5 运行 `cargo test -p sz-orm-core --features zero-downtime-rollback`（全部通过）
- [ ] M2-T6.6 `cargo clippy -p sz-orm-core --features zero-downtime-rollback -- -D warnings`
- [ ] M2-T6.7 `cargo fmt -p sz-orm-core -- --check`
- [ ] M2-T6.8 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-core/src/rollback_zero_downtime.rs` 无占位实现
- [ ] M2-T6.9 扫描 `grep -rn 'unsafe' packages/sz-orm-core/src/rollback_zero_downtime.rs` 无 unsafe 块
- [ ] M2-T6.10 验证默认 feature 行为与 v4.5.0 一致（`cargo build -p sz-orm-core` 无零停机回滚，既有 `rollback`/`down` 手动调用仍可用）
- [ ] M2-T6.11 验证 `zero-downtime-rollback` 与既有 feature（`migration-branch`/`migration-dry-run`）组合编译通过

**验收标准**：M2 集成测试通过；门禁通过；默认行为不变；三种策略 + 自动触发 + 窗口保护 + 一致性校验 + 日志可追溯全部验证

**依赖**：M2-T1、M2-T2、M2-T3、M2-T4、M2-T5

---

# 五、M3：批量事务原子性保证（REQ-V46-003，P1，2 天）

**目标**：扩展既有 `sz-orm-batch` 批量执行器，新增 `AtomicityGuarantee` 三种原子性保证级别（AllOrNothing/BestEffort/SagaCompensation），`BatchTransactionCoordinator` 批量事务协调器，`SagaCompensator` Saga 补偿器，复用既有 `BatchExecutor` + `sz-orm-dtx` Saga/2PC，确保跨批次操作的原子性。
**对应需求**：REQ-V46-003（spec.md §5.3，design.md §2.1.3.3 + §2.2.2.3）
**预期工作量**：2 天
**依赖**：无（M3 为 P1 独立需求，复用既有 sz-orm-batch BatchExecutor + sz-orm-dtx Saga/2PC，扩展包可与 M1/M2/M4~M7 并行）

## M3-T1：batch-atomic feature gate + AtomicityGuarantee + BatchAtomicConfig

**任务描述**：在 `sz-orm-batch` 完善 `batch-atomic` feature gate 隔离（M0-T2 已创建占位），定义 `AtomicityGuarantee` 原子性保证枚举 + `BatchAtomicConfig` 配置 + `BatchOperation` 批量操作枚举。

**涉及文件**：
- `packages/sz-orm-batch/Cargo.toml`（完善：`batch-atomic` feature 依赖 sz-orm-dtx optional）
- `packages/sz-orm-batch/src/lib.rs`（扩展：模块声明 `mod atomic;`，`#[cfg(feature = "batch-atomic")]` 门控）
- `packages/sz-orm-batch/src/atomic.rs`（新建，原子性配置 + 操作枚举）

**复用标注**：既有 `BatchExecutorConfig` `packages/sz-orm-batch/src/executor.rs:18`；既有 `BatchExecutionResult` `packages/sz-orm-batch/src/executor.rs:93`；既有 `DEFAULT_CHUNK_SIZE` `packages/sz-orm-batch/src/lib.rs:146`；既有 `RollbackStrategy` `packages/sz-orm-batch/src/lib.rs:518`；既有 `ProgressCallback` `packages/sz-orm-batch/src/lib.rs:482`；既有 `Saga` `packages/sz-orm-dtx/src/saga.rs:377`；既有 `SagaStep` `packages/sz-orm-dtx/src/saga.rs:255`；既有 `SagaLog` `packages/sz-orm-dtx/src/saga.rs:105`；既有 `DistributedTransaction` `packages/sz-orm-dtx/src/lib.rs:270`

**子任务**：
- [ ] M3-T1.1 `packages/sz-orm-batch/Cargo.toml` 完善 `batch-atomic = ["dep:sz-orm-dtx"]` feature，新增 `sz-orm-dtx`（workspace, optional）依赖
- [ ] M3-T1.2 `src/lib.rs` 声明 `mod atomic;`，`#[cfg(feature = "batch-atomic")]` 门控
- [ ] M3-T1.3 `src/atomic.rs` 定义 `pub enum AtomicityGuarantee { AllOrNothing, BestEffort, SagaCompensation }`（三种原子性保证级别，`Serialize + Deserialize`）
- [ ] M3-T1.4 定义 `pub struct BatchAtomicConfig { atomicity_guarantee: AtomicityGuarantee, chunk_size: usize, progress_callback: Option<ProgressCallback>, saga_log: Option<Arc<dyn SagaLog>> }`（配置，复用既有 `ProgressCallback` + `SagaLog`）
- [ ] M3-T1.5 实现 `impl BatchAtomicConfig { pub fn new() -> Self }`（默认：BestEffort, chunk_size 1000 复用 `DEFAULT_CHUNK_SIZE`）
- [ ] M3-T1.6 实现链式配置方法：`with_atomicity_guarantee` / `with_chunk_size` / `with_progress_callback` / `with_saga_log`
- [ ] M3-T1.7 定义 `pub enum BatchOperation { Insert { table, rows }, Update { table, rows }, Delete { table, primary_key, ids }, Upsert { table, rows } }`（批量操作枚举）
- [ ] M3-T1.8 定义 `pub struct BatchAtomicResult { success: bool, executed_batches: usize, failed_batch: Option<usize>, compensation_log: Vec<String>, batch_results: Vec<BatchExecutionResult> }`（原子结果，复用既有 `BatchExecutionResult`）
- [ ] M3-T1.9 单元测试：`BatchAtomicConfig::new()` 默认值正确（BestEffort, chunk_size 1000）
- [ ] M3-T1.10 单元测试：`AtomicityGuarantee` 各变体序列化/反序列化往返一致
- [ ] M3-T1.11 验证 `cargo check -p sz-orm-batch --features batch-atomic` 编译通过
- [ ] M3-T1.12 验证 `batch-atomic` 与既有 `batch-v2`/`batch-stream` feature 组合编译通过（三 feature 独立）

**验收标准**：feature gate 定义；AtomicityGuarantee/BatchAtomicConfig/BatchOperation 定义完整；复用既有 `ProgressCallback`/`SagaLog`/`BatchExecutionResult`；与既有 feature 独立

**依赖**：M0-T2

## M3-T2：BatchTransactionCoordinator 批量事务协调器

**任务描述**：实现 `BatchTransactionCoordinator` 批量事务协调器，`execute_atomic` 入口按 `AtomicityGuarantee` 分发到三种模式，复用既有 `BatchExecutor`。

**涉及文件**：
- `packages/sz-orm-batch/src/atomic.rs`（扩展：批量事务协调器核心）

**复用标注**：既有 `BatchExecutor` `packages/sz-orm-batch/src/executor.rs`（批量执行复用）；既有 `Connection` trait `packages/sz-orm-core/src/pool.rs:45`

**子任务**：
- [ ] M3-T2.1 定义 `pub struct BatchTransactionCoordinator { executor: BatchExecutor, config: BatchAtomicConfig }`（协调器，持有批量执行器 + 配置）
- [ ] M3-T2.2 实现 `impl BatchTransactionCoordinator { pub fn new(executor: BatchExecutor, config: BatchAtomicConfig) -> Self }`
- [ ] M3-T2.3 实现 `pub async fn execute_atomic(&self, conn: &mut dyn Connection, batches: Vec<BatchOperation>) -> Result<BatchAtomicResult, BatchAtomicError>`：按 `config.atomicity_guarantee` 分发
- [ ] M3-T2.4 实现 `async fn execute_all_or_nothing(&self, conn: &mut dyn Connection, batches: Vec<BatchOperation>) -> Result<BatchAtomicResult, BatchAtomicError>`（M3-T3 展开）
- [ ] M3-T2.5 实现 `async fn execute_saga_compensation(&self, conn: &mut dyn Connection, batches: Vec<BatchOperation>) -> Result<BatchAtomicResult, BatchAtomicError>`（M3-T4 展开）
- [ ] M3-T2.6 实现 `async fn execute_best_effort(&self, conn: &mut dyn Connection, batches: Vec<BatchOperation>) -> Result<BatchAtomicResult, BatchAtomicError>`（M3-T3 展开）
- [ ] M3-T2.7 定义 `pub enum BatchAtomicError { AtomicityViolated, CompensationFailed, TwoPhaseCommitFailed, BatchEmpty, ChunkSizeZero }`（错误类型）
- [ ] M3-T2.8 前置条件校验：`batches` 为空 → 返回 `BatchEmpty`；`chunk_size = 0` → 返回 `ChunkSizeZero`
- [ ] M3-T2.9 单元测试：`execute_atomic` 按 `AtomicityGuarantee` 正确分发到三种模式
- [ ] M3-T2.10 边界测试：空 batches → 返回 `BatchEmpty` 错误

**验收标准**：协调器入口完整；按原子性级别正确分发；前置条件校验

**依赖**：M3-T1

## M3-T3：AllOrNothing（2PC）+ BestEffort 模式

**任务描述**：实现 AllOrNothing 模式（复用既有 `DistributedTransaction` 2PC，prepare 全成功后 commit，任一失败 rollback）和 BestEffort 模式（复用既有 `BatchExecutor` + `RollbackStrategy::None`，允许部分成功）。

**涉及文件**：
- `packages/sz-orm-batch/src/atomic.rs`（扩展：AllOrNothing + BestEffort 实现）

**复用标注**：既有 `DistributedTransaction` `packages/sz-orm-dtx/src/lib.rs:270`（2PC 协调）；既有 `DistributedTransaction::prepare` `packages/sz-orm-dtx/src/lib.rs:334`；既有 `DistributedTransaction::commit` `packages/sz-orm-dtx/src/lib.rs:372`；既有 `DistributedTransaction::rollback` `packages/sz-orm-dtx/src/lib.rs:407`；既有 `RollbackStrategy::None` `packages/sz-orm-batch/src/lib.rs:518`

**子任务**：
- [ ] M3-T3.1 AllOrNothing 模式：创建 `DistributedTransaction`（复用既有 `packages/sz-orm-dtx/src/lib.rs:270`），每个批次作为 participant
- [ ] M3-T3.2 AllOrNothing prepare 阶段：所有批次 prepare，全部成功后进入 commit 阶段
- [ ] M3-T3.3 AllOrNothing commit 阶段：全部 commit，返回 `BatchAtomicResult { success: true, ... }`
- [ ] M3-T3.4 AllOrNothing rollback：任一 prepare 失败 → 全部 rollback，返回错误"atomicity violated, all batches rolled back"
- [ ] M3-T3.5 BestEffort 模式：复用既有 `BatchExecutor` + `RollbackStrategy::None` `packages/sz-orm-batch/src/lib.rs:518`，允许部分成功
- [ ] M3-T3.6 BestEffort 行为兼容：与既有 `RollbackStrategy::None` 行为一致，返回 `BatchAtomicResult` 含部分成功结果
- [ ] M3-T3.7 单元测试：AllOrNothing + 3 批次 + 第 2 批次失败 → 全部回滚，返回 `AtomicityViolated`，不产生部分提交
- [ ] M3-T3.8 单元测试：AllOrNothing + 3 批次全部成功 → 全部 commit，返回 success
- [ ] M3-T3.9 单元测试：BestEffort + 3 批次 + 第 2 批次失败 → 第 1/3 批次成功，第 2 批次失败，返回部分成功结果
- [ ] M3-T3.10 边界测试：AllOrNothing + 单批次 → 等效于单批次事务
- [ ] M3-T3.11 性能测试：批量事务原子性协调开销 ≤ 10ms（含 Saga 步骤编排 + 补偿回滚判定）

**验收标准**：AllOrNothing 保证 all-or-nothing 语义；BestEffort 兼容既有 `RollbackStrategy::None`；复用既有 `DistributedTransaction` 2PC；性能 ≤ 10ms

**依赖**：M3-T2

## M3-T4：SagaCompensation 模式 + SagaCompensator

**任务描述**：实现 SagaCompensation 模式（复用既有 `Saga`，每批次作为 `SagaStep`，失败时补偿回滚已成功批次）和 `SagaCompensator` Saga 补偿器。

**涉及文件**：
- `packages/sz-orm-batch/src/atomic.rs`（扩展：SagaCompensation + SagaCompensator）

**复用标注**：既有 `Saga` `packages/sz-orm-dtx/src/saga.rs:377`；既有 `SagaStep` `packages/sz-orm-dtx/src/saga.rs:255`；既有 `Saga::execute` `packages/sz-orm-dtx/src/saga.rs:507`；既有 `SagaLog` `packages/sz-orm-dtx/src/saga.rs:105`

**子任务**：
- [ ] M3-T4.1 定义 `pub struct SagaCompensator { saga: Saga }`（Saga 补偿器，复用既有 `Saga`）
- [ ] M3-T4.2 实现 `impl SagaCompensator { pub fn new(saga_log: Option<Arc<dyn SagaLog>>) -> Self }`（复用既有 `SagaLog`）
- [ ] M3-T4.3 实现 `pub fn add_batch_as_step(&mut self, batch: BatchOperation, compensation: BatchOperation)`：每批次作为 `SagaStep`（复用既有 `SagaStep` `packages/sz-orm-dtx/src/saga.rs:255`），action=批次，compensation=反向操作
- [ ] M3-T4.4 实现 `pub async fn execute(&mut self) -> Result<SagaResult, BatchAtomicError>`：复用既有 `Saga::execute` `packages/sz-orm-dtx/src/saga.rs:507`
- [ ] M3-T4.5 SagaCompensation 模式：创建 `SagaCompensator`，每批次作为 `SagaStep`，失败时补偿回滚已成功批次
- [ ] M3-T4.6 补偿失败处理：补偿操作失败时记录补偿失败日志供人工干预，不静默忽略，返回 `CompensationFailed`
- [ ] M3-T4.7 单元测试：SagaCompensation + 3 批次 + 第 2 批次失败 → 第 1 批次补偿回滚，第 2/3 批次不执行，返回 Saga 结果含补偿日志
- [ ] M3-T4.8 单元测试：SagaCompensation + 3 批次全部成功 → 返回 SagaResult(完成)
- [ ] M3-T4.9 单元测试：Saga 补偿失败 → 记录补偿失败日志，返回 `CompensationFailed`，标注"compensation failed for step X, manual intervention required"
- [ ] M3-T4.10 边界测试：SagaCompensation + 单批次失败 → 无已成功批次需补偿，直接返回失败

**验收标准**：SagaCompensation 复用既有 `Saga`/`SagaStep`；补偿回滚正确；补偿失败不静默忽略

**依赖**：M3-T2

## M3-T5：原子提交日志 + 异常处理

**任务描述**：实现原子提交日志记录（批次列表 + 原子性级别 + 提交/回滚结果 + Saga 补偿日志），处理四种异常场景。

**涉及文件**：
- `packages/sz-orm-batch/src/atomic.rs`（扩展：日志 + 异常处理）

**子任务**：
- [ ] M3-T5.1 原子提交日志记录：记录（批次列表 + 原子性级别 + 提交/回滚结果 + Saga 补偿日志），供审计追溯
- [ ] M3-T5.2 异常处理 1：AllOrNothing 部分批次失败 → 全部回滚，返回 `AtomicityViolated`
- [ ] M3-T5.3 异常处理 2：Saga 补偿失败 → 记录补偿失败日志供人工干预，返回 `CompensationFailed`
- [ ] M3-T5.4 异常处理 3：2PC 协调器失败 → 按 2PC 协议处理（prepare 失败 rollback，commit 失败记录待提交事务），返回 `TwoPhaseCommitFailed`
- [ ] M3-T5.5 异常处理 4：批次执行超时 → 按原子性级别处理（AllOrNothing 全回滚 / SagaCompensation 补偿 / BestEffort 计入失败）
- [ ] M3-T5.6 批量操作参数化：所有 SQL 通过 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82` 参数化绑定，禁止 SQL 字符串拼接
- [ ] M3-T5.7 单元测试：AllOrNothing 3 批次提交 → 记录原子提交日志，含批次列表 + 级别 + 结果
- [ ] M3-T5.8 单元测试：SagaCompensation 补偿回滚 → 日志含补偿日志
- [ ] M3-T5.9 边界测试：2PC commit 阶段失败 → 记录待提交事务，不产生不一致状态

**验收标准**：原子提交日志可追溯；四种异常场景处理正确；参数化查询强制

**依赖**：M3-T3、M3-T4

## M3-T6：M3 集成测试与门禁验证

**任务描述**：M3 里程碑集成测试与门禁验证，确保 REQ-V46-003 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M3-T6.1 集成测试：`BatchTransactionCoordinator::execute_atomic` 完整流程（三种原子性级别 + 批次执行 + 原子提交/回滚 + 日志记录）
- [ ] M3-T6.2 集成测试：AllOrNothing all-or-nothing 语义（全成功提交/全失败回滚，不产生部分提交）
- [ ] M3-T6.3 集成测试：SagaCompensation 补偿回滚（失败时补偿已成功批次，补偿失败记录日志）
- [ ] M3-T6.4 集成测试：BestEffort 兼容既有 `RollbackStrategy::None` 行为
- [ ] M3-T6.5 集成测试：复用既有 `BatchExecutor` `packages/sz-orm-batch/src/executor.rs` + `Saga` `packages/sz-orm-dtx/src/saga.rs:377` + `DistributedTransaction` `packages/sz-orm-dtx/src/lib.rs:270`，不新建执行逻辑
- [ ] M3-T6.6 运行 `cargo test -p sz-orm-batch --features batch-atomic`（全部通过）
- [ ] M3-T6.7 `cargo clippy -p sz-orm-batch --features batch-atomic -- -D warnings`
- [ ] M3-T6.8 `cargo fmt -p sz-orm-batch -- --check`
- [ ] M3-T6.9 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-batch/src/atomic.rs` 无占位实现
- [ ] M3-T6.10 扫描 `grep -rn 'unsafe' packages/sz-orm-batch/src/atomic.rs` 无 unsafe 块
- [ ] M3-T6.11 验证默认 feature 行为与 v4.5.0 一致（`cargo build -p sz-orm-batch` 无原子性保证，既有 `BatchExecutor` 仍可用）
- [ ] M3-T6.12 验证 `batch-atomic` 与既有 `batch-v2`/`batch-stream` feature 组合编译通过

**验收标准**：M3 集成测试通过；门禁通过；默认行为不变；三种原子性级别 + Saga 补偿 + 2PC 协调 + 日志可追溯全部验证

**依赖**：M3-T1、M3-T2、M3-T3、M3-T4、M3-T5

---

# 六、M4：连接级多租户隔离（REQ-V46-006，P1，2 天）

**目标**：扩展既有 `sz-orm-core` 连接池与多租户，新增 `ConnectionTenantBinder` 连接租户绑定器，在同一连接池中连接绑定到特定租户（通过 `SET app.tenant_id = ?`），避免 `TenantPoolRegistry` 每租户独立池的资源开销，支持 `ConnectionAffinityPolicy` 连接亲和策略，`TenantConnectionGuard` RAII 守卫，复用既有 `Pool`/`Connection`/`TenantContext`/`RowLevelSecurityPolicy`。
**对应需求**：REQ-V46-006（spec.md §5.6，design.md §2.1.3.5 + §2.2.2.6）
**预期工作量**：2 天
**依赖**：无（M4 为 P1 独立需求，复用既有 sz-orm-core pool/tenant_context，扩展包可与 M1~M3/M5~M7 并行）

## M4-T1：connection-level-tenant feature gate + ConnectionLevelTenantConfig 配置

**任务描述**：在 `sz-orm-core` 完善 `connection-level-tenant` feature gate 隔离（M0-T2 已创建占位），定义 `ConnectionLevelTenantConfig` 配置 + `ConnectionLevelIsolation` 隔离机制枚举 + `ConnectionAffinityPolicy` 亲和策略枚举。

**涉及文件**：
- `packages/sz-orm-core/Cargo.toml`（完善：`connection-level-tenant` feature 依赖既有 `multi-tenant-enhanced`）
- `packages/sz-orm-core/src/lib.rs`（扩展：模块声明 `mod connection_tenant;`，`#[cfg(feature = "connection-level-tenant")]` 门控）
- `packages/sz-orm-core/src/connection_tenant.rs`（新建，连接级多租户配置 + 隔离机制 + 亲和策略）

**复用标注**：既有 `Pool` `packages/sz-orm-core/src/pool.rs:743`；既有 `Connection` trait `packages/sz-orm-core/src/pool.rs:45`；既有 `PooledConnection` `packages/sz-orm-core/src/pool.rs:239`；既有 `TenantContext` `packages/sz-orm-core/src/tenant_context.rs:80`；既有 `IsolationStrategy` `packages/sz-orm-core/src/tenant_context.rs:22`；既有 `TenantContextGuard` `packages/sz-orm-core/src/tenant_context.rs:166`；既有 `SchemaIsolationRouter` `packages/sz-orm-core/src/tenant_context.rs:194`；既有 `TenantPoolRegistry` `packages/sz-orm-core/src/tenant_context.rs:224`；既有 `RowLevelSecurityPolicy` `packages/sz-orm-core/src/tenant_security.rs:67`；既有 `DbType` `packages/sz-orm-core/src/db_type.rs:11`

**子任务**：
- [ ] M4-T1.1 `packages/sz-orm-core/Cargo.toml` 完善 `connection-level-tenant = ["multi-tenant-enhanced"]` feature（依赖既有 `multi-tenant-enhanced`）
- [ ] M4-T1.2 `src/lib.rs` 声明 `mod connection_tenant;`，`#[cfg(feature = "connection-level-tenant")]` 门控
- [ ] M4-T1.3 `src/connection_tenant.rs` 定义 `pub enum ConnectionLevelIsolation { SetTenantId, SchemaIsolation, ConnectionBinding }`（三种隔离机制，`Serialize + Deserialize`）
- [ ] M4-T1.4 定义 `pub enum ConnectionAffinityPolicy { Strict, Preferred, None }`（三种亲和策略，`Serialize + Deserialize`）
- [ ] M4-T1.5 定义 `pub struct ConnectionLevelTenantConfig { isolation: ConnectionLevelIsolation, affinity_policy: ConnectionAffinityPolicy, affinity_timeout_ms: u64, db_type: DbType }`（配置，复用既有 `DbType`）
- [ ] M4-T1.6 实现 `impl ConnectionLevelTenantConfig { pub fn new(db_type: DbType) -> Self }`（默认：SetTenantId, Preferred, 5000ms）
- [ ] M4-T1.7 实现链式配置方法：`with_isolation` / `with_affinity_policy` / `with_affinity_timeout_ms`
- [ ] M4-T1.8 单元测试：`ConnectionLevelTenantConfig::new(DbType::PostgreSQL)` 默认值正确（SetTenantId, Preferred, 5000ms）
- [ ] M4-T1.9 单元测试：`ConnectionLevelIsolation`/`ConnectionAffinityPolicy` 序列化/反序列化往返一致
- [ ] M4-T1.10 验证 `cargo check -p sz-orm-core --features connection-level-tenant` 编译通过

**验收标准**：feature gate 定义；配置/隔离机制/亲和策略定义完整；复用既有 `DbType`；链式配置方法正确

**依赖**：M0-T2

## M4-T2：ConnectionTenantBinder 连接租户绑定器

**任务描述**：实现 `ConnectionTenantBinder` 连接租户绑定器，`acquire_with_tenant` 获取绑定到指定租户的连接，复用既有 `Pool::acquire`，通过 `SET app.tenant_id = ?` 参数化绑定设置租户上下文。

**涉及文件**：
- `packages/sz-orm-core/src/connection_tenant.rs`（扩展：连接租户绑定器核心）

**复用标注**：既有 `Pool::acquire` `packages/sz-orm-core/src/pool.rs:1268`；既有 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82`（`SET app.tenant_id = ?` 参数化绑定）；既有 `PooledConnection` `packages/sz-orm-core/src/pool.rs:239`

**子任务**：
- [ ] M4-T2.1 定义 `pub struct ConnectionTenantBinder { pool: Arc<Pool>, config: ConnectionLevelTenantConfig, tenant_bindings: RwLock<HashMap<String, Vec<ConnectionId>>> }`（绑定器，持有连接池 + 配置 + 租户绑定映射）
- [ ] M4-T2.2 实现 `impl ConnectionTenantBinder { pub fn new(pool: Arc<Pool>, config: ConnectionLevelTenantConfig) -> Self }`
- [ ] M4-T2.3 实现 `pub async fn acquire_with_tenant(&self, tenant_id: &str) -> Result<TenantConnectionGuard, TenantError>`：获取绑定到指定租户的连接
- [ ] M4-T2.4 实现 `async fn find_bound_connection(&self, tenant_id: &str) -> Option<PooledConnection>`：查找已绑定到该租户的连接（亲和性）
- [ ] M4-T2.5 实现 `async fn set_tenant_context(&self, conn: &mut PooledConnection, tenant_id: &str) -> Result<(), TenantError>`：通过 `SET app.tenant_id = ?` 参数化绑定设置租户上下文（复用 `Connection::execute_with_params` `pool.rs:82`）
- [ ] M4-T2.6 实现 `async fn clear_tenant_context(&self, conn: &mut PooledConnection) -> Result<(), TenantError>`：`SET app.tenant_id = NULL` 清理租户上下文
- [ ] M4-T2.7 实现 `fn supports_set_tenant_id(&self) -> bool`：按 `config.db_type` 判定是否支持 `SET app.tenant_id`（PostgreSQL/MySQL 支持，SQLite/Oracle/MSSQL 不支持）
- [ ] M4-T2.8 方言降级：`SetTenantId` + 不支持方言 → 降级为 `SchemaIsolation`（复用既有 `SchemaIsolationRouter` `tenant_context.rs:194`），标注"SET app.tenant_id not supported, fallback to schema isolation"
- [ ] M4-T2.9 定义 `pub enum TenantError { NoBoundConnection, TamperingRejected, CleanupFailed, UnsupportedDialect }`（错误类型）
- [ ] M4-T2.10 单元测试：`acquire_with_tenant("tenant_1")` + PostgreSQL → 连接执行 `SET app.tenant_id = 'tenant_1'`，标记绑定
- [ ] M4-T2.11 单元测试：`acquire_with_tenant("tenant_1")` + SQLite → 降级 SchemaIsolation，标注"fallback to schema isolation"
- [ ] M4-T2.12 边界测试：`tenant_id` 为空 → 返回错误

**验收标准**：连接租户绑定器正确；复用既有 `Pool::acquire`；`SET app.tenant_id` 参数化绑定；方言降级处理

**依赖**：M4-T1

## M4-T3：连接亲和性 + 三种亲和策略

**任务描述**：实现三种连接亲和策略（Strict 严格亲和 / Preferred 优先亲和 / None 无亲和），同一租户请求优先复用绑定到该租户的连接。

**涉及文件**：
- `packages/sz-orm-core/src/connection_tenant.rs`（扩展：亲和策略逻辑）

**子任务**：
- [ ] M4-T3.1 策略 Strict：仅使用绑定到该租户的连接，无可用时等待（`tokio::time::timeout(affinity_timeout_ms)`），超时返回 `NoBoundConnection`
- [ ] M4-T3.2 策略 Preferred：优先查找绑定连接，无可用时获取任意连接并重新绑定（`set_tenant_context`）
- [ ] M4-T3.3 策略 None：任意连接，每次设置租户上下文（无亲和性复用）
- [ ] M4-T3.4 亲和性查找：`find_bound_connection` 从 `tenant_bindings` 查找绑定到该租户的连接 ID
- [ ] M4-T3.5 单元测试：Strict + tenant_id 1 请求 → 优先获取绑定到 tenant_id 1 的连接，无可用时等待
- [ ] M4-T3.6 单元测试：Preferred + tenant_id 1 请求 + 有绑定连接 → 获取绑定连接
- [ ] M4-T3.7 单元测试：Preferred + tenant_id 1 请求 + 无绑定连接 → 获取任意连接并重新绑定
- [ ] M4-T3.8 单元测试：None + tenant_id 1 请求 → 获取任意连接，每次设置租户上下文
- [ ] M4-T3.9 边界测试：Strict + 无绑定连接 + 超时 → 返回 `NoBoundConnection` 错误"no connection bound to tenant X, timeout waiting"
- [ ] M4-T3.10 性能测试：连接租户绑定开销 ≤ 0.5ms/次（含 `SET app.tenant_id` 执行 + 亲和性判定）

**验收标准**：三种亲和策略实现正确；减少租户上下文切换开销；性能 ≤ 0.5ms/次

**依赖**：M4-T2

## M4-T4：TenantConnectionGuard RAII 守卫 + 租户上下文清理

**任务描述**：实现 `TenantConnectionGuard` RAII 守卫，Drop 时清理租户上下文 + 归还连接，连接归还时清理失败则销毁连接避免残留。

**涉及文件**：
- `packages/sz-orm-core/src/connection_tenant.rs`（扩展：RAII 守卫）

**复用标注**：既有 `PooledConnection` Drop `packages/sz-orm-core/src/pool.rs:239`（自动归还连接）；既有 `TenantContextGuard` `packages/sz-orm-core/src/tenant_context.rs:166`（RAII 守卫语义复用）

**子任务**：
- [ ] M4-T4.1 定义 `pub struct TenantConnectionGuard { conn: Option<PooledConnection>, binder: Arc<ConnectionTenantBinder>, tenant_id: String }`（RAII 守卫）
- [ ] M4-T4.2 实现 `impl Drop for TenantConnectionGuard`：Drop 时 `clear_tenant_context`（`SET app.tenant_id = NULL`）+ 归还连接到 Pool
- [ ] M4-T4.3 清理失败处理：`clear_tenant_context` 失败 → 销毁该连接（不从池复用），避免租户上下文残留，记录日志"connection destroyed, tenant context cleanup failed"
- [ ] M4-T4.4 实现 `impl TenantConnectionGuard { pub fn connection(&self) -> &PooledConnection }`（获取底层连接引用）
- [ ] M4-T4.5 单元测试：`TenantConnectionGuard` drop → 清理租户上下文（`SET app.tenant_id = NULL`）+ 归还连接
- [ ] M4-T4.6 单元测试：连接归还后 → 下一个请求获取连接时无租户上下文残留
- [ ] M4-T4.7 单元测试：清理失败 → 销毁连接，日志标注"connection destroyed, tenant context cleanup failed"
- [ ] M4-T4.8 连接泄漏验证：guard drop 后，池可用连接数恢复（或销毁的连接不计入可用）

**验收标准**：RAII 守卫正确；Drop 清理租户上下文 + 归还连接；清理失败销毁连接；不泄漏连接

**依赖**：M4-T2

## M4-T5：查询自动注入租户过滤 + 防篡改

**任务描述**：实现查询自动注入 tenant_id 过滤（复用既有 `RowLevelSecurityPolicy`），连接租户绑定防篡改（tenant_id 不可被客户端篡改）。

**涉及文件**：
- `packages/sz-orm-core/src/connection_tenant.rs`（扩展：租户过滤 + 防篡改）

**复用标注**：既有 `RowLevelSecurityPolicy` `packages/sz-orm-core/src/tenant_security.rs:67`（行级安全策略，自动注入 `WHERE tenant_id = ?`）；既有 `TenantContext` `packages/sz-orm-core/src/tenant_context.rs:80`（可信路径设置语义）

**子任务**：
- [ ] M4-T5.1 查询自动注入租户过滤：复用既有 `RowLevelSecurityPolicy` `packages/sz-orm-core/src/tenant_security.rs:67`，追加 `WHERE tenant_id = ?` 参数化绑定
- [ ] M4-T5.2 防篡改：连接绑定的 tenant_id 不可被客户端篡改（由可信路径/中间件设置，复用既有 `TenantContext` `tenant_context.rs:80` 可信路径语义）
- [ ] M4-T5.3 篡改拒绝：客户端尝试篡改 tenant_id → 拒绝篡改，使用可信路径设置的 tenant_id，返回 `TamperingRejected`
- [ ] M4-T5.4 单元测试：tenant_id 1 查询 users 表 → 自动追加 `WHERE tenant_id = 1`，不返回其他租户数据
- [ ] M4-T5.5 单元测试：客户端尝试篡改 tenant_id → 拒绝篡改，返回 `TamperingRejected`
- [ ] M4-T5.6 边界测试：多租户环境下不同租户查询同表 → 各自只返回各自租户数据

**验收标准**：查询自动注入 tenant_id 过滤；防篡改；不泄露跨租户数据

**依赖**：M4-T2、M4-T4

## M4-T6：M4 集成测试与门禁验证

**任务描述**：M4 里程碑集成测试与门禁验证，确保 REQ-V46-006 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M4-T6.1 集成测试：`ConnectionTenantBinder::acquire_with_tenant` 完整流程（亲和查找 → SET 租户上下文 → 查询自动注入过滤 → guard drop 清理 → 归还连接）
- [ ] M4-T6.2 集成测试：三种隔离机制（SetTenantId/SchemaIsolation/ConnectionBinding）完整验证
- [ ] M4-T6.3 集成测试：三种亲和策略（Strict/Preferred/None）完整验证
- [ ] M4-T6.4 集成测试：复用既有 `Pool` `packages/sz-orm-core/src/pool.rs:743` + `RowLevelSecurityPolicy` `packages/sz-orm-core/src/tenant_security.rs:67`，不新建连接池
- [ ] M4-T6.5 集成测试：方言降级（SQLite/Oracle/MSSQL → SchemaIsolation）+ 防篡改 + 连接归还清理
- [ ] M4-T6.6 运行 `cargo test -p sz-orm-core --features connection-level-tenant`（全部通过）
- [ ] M4-T6.7 `cargo clippy -p sz-orm-core --features connection-level-tenant -- -D warnings`
- [ ] M4-T6.8 `cargo fmt -p sz-orm-core -- --check`
- [ ] M4-T6.9 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-core/src/connection_tenant.rs` 无占位实现
- [ ] M4-T6.10 扫描 `grep -rn 'unsafe' packages/sz-orm-core/src/connection_tenant.rs` 无 unsafe 块
- [ ] M4-T6.11 验证默认 feature 行为与 v4.5.0 一致（`cargo build -p sz-orm-core` 无连接级隔离，既有 `TenantPoolRegistry` 每租户独立池仍可用）
- [ ] M4-T6.12 验证 `connection-level-tenant` 与既有 `multi-tenant-enhanced` feature 组合编译通过

**验收标准**：M4 集成测试通过；门禁通过；默认行为不变；三种隔离 + 三种亲和 + 防篡改 + 自动注入过滤 + 连接清理全部验证

**依赖**：M4-T1、M4-T2、M4-T3、M4-T4、M4-T5

---

# 七、M5：进程级 L1 缓存（REQ-V46-007，P1，2 天）

**目标**：扩展既有 `sz-orm-core` L1 缓存，新增 `ProcessL1Cache` 进程级 L1 缓存（跨 Session 共享 Identity Map，线程安全 `Send + Sync`），`CrossSessionIdentityMap` 跨 Session Identity Map，与既有 `L2Cache` 协同（L1→L2→DB 查询协作），缓存一致性（复用既有 `CacheCoherenceProtocol`），LRU 淘汰 + TTL 过期。
**对应需求**：REQ-V46-007（spec.md §5.7，design.md §2.1.3.6 + §2.2.2.7）
**预期工作量**：2 天
**依赖**：无（M5 为 P1 独立需求，复用既有 sz-orm-core l1_cache/l2_cache/cache_coherence，扩展包可与 M1~M4/M6~M7 并行）

## M5-T1：process-l1-cache feature gate + ProcessL1Config 配置

**任务描述**：在 `sz-orm-core` 完善 `process-l1-cache` feature gate 隔离（M0-T2 已创建占位），定义 `ProcessL1Config` 配置 + `ProcessL1Stats` 统计结构。

**涉及文件**：
- `packages/sz-orm-core/Cargo.toml`（完善：`process-l1-cache` feature 依赖既有 `l1-cache` + `cache-coherence`）
- `packages/sz-orm-core/src/lib.rs`（扩展：模块声明 `mod process_l1_cache;`，`#[cfg(feature = "process-l1-cache")]` 门控）
- `packages/sz-orm-core/src/process_l1_cache.rs`（新建，进程级 L1 缓存配置 + 核心）

**复用标注**：既有 `L1Cache` `packages/sz-orm-core/src/l1_cache.rs:87`（Session 级，Identity Map 语义复用）；既有 `L1CacheStats` `packages/sz-orm-core/src/l1_cache.rs:47`；既有 `L2Cache` `packages/sz-orm-core/src/l2_cache.rs:517`；既有 `CacheKey` `packages/sz-orm-core/src/l2_cache.rs:143`；既有 `L2Cache::invalidate_table` `packages/sz-orm-core/src/l2_cache.rs:740`；既有 `CacheCoherenceProtocol` `packages/sz-orm-core/src/cache_coherence.rs:103`；既有 `MesiState` `packages/sz-orm-core/src/cache_coherence.rs:12`；既有 `ConsistencyStrategy` `packages/sz-orm-core/src/cache_coherence.rs:25`

**子任务**：
- [ ] M5-T1.1 `packages/sz-orm-core/Cargo.toml` 完善 `process-l1-cache = ["l1-cache", "cache-coherence"]` feature（依赖既有 `l1-cache` + `cache-coherence`）
- [ ] M5-T1.2 `src/lib.rs` 声明 `mod process_l1_cache;`，`#[cfg(feature = "process-l1-cache")]` 门控
- [ ] M5-T1.3 `src/process_l1_cache.rs` 定义 `pub struct ProcessL1Config { capacity: usize, ttl_ms: u64, enable_coherence: bool, tenant_isolated: bool }`（配置）
- [ ] M5-T1.4 实现 `impl ProcessL1Config { pub fn new() -> Self }`（默认：capacity 10000, ttl 300000ms, enable_coherence true, tenant_isolated true）
- [ ] M5-T1.5 实现链式配置方法：`with_capacity` / `with_ttl_ms` / `with_coherence` / `with_tenant_isolated`
- [ ] M5-T1.6 定义 `pub struct ProcessL1Stats { hits: AtomicU64, misses: AtomicU64, entry_count: AtomicU64, evict_count: AtomicU64 }`（统计，复用既有 `L1CacheStats` 语义）
- [ ] M5-T1.7 定义 `pub struct ProcessL1StatsSnapshot { hits: u64, misses: u64, entry_count: u64, evict_count: u64, hit_rate: f64 }`（统计快照）
- [ ] M5-T1.8 单元测试：`ProcessL1Config::new()` 默认值正确（10000, 300000ms, true, true）
- [ ] M5-T1.9 验证 `cargo check -p sz-orm-core --features process-l1-cache` 编译通过

**验收标准**：feature gate 定义；ProcessL1Config/ProcessL1Stats 定义完整；链式配置方法正确

**依赖**：M0-T2

## M5-T2：ProcessL1Cache 进程级 L1 缓存核心

**任务描述**：实现 `ProcessL1Cache` 进程级 L1 缓存核心，`RwLock` 保护内部数据（线程安全 `Send + Sync`），LRU 淘汰 + TTL 过期，跨 Session 共享 Identity Map。

**涉及文件**：
- `packages/sz-orm-core/src/process_l1_cache.rs`（扩展：进程级 L1 缓存核心）

**复用标注**：既有 `L1Cache` LRU 淘汰语义 `packages/sz-orm-core/src/l1_cache.rs:91`；既有 `CacheKey` `packages/sz-orm-core/src/l2_cache.rs:143`（统一缓存键）

**子任务**：
- [ ] M5-T2.1 定义 `pub struct ProcessL1Cache<T: Clone + Send + Sync + 'static> { inner: RwLock<ProcessL1Inner<T>>, config: ProcessL1Config, l2: Option<Arc<L2Cache>>, coherence: Option<Arc<CacheCoherenceProtocol>> }`（进程级 L1，`RwLock` 线程安全）
- [ ] M5-T2.2 定义 `struct ProcessL1Inner<T> { entries: LinkedHashMap<CacheKey, Arc<CacheEntry<T>>>, stats: ProcessL1Stats }`（内部数据）
- [ ] M5-T2.3 定义 `struct CacheEntry<T> { value: Arc<T>, inserted_at: u64, last_accessed_at: u64 }`（缓存条目）
- [ ] M5-T2.4 实现 `impl<T> ProcessL1Cache<T> { pub fn new(config: ProcessL1Config) -> Self }`
- [ ] M5-T2.5 实现 `pub fn with_l2(mut self, l2: Arc<L2Cache>) -> Self` + `pub fn with_coherence(mut self, coherence: Arc<CacheCoherenceProtocol>) -> Self`
- [ ] M5-T2.6 实现 `pub async fn get(&self, table: &str, pk: &Value) -> Option<Arc<T>>`：`RwLock.read()` 查找 L1，命中返回 `Arc<T>`（Identity Map）
- [ ] M5-T2.7 实现 `pub async fn put(&self, table: &str, pk: Value, value: Arc<T>)`：`RwLock.write()` 写入 L1，超过容量按 LRU 淘汰
- [ ] M5-T2.8 实现 `fn evict_lru(&self, inner: &mut ProcessL1Inner<T>)`：LRU 淘汰最久未使用条目（复用既有 `L1Cache` LRU 语义 `l1_cache.rs:91`）
- [ ] M5-T2.9 实现 `fn check_ttl(&self, entry: &CacheEntry<T>) -> bool`：TTL 过期判定（`now - inserted_at > ttl_ms`）
- [ ] M5-T2.10 实现 `pub fn stats(&self) -> ProcessL1StatsSnapshot`：返回统计快照（含 hit_rate）
- [ ] M5-T2.11 单元测试：`get` 命中 → 返回 `Arc<T>`，hits + 1
- [ ] M5-T2.12 单元测试：`get` 未命中 → 返回 None，misses + 1
- [ ] M5-T2.13 单元测试：容量 10000 + 插入 10001 条 → 淘汰最久未使用条目，evict_count + 1
- [ ] M5-T2.14 单元测试：TTL 5 分钟 + 条目 6 分钟 → 过期失效
- [ ] M5-T2.15 边界测试：`capacity = 0` → 不缓存任何条目
- [ ] M5-T2.16 性能测试：缓存查找开销 ≤ 100ns/次（含 `RwLock` 读锁 + HashMap 查找）

**验收标准**：ProcessL1Cache 线程安全（`Send + Sync`）；LRU 淘汰 + TTL 过期；Identity Map 语义；性能 ≤ 100ns/次

**依赖**：M5-T1

## M5-T3：跨 Session Identity Map + L1→L2→DB 协同

**任务描述**：实现 `CrossSessionIdentityMap` 跨 Session Identity Map，L1→L2→DB 查询协作（L1 命中直接返回，L1 未命中查 L2 回填 L1，L2 未命中查 DB 回填 L1+L2）。

**涉及文件**：
- `packages/sz-orm-core/src/process_l1_cache.rs`（扩展：跨 Session Identity Map + L1→L2→DB 协同）

**复用标注**：既有 `L2Cache` `packages/sz-orm-core/src/l2_cache.rs:517`（L1→L2→DB 协同复用）；既有 `L1Cache` L1→L2→DB 协作语义 `packages/sz-orm-core/src/l1_cache.rs:17`

**子任务**：
- [ ] M5-T3.1 定义 `pub struct CrossSessionIdentityMap<T: Clone + Send + Sync + 'static> { cache: Arc<ProcessL1Cache<T>> }`（跨 Session Identity Map）
- [ ] M5-T3.2 实现 `impl<T> CrossSessionIdentityMap<T> { pub fn new(cache: Arc<ProcessL1Cache<T>>) -> Self }`
- [ ] M5-T3.3 实现 `pub async fn get_or_load<F>(&self, table: &str, pk: &Value, loader: F) -> Result<Arc<T>, CacheError> where F: FnOnce() -> Pin<Box<dyn Future<Output = Result<T, CacheError>> + Send>>`：L1→L2→DB 查询协作
- [ ] M5-T3.4 L1 命中：`ProcessL1Cache::get` 命中 → 直接返回 `Arc<T>`（Identity Map）
- [ ] M5-T3.5 L1 未命中 + L2 命中：`L2Cache::get` 命中 → 回填 L1 → 返回 `Arc<T>`
- [ ] M5-T3.6 L1+L2 未命中：调用 `loader` 闭包查 DB → 回填 L1 + L2 → 返回 `Arc<T>`
- [ ] M5-T3.7 实现 `async fn get_from_l2(&self, key: &CacheKey) -> Option<Arc<T>>` + `async fn put_to_l2(&self, key: CacheKey, value: &Arc<T>)`（L2 协同）
- [ ] M5-T3.8 单元测试：Session A 查询 pk=1 + Session B 查询 pk=1 → 返回相同 `Arc<T>` 引用，`Arc::ptr_eq` 为 true
- [ ] M5-T3.9 单元测试：L1 未命中 + L2 命中 → 回填 L1 返回
- [ ] M5-T3.10 单元测试：L1+L2 未命中 → 调用 loader 查 DB + 回填 L1+L2
- [ ] M5-T3.11 边界测试：loader 返回错误 → 不回填缓存，返回错误

**验收标准**：跨 Session Identity Map 语义（`Arc::ptr_eq` 为 true）；L1→L2→DB 协同正确；复用既有 `L2Cache`

**依赖**：M5-T2

## M5-T4：缓存一致性 + LRU 淘汰 + TTL 过期

**任务描述**：实现缓存一致性（L1 失效时 L2 同步失效，复用既有 `CacheCoherenceProtocol`），`invalidate`/`invalidate_table` 失效方法。

**涉及文件**：
- `packages/sz-orm-core/src/process_l1_cache.rs`（扩展：缓存一致性 + 失效）

**复用标注**：既有 `CacheCoherenceProtocol` `packages/sz-orm-core/src/cache_coherence.rs:103`（缓存一致性协议）；既有 `MesiState` `packages/sz-orm-core/src/cache_coherence.rs:12`（MESI 状态机）；既有 `L2Cache::invalidate_table` `packages/sz-orm-core/src/l2_cache.rs:740`（表级失效）

**子任务**：
- [ ] M5-T4.1 实现 `pub async fn invalidate(&self, table: &str, pk: &Value)`：失效 L1 单条目，`enable_coherence` 时通过 `CacheCoherenceProtocol` 同步失效 L2
- [ ] M5-T4.2 实现 `pub async fn invalidate_table(&self, table: &str)`：失效 L1 整表，复用既有 `L2Cache::invalidate_table` `l2_cache.rs:740` 同步失效 L2
- [ ] M5-T4.3 缓存一致性：L1 失效 pk=1 → L2 同步失效 pk=1，下次查询从 DB 加载
- [ ] M5-T4.4 L1 与 L2 数据不一致检测：通过 `CacheCoherenceProtocol` 同步失效，记录日志"cache inconsistency detected, synchronized"
- [ ] M5-T4.5 单元测试：L1 失效 pk=1 → L2 同步失效 pk=1，下次查询从 DB 加载
- [ ] M5-T4.6 单元测试：`invalidate_table("users")` → L1+L2 users 表全部失效
- [ ] M5-T4.7 单元测试：`enable_coherence = false` → L1 失效不同步 L2
- [ ] M5-T4.8 边界测试：失效不存在的条目 → 不 panic

**验收标准**：缓存一致性正确；复用既有 `CacheCoherenceProtocol`；L1 失效同步 L2 失效

**依赖**：M5-T2

## M5-T5：多租户缓存隔离 + 线程安全验证

**任务描述**：实现多租户缓存隔离（`tenant_isolated` 时 CacheKey 含 tenant_id），验证线程安全（`Send + Sync`，跨线程共享无数据竞争）。

**涉及文件**：
- `packages/sz-orm-core/src/process_l1_cache.rs`（扩展：多租户隔离 + 线程安全）

**子任务**：
- [ ] M5-T5.1 多租户缓存隔离：`tenant_isolated = true` 时 CacheKey 含 tenant_id，不同租户缓存项隔离
- [ ] M5-T5.2 线程安全验证：`ProcessL1Cache` 为 `Send + Sync`（`RwLock` 保护内部数据，跨线程共享无数据竞争）
- [ ] M5-T5.3 单元测试：多线程并发查询 `ProcessL1Cache` → 无数据竞争，线程安全
- [ ] M5-T5.4 单元测试：多租户环境下不同租户查询同主键 → 各自返回各自数据（CacheKey 含 tenant_id）
- [ ] M5-T5.5 单元测试：`tenant_isolated = false` → 不同租户共享缓存项
- [ ] M5-T5.6 边界测试：LRU 淘汰热点数据 → 按 LRU 语义淘汰，下次查询从 L2/DB 加载

**验收标准**：多租户缓存隔离；线程安全（`Send + Sync`）；无数据竞争

**依赖**：M5-T2、M5-T4

## M5-T6：M5 集成测试与门禁验证

**任务描述**：M5 里程碑集成测试与门禁验证，确保 REQ-V46-007 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M5-T6.1 集成测试：`CrossSessionIdentityMap::get_or_load` 完整流程（L1 命中 / L1 未命中 L2 命中 / L1+L2 未命中查 DB + 回填）
- [ ] M5-T6.2 集成测试：跨 Session Identity Map（Session A/B 查询同主键 → `Arc::ptr_eq` 为 true）
- [ ] M5-T6.3 集成测试：缓存一致性（L1 失效同步 L2 失效）+ LRU 淘汰 + TTL 过期
- [ ] M5-T6.4 集成测试：复用既有 `L1Cache` `packages/sz-orm-core/src/l1_cache.rs:87` + `L2Cache` `packages/sz-orm-core/src/l2_cache.rs:517` + `CacheCoherenceProtocol` `packages/sz-orm-core/src/cache_coherence.rs:103`，不新建缓存逻辑
- [ ] M5-T6.5 集成测试：多租户缓存隔离 + 线程安全（多线程并发无数据竞争）
- [ ] M5-T6.6 运行 `cargo test -p sz-orm-core --features process-l1-cache`（全部通过）
- [ ] M5-T6.7 `cargo clippy -p sz-orm-core --features process-l1-cache -- -D warnings`
- [ ] M5-T6.8 `cargo fmt -p sz-orm-core -- --check`
- [ ] M5-T6.9 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-core/src/process_l1_cache.rs` 无占位实现
- [ ] M5-T6.10 扫描 `grep -rn 'unsafe' packages/sz-orm-core/src/process_l1_cache.rs` 无 unsafe 块
- [ ] M5-T6.11 验证默认 feature 行为与 v4.5.0 一致（`cargo build -p sz-orm-core` 无进程级 L1，既有 `L1Cache` Session 级仍可用）
- [ ] M5-T6.12 验证 `process-l1-cache` 与既有 `l1-cache`/`cache-coherence`/`dist-cache` feature 组合编译通过

**验收标准**：M5 集成测试通过；门禁通过；默认行为不变；ProcessL1Cache + 跨 Session Identity Map + L1→L2→DB + 缓存一致性 + 线程安全全部验证

**依赖**：M5-T1、M5-T2、M5-T3、M5-T4、M5-T5

---

# 八、M6：异常检测（REQ-V46-004，P2，1.5 天）

**目标**：扩展既有 `sz-orm-observability` 可观测性，新增 `AnomalyDetector` 异常检测器，`AnomalyAlgorithm` 五种检测算法（Threshold/Trend/Statistical/ZScore/IQR），`AnomalyAlert` 异常告警联动，异常标注，复用既有 `Metrics937` / `SloMonitor` / `QueryLogger`。
**对应需求**：REQ-V46-004（spec.md §5.4，design.md §2.1.3.4 + §2.2.2.4）
**预期工作量**：1.5 天
**依赖**：无（M6 为 P2 独立需求，复用既有 sz-orm-observability MetricsRegistry/SloMonitor/QueryLogger，扩展包可与 M1~M5/M7 并行）

## M6-T1：anomaly-detection feature gate + AnomalyConfig 配置

**任务描述**：在 `sz-orm-observability` 完善 `anomaly-detection` feature gate 隔离（M0-T2 已创建占位），定义 `AnomalyConfig` 配置 + `AnomalyAlgorithm` 算法枚举 + `AlertChannel` 告警通道枚举。

**涉及文件**：
- `packages/sz-orm-observability/Cargo.toml`（完善：`anomaly-detection` feature）
- `packages/sz-orm-observability/src/lib.rs`（扩展：模块声明 `mod anomaly;`，`#[cfg(feature = "anomaly-detection")]` 门控）
- `packages/sz-orm-observability/src/anomaly.rs`（新建，异常检测配置 + 算法 + 告警通道）

**复用标注**：既有 `MetricsRegistry` `packages/sz-orm-observability/src/lib.rs:259`；既有 `MetricKind` `packages/sz-orm-observability/src/lib.rs:75`；既有 `SloMonitor` `packages/sz-orm-observability/src/slo.rs:223`；既有 `SloConfig` `packages/sz-orm-observability/src/slo.rs:52`；既有 `QueryLogger` `packages/sz-orm-observability/src/query_logger.rs:73`；既有 `QueryLogEntry` `packages/sz-orm-observability/src/query_logger.rs:46`

**子任务**：
- [ ] M6-T1.1 `packages/sz-orm-observability/Cargo.toml` 确认 `anomaly-detection = []` feature（M0-T2 已创建）
- [ ] M6-T1.2 `src/lib.rs` 声明 `mod anomaly;`，`#[cfg(feature = "anomaly-detection")]` 门控
- [ ] M6-T1.3 `src/anomaly.rs` 定义 `pub enum AnomalyAlgorithm { Threshold, Trend, Statistical, ZScore, IQR }`（五种算法，`Serialize + Deserialize`）
- [ ] M6-T1.4 定义 `pub enum AlertChannel { Log, Webhook, Slo }`（三种告警通道）
- [ ] M6-T1.5 定义 `pub struct AnomalyConfig { algorithm, window_ms, threshold, zscore_threshold, alert_channel, webhook_url }`（配置）
- [ ] M6-T1.6 实现 `impl AnomalyConfig { pub fn new() -> Self }`（默认：Threshold, 300000ms, 1.0, 3.0, Log）
- [ ] M6-T1.7 实现链式配置方法：`with_algorithm` / `with_window_ms` / `with_threshold` / `with_zscore_threshold` / `with_alert_channel`
- [ ] M6-T1.8 单元测试：`AnomalyConfig::new()` 默认值正确（Threshold, 300000ms, 1.0, 3.0, Log）
- [ ] M6-T1.9 验证 `cargo check -p sz-orm-observability --features anomaly-detection` 编译通过

**验收标准**：feature gate 定义；AnomalyConfig/AnomalyAlgorithm/AlertChannel 定义完整；链式配置方法正确

**依赖**：M0-T2

## M6-T2：AnomalyAlgorithm 五种检测算法

**任务描述**：实现五种异常检测算法（Threshold 阈值 / Trend 趋势 / Statistical 统计 / ZScore Z-Score / IQR 四分位距），每种算法检测指标异常。

**涉及文件**：
- `packages/sz-orm-observability/src/anomaly.rs`（扩展：五种检测算法）

**子任务**：
- [ ] M6-T2.1 定义 `pub struct Anomaly { metric_name: String, anomaly_value: f64, threshold: f64, window_ms: u64, algorithm: AnomalyAlgorithm, detected_at: u64 }`（异常结构）
- [ ] M6-T2.2 实现 `async fn detect_threshold(&self, history: &[f64]) -> Result<Vec<Anomaly>, AnomalyError>`：指标值 > 阈值即异常
- [ ] M6-T2.3 实现 `async fn detect_trend(&self, history: &[f64]) -> Result<Vec<Anomaly>, AnomalyError>`：指标持续上升/下降即异常
- [ ] M6-T2.4 实现 `async fn detect_statistical(&self, history: &[f64]) -> Result<Vec<Anomaly>, AnomalyError>`：基于均值/方差检测异常
- [ ] M6-T2.5 实现 `async fn detect_zscore(&self, history: &[f64]) -> Result<Vec<Anomaly>, AnomalyError>`：Z-Score = (value - mean) / std_dev，Z-Score > zscore_threshold 即异常
- [ ] M6-T2.6 实现 `async fn detect_iqr(&self, history: &[f64]) -> Result<Vec<Anomaly>, AnomalyError>`：IQR = Q3 - Q1，指标值 < Q1-1.5×IQR 或 > Q3+1.5×IQR 即异常
- [ ] M6-T2.7 单元测试：Threshold + 阈值 1 秒 + 指标值 1.5 秒 → 检测到异常"query_duration 1.5s exceeds threshold 1s"
- [ ] M6-T2.8 单元测试：ZScore + Z-Score 3.5 + 阈值 3.0 → 检测到异常"Z-Score 3.5 indicates anomaly"
- [ ] M6-T2.9 单元测试：IQR + 指标值超出 Q3+1.5×IQR → 检测到异常
- [ ] M6-T2.10 边界测试：历史数据不足（< 2 个数据点）→ 返回 `InsufficientData`，跳过检测
- [ ] M6-T2.11 边界测试：算法计算异常（除零/溢出）→ 返回 `CalculationError`，跳过该次检测
- [ ] M6-T2.12 性能测试：异常检测算法执行开销 ≤ 1ms/指标/次

**验收标准**：五种算法实现正确；边界处理不 panic；性能 ≤ 1ms/指标/次

**依赖**：M6-T1

## M6-T3：AnomalyDetector 异常检测器 + 告警联动

**任务描述**：实现 `AnomalyDetector` 异常检测器，`detect` 入口按 `AnomalyAlgorithm` 检测指标异常，`trigger_alert` 触发 `AnomalyAlert` 告警，复用既有 `MetricsRegistry` 历史数据。

**涉及文件**：
- `packages/sz-orm-observability/src/anomaly.rs`（扩展：异常检测器 + 告警联动）

**复用标注**：既有 `MetricsRegistry` `packages/sz-orm-observability/src/lib.rs:259`（指标历史数据复用）；既有 `SloMonitor` `packages/sz-orm-observability/src/slo.rs:223`（SLO 告警通道复用）

**子任务**：
- [ ] M6-T3.1 定义 `pub struct AnomalyDetector { metrics: Arc<MetricsRegistry>, config: AnomalyConfig, query_logger: Option<Arc<QueryLogger>> }`（异常检测器，复用既有 `MetricsRegistry`）
- [ ] M6-T3.2 实现 `impl AnomalyDetector { pub fn new(metrics: Arc<MetricsRegistry>, config: AnomalyConfig) -> Self }`
- [ ] M6-T3.3 实现 `pub fn with_query_logger(mut self, logger: Arc<QueryLogger>) -> Self`
- [ ] M6-T3.4 实现 `pub async fn detect(&self, metric_name: &str) -> Result<Vec<Anomaly>, AnomalyError>`：从 `MetricsRegistry` 获取历史数据 → 按算法检测 → 触发告警
- [ ] M6-T3.5 实现 `async fn trigger_alert(&self, anomaly: &Anomaly) -> Result<(), AnomalyError>`：触发 `AnomalyAlert` 告警
- [ ] M6-T3.6 定义 `pub struct AnomalyAlert { anomaly: Anomaly, channel: AlertChannel }` + `impl AnomalyAlert { pub async fn send(&self) -> Result<(), AnomalyError> }`
- [ ] M6-T3.7 告警通道分发：Log → 记日志；Webhook → 发送 HTTP POST；Slo → 复用既有 `SloMonitor` 告警通道
- [ ] M6-T3.8 单元测试：指标 "query_duration" 历史数据 + 异常检测 → `AnomalyDetector` 检测异常 + 触发告警
- [ ] M6-T3.9 单元测试：检测到异常 → 触发 `AnomalyAlert`，含指标名 + 异常值 + 阈值 + 时间窗口 + 算法
- [ ] M6-T3.10 边界测试：告警通道不可用（Webhook 连接失败）→ 记录告警到日志，标注"alert channel unavailable, logged only"

**验收标准**：异常检测器正确；复用既有 `MetricsRegistry`；告警联动；告警通道分发

**依赖**：M6-T1、M6-T2

## M6-T4：异常标注 + 异常处理

**任务描述**：实现异常标注（在查询日志上标注异常标记，复用既有 `QueryLogger`），处理三种异常场景。

**涉及文件**：
- `packages/sz-orm-observability/src/anomaly.rs`（扩展：异常标注 + 异常处理）

**复用标注**：既有 `QueryLogger` `packages/sz-orm-observability/src/query_logger.rs:73`（异常标注复用）；既有 `QueryLogEntry` `packages/sz-orm-observability/src/query_logger.rs:46`

**子任务**：
- [ ] M6-T4.1 异常标注：检测到异常 → 在 `QueryLogger` 查询日志上标注"anomaly detected: ..."（复用既有 `QueryLogger` `query_logger.rs:73`）
- [ ] M6-T4.2 异常处理 1：指标历史数据不足 → 跳过检测，记录日志"insufficient history data for metric X"
- [ ] M6-T4.3 异常处理 2：算法计算异常（除零/溢出）→ 跳过该次检测，记录日志"anomaly algorithm calculation error: ..."
- [ ] M6-T4.4 异常处理 3：告警通道不可用 → 记录告警到日志，标注"alert channel unavailable, anomaly logged only"
- [ ] M6-T4.5 单元测试：检测到异常 → 在查询日志上标注"anomaly detected: ..."
- [ ] M6-T4.6 单元测试：历史数据不足 → 跳过检测，日志标注"anomaly detection skipped, insufficient data"
- [ ] M6-T4.7 边界测试：`query_logger = None` → 异常标注跳过，不 panic

**验收标准**：异常标注正确；复用既有 `QueryLogger`；三种异常场景处理正确

**依赖**：M6-T3

## M6-T5：M6 集成测试与门禁验证

**任务描述**：M6 里程碑集成测试与门禁验证，确保 REQ-V46-004 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M6-T5.1 集成测试：`AnomalyDetector::detect` 完整流程（获取历史数据 → 算法检测 → 触发告警 → 异常标注）
- [ ] M6-T5.2 集成测试：五种算法（Threshold/Trend/Statistical/ZScore/IQR）完整验证
- [ ] M6-T5.3 集成测试：复用既有 `MetricsRegistry` `packages/sz-orm-observability/src/lib.rs:259` + `QueryLogger` `packages/sz-orm-observability/src/query_logger.rs:73`，不新建指标采集
- [ ] M6-T5.4 集成测试：三种告警通道（Log/Webhook/Slo）+ 异常标注 + 异常处理
- [ ] M6-T5.5 运行 `cargo test -p sz-orm-observability --features anomaly-detection`（全部通过）
- [ ] M6-T5.6 `cargo clippy -p sz-orm-observability --features anomaly-detection -- -D warnings`
- [ ] M6-T5.7 `cargo fmt -p sz-orm-observability -- --check`
- [ ] M6-T5.8 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-observability/src/anomaly.rs` 无占位实现
- [ ] M6-T5.9 扫描 `grep -rn 'unsafe' packages/sz-orm-observability/src/anomaly.rs` 无 unsafe 块
- [ ] M6-T5.10 验证默认 feature 行为与 v4.5.0 一致（`cargo build -p sz-orm-observability` 无异常检测器）
- [ ] M6-T5.11 验证 `anomaly-detection` 与既有 `query-logging`/`service-mesh` feature 组合编译通过

**验收标准**：M6 集成测试通过；门禁通过；默认行为不变；五种算法 + 告警联动 + 异常标注全部验证

**依赖**：M6-T1、M6-T2、M6-T3、M6-T4

---

# 九、M7：存储成本分析与优化建议（REQ-V46-005，P2，2 天）

**目标**：扩展既有 `sz-orm-storage` 存储，新增 `CostAnalyzer` 成本分析器，按 provider/bucket/tier 统计存储成本（容量+请求+流量），`CostOptimizationSuggestion` 四种优化建议，`CostReport` 成本报表周期性生成，复用既有 `Storage`/`StorageProvider`/`BucketLifecycle`/`LifecycleRule`。
**对应需求**：REQ-V46-005（spec.md §5.5，design.md §2.2.2.5）
**预期工作量**：2 天
**依赖**：无（M7 为 P2 独立需求，复用既有 sz-orm-storage Storage/BucketLifecycle，扩展包可与 M1~M6 并行）

## M7-T1：cost-analysis feature gate + CostConfig 配置

**任务描述**：在 `sz-orm-storage` 完善 `cost-analysis` feature gate 隔离（M0-T2 已创建占位），定义 `CostConfig` 配置 + `ReportFormat` 报表格式枚举 + `CostOptimizationSuggestion` 优化建议枚举。

**涉及文件**：
- `packages/sz-orm-storage/Cargo.toml`（完善：`cost-analysis` feature）
- `packages/sz-orm-storage/src/lib.rs`（扩展：模块声明 `mod cost;`，`#[cfg(feature = "cost-analysis")]` 门控）
- `packages/sz-orm-storage/src/cost.rs`（新建，成本分析配置 + 报表格式 + 优化建议）

**复用标注**：既有 `Storage` trait `packages/sz-orm-storage/src/storage.rs:14`；既有 `StorageBuilder` `packages/sz-orm-storage/src/storage.rs:22`；既有 `StorageProvider` `packages/sz-orm-storage/src/storage.rs:287`；既有 `BucketLifecycle` `packages/sz-orm-storage/src/advanced.rs:438`；既有 `LifecycleRule` `packages/sz-orm-storage/src/advanced.rs:400`；既有 `LifecycleAction` `packages/sz-orm-storage/src/advanced.rs:378`

**子任务**：
- [ ] M7-T1.1 `packages/sz-orm-storage/Cargo.toml` 确认 `cost-analysis = []` feature（M0-T2 已创建）
- [ ] M7-T1.2 `src/lib.rs` 声明 `mod cost;`，`#[cfg(feature = "cost-analysis")]` 门控
- [ ] M7-T1.3 `src/cost.rs` 定义 `pub enum ReportFormat { Json, Csv }`（报表格式）
- [ ] M7-T1.4 定义 `pub enum CostOptimizationSuggestion { TierDowngrade { bucket, from_tier, to_tier, expected_saving_percent }, LifecycleOptimize { bucket, rule }, DeleteExpired { bucket, expired_count }, CompressCold { bucket, cold_data_size_gb } }`（四种建议，复用既有 `LifecycleRule`）
- [ ] M7-T1.5 定义 `pub struct CostConfig { analysis_interval_ms, suggestion_types, report_format, providers }`（配置，复用既有 `StorageProvider`）
- [ ] M7-T1.6 实现 `impl CostConfig { pub fn new() -> Self }`（默认：86400000ms 每日, 全部建议类型, Json, 全部 provider）
- [ ] M7-T1.7 实现链式配置方法：`with_analysis_interval_ms` / `with_suggestion_types` / `with_report_format`
- [ ] M7-T1.8 单元测试：`CostConfig::new()` 默认值正确（86400000ms, Json, 全部 provider）
- [ ] M7-T1.9 验证 `cargo check -p sz-orm-storage --features cost-analysis` 编译通过

**验收标准**：feature gate 定义；CostConfig/ReportFormat/CostOptimizationSuggestion 定义完整；复用既有 `StorageProvider`/`LifecycleRule`

**依赖**：M0-T2

## M7-T2：CostAnalyzer 成本分析器 + ProviderCost/BucketCost

**任务描述**：实现 `CostAnalyzer` 成本分析器，`analyze` 按 provider/bucket/tier 统计存储成本，复用既有 `Storage` 获取存储用量。

**涉及文件**：
- `packages/sz-orm-storage/src/cost.rs`（扩展：成本分析器核心）

**复用标注**：既有 `Storage` trait `packages/sz-orm-storage/src/storage.rs:14`（获取存储用量）；既有 `BucketLifecycle` `packages/sz-orm-storage/src/advanced.rs:438`（生命周期数据）

**子任务**：
- [ ] M7-T2.1 定义 `pub struct CostAnalyzer { storage: Arc<dyn Storage>, config: CostConfig }`（成本分析器，复用既有 `Storage`）
- [ ] M7-T2.2 实现 `impl CostAnalyzer { pub fn new(storage: Arc<dyn Storage>, config: CostConfig) -> Self }`
- [ ] M7-T2.3 实现 `pub async fn analyze(&self) -> Result<CostReport, CostError>`：遍历 provider → 分析每 provider 成本 → 汇总
- [ ] M7-T2.4 实现 `async fn analyze_provider(&self, provider: StorageProvider) -> Result<ProviderCost, CostError>`：按 provider 统计成本
- [ ] M7-T2.5 定义 `pub struct CostReport { generated_at: u64, provider_costs: Vec<ProviderCost>, total_cost: f64, suggestions: Vec<CostOptimizationSuggestion> }`（成本报表）
- [ ] M7-T2.6 定义 `pub struct ProviderCost { provider: StorageProvider, bucket_costs: Vec<BucketCost>, total_cost: f64 }`（provider 成本）
- [ ] M7-T2.7 定义 `pub struct BucketCost { bucket: String, tier: String, capacity_cost: f64, request_cost: f64, traffic_cost: f64, total_cost: f64 }`（bucket 成本，容量+请求+流量）
- [ ] M7-T2.8 成本数据准确：使用 provider API 返回的计费数据（非估算），不估算不编造
- [ ] M7-T2.9 单元测试：`CostAnalyzer::analyze` 7 provider → 生成 `CostReport`，含每 provider/bucket/tier 的容量+请求+流量成本
- [ ] M7-T2.10 边界测试：provider API 不可用 → 跳过该 provider，记录日志"provider X API unavailable, skipped"
- [ ] M7-T2.11 边界测试：成本数据异常（负数/超大值）→ 标注异常数据，记录日志"abnormal cost data from provider X"

**验收标准**：成本分析器正确；复用既有 `Storage`；成本数据准确（不估算）；异常处理

**依赖**：M7-T1

## M7-T3：CostOptimizationSuggestion 成本优化建议

**任务描述**：实现 `suggest_optimization` 基于成本分析结果生成四种优化建议（TierDowngrade/LifecycleOptimize/DeleteExpired/CompressCold），附预期节省成本。

**涉及文件**：
- `packages/sz-orm-storage/src/cost.rs`（扩展：优化建议生成）

**复用标注**：既有 `BucketLifecycle` `packages/sz-orm-storage/src/advanced.rs:438`（生命周期规则优化复用）；既有 `LifecycleRule` `packages/sz-orm-storage/src/advanced.rs:400`

**子任务**：
- [ ] M7-T3.1 实现 `pub async fn suggest_optimization(&self, report: &CostReport) -> Result<Vec<CostOptimizationSuggestion>, CostError>`
- [ ] M7-T3.2 建议 TierDowngrade：冷数据在 Standard tier → 建议降级到 Infrequent Access/Archive tier，附预期节省百分比
- [ ] M7-T3.3 建议 LifecycleOptimize：优化 `LifecycleRule` `packages/sz-orm-storage/src/advanced.rs:400` 规则（如调整过期转换规则）
- [ ] M7-T3.4 建议 DeleteExpired：超过保留期的数据 → 建议删除，附 expired_count
- [ ] M7-T3.5 建议 CompressCold：冷数据 → 建议压缩存储，附 cold_data_size_gb
- [ ] M7-T3.6 单元测试：冷数据 100GB 在 Standard tier → 生成 TierDowngrade 建议，降级到 Infrequent Access tier，预期节省 60%
- [ ] M7-T3.7 单元测试：超过保留期数据 → 生成 DeleteExpired 建议
- [ ] M7-T3.8 边界测试：已是最低 tier 无法降级 → 跳过 TierDowngrade 建议，记录日志"suggestion not applicable for bucket Y"
- [ ] M7-T3.9 性能测试：优化建议生成开销 ≤ 100ms

**验收标准**：四种优化建议实现正确；附预期节省成本；不适用建议跳过

**依赖**：M7-T2

## M7-T4：CostReport 成本报表 + 周期性生成

**任务描述**：实现 `generate_report` 生成成本报表（JSON/CSV 格式），支持周期性生成。

**涉及文件**：
- `packages/sz-orm-storage/src/cost.rs`（扩展：报表生成）

**子任务**：
- [ ] M7-T4.1 实现 `pub async fn generate_report(&self, report: &CostReport) -> Result<String, CostError>`：按 `config.report_format` 生成报表
- [ ] M7-T4.2 报表格式 Json → 序列化 `CostReport` 为 JSON 字符串
- [ ] M7-T4.3 报表格式 Csv → 生成 CSV 格式报表（每行一个 bucket 成本）
- [ ] M7-T4.4 周期性生成：`config.analysis_interval_ms` 控制分析周期（默认每日一次）
- [ ] M7-T4.5 单元测试：`generate_report` + Json → 生成 JSON 成本报表
- [ ] M7-T4.6 单元测试：`generate_report` + Csv → 生成 CSV 成本报表
- [ ] M7-T4.7 单元测试：配置周期每日 + 格式 Json → 每日生成 JSON 成本报表
- [ ] M7-T4.8 边界测试：空 provider 列表 → 生成空报表，不 panic

**验收标准**：成本报表生成正确；JSON/CSV 格式支持；周期性生成

**依赖**：M7-T2、M7-T3

## M7-T5：M7 集成测试与门禁验证

**任务描述**：M7 里程碑集成测试与门禁验证，确保 REQ-V46-005 全部验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M7-T5.1 集成测试：`CostAnalyzer::analyze` 完整流程（遍历 provider → 统计成本 → 生成建议 → 生成报表）
- [ ] M7-T5.2 集成测试：四种优化建议（TierDowngrade/LifecycleOptimize/DeleteExpired/CompressCold）完整验证
- [ ] M7-T5.3 集成测试：复用既有 `Storage` `packages/sz-orm-storage/src/storage.rs:14` + `BucketLifecycle` `packages/sz-orm-storage/src/advanced.rs:438`，不新建存储操作
- [ ] M7-T5.4 集成测试：JSON/CSV 报表格式 + 周期性生成 + 成本数据准确
- [ ] M7-T5.5 运行 `cargo test -p sz-orm-storage --features cost-analysis`（全部通过）
- [ ] M7-T5.6 `cargo clippy -p sz-orm-storage --features cost-analysis -- -D warnings`
- [ ] M7-T5.7 `cargo fmt -p sz-orm-storage -- --check`
- [ ] M7-T5.8 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-storage/src/cost.rs` 无占位实现
- [ ] M7-T5.9 扫描 `grep -rn 'unsafe' packages/sz-orm-storage/src/cost.rs` 无 unsafe 块
- [ ] M7-T5.10 验证默认 feature 行为与 v4.5.0 一致（`cargo build -p sz-orm-storage` 无成本分析器）
- [ ] M7-T5.11 验证 `cost-analysis` 与既有 `storage-lifecycle`/`real-cloud` feature 组合编译通过

**验收标准**：M7 集成测试通过；门禁通过；默认行为不变；成本分析 + 四种建议 + 报表生成全部验证

**依赖**：M7-T1、M7-T2、M7-T3、M7-T4

---

# 十、M8：集成验证与文档同步（P0，0.5 天）

**目标**：M1~M7 全部完成后，进行 workspace 全量集成测试、feature 全组合编译验证、文档同步与版本号更新，确保 v4.6.0 整体交付质量。
**对应需求**：全局（集成验证与文档同步，非功能需求）
**预期工作量**：0.5 天
**依赖**：M0、M1、M2、M3、M4、M5、M6、M7 全部完成

## M8-T1：workspace 全量集成测试

**任务描述**：运行 workspace 全量测试 + 14 道门禁全量验证，确保 v4.6.0 七项需求集成后整体通过，v4.5.0 测试基线不回退。

**涉及文件**：`Cargo.toml`（workspace 全量）、`packages/sz-orm-queue/`、`packages/sz-orm-core/`、`packages/sz-orm-batch/`、`packages/sz-orm-observability/`、`packages/sz-orm-storage/`

**子任务**：
- [ ] M8-T1.1 运行 `cargo test --workspace -j 2 --no-fail-fast`（全量测试通过，v4.5.0 基线不回退）
- [ ] M8-T1.2 运行 7 项需求 feature 测试：`cargo test -p sz-orm-queue --features dlx-auto-redelivery` + `cargo test -p sz-orm-core --features zero-downtime-rollback` + `cargo test -p sz-orm-batch --features batch-atomic` + `cargo test -p sz-orm-core --features connection-level-tenant` + `cargo test -p sz-orm-core --features process-l1-cache` + `cargo test -p sz-orm-observability --features anomaly-detection` + `cargo test -p sz-orm-storage --features cost-analysis`
- [ ] M8-T1.3 门禁 1：`cargo fmt --all -- --check`（fmt 格式检查）
- [ ] M8-T1.4 门禁 2：`cargo check --workspace --all-targets`（编译检查）
- [ ] M8-T1.5 门禁 3：`cargo clippy --workspace --all-targets -- -D warnings`（clippy 静态分析）
- [ ] M8-T1.6 门禁 4：`cargo test --workspace`（单元/集成测试）
- [ ] M8-T1.7 门禁 5：`cargo doc --workspace --no-deps --all-features`（文档构建）
- [ ] M8-T1.8 门禁 8：扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs' packages/sz-orm-queue/src/dlx.rs packages/sz-orm-core/src/rollback_zero_downtime.rs packages/sz-orm-core/src/connection_tenant.rs packages/sz-orm-core/src/process_l1_cache.rs packages/sz-orm-batch/src/atomic.rs packages/sz-orm-observability/src/anomaly.rs packages/sz-orm-storage/src/cost.rs` 无占位实现
- [ ] M8-T1.9 门禁 10：`cargo check --workspace --all-targets --all-features`（feature 全组合编译）
- [ ] M8-T1.10 验证 v4.5.0 测试基线不回退（v4.6.0 测试数 ≥ v4.5.0 测试数）

**验收标准**：workspace 全量测试通过；14 道门禁全通过；v4.5.0 测试基线不回退；7 项需求 feature 测试通过

**依赖**：M1-T6、M2-T6、M3-T6、M4-T6、M5-T6、M6-T5、M7-T5

## M8-T2：feature 全组合编译验证

**任务描述**：验证 v4.6.0 7 个新 feature 与既有 feature（v4.3.0 7 个 + v4.4.0 6 个 + v4.5.0 3 个）任意组合编译通过，确保无 feature 冲突。

**涉及文件**：`packages/sz-orm-queue/Cargo.toml`、`packages/sz-orm-core/Cargo.toml`、`packages/sz-orm-batch/Cargo.toml`、`packages/sz-orm-observability/Cargo.toml`、`packages/sz-orm-storage/Cargo.toml`

**子任务**：
- [ ] M8-T2.1 验证默认（无 feature）编译通过，行为与 v4.5.0 一致：`cargo build --workspace`
- [ ] M8-T2.2 验证 7 个新 feature 单独编译通过（7 条 `cargo build -p <package> --features <feature>` 命令）
- [ ] M8-T2.3 验证 v4.6.0 7 feature 全组合编译：`cargo build --features sz-orm-queue/dlx-auto-redelivery,sz-orm-core/zero-downtime-rollback,sz-orm-batch/batch-atomic,sz-orm-observability/anomaly-detection,sz-orm-storage/cost-analysis,sz-orm-core/connection-level-tenant,sz-orm-core/process-l1-cache`
- [ ] M8-T2.4 验证 v4.6.0 + v4.5.0 feature 组合编译通过
- [ ] M8-T2.5 验证 v4.6.0 + v4.4.0 feature 组合编译通过
- [ ] M8-T2.6 验证 v4.6.0 + v4.3.0 feature 组合编译通过
- [ ] M8-T2.7 验证 `batch-atomic` + 既有 `batch-v2`/`batch-stream` 三 feature 组合编译通过
- [ ] M8-T2.8 验证 `connection-level-tenant` + 既有 `multi-tenant-enhanced` 组合编译通过
- [ ] M8-T2.9 验证 `process-l1-cache` + 既有 `l1-cache`/`cache-coherence`/`dist-cache` 组合编译通过
- [ ] M8-T2.10 验证全 feature 组合编译：`cargo build --workspace --all-features`

**验收标准**：所有 feature 组合编译通过；无 feature 冲突；默认行为与 v4.5.0 一致

**依赖**：M8-T1

## M8-T3：文档同步与版本号更新

**任务描述**：更新 v4.6.0 相关文档（AGENTS.md feature gate 列 + 版本号）、运行文档一致性门禁，确保文档与代码同步。

**涉及文件**：`AGENTS.md`（版本 4.5.0 → 4.6.0，新增 7 feature）、`docs/spec/v4.6.0/tasks.md`（本文件，标记任务完成）

**子任务**：
- [ ] M8-T3.1 更新 `AGENTS.md` 版本号 4.5.0 → 4.6.0，新增 7 feature（dlx-auto-redelivery/zero-downtime-rollback/batch-atomic/anomaly-detection/cost-analysis/connection-level-tenant/process-l1-cache）
- [ ] M8-T3.2 更新 `AGENTS.md` v4.6.0 新增能力说明（可靠性 + 运维智能化层）
- [ ] M8-T3.3 运行 `python scripts/check-doc-consistency.py`（门禁 12，文档与代码一致）
- [ ] M8-T3.4 运行 `python scripts/check-doc-sync.py --diff HEAD`（门禁 14，文档与 HEAD 同步）
- [ ] M8-T3.5 确认 `docs/spec/v4.6.0/tasks.md` 46 任务 / 264 子任务全部标记 `[x]`
- [ ] M8-T3.6 运行 `bash scripts/audit-verify.sh docs/spec/v4.6.0/tasks.md`（门禁 13），验证所有 file:line 引用真实存在

**验收标准**：AGENTS.md 更新（版本 4.6.0 + 7 feature）；文档一致性门禁通过；tasks.md 全部完成；审计证据验证通过

**依赖**：M8-T2

---

# 十一、任务依赖关系图

```
M0（P0，文档基线，立即）
  ├─ M0-T1（v4.5.0 基线锁定）
  ├─ M0-T2（v4.6.0 环境准备，7 feature gate + 版本号）
  └─ M0-T3（基线验证）

M1（P1，DLX 自动重投递，独立扩展 sz-orm-queue，M0-T2 后启动）
  ├─ M1-T1（dlx-auto-redelivery feature + DlxConfig）← M0-T2
  ├─ M1-T2（DlxEntry + RedeliveryOutcome）← M1-T1
  ├─ M1-T3（RedeliveryScheduler 调度器）← M1-T1, M1-T2
  ├─ M1-T4（DLX 路由执行 + 四种策略）← M1-T3
  ├─ M1-T5（重投递日志 + 异常处理）← M1-T3, M1-T4
  └─ M1-T6（集成测试与门禁）← M1-T1~M1-T5

M2（P1，零停机回滚，独立扩展 sz-orm-core migration，M0-T2 后启动）
  ├─ M2-T1（zero-downtime-rollback feature + Config）← M0-T2
  ├─ M2-T2（HealthCheck 健康检查）← M2-T1
  ├─ M2-T3（RollbackExecutor + 三种策略）← M2-T1
  ├─ M2-T4（AutoRollbackTrigger 自动触发）← M2-T2, M2-T3
  ├─ M2-T5（回滚日志 + 一致性校验）← M2-T3, M2-T4
  └─ M2-T6（集成测试与门禁）← M2-T1~M2-T5

M3（P1，批量事务原子性，独立扩展 sz-orm-batch + sz-orm-dtx，M0-T2 后启动）
  ├─ M3-T1（batch-atomic feature + AtomicityGuarantee）← M0-T2
  ├─ M3-T2（BatchTransactionCoordinator 协调器）← M3-T1
  ├─ M3-T3（AllOrNothing 2PC + BestEffort）← M3-T2
  ├─ M3-T4（SagaCompensation + SagaCompensator）← M3-T2
  ├─ M3-T5（原子提交日志 + 异常处理）← M3-T3, M3-T4
  └─ M3-T6（集成测试与门禁）← M3-T1~M3-T5

M4（P1，连接级多租户，独立扩展 sz-orm-core pool/tenant，M0-T2 后启动）
  ├─ M4-T1（connection-level-tenant feature + Config）← M0-T2
  ├─ M4-T2（ConnectionTenantBinder 绑定器）← M4-T1
  ├─ M4-T3（连接亲和性 + 三种策略）← M4-T2
  ├─ M4-T4（TenantConnectionGuard RAII 守卫）← M4-T2
  ├─ M4-T5（查询自动注入过滤 + 防篡改）← M4-T2, M4-T4
  └─ M4-T6（集成测试与门禁）← M4-T1~M4-T5

M5（P1，进程级 L1 缓存，独立扩展 sz-orm-core cache，M0-T2 后启动）
  ├─ M5-T1（process-l1-cache feature + Config）← M0-T2
  ├─ M5-T2（ProcessL1Cache 核心）← M5-T1
  ├─ M5-T3（跨 Session Identity Map + L1→L2→DB）← M5-T2
  ├─ M5-T4（缓存一致性 + LRU + TTL）← M5-T2
  ├─ M5-T5（多租户隔离 + 线程安全）← M5-T2, M5-T4
  └─ M5-T6（集成测试与门禁）← M5-T1~M5-T5

M6（P2，异常检测，独立扩展 sz-orm-observability，M0-T2 后启动）
  ├─ M6-T1（anomaly-detection feature + Config）← M0-T2
  ├─ M6-T2（五种检测算法）← M6-T1
  ├─ M6-T3（AnomalyDetector + 告警联动）← M6-T1, M6-T2
  ├─ M6-T4（异常标注 + 异常处理）← M6-T3
  └─ M6-T5（集成测试与门禁）← M6-T1~M6-T4

M7（P2，存储成本分析，独立扩展 sz-orm-storage，M0-T2 后启动）
  ├─ M7-T1（cost-analysis feature + Config）← M0-T2
  ├─ M7-T2（CostAnalyzer + ProviderCost/BucketCost）← M7-T1
  ├─ M7-T3（CostOptimizationSuggestion 优化建议）← M7-T2
  ├─ M7-T4（CostReport 报表 + 周期生成）← M7-T2, M7-T3
  └─ M7-T5（集成测试与门禁）← M7-T1~M7-T4

M8（P0，集成验证与文档同步，M1~M7 全部完成后）
  ├─ M8-T1（workspace 全量集成测试）← M1-T6, M2-T6, M3-T6, M4-T6, M5-T6, M6-T5, M7-T5
  ├─ M8-T2（feature 全组合编译验证）← M8-T1
  └─ M8-T3（文档同步与版本号更新）← M8-T2
```

> **并行开发说明**：M1~M7 七项需求主体相互独立（design.md §7.2），可并行开发。M0 完成后，M1~M7 同时启动；M1~M7 全部完成后，M8 启动。七项需求无强依赖，可并行推进。

---

# 十二、风险与缓解措施

| 风险 ID | 风险描述 | 影响 | 概率 | 缓解措施 | 责任任务 |
|---------|---------|------|------|---------|---------|
| R-001 | DLX 重投递导致消息风暴（消费者跟不上） | 高 | 中 | `max_redelivery_count` 上限（默认 10）+ 指数退避 + 死信队列兜底 | M1-T3 |
| R-002 | DLX 重投递消息丢失（重投递时消息已被消费） | 高 | 低 | 重投递前检查消息存在性，不存在则跳过并记录日志 | M1-T5 |
| R-003 | 零停机回滚期间新旧版本连接混用导致数据不一致 | 高 | 中 | 连接池热切换原子性保证 + 版本号校验 + ShadowTable 数据一致性校验 | M2-T3 |
| R-004 | 健康检查抖动误触发回滚 | 中 | 中 | 连续失败 N 次（默认 3）才触发回滚，单次抖动不触发 | M2-T2 |
| R-005 | 回滚窗口超时后自动回滚导致数据丢失 | 高 | 低 | 回滚窗口保护（默认 5 分钟），超时拒绝自动回滚 | M2-T4 |
| R-006 | 批量原子提交大事务导致 DB 锁竞争 | 中 | 中 | 批量大小上限（默认 1000）+ 分批提交 + Saga 补偿事务超时回滚 | M3-T3 |
| R-007 | Saga 补偿失败导致数据不一致 | 高 | 低 | 补偿失败记录日志供人工干预，不静默忽略 | M3-T4 |
| R-008 | 2PC 协调器失败导致不一致状态 | 高 | 低 | 按 2PC 协议处理（prepare 失败 rollback，commit 失败记录待提交事务） | M3-T3 |
| R-009 | 异常检测误报导致告警风暴 | 中 | 中 | 告警去重（同 key 5 分钟内只告警一次）+ 告警阈值可配置 + 告警冷却期 | M6-T3 |
| R-010 | 异常检测算法计算异常（除零/溢出） | 低 | 中 | 算法计算异常时跳过该次检测，记录日志 | M6-T2 |
| R-011 | 成本分析扫描大量对象导致 S3 API 调用费用激增 | 中 | 中 | 分页扫描（默认 1000 对象/页）+ 缓存扫描结果 + 增量扫描 | M7-T2 |
| R-012 | 成本数据异常（负数/超大值） | 低 | 低 | 标注异常数据，记录日志"abnormal cost data" | M7-T2 |
| R-013 | 连接级租户隔离导致连接池利用率下降 | 中 | 中 | 连接亲和性策略可配置（Strict/Preferred/None）+ 连接池监控指标 | M4-T3 |
| R-014 | 连接归还时租户上下文清理失败导致残留 | 高 | 低 | 清理失败时销毁连接（不从池复用），避免残留 | M4-T4 |
| R-015 | 租户上下文篡改导致越权 | 高 | 低 | tenant_id 由可信路径设置，客户端篡改拒绝 | M4-T5 |
| R-016 | 方言不支持 SET app.tenant_id | 中 | 中 | 降级为 SchemaIsolation（复用既有 `SchemaIsolationRouter`） | M4-T2 |
| R-017 | 进程级 L1 缓存导致内存溢出 | 中 | 中 | L1 缓存容量上限（默认 10000）+ LRU 淘汰 + 内存监控指标 | M5-T2 |
| R-018 | L1 与 L2 缓存数据不一致 | 高 | 低 | 通过 `CacheCoherenceProtocol` 同步失效，记录日志 | M5-T4 |
| R-019 | TTL 过期导致缓存穿透 | 中 | 中 | 正常从 DB 加载回填，可配预加载（prewarm） | M5-T2 |
| R-020 | 多租户缓存隔离失效导致跨租户数据泄露 | 高 | 低 | CacheKey 含 tenant_id，按租户隔离缓存项 | M5-T5 |
| R-021 | 7 项需求并行开发导致合并冲突 | 中 | 中 | 每项需求独立 feature gate + 模块边界清晰 + 分支策略（每需求一分支） | 全局 |
| R-022 | 新增 feature 与既有 feature 组合编译失败 | 中 | 低 | 门禁 10 全组合编译 + feature 依赖关系验证（M8-T2） | M8-T2 |
| R-023 | sz-pay 既有代码因 API 变更破坏 | 高 | 低 | 无 Breaking Change，feature gate 隔离默认关闭，既有公开 API 完全向后兼容 | M8-T1 |
| R-024 | 批量操作 SQL 注入（参数拼接） | 高 | 低 | 复用既有 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82` 参数化绑定 | M3-T5 |

---

# 十三、验收标准总览

## 13.1 REQ-V46-001 消息死信队列自动重投递（P1，M1）

1. `RedeliveryScheduler` 自动重投递调度器，死信消息按退避策略自动调度重投递（无需手动调用 `requeue_dead_letter`）← M1-T3
2. `BackoffPolicy` 四种退避策略（Fixed/Exponential/Linear/RandomJitter），默认 Exponential ← M1-T1
3. `DlxRoutingStrategy` 四种路由策略（RequeueToOriginal/ForwardToDlxTopic/ForwardToDlxQueue/Drop），默认 RequeueToOriginal ← M1-T4
4. 重投递次数上限（默认 10 次），超过上限按路由策略处理 ← M1-T3
5. 复用既有 `InMemoryQueue.dead_letters` `packages/sz-orm-queue/src/queue.rs:364` / `Message.retry_count` / `max_retries`，不重复实现死信存储 ← M1-T3
6. 重投递日志可追溯（消息 ID + 次数 + 退避时间 + 结果）← M1-T5
7. `dlx-auto-redelivery` feature gate 隔离，默认关闭，既有 `MessageQueue` trait 与 `requeue_dead_letter` 保留 ← M1-T1/T6

## 13.2 REQ-V46-002 迁移回滚自动化（P1，M2）

1. `ZeroDowntimeRollbackStrategy` 三种策略（ShadowTable/ReverseMigration/BlueGreen），默认 ShadowTable ← M2-T3
2. 复用既有 `Migration.sql_down` `packages/sz-orm-core/src/migration.rs:18` / `rollback` `:587` / `down` `:677`，不重复实现回滚 ← M2-T3
3. 回滚窗口（默认 5 分钟），超时拒绝自动回滚 ← M2-T4
4. 健康检查触发自动回滚（连续失败 N 次，默认 3 次）← M2-T4
5. 回滚数据一致性校验（ShadowTable 模式校验数据一致）← M2-T5
6. 回滚日志可追溯（版本 + 策略 + 触发原因 + 耗时 + 结果）← M2-T5
7. `zero-downtime-rollback` feature gate 隔离，默认关闭，既有 `rollback`/`down` 保留 ← M2-T1/T6

## 13.3 REQ-V46-003 批量事务原子性保证（P1，M3）

1. `AtomicityGuarantee` 三种级别（AllOrNothing/BestEffort/SagaCompensation），默认 BestEffort ← M3-T1
2. 复用既有 `BatchExecutor` `packages/sz-orm-batch/src/executor.rs` + `sz-orm-dtx` Saga/2PC，不重复实现 ← M3-T2
3. Saga 补偿模式（每批次作为 `SagaStep` `packages/sz-orm-dtx/src/saga.rs:255`，失败时补偿回滚已成功批次）← M3-T4
4. 跨批次原子提交（复用 `DistributedTransaction` 2PC `packages/sz-orm-dtx/src/lib.rs:270`，prepare 全成功后 commit）← M3-T3
5. 原子性保证可配置（默认 BestEffort 兼容既有 `RollbackStrategy::None` `packages/sz-orm-batch/src/lib.rs:518`）← M3-T3
6. 原子提交日志可追溯（批次列表 + 级别 + 结果 + 补偿日志）← M3-T5
7. `batch-atomic` feature gate 隔离，默认关闭，既有 `BatchExecutor` 与 `RollbackStrategy` 保留 ← M3-T1/T6

## 13.4 REQ-V46-004 异常检测（P2，M6）

1. `AnomalyDetector` 异常检测器，基于既有 `MetricsRegistry` `packages/sz-orm-observability/src/lib.rs:259` 指标历史数据检测异常 ← M6-T3
2. `AnomalyAlgorithm` 五种算法（Threshold/Trend/Statistical/ZScore/IQR），默认 Threshold ← M6-T2
3. 复用既有 `MetricsRegistry` / `SloMonitor` `packages/sz-orm-observability/src/slo.rs:223` / `QueryLogger` `packages/sz-orm-observability/src/query_logger.rs:73`，不重复实现指标采集 ← M6-T3
4. 异常告警联动（触发 `AnomalyAlert`，含指标名 + 异常值 + 阈值 + 窗口 + 算法）← M6-T3
5. 异常标注（在查询日志上标注异常标记）← M6-T4
6. 异常检测可配置（算法 + 阈值/窗口 + 告警通道）← M6-T1
7. `anomaly-detection` feature gate 隔离，默认关闭，既有 `MetricsRegistry`/`SloMonitor`/`QueryLogger` 保留 ← M6-T1/T5

## 13.5 REQ-V46-005 存储成本分析与优化建议（P2，M7）

1. `CostAnalyzer` 成本分析器，按 provider/bucket/tier 统计成本（容量+请求+流量）← M7-T2
2. 复用既有 `Storage` `packages/sz-orm-storage/src/storage.rs:14` / `StorageProvider` `:287` / `BucketLifecycle` `packages/sz-orm-storage/src/advanced.rs:438` / `LifecycleRule` `:400`，不重复实现存储 ← M7-T2
3. `CostOptimizationSuggestion` 四种建议（TierDowngrade/LifecycleOptimize/DeleteExpired/CompressCold），附预期节省 ← M7-T3
4. 成本报表周期性生成（默认每日，JSON/CSV 格式）← M7-T4
5. 成本数据准确（使用 provider API 计费数据，不估算）← M7-T2
6. 成本分析可配置（周期 + 建议类型 + 报表格式）← M7-T1
7. `cost-analysis` feature gate 隔离，默认关闭，既有 `Storage`/`BucketLifecycle` 保留 ← M7-T1/T5

## 13.6 REQ-V46-006 连接级多租户隔离（P1，M4）

1. `ConnectionLevelIsolation` 三种隔离机制（SetTenantId/SchemaIsolation/ConnectionBinding），默认 SetTenantId ← M4-T1
2. 复用既有 `Pool` `packages/sz-orm-core/src/pool.rs:743` / `Connection` `:45` / `TenantContext` `packages/sz-orm-core/src/tenant_context.rs:80` / `IsolationStrategy` `:22`，不新建连接池 ← M4-T2
3. `ConnectionAffinityPolicy` 三种亲和策略（Strict/Preferred/None），默认 Preferred ← M4-T3
4. 连接租户绑定防篡改（tenant_id 不可被客户端篡改）← M4-T5
5. 查询自动注入租户过滤（复用既有 `RowLevelSecurityPolicy` `packages/sz-orm-core/src/tenant_security.rs:67`，追加 `WHERE tenant_id = ?`）← M4-T5
6. 连接归还时清理租户上下文（避免残留）← M4-T4
7. `connection-level-tenant` feature gate 隔离，默认关闭，既有 `Pool`/`TenantContext`/`TenantPoolRegistry` 保留 ← M4-T1/T6

## 13.7 REQ-V46-007 进程级 L1 缓存（P1，M5）

1. `ProcessL1Cache` 进程级 L1 缓存（跨 Session 共享 Identity Map，线程安全 `Send + Sync`）← M5-T2
2. 跨 Session Identity Map 语义（相同主键跨 Session 返回相同 `Arc<T>` 引用，`Arc::ptr_eq` 为 true）← M5-T3
3. 复用既有 `L1Cache` `packages/sz-orm-core/src/l1_cache.rs:87` / `L2Cache` `packages/sz-orm-core/src/l2_cache.rs:517` / `CacheKey` `:143` / `CacheCoherenceProtocol` `packages/sz-orm-core/src/cache_coherence.rs:103`，不重复实现缓存 ← M5-T2
4. L1→L2→DB 查询协作（L1 命中直接返回，L1 未命中查 L2，L2 未命中查 DB）← M5-T3
5. 缓存一致性（L1 失效时 L2 同步失效，复用 `CacheCoherenceProtocol`）← M5-T4
6. 线程安全（`Send + Sync`，跨线程共享无数据竞争）← M5-T5
7. 可配置容量与 TTL（默认容量 10000，TTL 5 分钟，LRU 淘汰）← M5-T2
8. `process-l1-cache` feature gate 隔离，默认关闭，既有 `L1Cache`（Session 级）/ `L2Cache` 保留 ← M5-T1/T6

## 13.8 全局验收

1. 无 Breaking Change，feature gate 隔离默认全关闭，既有公开 API 完全向后兼容 ← M1-T6~M7-T5
2. v4.5.0 测试基线不回退（6,900+ 个测试仅增不减）← M8-T1
3. 14 道门禁全通过 ← M8-T1
4. feature 全组合编译通过 ← M8-T2
5. 五方言覆盖（MySQL/PostgreSQL/SQLite/Oracle/MSSQL）← M2-T6/M3-T6/M4-T6
6. 禁止占位实现（todo!/unimplemented!/unreachable!）← M1-T6~M7-T5
7. unsafe 零容忍 ← M1-T6~M7-T5
8. 参数化查询强制（复用 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82`）← M2-T5/M3-T5/M4-T2
9. 审计证据（每项结论附 file:line 证据）← 全任务
10. 与 v4.5.0 零重叠（可靠性+运维智能化层 vs 执行优化层）← 全任务
11. 不新增包（workspace 成员保持 60）← M0-T2

---

# 十四、feature gate 总览

| feature gate | 所属包 | 控制能力 | 默认 | 对应需求 | 测试命令 |
|-------------|--------|---------|------|---------|---------|
| `dlx-auto-redelivery` | sz-orm-queue（扩展） | 消息死信队列自动重投递（调度器 + 退避策略 + DLX 路由） | 关闭 | REQ-V46-001 | `cargo test -p sz-orm-queue --features dlx-auto-redelivery` |
| `zero-downtime-rollback` | sz-orm-core（扩展） | 迁移回滚自动化（零停机策略 + 健康检查 + 回滚窗口） | 关闭 | REQ-V46-002 | `cargo test -p sz-orm-core --features zero-downtime-rollback` |
| `batch-atomic` | sz-orm-batch（扩展）+ sz-orm-dtx（只读复用 Saga/2PC） | 批量事务原子性保证（all-or-nothing + Saga 补偿 + 跨批次原子提交） | 关闭 | REQ-V46-003 | `cargo test -p sz-orm-batch --features batch-atomic` |
| `anomaly-detection` | sz-orm-observability（扩展） | 异常检测（算法 + 告警 + 标注） | 关闭 | REQ-V46-004 | `cargo test -p sz-orm-observability --features anomaly-detection` |
| `cost-analysis` | sz-orm-storage（扩展） | 存储成本分析与优化建议（成本分析 + 优化建议 + 报表） | 关闭 | REQ-V46-005 | `cargo test -p sz-orm-storage --features cost-analysis` |
| `connection-level-tenant` | sz-orm-core（扩展） | 连接级多租户隔离（连接绑定 + 亲和性 + SET app.tenant_id） | 关闭 | REQ-V46-006 | `cargo test -p sz-orm-core --features connection-level-tenant` |
| `process-l1-cache` | sz-orm-core（扩展） | 进程级 L1 缓存（跨 Session Identity Map + 线程安全 + L1→L2→DB 协同） | 关闭 | REQ-V46-007 | `cargo test -p sz-orm-core --features process-l1-cache` |

---

# 十五、复用点清单

## 15.1 复用统计

| 需求 | 复用点数 | 新增点数 | 复用率 |
|------|---------|---------|--------|
| REQ-V46-001 DLX 自动重投递 | 10 | 6 | 62.5% |
| REQ-V46-002 零停机回滚 | 7 | 8 | 46.7% |
| REQ-V46-003 批量事务原子性 | 9 | 5 | 64.3% |
| REQ-V46-004 异常检测 | 4 | 7 | 36.4% |
| REQ-V46-005 成本分析 | 6 | 6 | 50.0% |
| REQ-V46-006 连接级多租户 | 11 | 5 | 68.8% |
| REQ-V46-007 进程级 L1 缓存 | 8 | 4 | 66.7% |
| **合计** | **55** | **41** | **57.3%** |

> **复用率说明**：v4.6.0 整体复用率 57.3%，优先复用既有能力，不重复实现，符合 spec.md §1.4 复用优先约束。复用率较 v4.5.0（71.2%）低，因为 v4.6.0 是"可靠性+运维智能化"层（新增检测器/分析器/触发器等新逻辑），而 v4.5.0 是"执行优化"层（复用既有执行器较多）。详见 design.md §3.8。

## 15.2 关键复用点（附 file:line 证据）

| 复用项 | 复用位置 | 用途 | 对应需求 |
|--------|---------|------|---------|
| `InMemoryQueue.dead_letters` | `packages/sz-orm-queue/src/queue.rs:364` | 既有死信存储，DLX 自动重投递复用 | REQ-V46-001 |
| `InMemoryQueue::requeue_dead_letter` | `packages/sz-orm-queue/src/queue.rs:484` | 既有手动重投递，RequeueToOriginal 路由策略复用 | REQ-V46-001 |
| `MigrationContext::rollback` | `packages/sz-orm-core/src/migration.rs:587` | 既有回滚指定版本，ReverseMigration 策略复用 | REQ-V46-002 |
| `MigrationContext::down` | `packages/sz-orm-core/src/migration.rs:677` | 既有回滚到版本，ShadowTable 策略复用 | REQ-V46-002 |
| `BatchExecutor` | `packages/sz-orm-batch/src/executor.rs` | 既有批量执行器，BatchTransactionCoordinator 复用 | REQ-V46-003 |
| `Saga` | `packages/sz-orm-dtx/src/saga.rs:377` | 既有 Saga，SagaCompensation 模式复用 | REQ-V46-003 |
| `DistributedTransaction` | `packages/sz-orm-dtx/src/lib.rs:270` | 既有 2PC，AllOrNothing 模式复用 | REQ-V46-003 |
| `MetricsRegistry` | `packages/sz-orm-observability/src/lib.rs:259` | 既有指标注册中心，异常检测复用历史数据 | REQ-V46-004 |
| `QueryLogger` | `packages/sz-orm-observability/src/query_logger.rs:73` | 既有查询日志器，异常标注复用 | REQ-V46-004 |
| `Storage` trait | `packages/sz-orm-storage/src/storage.rs:14` | 既有存储抽象，成本分析复用获取用量 | REQ-V46-005 |
| `BucketLifecycle` | `packages/sz-orm-storage/src/advanced.rs:438` | 既有生命周期管理，优化建议复用 | REQ-V46-005 |
| `Pool` | `packages/sz-orm-core/src/pool.rs:743` | 既有连接池，连接租户绑定复用 | REQ-V46-006 |
| `RowLevelSecurityPolicy` | `packages/sz-orm-core/src/tenant_security.rs:67` | 既有行级安全策略，自动注入 tenant_id 过滤复用 | REQ-V46-006 |
| `L1Cache` | `packages/sz-orm-core/src/l1_cache.rs:87` | 既有 Session 级 L1，Identity Map 语义复用 | REQ-V46-007 |
| `L2Cache` | `packages/sz-orm-core/src/l2_cache.rs:517` | 既有进程级 L2，L1→L2→DB 协同复用 | REQ-V46-007 |
| `CacheCoherenceProtocol` | `packages/sz-orm-core/src/cache_coherence.rs:103` | 既有缓存一致性协议，L1-L2 一致性复用 | REQ-V46-007 |
| `Connection::execute_with_params` | `packages/sz-orm-core/src/pool.rs:82` | 参数化绑定执行，防 SQL 注入（全局复用） | 全部 |

---

# 十六、与 v4.5.0 的关系

## 16.1 零重叠声明

v4.6.0 与 v4.5.0 零重叠：

| v4.5.0 能力（执行优化层） | v4.6.0 能力（可靠性 + 运维智能化层） | 关系 |
|-------------------------------|-------------------------|------|
| 并行查询执行器（`sz-orm-parallel`） | 连接级多租户隔离 / 进程级 L1 缓存 | v4.6.0 复用既有 `Pool`（`packages/sz-orm-core/src/pool.rs:743`），与并行查询复用同一连接池，不冲突 |
| 批量 INSERT/UPDATE/DELETE 优化（`sz-orm-batch` batch-v2） | 批量事务原子性保证（`sz-orm-batch` batch-atomic） | v4.6.0 复用 v4.5.0 `BatchExecutor`（`packages/sz-orm-batch/src/executor.rs`），扩展原子性保证，不修改既有 batch-v2 逻辑 |
| 异步流式结果集（`sz-orm-stream`） | 进程级 L1 缓存 | v4.6.0 进程级 L1 缓存可与流式结果集协同，不冲突 |

## 16.2 依赖关系

```
v4.5.0 已验收基线（3 个 feature gate: parallel-query / batch-v2 / stream-resultset）
  │
  ├─ batch-v2 ───→ REQ-V46-003 批量事务原子性（复用 BatchExecutor + sz-orm-dtx Saga/2PC）
  │
  └─ (其他 v4.5.0 feature) ──→ 无 v4.6.0 强依赖（v4.6.0 七项需求主体独立）

v4.6.0 七项需求相互独立，可并行开发：
  ├─ REQ-V46-001 DLX 自动重投递（扩展 sz-orm-queue，复用既有 MessageQueue/InMemoryQueue/dead_letters）
  ├─ REQ-V46-002 零停机回滚（扩展 sz-orm-core migration，复用既有 Migration/rollback/down）
  ├─ REQ-V46-003 批量事务原子性（扩展 sz-orm-batch，复用既有 BatchExecutor + sz-orm-dtx Saga/2PC）
  ├─ REQ-V46-004 异常检测（扩展 sz-orm-observability，复用既有 MetricsRegistry/SloMonitor/QueryLogger）
  ├─ REQ-V46-005 成本分析（扩展 sz-orm-storage，复用既有 Storage/BucketLifecycle/LifecycleRule）
  ├─ REQ-V46-006 连接级多租户（扩展 sz-orm-core pool/tenant_context，复用既有 Pool/TenantContext）
  └─ REQ-V46-007 进程级 L1 缓存（扩展 sz-orm-core l1_cache/l2_cache，复用既有 L1Cache/L2Cache/CacheCoherenceProtocol）
```

## 16.3 扩展包

| 包名 | 对应需求 | 扩展内容 |
|------|---------|---------|
| `sz-orm-queue` | REQ-V46-001 | DLX 自动重投递调度器 + 退避策略 + DLX 路由策略（`dlx-auto-redelivery` feature） |
| `sz-orm-core` | REQ-V46-002 / REQ-V46-006 / REQ-V46-007 | 零停机回滚（`zero-downtime-rollback` feature）+ 连接级多租户隔离（`connection-level-tenant` feature）+ 进程级 L1 缓存（`process-l1-cache` feature） |
| `sz-orm-batch` | REQ-V46-003 | 批量事务原子性保证（all-or-nothing + Saga 补偿 + 跨批次原子提交，`batch-atomic` feature） |
| `sz-orm-observability` | REQ-V46-004 | 异常检测（算法 + 告警 + 标注，`anomaly-detection` feature） |
| `sz-orm-storage` | REQ-V46-005 | 存储成本分析与优化建议（成本分析 + 优化建议 + 报表，`cost-analysis` feature） |

## 16.4 新增包

本版本不新增包，所有能力通过既有包扩展实现（sz-orm-queue / sz-orm-core / sz-orm-batch / sz-orm-observability / sz-orm-storage），workspace 成员保持 60 个。

## 16.5 版本号变更

| 项目 | v4.5.0 | v4.6.0 | 变更类型 |
|------|--------|--------|---------|
| workspace.package.version | 4.5.0 | 4.6.0 | minor 版本号升级 |
| workspace 成员数 | 60 | 60 | 0（不新增包） |
| feature gate 数 | v4.5.0 3 个 + 既有 | v4.6.0 7 个 + v4.5.0 3 个 + 既有 | 新增 7 feature |
| sz-orm-queue feature | cdc / message-tracing | cdc / message-tracing / dlx-auto-redelivery | 扩展 1 feature |
| sz-orm-core feature | 既有 40+ | 既有 40+ / zero-downtime-rollback / connection-level-tenant / process-l1-cache | 扩展 3 feature |
| sz-orm-batch feature | batch-stream / batch-v2 | batch-stream / batch-v2 / batch-atomic | 扩展 1 feature |
| sz-orm-observability feature | query-logging / service-mesh | query-logging / service-mesh / anomaly-detection | 扩展 1 feature |
| sz-orm-storage feature | storage-lifecycle / real-cloud | storage-lifecycle / real-cloud / cost-analysis | 扩展 1 feature |

---

> 文档生成依据：`docs/spec/v4.6.0/spec.md`（需求规格，1139 行）+ `docs/spec/v4.6.0/design.md`（技术设计，2242 行）+ `docs/spec/v4.5.0/tasks.md`（v4.5.0 任务规划，27 任务 / 148 子任务已完成）+ 2026-08-12 逐项代码验证（所有 file:line 证据均已实测存在）
> 审计合规：本文档所有 file:line 证据均引用真实存在的代码，遵循 AGENTS.md 审计合规铁律
> 任务约束：46 任务 / 264 子任务，每项任务附输入/输出/验收标准/复用点（file:line 证据），任务粒度 0.5-1 天可完成，依赖关系清晰，里程碑划分合理（M0 文档基线 → M1~M7 七项需求并行 → M8 集成验证）
> 下一阶段：编码实施（按 tasks.md 任务顺序执行，M0 → M1~M7 并行 → M8）