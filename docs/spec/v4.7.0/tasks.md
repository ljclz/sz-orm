# sz-orm v4.7.0 编码任务规划

> 版本：v4.7.0（消息延迟队列与优先级调度 + 迁移前向兼容性检查与沙箱预演 + 批量 COPY 协议与并行分片执行 + 异常自愈与根因分析 + 多云成本对比与容量预测 + 租户资源配额与行级安全增强 + 缓存预热与穿透防护）
> 基线：v4.6.0（消息死信队列自动重投递 + 迁移回滚自动化 + 批量事务原子性保证 + 异常检测 + 存储成本分析 + 连接级多租户隔离 + 进程级 L1 缓存，7 项需求 REQ-V46-001~007 全部通过 feature gate 隔离，已验收基线，已发布到 crates.io）
> 日期：2026-08-13
> 文档定位：编码任务规划（How to execute），对应需求规格 `spec.md`（What to build，1218 行，7 项 EARS 需求）+ 技术设计 `design.md`（How to build，1523 行）
> 任务约束：无 Breaking Change，7 个新 feature gate 隔离，默认全关闭；优先复用既有能力 + 五方言覆盖 + 每项任务附 file:line 代码证据 + unsafe 零容忍 + 禁止占位实现（todo!/unimplemented!/unreachable!）+ 参数化查询强制
> 审计合规铁律：每项任务结论须附真实存在的 file:line 证据，修复后必须运行 `cargo test` 并附输出，禁止未验证即标记 ✅
> 实施顺序：按 spec.md 优先级声明与 design.md 依赖关系，M0（P0 文档基线，立即）→ M1~M7（7 项需求并行开发，主体独立）→ M8（最终集成验证与文档同步，全部完成后）
> 与 v4.6.0 零重叠：v4.6.0 是"可靠性 + 运维智能化"层（消息可靠/迁移安全/批量原子/异常自检/成本自优/租户隔离/缓存升级），v4.7.0 是"智能化运维深化 + 性能深化"层（消息时间优先级/迁移前向兼容沙箱/COPY 并行分片/异常自愈 RCA/多云成本预测/租户配额 RLS/缓存预热防护），新增范围全部落在既有包扩展（sz-orm-queue / sz-orm-core / sz-orm-batch / sz-orm-observability / sz-orm-storage），不新增包

---

# 一、任务总览

## 1.1 里程碑 × 任务数 × 预期工作量

| 里程碑 | 名称 | 对应需求 | 优先级 | 任务数 | 子任务数 | 预期工作量 | 启动时机 |
|--------|------|---------|--------|--------|----------|-----------|---------|
| M0 | 文档基线与准备 | — | P0 | 3 | 12 | 0.5 天 | 立即（v4.6.0 已完成） |
| M1 | 消息延迟队列与优先级调度 | REQ-V47-001 | P1 | 6 | 38 | 2.5 天 | 立即（独立扩展 sz-orm-queue） |
| M2 | 迁移前向兼容性检查与沙箱预演 | REQ-V47-002 | P1 | 6 | 40 | 3 天 | 立即（独立扩展 sz-orm-core migration） |
| M3 | 批量 COPY 协议与并行分片执行 | REQ-V47-003 | P1 | 6 | 40 | 3 天 | 立即（独立扩展 sz-orm-batch） |
| M4 | 租户资源配额与行级安全增强 | REQ-V47-006 | P1 | 6 | 38 | 2.5 天 | 立即（独立扩展 sz-orm-core tenant） |
| M5 | 缓存预热与穿透防护 | REQ-V47-007 | P1 | 6 | 38 | 2.5 天 | 立即（独立扩展 sz-orm-core cache） |
| M6 | 异常自愈与根因分析 | REQ-V47-004 | P2 | 5 | 32 | 2 天 | 立即（独立扩展 sz-orm-observability） |
| M7 | 多云成本对比与容量预测 | REQ-V47-005 | P2 | 5 | 34 | 2.5 天 | 立即（独立扩展 sz-orm-storage） |
| M8 | 集成验证与文档同步 | 全局 | P0 | 3 | 18 | 0.5 天 | M1~M7 全部完成后 |
| **合计** | — | **7 项全覆盖** | — | **46** | **290** | **19 天** | — |

## 1.2 任务编号约定

- 主任务：`M{里程碑号}-T{任务序号}`（如 M1-T1）
- 子任务：`M{里程碑号}-T{任务序号}.{子任务序号}`（如 M1-T2.1）
- 集成验证任务：每个里程碑末尾固定一个集成测试与门禁验证任务（如 M1-T6）
- 里程碑内需求按 spec.md 优先级声明推进顺序编排（REQ-V47-001 → 002 → 003 → 006 → 007 → 004 → 005 对应 M1~M7）

## 1.3 全局约束（适用于所有任务）

1. **feature gate 隔离**：7 个新 feature（`delayed-priority-queue` / `forward-compat-sandbox` / `copy-parallel-shard` / `anomaly-remediation-rca` / `multicloud-cost-forecast` / `tenant-quota-rls-enhanced` / `cache-warmup-protection`），默认全关闭，默认 feature 行为不变
2. **既有 API 不变**：既有公开 API 签名完全向后兼容，sz-pay 既有代码不受影响（sz-pay 从 crates.io 拉取 sz-orm-* 6 个包）
3. **禁止占位实现**：禁止 `todo!`/`unimplemented!`/`unreachable!`
4. **unsafe 零容忍**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释
5. **五方言覆盖**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL（COPY 协议/并行分片/租户配额/RLS 增强按方言能力适配，如 COPY 协议仅 PostgreSQL 系原生支持，其他方言降级为 multi-value INSERT 或方言特定批量加载）
6. **参数化查询强制**：任何 WHERE 条件必须参数化绑定，禁止 SQL 字符串拼接（复用既有 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82`）
7. **审计证据**：每项任务结论附真实存在的 file:line 证据
8. **测试基线不回退**：v4.6.0 已验收测试基线不回退，v4.7.0 仅增不减
9. **复用优先**：优先复用既有能力，不重复实现（每项需求复用对应 v4.6.0 feature 基线）
10. **不新增包**：所有新能力通过既有包扩展实现（sz-orm-queue / sz-orm-core / sz-orm-batch / sz-orm-observability / sz-orm-storage），workspace 成员保持 60 个
11. **Windows MSVC 编译环境**：RUST_MIN_STACK=134217728, CARGO_INCREMENTAL=0
12. **测试命令**：`cargo test --workspace -j 2 --no-fail-fast`；feature 包测试：`cargo test -p <package> --features <feature>`

## 1.4 里程碑依赖关系

```
M0（P0，文档基线，立即）
M1（P1，延迟队列与优先级调度，独立扩展 sz-orm-queue）
  - REQ-V47-001 复用既有 MessageQueue/Message/InMemoryQueue + v4.6.0 RedeliveryScheduler/BackoffPolicy
M2（P1，前向兼容与沙箱预演，独立扩展 sz-orm-core migration）
  - REQ-V47-002 复用既有 Migration/DryRunMigration/ImpactReport + v4.6.0 RollbackExecutor
M3（P1，COPY 协议与并行分片，独立扩展 sz-orm-batch）
  - REQ-V47-003 复用既有 CopyProtocolExecutor/BatchExecutor + v4.6.0 BatchTransactionCoordinator/AtomicityGuarantee
M4（P1，租户配额与 RLS 增强，独立扩展 sz-orm-core tenant）
  - REQ-V47-006 复用 v4.6.0 ConnectionTenantBinder + 既有 RowLevelSecurityPolicy/ColumnMaskingRule/Pool
M5（P1，缓存预热与穿透防护，独立扩展 sz-orm-core cache）
  - REQ-V47-007 复用 v4.6.0 ProcessL1Cache/CrossSessionIdentityMap + 既有 L2Cache/CacheCoherenceProtocol
M6（P2，异常自愈与根因分析，独立扩展 sz-orm-observability）
  - REQ-V47-004 复用 v4.6.0 AnomalyDetector/AnomalyAlert + 既有 MetricsRegistry/QueryLogger
M7（P2，多云成本对比与容量预测，独立扩展 sz-orm-storage）
  - REQ-V47-005 复用 v4.6.0 CostAnalyzer/CostReport/ProviderCost + 既有 Storage/StorageProvider
M8（P0，集成验证与文档同步，M1~M7 全部完成后）
  - 依赖 M0~M7 全部完成
```

> **依赖关系说明**：M0 立即启动；M1~M7 七项需求主体相互独立，可并行开发（design.md 依赖关系图明确声明）；M8 必须在 M1~M7 全部完成后执行。七项需求无强依赖，可并行推进。

## 1.5 feature gate 定义与测试命令

| feature gate | 所属包 | 依赖既有 feature | 测试命令 | 默认 |
|-------------|--------|------------------|---------|------|
| `delayed-priority-queue` | sz-orm-queue（扩展） | `dlx-auto-redelivery`（v4.6.0） | `cargo test -p sz-orm-queue --features delayed-priority-queue` | 关闭 |
| `forward-compat-sandbox` | sz-orm-core（扩展） | `zero-downtime-rollback`（v4.6.0） | `cargo test -p sz-orm-core --features forward-compat-sandbox` | 关闭 |
| `copy-parallel-shard` | sz-orm-batch（扩展） | `batch-atomic`（v4.6.0） | `cargo test -p sz-orm-batch --features copy-parallel-shard` | 关闭 |
| `anomaly-remediation-rca` | sz-orm-observability（扩展） | `anomaly-detection`（v4.6.0） | `cargo test -p sz-orm-observability --features anomaly-remediation-rca` | 关闭 |
| `multicloud-cost-forecast` | sz-orm-storage（扩展） | `cost-analysis`（v4.6.0） | `cargo test -p sz-orm-storage --features multicloud-cost-forecast` | 关闭 |
| `tenant-quota-rls-enhanced` | sz-orm-core（扩展） | `connection-level-tenant`（v4.6.0） | `cargo test -p sz-orm-core --features tenant-quota-rls-enhanced` | 关闭 |
| `cache-warmup-protection` | sz-orm-core（扩展） | `process-l1-cache`（v4.6.0） | `cargo test -p sz-orm-core --features cache-warmup-protection` | 关闭 |

---

# 二、M0：文档基线与准备（P0，0.5 天）

**目标**：锁定 v4.6.0 已验收基线，准备 v4.7.0 开发环境（7 个 feature gate 骨架 + 版本号升级），不新增包。
**对应需求**：—（文档基线与环境准备，非功能需求）
**预期工作量**：0.5 天
**依赖**：无（v4.6.0 已全部完成并发布 crates.io）

## M0-T1：v4.6.0 完成总结与基线锁定

**任务描述**：总结 v4.6.0 交付成果（46 任务 / 264 子任务全部完成），锁定测试基线（v4.6.0 全部测试通过 + 14 道门禁全通过），作为 v4.7.0 开发的基准。

**涉及文件**：`docs/spec/v4.6.0/tasks.md`（既有，确认全部 `[x]`）、`docs/spec/v4.6.0/spec.md`（既有，1139 行）、`docs/spec/v4.6.0/design.md`（既有，2242 行）

**子任务**：
- [ ] M0-T1.1 确认 `docs/spec/v4.6.0/tasks.md` 46 任务 / 264 子任务全部标记 `[x]`（v4.6.0 已完成）
- [ ] M0-T1.2 运行 `cargo test --workspace -j 2 --no-fail-fast` 记录 v4.6.0 测试基线（全部通过）
- [ ] M0-T1.3 运行 14 道门禁全量验证，记录基线通过状态（fmt/check/clippy/test/doc/audit/integration/占位/SQL注入/feature组合/ADR-0001/文档一致性/审计证据/文档同步）
- [ ] M0-T1.4 确认 v4.6.0 7 个 feature gate（`dlx-auto-redelivery`/`zero-downtime-rollback`/`batch-atomic`/`anomaly-detection`/`cost-analysis`/`connection-level-tenant`/`process-l1-cache`）默认全关闭，行为不变

**验收标准**：v4.6.0 基线锁定（测试数 + 门禁通过状态 + feature gate 状态），每项附 file:line 或命令输出证据

**依赖**：无

## M0-T2：v4.7.0 开发环境准备

**任务描述**：在 5 个既有包新增 7 个 feature gate 占位（默认关闭，依赖对应 v4.6.0 feature），升级版本号 4.6.0 → 4.7.0，验证 workspace 编译通过，不新增包。

**涉及文件**：
- `Cargo.toml`（workspace.package.version 4.6.0 → 4.7.0）
- `packages/sz-orm-queue/Cargo.toml`（扩展：新增 `delayed-priority-queue` feature 占位，依赖 `dlx-auto-redelivery`）
- `packages/sz-orm-core/Cargo.toml`（扩展：新增 `forward-compat-sandbox` / `tenant-quota-rls-enhanced` / `cache-warmup-protection` feature 占位）
- `packages/sz-orm-batch/Cargo.toml`（扩展：新增 `copy-parallel-shard` feature 占位，依赖 `batch-atomic`）
- `packages/sz-orm-observability/Cargo.toml`（扩展：新增 `anomaly-remediation-rca` feature 占位，依赖 `anomaly-detection`）
- `packages/sz-orm-storage/Cargo.toml`（扩展：新增 `multicloud-cost-forecast` feature 占位，依赖 `cost-analysis`）

**复用标注**：既有 workspace 结构 `Cargo.toml`（60 个成员）；既有 v4.6.0 feature gate 模式（`packages/sz-orm-queue/Cargo.toml:38` dlx-auto-redelivery 等 7 个）

**子任务**：
- [ ] M0-T2.1 `packages/sz-orm-queue/Cargo.toml` 新增 `delayed-priority-queue = ["dlx-auto-redelivery"]` feature（默认关闭，依赖 v4.6.0 `dlx-auto-redelivery` `:38`）
- [ ] M0-T2.2 `packages/sz-orm-core/Cargo.toml` 新增 `forward-compat-sandbox = ["zero-downtime-rollback"]` feature（默认关闭，依赖 v4.6.0 `zero-downtime-rollback` `:134`）
- [ ] M0-T2.3 `packages/sz-orm-batch/Cargo.toml` 新增 `copy-parallel-shard = ["batch-atomic"]` feature（默认关闭，依赖 v4.6.0 `batch-atomic` `:28`）
- [ ] M0-T2.4 `packages/sz-orm-observability/Cargo.toml` 新增 `anomaly-remediation-rca = ["anomaly-detection"]` feature（默认关闭，依赖 v4.6.0 `anomaly-detection` `:28`）
- [ ] M0-T2.5 `packages/sz-orm-storage/Cargo.toml` 新增 `multicloud-cost-forecast = ["cost-analysis"]` feature（默认关闭，依赖 v4.6.0 `cost-analysis` `:19`）
- [ ] M0-T2.6 `packages/sz-orm-core/Cargo.toml` 新增 `tenant-quota-rls-enhanced = ["connection-level-tenant"]` feature（默认关闭，依赖 v4.6.0 `connection-level-tenant` `:136`）
- [ ] M0-T2.7 `packages/sz-orm-core/Cargo.toml` 新增 `cache-warmup-protection = ["process-l1-cache"]` feature（默认关闭，依赖 v4.6.0 `process-l1-cache` `:138`）
- [ ] M0-T2.8 `Cargo.toml` workspace.package.version 从 `4.6.0` 升级为 `4.7.0`
- [ ] M0-T2.9 验证 `cargo check --workspace` 编译通过（7 个 feature gate 占位不影响既有编译，workspace 成员仍 60）
- [ ] M0-T2.10 验证默认 feature 行为与 v4.6.0 一致（`cargo build --workspace` 行为不变）

**验收标准**：7 个 feature gate 占位创建成功，workspace 成员仍 60，版本号 4.7.0，workspace 编译通过，默认 feature 行为不变

**依赖**：M0-T1

## M0-T3：基线验证

**任务描述**：运行文档一致性、审计证据、文档同步三道门禁，验证 v4.6.0 基线可被工具消费，v4.7.0 骨架不破坏既有基线。

**涉及文件**：`scripts/check-doc-consistency.py`、`scripts/audit-verify.sh`、`scripts/check-doc-sync.py`

**子任务**：
- [ ] M0-T3.1 运行 `python scripts/check-doc-consistency.py`（门禁 12），验证文档与代码一致
- [ ] M0-T3.2 运行 `bash scripts/audit-verify.sh docs/spec/v4.6.0/tasks.md`（门禁 13），验证 v4.6.0 tasks.md 所有 file:line 引用真实存在
- [ ] M0-T3.3 运行 `python scripts/check-doc-sync.py --diff HEAD`（门禁 14），验证文档与 HEAD 同步
- [ ] M0-T3.4 验证 v4.7.0 spec.md（1218 行）与 design.md（1523 行）所有 file:line 证据真实存在（audit-verify 扩展验证）

**验收标准**：三道门禁全部通过；v4.6.0 tasks.md + v4.7.0 spec.md + v4.7.0 design.md 所有 file:line 引用经 audit-verify 验证真实存在

**依赖**：M0-T2

---

# 三、M1：消息延迟队列与优先级调度（REQ-V47-001，P1，2.5 天）

**目标**：扩展既有 `sz-orm-queue` 包，新增 `DelayScheduler` 延迟调度器 + `PriorityQueue` 优先级队列 + `ScheduledMessage` 定时消息，复用既有 `MessageQueue` trait + v4.6.0 `RedeliveryScheduler` 调度器基线，补齐延迟投递（按 `deliver_at` 投递）+ 优先级排序（`PriorityPolicy` Strict/Weighted/FairShare）+ 定时调度（Cron 表达式），aging 机制避免低优先级饿死。
**对应需求**：REQ-V47-001（spec.md §5.1，design.md §2.1.3.1 + §2.2.2.1）
**预期工作量**：2.5 天
**依赖**：无（M1 为 P1 独立需求，复用既有 sz-orm-queue MessageQueue + v4.6.0 RedeliveryScheduler，扩展包可与 M2~M7 并行）

## M1-T1：delayed-priority-queue feature gate + 配置结构

**任务描述**：在 `sz-orm-queue` 完善 `delayed-priority-queue` feature gate 隔离（M0-T2 已创建占位），定义 `PriorityPolicy` 优先级策略枚举 + `ScheduleConfig` 调度配置，作为延迟队列与优先级调度的数据模型。

**涉及文件**：
- `packages/sz-orm-queue/Cargo.toml`（完善：`delayed-priority-queue` feature 依赖 `dlx-auto-redelivery`）
- `packages/sz-orm-queue/src/lib.rs`（扩展：模块声明 `mod delayed_priority;`，`#[cfg(feature = "delayed-priority-queue")]` 门控）
- `packages/sz-orm-queue/src/delayed_priority.rs`（新建，延迟优先级队列配置 + 策略枚举）

**复用标注**：既有 `MessageQueue` trait `packages/sz-orm-queue/src/queue.rs:18`；既有 `Message` `packages/sz-orm-queue/src/queue.rs:57`；既有 `InMemoryQueue` `packages/sz-orm-queue/src/queue.rs:339`；v4.6.0 `RedeliveryScheduler` `packages/sz-orm-queue/src/dlx.rs:216`；v4.6.0 `BackoffPolicy` `packages/sz-orm-queue/src/dlx.rs:47`；v4.6.0 `DlxRoutingStrategy` `packages/sz-orm-queue/src/dlx.rs:83`

**feature gate 隔离**：`delayed-priority-queue = ["dlx-auto-redelivery"]`，默认关闭

**子任务**：
- [ ] M1-T1.1 `packages/sz-orm-queue/Cargo.toml` 确认 `delayed-priority-queue = ["dlx-auto-redelivery"]` feature（M0-T2 已创建），新增 `chrono` 依赖（workspace, 既有）
- [ ] M1-T1.2 `src/lib.rs` 声明 `mod delayed_priority;`，`#[cfg(feature = "delayed-priority-queue")]` 门控
- [ ] M1-T1.3 `src/delayed_priority.rs` 定义 `pub enum PriorityPolicy { Strict, Weighted, FairShare }`（三种优先级策略，`Serialize + Deserialize`）
- [ ] M1-T1.4 定义 `pub struct DelayedMessage { message: Message, deliver_at: DateTime<Utc>, priority: i32 }`（延迟消息，包装既有 `Message` `queue.rs:57`）
- [ ] M1-T1.5 定义 `pub struct ScheduledMessage { message: Message, cron: Option<String>, interval: Option<Duration> }`（定时消息，Cron 或间隔）
- [ ] M1-T1.6 定义 `pub struct ScheduleConfig { enabled, priority_policy, aging_enabled, aging_threshold_ms, queue_capacity, check_interval_ms }`（调度配置）
- [ ] M1-T1.7 实现 `impl ScheduleConfig { pub fn new() -> Self }`（默认：enabled false, Strict, aging true, 300000ms, 100000, 100ms）
- [ ] M1-T1.8 实现链式配置方法：`with_priority_policy` / `with_aging` / `with_queue_capacity` / `with_check_interval_ms`
- [ ] M1-T1.9 单元测试：`ScheduleConfig::new()` 默认值正确（Strict, aging 5 分钟, 容量 100000, 检查间隔 100ms）
- [ ] M1-T1.10 验证 `cargo check -p sz-orm-queue --features delayed-priority-queue` 编译通过

**验收标准**：feature gate 定义；PriorityPolicy/DelayedMessage/ScheduledMessage/ScheduleConfig 定义完整；复用既有 `Message`

**依赖**：M0-T2

## M1-T2：PriorityQueue 优先级队列 + aging 机制

**任务描述**：实现 `PriorityQueue` 优先级队列（基于 `BinaryHeap` 按 `priority` 排序），支持三种 `PriorityPolicy` 策略，aging 机制避免低优先级饿死，复用既有 `MessageQueue.consume`。

**涉及文件**：
- `packages/sz-orm-queue/src/delayed_priority.rs`（扩展：优先级队列核心）

**复用标注**：既有 `MessageQueue` trait `packages/sz-orm-queue/src/queue.rs:18`（`consume` 方法复用）

**子任务**：
- [ ] M1-T2.1 定义 `pub struct PriorityQueue { heap: Mutex<BinaryHeap<PriorityMessage>>, policy: PriorityPolicy, capacity: usize, aging_enabled: bool, aging_threshold_ms: u64 }`（优先级队列，`Mutex` 保护 `BinaryHeap`）
- [ ] M1-T2.2 定义 `struct PriorityMessage { message: Message, priority: i32, enqueued_at: Instant }`（含入队时间，用于 aging）
- [ ] M1-T2.3 实现 `impl PriorityQueue { pub fn new(policy: PriorityPolicy, capacity: usize) -> Self }`
- [ ] M1-T2.4 实现 `pub fn enqueue(&self, message: Message, priority: i32) -> Result<(), QueueError>`：容量检查 + 按策略入队
- [ ] M1-T2.5 实现 `pub fn dequeue(&self) -> Option<Message>`：按策略出队（Strict: 最高优先级；Weighted: 按权重比例；FairShare: 公平份额）
- [ ] M1-T2.6 实现 Strict 策略：`BinaryHeap` 按 `priority` 大顶堆，O(log n) 插入/弹出
- [ ] M1-T2.7 实现 Weighted 策略：按权重比例随机选择优先级层
- [ ] M1-T2.8 实现 FairShare 策略：按租户/类别公平分配投递份额（从 `Message.headers` 提取租户/类别）
- [ ] M1-T2.9 实现 aging 机制：`Strict` 策略下检查 `Enqueued` 状态低优先级消息等待时间，超过 `aging_threshold_ms`（默认 5 分钟）提升优先级
- [ ] M1-T2.10 单元测试：Strict + 消息 A 优先级 10 + 消息 B 优先级 5 → 消息 A 先出队
- [ ] M1-T2.11 单元测试：Weighted + 权重配置 → 按权重比例出队
- [ ] M1-T2.12 单元测试：FairShare + 两个租户 → 公平分配投递份额
- [ ] M1-T2.13 单元测试：aging + Strict + 低优先级消息等待 6 分钟（阈值 5 分钟）→ 提升优先级，避免饿死
- [ ] M1-T2.14 边界测试：容量超限（默认 100000）→ 拒绝入队，返回 `QueueError::CapacityExceeded`
- [ ] M1-T2.15 性能测试：插入排序开销 O(log n)，10000 消息插入 < 10ms

**验收标准**：三种策略实现正确；aging 机制避免饿死；容量限制；性能 O(log n)

**依赖**：M1-T1

## M1-T3：DelayScheduler 延迟调度器 + 定时调度

**任务描述**：实现 `DelayScheduler` 延迟调度器，管理 `DelayedMessage` 到期检查 + `PriorityQueue` 投递 + `ScheduledMessage` Cron 调度，复用 v4.6.0 `RedeliveryScheduler` 调度循环基线 + 既有 `MessageQueue.publish`。

**涉及文件**：
- `packages/sz-orm-queue/src/delayed_priority.rs`（扩展：延迟调度器核心）

**复用标注**：v4.6.0 `RedeliveryScheduler` `packages/sz-orm-queue/src/dlx.rs:216`（调度循环基线复用）；v4.6.0 `BackoffPolicy` `packages/sz-orm-queue/src/dlx.rs:47`（投递失败退避复用）；既有 `MessageQueue` `packages/sz-orm-queue/src/queue.rs:18`（`publish` 方法复用）；既有 `message_tracing` `packages/sz-orm-queue/src/message_tracing.rs`（调度日志复用）

**子任务**：
- [ ] M1-T3.1 定义 `pub struct DelayScheduler { delayed_messages: RwLock<BTreeMap<DateTime<Utc>, Vec<DelayedMessage>>>, priority_queue: PriorityQueue, scheduled: Vec<ScheduledMessage>, queue: Arc<dyn MessageQueue>, config: ScheduleConfig, shutdown: CancellationToken }`（延迟调度器，复用既有 `MessageQueue`）
- [ ] M1-T3.2 实现 `impl DelayScheduler { pub fn new(queue: Arc<dyn MessageQueue>, config: ScheduleConfig) -> Self }`
- [ ] M1-T3.3 实现 `pub async fn publish_delayed(&self, msg: DelayedMessage) -> Result<(), QueueError>`：存储延迟消息（按 `deliver_at` 排序存入 `BTreeMap`），到期前不可消费
- [ ] M1-T3.4 实现 `pub async fn publish_scheduled(&self, msg: ScheduledMessage) -> Result<(), QueueError>`：校验 Cron 表达式 + 存储定时消息
- [ ] M1-T3.5 实现调度循环：`tokio::time::interval`（100ms 可配）周期检查到期消息，到期则转入 `PriorityQueue`
- [ ] M1-T3.6 实现到期检查分支：`now >= deliver_at` 则消息从 `Waiting` 转 `Ready`，加入 `PriorityQueue`
- [ ] M1-T3.7 实现优先级投递分支：`PriorityQueue.dequeue` 取最高优先级消息，调用既有 `MessageQueue.publish` 投递
- [ ] M1-T3.8 实现定时调度分支：`ScheduledMessage` 按 Cron 表达式计算下次投递时间，到点后创建消息加入 `PriorityQueue`
- [ ] M1-T3.9 实现投递失败处理：按 v4.6.0 `BackoffPolicy` `dlx.rs:47` 退避策略重试，重试超限进死信队列（复用既有 `requeue_dead_letter` `queue.rs:484`）
- [ ] M1-T3.10 实现链式配置方法：`with_priority_policy` / `with_aging`
- [ ] M1-T3.11 实现 `pub async fn shutdown(&self) -> Result<(), QueueError>`：`CancellationToken` 优雅关闭调度循环
- [ ] M1-T3.12 单元测试：发布延迟消息 `deliver_at=10:00` + 当前 09:00 → 消息在 10:00 前不可消费，10:00 后可消费
- [ ] M1-T3.13 单元测试：定时消息 `cron="0 * * * *"` + 启用调度 → 每分钟自动投递一条消息
- [ ] M1-T3.14 单元测试：延迟与优先级组合（消息 A deliver_at=10:00 优先级 5 + 消息 B deliver_at=09:00 优先级 10 + 当前 09:30）→ 消息 B 已到期按优先级 10 投递，消息 A 未到期不投递
- [ ] M1-T3.15 边界测试：Cron 表达式无效 → 拒绝发布，返回 `QueueError::InvalidCron`
- [ ] M1-T3.16 边界测试：投递失败 → 按 `BackoffPolicy` 退避重试，重试超限进死信队列
- [ ] M1-T3.17 性能测试：调度器检查开销 ≤ 1ms/次（含到期判定 + 优先级排序）

**验收标准**：延迟投递 + 定时调度 + 优先级组合正确；复用 v4.6.0 `RedeliveryScheduler` 调度循环 + `BackoffPolicy`；调度开销 ≤ 1ms/次

**依赖**：M1-T1、M1-T2

## M1-T4：调度日志 + 异常处理

**任务描述**：实现调度日志记录（消息 ID + 延迟时间 + 优先级 + 投递时间 + 结果），处理三种异常场景，复用既有 `message_tracing` 模块。

**涉及文件**：
- `packages/sz-orm-queue/src/delayed_priority.rs`（扩展：调度日志 + 异常处理）

**复用标注**：既有 `message_tracing` `packages/sz-orm-queue/src/message_tracing.rs`（调度日志复用）

**子任务**：
- [ ] M1-T4.1 调度日志：延迟消息投递 → 记录调度日志（消息 ID + 延迟时间 + 优先级 + 投递时间 + 结果），复用既有 `message_tracing`
- [ ] M1-T4.2 异常处理 1：延迟消息投递失败 → 按 v4.6.0 退避策略重试，日志标注"delayed message delivery failed, retried N times"
- [ ] M1-T4.3 异常处理 2：Cron 表达式无效 → 拒绝发布，返回错误"invalid cron expression: ..."
- [ ] M1-T4.4 异常处理 3：优先级队列容量超限 → 拒绝入队，返回错误"priority queue capacity exceeded, please increase capacity or drain queue"
- [ ] M1-T4.5 单元测试：延迟消息投递 → 记录调度日志，含消息 ID + 延迟时间 + 优先级 + 投递时间 + 结果
- [ ] M1-T4.6 单元测试：投递失败 → 日志标注"delayed message delivery failed, retried N times"
- [ ] M1-T4.7 边界测试：调度日志写入失败 → 不阻断主流程，记录到 stderr

**验收标准**：调度日志可追溯；复用既有 `message_tracing`；三种异常场景处理正确

**依赖**：M1-T3

## M1-T5：消息脱敏 + 多租户隔离

**任务描述**：确保延迟消息保留原始消息脱敏状态（复用既有 `sz-orm-masking`），延迟消息按租户隔离（消息 `headers` 含 `tenant_id`）。

**涉及文件**：
- `packages/sz-orm-queue/src/delayed_priority.rs`（扩展：脱敏 + 租户隔离）

**复用标注**：既有 `sz-orm-masking` 脱敏模块；既有 `Message.headers` `packages/sz-orm-queue/src/queue.rs:57`

**子任务**：
- [ ] M1-T5.1 延迟消息保留原始消息脱敏状态（复用既有 `sz-orm-masking`），不泄露敏感数据
- [ ] M1-T5.2 延迟消息按租户隔离：消息 `headers` 含 `tenant_id`，投递时按租户隔离
- [ ] M1-T5.3 单元测试：脱敏消息发布延迟 → 延迟消息保留脱敏状态
- [ ] M1-T5.4 单元测试：租户 1 消息 + 租户 2 消息 → 按租户隔离投递，不跨租户

**验收标准**：延迟消息脱敏 + 租户隔离正确；复用既有 `sz-orm-masking` + `Message.headers`

**依赖**：M1-T3

## M1-T6：M1 集成测试与门禁验证

**任务描述**：M1 里程碑集成测试与门禁验证，确保 REQ-V47-001 全部 8 条验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M1-T6.1 集成测试：`DelayScheduler` 完整流程（发布延迟消息 → 到期检查 → 优先级投递 → 调度日志）
- [ ] M1-T6.2 集成测试：三种优先级策略（Strict/Weighted/FairShare）+ aging 机制完整验证
- [ ] M1-T6.3 集成测试：定时调度（Cron 表达式）+ 延迟与优先级组合
- [ ] M1-T6.4 集成测试：复用既有 `MessageQueue` `packages/sz-orm-queue/src/queue.rs:18` + v4.6.0 `RedeliveryScheduler` `packages/sz-orm-queue/src/dlx.rs:216` + `BackoffPolicy` `:47`，不新建消息存储与调度逻辑
- [ ] M1-T6.5 集成测试：调度日志可追溯 + 三种异常场景处理
- [ ] M1-T6.6 运行 `cargo test -p sz-orm-queue --features delayed-priority-queue`（全部通过）
- [ ] M1-T6.7 `cargo clippy -p sz-orm-queue --features delayed-priority-queue -- -D warnings`
- [ ] M1-T6.8 `cargo fmt -p sz-orm-queue -- --check`
- [ ] M1-T6.9 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-queue/src/delayed_priority.rs` 无占位实现
- [ ] M1-T6.10 扫描 `grep -rn 'unsafe' packages/sz-orm-queue/src/delayed_priority.rs` 无 unsafe 块
- [ ] M1-T6.11 验证默认 feature 行为与 v4.6.0 一致（`cargo build -p sz-orm-queue` 无延迟队列与优先级调度）
- [ ] M1-T6.12 验证 `delayed-priority-queue` 与既有 `dlx-auto-redelivery`/`cdc`/`message-tracing` feature 组合编译通过

**验收标准**：M1 集成测试通过；门禁通过；默认行为不变；延迟投递 + 优先级 + 定时调度 + aging 全部验证

**依赖**：M1-T1、M1-T2、M1-T3、M1-T4、M1-T5

---

# 四、M2：迁移前向兼容性检查与沙箱预演（REQ-V47-002，P1，3 天）

**目标**：扩展既有 `sz-orm-core` 迁移管理，新增 `ForwardCompatChecker` 前向兼容性检查器 + `SandboxDryRunner` 沙箱预演器 + `DependencyAnalyzer` 依赖分析器，复用既有 `DryRunMigration`/`ImpactReport` + v4.6.0 `RollbackExecutor` 影子表能力，补齐前向兼容性检查 + 沙箱预演 + 迁移依赖图分析。
**对应需求**：REQ-V47-002（spec.md §5.2，design.md §2.1.3.2 + §2.2.2.2）
**预期工作量**：3 天
**依赖**：无（M2 为 P1 独立需求，复用既有 sz-orm-core Migration/DryRunMigration + v4.6.0 RollbackExecutor，扩展包可与 M1、M3~M7 并行）

## M2-T1：forward-compat-sandbox feature gate + 配置结构

**任务描述**：在 `sz-orm-core` 完善 `forward-compat-sandbox` feature gate 隔离（M0-T2 已创建占位），定义 `CompatStrictness`/`BreakingChangeType`/`SandboxVerifyItem` 枚举 + `ForwardCompatConfig`/`SandboxConfig` 配置。

**涉及文件**：
- `packages/sz-orm-core/Cargo.toml`（完善：`forward-compat-sandbox` feature 依赖 `zero-downtime-rollback`）
- `packages/sz-orm-core/src/lib.rs`（扩展：模块声明 `mod forward_compat_sandbox;`，`#[cfg(feature = "forward-compat-sandbox")]` 门控）
- `packages/sz-orm-core/src/forward_compat_sandbox.rs`（新建，前向兼容配置 + 沙箱配置 + 枚举）

**复用标注**：既有 `Migration` `packages/sz-orm-core/src/migration.rs:11`；既有 `MigrationResolver` `packages/sz-orm-core/src/migration.rs:63`；既有 `DryRunMigration` `packages/sz-orm-core/src/migration_dry_run.rs:11`；既有 `DryRunReport` `packages/sz-orm-core/src/migration_dry_run.rs:24`；既有 `ImpactReport` `packages/sz-orm-core/src/migration_dry_run.rs:80`；既有 `DdlType` `packages/sz-orm-core/src/migration_dry_run.rs:33`；v4.6.0 `RollbackExecutor` `packages/sz-orm-core/src/rollback_zero_downtime.rs:305`；v4.6.0 `RollbackPlan` `packages/sz-orm-core/src/rollback_zero_downtime.rs:157`

**feature gate 隔离**：`forward-compat-sandbox = ["zero-downtime-rollback"]`，默认关闭

**子任务**：
- [ ] M2-T1.1 `packages/sz-orm-core/Cargo.toml` 确认 `forward-compat-sandbox = ["zero-downtime-rollback"]` feature（M0-T2 已创建）
- [ ] M2-T1.2 `src/lib.rs` 声明 `mod forward_compat_sandbox;`，`#[cfg(feature = "forward-compat-sandbox")]` 门控
- [ ] M2-T1.3 `src/forward_compat_sandbox.rs` 定义 `pub enum CompatStrictness { Strict, Lenient }`（检查严格度）
- [ ] M2-T1.4 定义 `pub enum BreakingChangeType { DropColumn, AlterColumnType, AlterColumnConstraint, RenameTable }`（破坏性变更类型）
- [ ] M2-T1.5 定义 `pub enum SandboxVerifyItem { DataIntegrity, QueryCompat, PerformanceImpact }`（沙箱验证项）
- [ ] M2-T1.6 定义 `pub struct ForwardCompatConfig { strictness, breaking_changes, sandbox_table_prefix, sandbox_verify_items }`（前向兼容配置）
- [ ] M2-T1.7 定义 `pub struct SandboxConfig { table_prefix, verify_items, cleanup_on_exit }`（沙箱配置，默认前缀 "shadow_"）
- [ ] M2-T1.8 实现 `impl ForwardCompatConfig { pub fn new() -> Self }`（默认：Strict, 全部破坏性变更类型, "shadow_", 全部验证项）
- [ ] M2-T1.9 实现链式配置方法：`with_strictness` / `with_breaking_changes` / `with_sandbox_verify_items`
- [ ] M2-T1.10 单元测试：`ForwardCompatConfig::new()` 默认值正确（Strict, 全部类型, shadow_, 全部验证项）
- [ ] M2-T1.11 验证 `cargo check -p sz-orm-core --features forward-compat-sandbox` 编译通过

**验收标准**：feature gate 定义；配置结构定义完整；复用既有 `Migration`/`DryRunMigration`

**依赖**：M0-T2

## M2-T2：ForwardCompatChecker 前向兼容性检查器

**任务描述**：实现 `ForwardCompatChecker` 前向兼容性检查器，复用既有 `DryRunMigration.analyze_impact` 获取 `ImpactReport`，基于 `DdlType` 识别破坏性变更，生成 `CompatCheckResult`。

**涉及文件**：
- `packages/sz-orm-core/src/forward_compat_sandbox.rs`（扩展：前向兼容性检查器核心）

**复用标注**：既有 `DryRunMigration` `packages/sz-orm-core/src/migration_dry_run.rs:11`（`analyze_impact` 方法复用）；既有 `ImpactReport` `packages/sz-orm-core/src/migration_dry_run.rs:80`；既有 `MigrationImpact` `packages/sz-orm-core/src/migration_dry_run.rs:59`；既有 `DdlType` `packages/sz-orm-core/src/migration_dry_run.rs:33`

**子任务**：
- [ ] M2-T2.1 定义 `pub struct ForwardCompatChecker { dry_run: DryRunMigration, config: ForwardCompatConfig }`（前向兼容性检查器，复用既有 `DryRunMigration`）
- [ ] M2-T2.2 实现 `impl ForwardCompatChecker { pub fn new(config: ForwardCompatConfig) -> Self }`
- [ ] M2-T2.3 实现 `pub async fn check_compatibility(&self, migration: &Migration) -> Result<CompatCheckResult, MigrationError>`：调用 `DryRunMigration.analyze_impact` 获取 `ImpactReport` → 识别破坏性变更 → 生成 `CompatCheckResult`
- [ ] M2-T2.4 定义 `pub struct CompatCheckResult { breaking_changes: Vec<BreakingChangeType>, affected_apps: Vec<String>, suggested_strategy: String, evidence: Vec<String> }`（兼容性检查结果）
- [ ] M2-T2.5 实现破坏性变更识别：`DdlType::DropColumn` → `BreakingChangeType::DropColumn`；`AlterColumnType` → `AlterColumnType`；`AlterColumnConstraint` → `AlterColumnConstraint`；`RenameTable` → `RenameTable`
- [ ] M2-T2.6 实现 `Lenient` 严格度：宽松模式下部分变更不视为破坏性（可配）
- [ ] M2-T2.7 实现链式配置方法：`with_strictness` / `with_breaking_changes`
- [ ] M2-T2.8 单元测试：迁移删除列 "users.email" + 前向兼容性检查 → 识别为破坏性变更，生成 `CompatCheckResult` 标注"删除列 email 可能破坏依赖该列的旧应用"
- [ ] M2-T2.9 单元测试：迁移加列（非破坏性）→ 不识别为破坏性变更
- [ ] M2-T2.10 单元测试：配置删除列为非破坏性 + 检查 → 删除列不视为破坏性变更
- [ ] M2-T2.11 边界测试：兼容性规则匹配异常 → 跳过该规则，记录日志"compat rule matching error, skipped"
- [ ] M2-T2.12 性能测试：前向兼容性检查开销 ≤ 500ms/迁移

**验收标准**：前向兼容性检查正确；复用既有 `DryRunMigration`/`ImpactReport`；不误报非破坏性变更；性能 ≤ 500ms/迁移

**依赖**：M2-T1

## M2-T3：SandboxDryRunner 沙箱预演器

**任务描述**：实现 `SandboxDryRunner` 沙箱预演器，复用 v4.6.0 `RollbackExecutor` 影子表能力，在影子表上预执行迁移 SQL（参数化绑定）+ 校验数据完整性/查询兼容性/性能影响 + 清理影子表。

**涉及文件**：
- `packages/sz-orm-core/src/forward_compat_sandbox.rs`（扩展：沙箱预演器核心）

**复用标注**：v4.6.0 `RollbackExecutor` `packages/sz-orm-core/src/rollback_zero_downtime.rs:305`（影子表管理复用）；v4.6.0 `ZeroDowntimeRollbackConfig` `packages/sz-orm-core/src/rollback_zero_downtime.rs:84`；既有 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82`（参数化执行复用）；既有 `Pool` `packages/sz-orm-core/src/pool.rs:743`

**子任务**：
- [ ] M2-T3.1 定义 `pub struct SandboxDryRunner { pool: Arc<Pool>, config: SandboxConfig, rollback_executor: RollbackExecutor }`（沙箱预演器，复用 v4.6.0 `RollbackExecutor`）
- [ ] M2-T3.2 实现 `impl SandboxDryRunner { pub fn new(pool: Arc<Pool>, config: SandboxConfig) -> Self }`
- [ ] M2-T3.3 实现 `pub async fn dry_run_sandbox(&self, migration: &Migration, shadow_prefix: &str) -> Result<SandboxResult, MigrationError>`：创建影子表 → 执行迁移 SQL → 校验 → 清理
- [ ] M2-T3.4 实现影子表创建：`shadow_` 前缀 + 原表名，复制原表 schema（`CREATE TABLE shadow_xxx LIKE xxx` 或 `CREATE TABLE shadow_xxx AS SELECT * FROM xxx WHERE 1=0`）
- [ ] M2-T3.5 实现迁移 SQL 执行：在影子表上执行迁移 SQL（参数化绑定，复用 `Connection::execute_with_params` `pool.rs:82`），将 SQL 中的原表名替换为影子表名
- [ ] M2-T3.6 实现数据完整性校验：校验影子表数据与原表数据一致性（行数 + 校验和 + 抽样比对），`SandboxVerifyItem::DataIntegrity`
- [ ] M2-T3.7 实现查询兼容性校验：在影子表上执行旧应用查询，校验可执行性（不报错 + 结果结构兼容），`SandboxVerifyItem::QueryCompat`
- [ ] M2-T3.8 实现性能影响校验：在影子表上执行代表性查询，测量执行时间，与原表对比，`SandboxVerifyItem::PerformanceImpact`
- [ ] M2-T3.9 实现影子表清理：预演完成后（无论成功失败）清理影子表，不残留
- [ ] M2-T3.10 定义 `pub struct SandboxResult { passed: bool, reason: String, verify_details: Vec<VerifyDetail> }`（沙箱预演结果）
- [ ] M2-T3.11 实现链式配置方法：`with_verify_items`
- [ ] M2-T3.12 单元测试：沙箱预演迁移 V + 影子表 shadow_users → 在 shadow_users 上执行迁移 V，校验数据完整性/查询兼容性/性能影响，不修改真实 users 表
- [ ] M2-T3.13 单元测试：配置仅验证数据完整性 + 沙箱预演 → 仅校验数据完整性，跳过查询兼容性与性能影响
- [ ] M2-T3.14 边界测试：影子表创建失败（权限不足/表名冲突）→ 中止预演，返回错误"sandbox table creation failed"，不修改真实数据
- [ ] M2-T3.15 边界测试：影子表上迁移 SQL 执行失败 → 中止预演，清理影子表，返回错误含 SQL 执行错误
- [ ] M2-T3.16 性能测试：沙箱预演开销 ≤ 10 秒/迁移

**验收标准**：沙箱预演正确；复用 v4.6.0 `RollbackExecutor` 影子表能力；参数化绑定；不修改真实数据；性能 ≤ 10 秒/迁移

**依赖**：M2-T1

## M2-T4：DependencyAnalyzer 依赖分析器 + 拓扑排序

**任务描述**：实现 `DependencyAnalyzer` 依赖分析器 + `MigrationDependencyGraph` 迁移依赖图，Kahn 算法拓扑排序确定执行顺序 + 循环检测。

**涉及文件**：
- `packages/sz-orm-core/src/forward_compat_sandbox.rs`（扩展：依赖分析器核心）

**子任务**：
- [ ] M2-T4.1 定义 `pub struct MigrationDependencyGraph { nodes: Vec<Migration>, edges: HashMap<MigrationId, Vec<MigrationId>> }`（迁移依赖图，邻接表）
- [ ] M2-T4.2 定义 `pub struct DependencyAnalyzer`（依赖分析器）
- [ ] M2-T4.3 实现 `impl DependencyAnalyzer { pub fn analyze_dependencies(&self, migrations: &[Migration]) -> Result<MigrationDependencyGraph, MigrationError> }`：分析迁移间依赖关系构建图
- [ ] M2-T4.4 实现依赖关系分析：迁移 A 依赖迁移 B 的 schema 变更（如 A 引用 B 创建的表）→ 边 A→B
- [ ] M2-T4.5 实现 Kahn 算法拓扑排序：确定迁移执行顺序（被依赖者在前）
- [ ] M2-T4.6 实现循环检测：拓扑排序后若剩余节点非空，则存在循环依赖，标注循环并返回错误"circular dependency detected"
- [ ] M2-T4.7 实现 `pub fn execution_order(&self) -> Result<Vec<MigrationId>, MigrationError>`：返回拓扑排序后的执行顺序
- [ ] M2-T4.8 单元测试：迁移 A 依赖迁移 B + 依赖图分析 → 生成依赖图，标注 A 依赖 B，执行顺序须 B 先于 A
- [ ] M2-T4.9 单元测试：循环依赖（A 依赖 B，B 依赖 A）→ 标注循环依赖，返回错误"circular dependency detected between migration A and B"
- [ ] M2-T4.10 边界测试：无依赖关系的迁移 → 拓扑排序为任意顺序，无循环
- [ ] M2-T4.11 性能测试：拓扑排序 O(V+E)，1000 迁移 < 100ms

**验收标准**：依赖图分析正确；拓扑排序确定执行顺序；循环检测；性能 O(V+E)

**依赖**：M2-T1

## M2-T5：检查与预演日志 + 异常处理

**任务描述**：实现检查与预演日志记录（迁移版本 + 检查结果 + 预演结果 + 耗时），处理四种异常场景。

**涉及文件**：
- `packages/sz-orm-core/src/forward_compat_sandbox.rs`（扩展：日志 + 异常处理）

**子任务**：
- [ ] M2-T5.1 检查日志：前向兼容性检查 → 记录检查日志（迁移版本 + 破坏性变更列表 + 影响应用 + 建议策略 + 耗时）
- [ ] M2-T5.2 预演日志：沙箱预演 → 记录预演日志（迁移版本 + 影子表 + 验证结果 + 耗时）
- [ ] M2-T5.3 异常处理 1：影子表创建失败 → 中止预演，返回错误"sandbox dry-run failed, table creation error: ..."
- [ ] M2-T5.4 异常处理 2：影子表上迁移 SQL 执行失败 → 中止预演，清理影子表，返回错误"sandbox migration SQL failed: ..."
- [ ] M2-T5.5 异常处理 3：依赖图存在循环依赖 → 标注循环依赖，返回错误"circular dependency detected between migration A and B"
- [ ] M2-T5.6 异常处理 4：兼容性规则匹配异常 → 跳过该规则，记录日志"compat check skipped, rule error"
- [ ] M2-T5.7 单元测试：前向兼容性检查 + 沙箱预演 → 记录检查日志（含破坏性变更列表）+ 预演日志（含验证结果）
- [ ] M2-T5.8 边界测试：日志写入失败 → 不阻断主流程，记录到 stderr

**验收标准**：检查与预演日志可追溯；四种异常场景处理正确

**依赖**：M2-T2、M2-T3、M2-T4

## M2-T6：M2 集成测试与门禁验证

**任务描述**：M2 里程碑集成测试与门禁验证，确保 REQ-V47-002 全部 8 条验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M2-T6.1 集成测试：`ForwardCompatChecker::check_compatibility` 完整流程（analyze_impact → 识别破坏性变更 → 生成 CompatCheckResult）
- [ ] M2-T6.2 集成测试：`SandboxDryRunner::dry_run_sandbox` 完整流程（创建影子表 → 执行迁移 SQL → 校验 → 清理）
- [ ] M2-T6.3 集成测试：`DependencyAnalyzer::analyze_dependencies` 完整流程（构建依赖图 → 拓扑排序 → 循环检测）
- [ ] M2-T6.4 集成测试：复用既有 `DryRunMigration` `packages/sz-orm-core/src/migration_dry_run.rs:11` + `ImpactReport` `:80` + v4.6.0 `RollbackExecutor` `packages/sz-orm-core/src/rollback_zero_downtime.rs:305`，不新建 dry-run/回滚逻辑
- [ ] M2-T6.5 集成测试：五方言覆盖（影子表创建 SQL 按方言适配：PG/MySQL `CREATE TABLE shadow_xxx LIKE xxx`，Oracle/MSSQL/SQLite `CREATE TABLE shadow_xxx AS SELECT * FROM xxx WHERE 1=0`）
- [ ] M2-T6.6 集成测试：参数化绑定验证（沙箱预演 SQL 全部参数化，复用 `Connection::execute_with_params` `pool.rs:82`）
- [ ] M2-T6.7 运行 `cargo test -p sz-orm-core --features forward-compat-sandbox`（全部通过）
- [ ] M2-T6.8 `cargo clippy -p sz-orm-core --features forward-compat-sandbox -- -D warnings`
- [ ] M2-T6.9 `cargo fmt -p sz-orm-core -- --check`
- [ ] M2-T6.10 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-core/src/forward_compat_sandbox.rs` 无占位实现
- [ ] M2-T6.11 扫描 `grep -rn 'unsafe' packages/sz-orm-core/src/forward_compat_sandbox.rs` 无 unsafe 块
- [ ] M2-T6.12 验证默认 feature 行为与 v4.6.0 一致（`cargo build -p sz-orm-core` 无前向兼容性检查与沙箱预演）
- [ ] M2-T6.13 验证 `forward-compat-sandbox` 与既有 `zero-downtime-rollback`/`connection-level-tenant`/`process-l1-cache` feature 组合编译通过

**验收标准**：M2 集成测试通过；门禁通过；默认行为不变；前向兼容检查 + 沙箱预演 + 依赖图全部验证

**依赖**：M2-T1、M2-T2、M2-T3、M2-T4、M2-T5

---

# 五、M3：批量 COPY 协议与并行分片执行（REQ-V47-003，P1，3 天）

**目标**：扩展既有 `sz-orm-batch` 包，新增 `CopyProtocolAdapter` COPY 协议方言适配器 + `ParallelShardExecutor` 并行分片执行器 + `ConflictResolution` 冲突解决策略，复用既有 `CopyProtocolExecutor` + v4.6.0 `BatchTransactionCoordinator`，补齐 MySQL LOAD DATA INFILE / Oracle SQL*Loader / MSSQL BULK INSERT 方言适配 + 并行分片 + 冲突解决。
**对应需求**：REQ-V47-003（spec.md §5.3，design.md §2.1.3.3 + §2.2.2.3）
**预期工作量**：3 天
**依赖**：无（M3 为 P1 独立需求，复用既有 sz-orm-batch CopyProtocolExecutor + v4.6.0 BatchTransactionCoordinator，扩展包可与 M1、M2、M4~M7 并行）

## M3-T1：copy-parallel-shard feature gate + 配置结构

**任务描述**：在 `sz-orm-batch` 完善 `copy-parallel-shard` feature gate 隔离（M0-T2 已创建占位），定义 `ConflictResolution`/`CopyDialect` 枚举 + `ShardConfig` 配置。

**涉及文件**：
- `packages/sz-orm-batch/Cargo.toml`（完善：`copy-parallel-shard` feature 依赖 `batch-atomic`）
- `packages/sz-orm-batch/src/lib.rs`（扩展：模块声明 `mod copy_parallel_shard;`，`#[cfg(feature = "copy-parallel-shard")]` 门控）
- `packages/sz-orm-batch/src/copy_parallel_shard.rs`（新建，COPY 并行分片配置 + 枚举）

**复用标注**：既有 `CopyProtocolExecutor` `packages/sz-orm-batch/src/copy.rs:14`；既有 `BatchExecutor` `packages/sz-orm-batch/src/executor.rs:141`；既有 `BatchExecutorConfig` `packages/sz-orm-batch/src/executor.rs:18`；v4.6.0 `BatchTransactionCoordinator` `packages/sz-orm-batch/src/atomic.rs:216`；v4.6.0 `AtomicityGuarantee` `packages/sz-orm-batch/src/atomic.rs:20`；v4.6.0 `SagaCompensator` `packages/sz-orm-batch/src/atomic.rs:436`

**feature gate 隔离**：`copy-parallel-shard = ["batch-atomic"]`，默认关闭

**子任务**：
- [ ] M3-T1.1 `packages/sz-orm-batch/Cargo.toml` 确认 `copy-parallel-shard = ["batch-atomic"]` feature（M0-T2 已创建）
- [ ] M3-T1.2 `src/lib.rs` 声明 `mod copy_parallel_shard;`，`#[cfg(feature = "copy-parallel-shard")]` 门控
- [ ] M3-T1.3 `src/copy_parallel_shard.rs` 定义 `pub enum ConflictResolution { Upsert, Ignore, Merge, Replace }`（四种冲突解决策略）
- [ ] M3-T1.4 定义 `pub enum CopyDialect { PostgresCopy, MysqlLoadData, OracleSqlLoader, MssqlBulkInsert, MultiValueInsert }`（COPY 方言枚举）
- [ ] M3-T1.5 定义 `pub struct ShardConfig { shard_key: String, shard_count: usize, parallelism: usize, atomicity_guarantee: AtomicityGuarantee, conflict_resolution: ConflictResolution }`（分片配置，复用 v4.6.0 `AtomicityGuarantee` `atomic.rs:20`）
- [ ] M3-T1.6 实现 `impl ShardConfig { pub fn new(shard_key: &str) -> Self }`（默认：4 分片, 并行度=分片数, BestEffort, Upsert）
- [ ] M3-T1.7 实现链式配置方法：`with_shard_count` / `with_parallelism` / `with_atomicity_guarantee` / `with_conflict_resolution`
- [ ] M3-T1.8 单元测试：`ShardConfig::new("id")` 默认值正确（4 分片, BestEffort, Upsert）
- [ ] M3-T1.9 验证 `cargo check -p sz-orm-batch --features copy-parallel-shard` 编译通过

**验收标准**：feature gate 定义；ConflictResolution/CopyDialect/ShardConfig 定义完整；复用 v4.6.0 `AtomicityGuarantee`

**依赖**：M0-T2

## M3-T2：CopyProtocolAdapter COPY 协议方言适配器

**任务描述**：实现 `CopyProtocolAdapter` COPY 协议方言适配器，复用既有 `CopyProtocolExecutor` PG COPY 实现，补齐 MySQL LOAD DATA INFILE / Oracle SQL*Loader / MSSQL BULK INSERT 方言适配，SQLite 降级 multi-value INSERT。

**涉及文件**：
- `packages/sz-orm-batch/src/copy_parallel_shard.rs`（扩展：COPY 方言适配器核心）

**复用标注**：既有 `CopyProtocolExecutor` `packages/sz-orm-batch/src/copy.rs:14`（PG COPY 实现复用）

**子任务**：
- [ ] M3-T2.1 定义 `pub struct CopyProtocolAdapter { executor: CopyProtocolExecutor }`（COPY 方言适配器，复用既有 `CopyProtocolExecutor`）
- [ ] M3-T2.2 实现 `impl CopyProtocolAdapter { pub fn new() -> Self }`
- [ ] M3-T2.3 实现 `pub async fn execute_copy(&self, table: &str, data: &[Row], dialect: CopyDialect, conflict: ConflictResolution) -> Result<u64, BatchError>`：按方言选择批量加载协议
- [ ] M3-T2.4 实现 PostgreSQL/GaussDB/Kingbase/PolarDB 方言：`COPY table FROM STDIN`（复用既有 `CopyProtocolExecutor` `copy.rs:14`）
- [ ] M3-T2.5 实现 MySQL 方言：`LOAD DATA INFILE`
- [ ] M3-T2.6 实现 Oracle 方言：`SQL*Loader`（生成控制文件 + 数据文件）
- [ ] M3-T2.7 实现 MSSQL 方言：`BULK INSERT`
- [ ] M3-T2.8 实现 SQLite 方言：不支持 COPY，降级为 multi-value INSERT，标注"fallback to multi-value INSERT"
- [ ] M3-T2.9 实现冲突解决 SQL 生成：Upsert → `ON CONFLICT DO UPDATE`；Ignore → `ON CONFLICT DO NOTHING`；Replace → `REPLACE INTO`；Merge → 自定义合并函数
- [ ] M3-T2.10 单元测试：PostgreSQL + COPY 100 万行 → 使用 `COPY table FROM STDIN`，性能不低于 multi-value INSERT 3 倍
- [ ] M3-T2.11 单元测试：MySQL + LOAD DATA 100 万行 → 使用 `LOAD DATA INFILE`
- [ ] M3-T2.12 单元测试：SQLite 不支持 COPY → 降级为 multi-value INSERT，标注"COPY not supported, fallback to multi-value INSERT"
- [ ] M3-T2.13 单元测试：Upsert + 主键冲突 → 更新已有行；Ignore + 主键冲突 → 跳过冲突行；Replace + 主键冲突 → 删除已有行再插入
- [ ] M3-T2.14 边界测试：COPY 协议加载失败（数据格式错误/约束冲突）→ 按冲突解决策略处理
- [ ] M3-T2.15 性能测试：COPY 协议加载 100 万行 ≤ 10 秒

**验收标准**：五方言适配正确；复用既有 `CopyProtocolExecutor` PG COPY；冲突解决四种策略；性能 ≤ 10 秒/100 万行

**依赖**：M3-T1

## M3-T3：ParallelShardExecutor 并行分片执行器

**任务描述**：实现 `ParallelShardExecutor` 并行分片执行器，按分片键拆分数据为 N 分片，`tokio::join!` 并行执行，复用 v4.6.0 `BatchTransactionCoordinator` 保证分片间原子性，并行度限制到连接池容量。

**涉及文件**：
- `packages/sz-orm-batch/src/copy_parallel_shard.rs`（扩展：并行分片执行器核心）

**复用标注**：v4.6.0 `BatchTransactionCoordinator` `packages/sz-orm-batch/src/atomic.rs:216`（原子性协调复用）；v4.6.0 `AtomicityGuarantee` `packages/sz-orm-batch/src/atomic.rs:20`；v4.6.0 `SagaCompensator` `packages/sz-orm-batch/src/atomic.rs:436`；既有 `Pool` `packages/sz-orm-core/src/pool.rs:743`

**子任务**：
- [ ] M3-T3.1 定义 `pub struct ParallelShardExecutor { adapter: CopyProtocolAdapter, coordinator: BatchTransactionCoordinator, pool: Arc<Pool> }`（并行分片执行器，复用 v4.6.0 `BatchTransactionCoordinator`）
- [ ] M3-T3.2 实现 `impl ParallelShardExecutor { pub fn new(pool: Arc<Pool>) -> Self }`
- [ ] M3-T3.3 实现 `pub async fn execute_copy_shards(&self, data: &[Row], config: &ShardConfig) -> Result<CopyBatchResult, BatchError>`：按分片键拆分 → 并行执行 → 结果合并
- [ ] M3-T3.4 实现分片拆分：按分片键（hash 或 range）将数据拆分为 N 分片，记录分片数据量供负载均衡分析
- [ ] M3-T3.5 实现并行执行：`tokio::join!` 并行执行 N 分片，每分片调用 `CopyProtocolAdapter.execute_copy` 按方言加载
- [ ] M3-T3.6 实现并行度限制：并行度限制到连接池容量（`Pool` `pool.rs:743`），多余分片排队等待
- [ ] M3-T3.7 实现原子性处理：全部分片成功则提交（复用 `BatchTransactionCoordinator.commit` `atomic.rs:216`）；某分片失败按 `AtomicityGuarantee` 处理
- [ ] M3-T3.8 实现 `AllOrNothing` 模式：分片失败全部回滚，不产生部分加载
- [ ] M3-T3.9 实现 `BestEffort` 模式：分片失败标记失败分片，其他分片成功
- [ ] M3-T3.10 实现 `SagaCompensation` 模式：调用 `SagaCompensator` `atomic.rs:436` 补偿回滚已成功分片
- [ ] M3-T3.11 定义 `pub struct CopyBatchResult { loaded_rows: u64, shard_results: Vec<ShardResult>, conflict_resolution: ConflictResolution, elapsed: Duration }`（COPY 批量结果）
- [ ] M3-T3.12 单元测试：批量加载 100 万行 + 4 分片 → 拆分为 4 分片并行执行，吞吐量不低于单分片 4 倍
- [ ] M3-T3.13 单元测试：COPY + 4 分片 + 100 万行 → 4 分片各自使用 COPY 协议并行加载 25 万行
- [ ] M3-T3.14 单元测试：AllOrNothing + 4 分片 + 分片 2 失败 → 全部回滚，不产生部分加载
- [ ] M3-T3.15 单元测试：BestEffort + 分片 2 失败 → 分片 1/3/4 成功，分片 2 标记失败
- [ ] M3-T3.16 边界测试：分片键不均匀导致负载倾斜 → 按分片键拆分执行，记录分片数据量供优化
- [ ] M3-T3.17 边界测试：并行分片数超限（超过连接池容量）→ 限制并行度到连接池容量，多余分片排队等待
- [ ] M3-T3.18 性能测试：并行分片吞吐量不低于单分片 N 倍（N=分片数）

**验收标准**：并行分片正确；复用 v4.6.0 `BatchTransactionCoordinator`；三种原子性级别；并行度限制；吞吐量 N 倍

**依赖**：M3-T1、M3-T2

## M3-T4：加载日志 + 异常处理

**任务描述**：实现加载日志记录（加载行数 + 分片列表 + 冲突解决 + 加载耗时 + 结果），处理四种异常场景。

**涉及文件**：
- `packages/sz-orm-batch/src/copy_parallel_shard.rs`（扩展：加载日志 + 异常处理）

**子任务**：
- [ ] M3-T4.1 加载日志：COPY 并行分片加载 → 记录加载日志（加载行数 + 分片列表 + 冲突解决策略 + 加载耗时 + 结果）
- [ ] M3-T4.2 异常处理 1：COPY 协议方言不支持（SQLite）→ 降级为 multi-value INSERT，标注"fallback to multi-value INSERT"
- [ ] M3-T4.3 异常处理 2：分片键不均匀导致负载倾斜 → 记录分片数据量供优化，可配再平衡策略
- [ ] M3-T4.4 异常处理 3：COPY 协议加载失败 → 按冲突解决策略处理，策略无法解决时按原子性级别处理
- [ ] M3-T4.5 异常处理 4：并行分片数超限 → 限制并行度到连接池容量，日志标注"parallelism limited by pool capacity"
- [ ] M3-T4.6 单元测试：COPY 并行分片加载 100 万行 → 记录加载日志，含行数 + 分片列表 + 冲突解决策略 + 耗时 + 结果
- [ ] M3-T4.7 边界测试：加载日志写入失败 → 不阻断主流程，记录到 stderr

**验收标准**：加载日志可追溯；四种异常场景处理正确

**依赖**：M3-T2、M3-T3

## M3-T5：COPY 数据按租户隔离 + 参数化

**任务描述**：确保 COPY 协议加载数据按租户隔离（多租户环境下数据按 tenant_id 隔离），COPY 协议数据加载参数化绑定或 COPY 协议原生参数化。

**涉及文件**：
- `packages/sz-orm-batch/src/copy_parallel_shard.rs`（扩展：租户隔离 + 参数化）

**复用标注**：既有 `CopyProtocolExecutor` `packages/sz-orm-batch/src/copy.rs:14`（COPY 协议原生参数化复用）

**子任务**：
- [ ] M3-T5.1 COPY 协议加载数据按租户隔离：加载数据含 `tenant_id` 列，按租户隔离
- [ ] M3-T5.2 COPY 协议数据加载参数化绑定或 COPY 协议原生参数化（复用既有 `CopyProtocolExecutor` `copy.rs:14`）
- [ ] M3-T5.3 单元测试：租户 1 数据 + 租户 2 数据 → 按租户隔离加载，不跨租户
- [ ] M3-T5.4 单元测试：COPY 协议加载 → 参数化绑定，无 SQL 注入风险

**验收标准**：COPY 数据按租户隔离；参数化绑定；复用既有 `CopyProtocolExecutor`

**依赖**：M3-T2

## M3-T6：M3 集成测试与门禁验证

**任务描述**：M3 里程碑集成测试与门禁验证，确保 REQ-V47-003 全部 8 条验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M3-T6.1 集成测试：`CopyProtocolAdapter::execute_copy` 完整流程（按方言选择 → COPY/LOAD DATA/SQL*Loader/BULK INSERT/降级 → 冲突解决）
- [ ] M3-T6.2 集成测试：`ParallelShardExecutor::execute_copy_shards` 完整流程（分片拆分 → 并行执行 → 原子性处理 → 结果合并）
- [ ] M3-T6.3 集成测试：五方言覆盖（PostgreSQL COPY / MySQL LOAD DATA / Oracle SQL*Loader / MSSQL BULK INSERT / SQLite 降级 multi-value INSERT）
- [ ] M3-T6.4 集成测试：四种冲突解决策略（Upsert/Ignore/Merge/Replace）+ 三种原子性级别（AllOrNothing/BestEffort/SagaCompensation）
- [ ] M3-T6.5 集成测试：复用既有 `CopyProtocolExecutor` `packages/sz-orm-batch/src/copy.rs:14` + v4.6.0 `BatchTransactionCoordinator` `packages/sz-orm-batch/src/atomic.rs:216` + `AtomicityGuarantee` `:20` + `SagaCompensator` `:436`，不新建 COPY/批量执行逻辑
- [ ] M3-T6.6 集成测试：真实 DB 集成测试（PostgreSQL COPY 100 万行 + MySQL LOAD DATA 100 万行，性能 ≥ multi-value INSERT 3 倍）
- [ ] M3-T6.7 运行 `cargo test -p sz-orm-batch --features copy-parallel-shard`（全部通过）
- [ ] M3-T6.8 `cargo clippy -p sz-orm-batch --features copy-parallel-shard -- -D warnings`
- [ ] M3-T6.9 `cargo fmt -p sz-orm-batch -- --check`
- [ ] M3-T6.10 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-batch/src/copy_parallel_shard.rs` 无占位实现
- [ ] M3-T6.11 扫描 `grep -rn 'unsafe' packages/sz-orm-batch/src/copy_parallel_shard.rs` 无 unsafe 块
- [ ] M3-T6.12 验证默认 feature 行为与 v4.6.0 一致（`cargo build -p sz-orm-batch` 无 COPY 方言适配与并行分片）
- [ ] M3-T6.13 验证 `copy-parallel-shard` 与既有 `batch-atomic`/`batch-v2`/`batch-stream` feature 组合编译通过

**验收标准**：M3 集成测试通过；门禁通过；默认行为不变；COPY 方言适配 + 并行分片 + 冲突解决全部验证

**依赖**：M3-T1、M3-T2、M3-T3、M3-T4、M3-T5

---

# 六、M4：租户资源配额与行级安全增强（REQ-V47-006，P1，2.5 天）

**目标**：扩展既有 `sz-orm-core` 多租户，新增 `TenantResourceQuota` 租户资源配额 + `QuotaEnforcer` 配额执行器 + `RlsPolicyEnhancer` RLS 策略增强器 + `TenantAuditLogger` 租户级审计日志器，复用 v4.6.0 `ConnectionTenantBinder` + 既有 `RowLevelSecurityPolicy`/`ColumnMaskingRule`/`Pool`，补齐租户资源配额 + RLS 策略增强 + 租户级审计日志。
**对应需求**：REQ-V47-006（spec.md §5.6，design.md §2.1.3.6 + §2.2.2.6）
**预期工作量**：2.5 天
**依赖**：无（M4 为 P1 独立需求，复用 v4.6.0 ConnectionTenantBinder + 既有 RowLevelSecurityPolicy/Pool，扩展包可与 M1~M3、M5~M7 并行）

## M4-T1：tenant-quota-rls-enhanced feature gate + 配置结构

**任务描述**：在 `sz-orm-core` 完善 `tenant-quota-rls-enhanced` feature gate 隔离（M0-T2 已创建占位），定义 `QuotaEnforceStrategy`/`QuotaResource`/`AuditLogLevel` 枚举 + `TenantQuotaRlsConfig` 配置 + `TenantResourceQuota` 配额结构。

**涉及文件**：
- `packages/sz-orm-core/Cargo.toml`（完善：`tenant-quota-rls-enhanced` feature 依赖 `connection-level-tenant`）
- `packages/sz-orm-core/src/lib.rs`（扩展：模块声明 `mod tenant_quota_rls;`，`#[cfg(feature = "tenant-quota-rls-enhanced")]` 门控）
- `packages/sz-orm-core/src/tenant_quota_rls.rs`（新建，租户配额 RLS 配置 + 枚举）

**复用标注**：v4.6.0 `ConnectionTenantBinder` `packages/sz-orm-core/src/connection_tenant.rs:133`；v4.6.0 `ConnectionLevelIsolation` `packages/sz-orm-core/src/connection_tenant.rs:24`；v4.6.0 `TenantConnectionGuard` `packages/sz-orm-core/src/connection_tenant.rs:249`；既有 `RowLevelSecurityPolicy` `packages/sz-orm-core/src/tenant_security.rs:67`；既有 `ColumnMaskingRule` `packages/sz-orm-core/src/tenant_security.rs:155`；既有 `TenantAuditContext` `packages/sz-orm-core/src/tenant_security.rs:244`；既有 `TenantAuditOperation` `packages/sz-orm-core/src/tenant_security.rs:197`；既有 `AuditResult` `packages/sz-orm-core/src/tenant_security.rs:224`；既有 `TenantContext` `packages/sz-orm-core/src/tenant_context.rs:80`；既有 `Pool` `packages/sz-orm-core/src/pool.rs:743`

**feature gate 隔离**：`tenant-quota-rls-enhanced = ["connection-level-tenant"]`，默认关闭

**子任务**：
- [ ] M4-T1.1 `packages/sz-orm-core/Cargo.toml` 确认 `tenant-quota-rls-enhanced = ["connection-level-tenant"]` feature（M0-T2 已创建）
- [ ] M4-T1.2 `src/lib.rs` 声明 `mod tenant_quota_rls;`，`#[cfg(feature = "tenant-quota-rls-enhanced")]` 门控
- [ ] M4-T1.3 `src/tenant_quota_rls.rs` 定义 `pub enum QuotaEnforceStrategy { FailClose, FailOpen }`（配额执行策略）
- [ ] M4-T1.4 定义 `pub enum QuotaResource { Connection, Qps, Storage }`（配额资源类型）
- [ ] M4-T1.5 定义 `pub enum AuditLogLevel { Full, Summary, Off }`（审计日志级别）
- [ ] M4-T1.6 定义 `pub struct TenantResourceQuota { max_connections: Option<u32>, max_qps: Option<u32>, max_storage: Option<u64> }`（租户资源配额）
- [ ] M4-T1.7 定义 `pub struct TenantQuotaRlsConfig { quota_enforce_strategy, rls_enhancement_enabled, audit_log_level, quota_check_enabled }`（配额与 RLS 配置）
- [ ] M4-T1.8 实现 `impl TenantQuotaRlsConfig { pub fn new() -> Self }`（默认：FailClose, RLS 增强 false, Summary, 配额检查 false）
- [ ] M4-T1.9 实现链式配置方法：`with_quota_enforce_strategy` / `with_rls_enhancement` / `with_audit_log_level` / `with_quota_check`
- [ ] M4-T1.10 单元测试：`TenantQuotaRlsConfig::new()` 默认值正确（FailClose, RLS false, Summary, 配额 false）
- [ ] M4-T1.11 验证 `cargo check -p sz-orm-core --features tenant-quota-rls-enhanced` 编译通过

**验收标准**：feature gate 定义；配置结构定义完整；复用 v4.6.0 `ConnectionTenantBinder` + 既有 `RowLevelSecurityPolicy`

**依赖**：M0-T2

## M4-T2：QuotaEnforcer 配额执行器

**任务描述**：实现 `QuotaEnforcer` 配额执行器，在 `ConnectionTenantBinder.acquire_with_tenant` 路径上插入配额检查（max_connections/max_qps/max_storage），超限拒绝请求，使用 `AtomicU32`/`AtomicU64` 原子计数器无锁检查。

**涉及文件**：
- `packages/sz-orm-core/src/tenant_quota_rls.rs`（扩展：配额执行器核心）

**复用标注**：v4.6.0 `ConnectionTenantBinder` `packages/sz-orm-core/src/connection_tenant.rs:133`（`acquire_with_tenant` 路径复用）；v4.6.0 `TenantConnectionGuard` `packages/sz-orm-core/src/connection_tenant.rs:249`；既有 `Pool` `packages/sz-orm-core/src/pool.rs:743`（`AtomicU32` 无锁模式复用）

**子任务**：
- [ ] M4-T2.1 定义 `pub struct QuotaEnforcer { quotas: RwLock<HashMap<String, TenantResourceQuota>>, counters: HashMap<String, QuotaCounters>, strategy: QuotaEnforceStrategy }`（配额执行器，`AtomicU32`/`AtomicU64` 原子计数器）
- [ ] M4-T2.2 定义 `struct QuotaCounters { current_connections: AtomicU32, current_qps: AtomicU32, current_storage: AtomicU64 }`（原子计数器，无锁）
- [ ] M4-T2.3 实现 `impl QuotaEnforcer { pub fn new(strategy: QuotaEnforceStrategy) -> Self }`
- [ ] M4-T2.4 实现 `pub async fn check_quota(&self, tenant_id: &str, resource: QuotaResource, current: u64) -> Result<(), QuotaError>`：检查当前值是否超限 → 超限拒绝 + 记录审计日志
- [ ] M4-T2.5 实现连接获取配额检查：在 `ConnectionTenantBinder.acquire_with_tenant` `connection_tenant.rs:133` 路径上检查 `max_connections`，当前连接数 >= 配额则拒绝
- [ ] M4-T2.6 实现查询执行配额检查：查询执行前检查 `max_qps`，当前 QPS >= 配额则拒绝
- [ ] M4-T2.7 实现存储配额检查：写入操作前检查 `max_storage`，当前存储 >= 配额则拒绝
- [ ] M4-T2.8 实现配额检查失败处理：配额存储不可用时按 `QuotaEnforceStrategy` 处理（`FailClose` 拒绝 / `FailOpen` 放行），记录日志
- [ ] M4-T2.9 实现链式配置方法：`with_strategy` / `set_quota`（设置租户配额）
- [ ] M4-T2.10 单元测试：租户 1 配额 max_connections=10 + 当前 10 连接 + 新请求 → 拒绝新连接，返回错误"quota exceeded: max_connections"
- [ ] M4-T2.11 单元测试：max_qps=100 + 当前 100 QPS + 新查询 → 拒绝查询，返回错误"quota exceeded: max_qps"
- [ ] M4-T2.12 单元测试：未配置配额 → 无配额限制，向后兼容
- [ ] M4-T2.13 边界测试：配额检查失败（配额存储不可用）+ FailClose → 拒绝请求；FailOpen → 放行
- [ ] M4-T2.14 边界测试：配额值配置错误（负数/超大值）→ 拒绝配置，返回错误"invalid quota value"
- [ ] M4-T2.15 性能测试：配额检查开销 ≤ 0.1ms/次（`AtomicU32` 原子操作）

**验收标准**：配额检查正确；复用 v4.6.0 `ConnectionTenantBinder`；三种资源配额；无锁原子计数器；性能 ≤ 0.1ms/次

**依赖**：M4-T1

## M4-T3：RlsPolicyEnhancer RLS 策略增强器

**任务描述**：实现 `RlsPolicyEnhancer` RLS 策略增强器 + `EnhancedRlsPolicy` �强行级安全策略，支持多条件组合 + 复杂谓词 + 列级脱敏联动，自动注入 WHERE 条件（参数化绑定），tenant_id 不可被客户端篡改。

**涉及文件**：
- `packages/sz-orm-core/src/tenant_quota_rls.rs`（扩展：RLS 策略增强器核心）

**复用标注**：既有 `RowLevelSecurityPolicy` `packages/sz-orm-core/src/tenant_security.rs:67`（WHERE 注入复用）；既有 `ColumnMaskingRule` `packages/sz-orm-core/src/tenant_security.rs:155`（列级脱敏联动）；既有 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82`（参数化绑定复用）；既有 `TenantContext` `packages/sz-orm-core/src/tenant_context.rs:80`（可信路径设置 tenant_id）

**子任务**：
- [ ] M4-T3.1 定义 `pub struct EnhancedRlsPolicy { conditions: Vec<RlsCondition>, masking_rules: Vec<ColumnMaskingRule>, priority: i32 }`（增强行级安全策略，多条件组合 + 列级脱敏联动）
- [ ] M4-T3.2 定义 `pub struct RlsCondition { column: String, operator: RlsOperator, value: Vec<ParameterValue> }`（RLS 条件，参数化值）
- [ ] M4-T3.3 定义 `pub enum RlsOperator { Eq, In, NotIn, Gt, Lt, Between }`（RLS 操作符）
- [ ] M4-T3.4 定义 `pub struct RlsPolicyEnhancer { policies: Vec<EnhancedRlsPolicy> }`（RLS 策略增强器，复用既有 `RowLevelSecurityPolicy`）
- [ ] M4-T3.5 实现 `impl RlsPolicyEnhancer { pub fn new() -> Self }`
- [ ] M4-T3.6 实现 `pub fn enhance_query(&self, query: &mut Query, tenant_id: &str) -> Result<(), QuotaError>`：匹配增强 RLS 策略 → 注入多条件 WHERE（参数化）→ 与列级脱敏联动
- [ ] M4-T3.7 实现 WHERE 条件注入：生成 `tenant_id = ? AND dept_id IN (?,?)`，参数化绑定（复用 `Connection::execute_with_params` `pool.rs:82`）
- [ ] M4-T3.8 实现多策略匹配冲突处理：多个 RLS 策略匹配同一查询时按 `priority` 选择最高优先级，记录冲突日志
- [ ] M4-T3.9 实现列级脱敏联动：RLS 注入后应用 `ColumnMaskingRule` `tenant_security.rs:155` 列级脱敏
- [ ] M4-T3.10 实现 tenant_id 防篡改：tenant_id 由可信路径设置（`TenantContext` `tenant_context.rs:80`），客户端篡改拒绝
- [ ] M4-T3.11 实现链式配置方法：`with_policy` / `add_policy`
- [ ] M4-T3.12 单元测试：增强 RLS 策略"tenant_id=1 AND dept_id IN (1,2)" + 查询 → 自动注入 `WHERE tenant_id = ? AND dept_id IN (?,?)`，参数化绑定
- [ ] M4-T3.13 单元测试：多个 RLS 策略匹配同一查询 → 按优先级选择最高优先级，记录冲突日志
- [ ] M4-T3.14 单元测试：客户端尝试篡改 tenant_id → 拒绝篡改，使用可信路径设置的 tenant_id
- [ ] M4-T3.15 边界测试：RLS 策略匹配冲突 → 按策略优先级选择最高优先级策略，记录冲突日志
- [ ] M4-T3.16 性能测试：RLS 自动注入开销 ≤ 0.2ms/查询

**验收标准**：RLS 策略增强正确；多条件组合 + 参数化绑定；列级脱敏联动；tenant_id 防篡改；性能 ≤ 0.2ms/查询

**依赖**：M4-T1

## M4-T4：TenantAuditLogger 租户级审计日志器

**任务描述**：实现 `TenantAuditLogger` 租户级审计日志器，按租户独立记录审计日志（连接获取/查询执行/配额超限/RLS 命中），追加写入不可篡改，复用既有 `TenantAuditContext`/`TenantAuditOperation`/`AuditResult`。

**涉及文件**：
- `packages/sz-orm-core/src/tenant_quota_rls.rs`（扩展：租户审计日志器核心）

**复用标注**：既有 `TenantAuditContext` `packages/sz-orm-core/src/tenant_security.rs:244`；既有 `TenantAuditOperation` `packages/sz-orm-core/src/tenant_security.rs:197`；既有 `AuditResult` `packages/sz-orm-core/src/tenant_security.rs:224`

**子任务**：
- [ ] M4-T4.1 定义 `pub struct TenantAuditLogger { log_level: AuditLogLevel, writer: Mutex<AuditWriter> }`（租户级审计日志器，`Mutex` 保护追加写入）
- [ ] M4-T4.2 定义 `pub struct TenantAuditEntry { tenant_id: String, operation: TenantAuditOperation, table: String, timestamp: DateTime<Utc>, result: AuditResult, details: String }`（审计条目，复用既有 `TenantAuditOperation`/`AuditResult`）
- [ ] M4-T4.3 实现 `impl TenantAuditLogger { pub fn new(log_level: AuditLogLevel) -> Self }`
- [ ] M4-T4.4 实现 `pub async fn log(&self, entry: TenantAuditEntry) -> Result<(), QuotaError>`：按租户独立记录 → 追加写入不可篡改
- [ ] M4-T4.5 实现审计日志级别过滤：`Full` 记录全部 / `Summary` 记录摘要 / `Off` 不记录
- [ ] M4-T4.6 实现追加写入不可篡改：审计日志文件追加写入（append-only），不可修改/删除
- [ ] M4-T4.7 实现审计详情脱敏：审计详情脱敏敏感字段
- [ ] M4-T4.8 单元测试：租户 1 查询 users 表 → 记录审计日志，含租户 ID + 操作 + 表 + 时间 + 结果，追加写入
- [ ] M4-T4.9 单元测试：配额超限 → 记录配额超限日志，含租户 ID + 配额类型 + 当前值 + 限制值
- [ ] M4-T4.10 边界测试：审计日志写入失败（磁盘满）→ 记录到备用日志（stderr），告警 SRE，不阻断主流程

**验收标准**：租户级审计日志正确；追加写入不可篡改；复用既有 `TenantAuditContext`/`TenantAuditOperation`/`AuditResult`；审计详情脱敏

**依赖**：M4-T1

## M4-T5：配额与 RLS 集成 + 异常处理

**任务描述**：集成 `QuotaEnforcer` + `RlsPolicyEnhancer` + `TenantAuditLogger` 到 `ConnectionTenantBinder.acquire_with_tenant` 路径，处理四种异常场景。

**涉及文件**：
- `packages/sz-orm-core/src/tenant_quota_rls.rs`（扩展：集成 + 异常处理）

**子任务**：
- [ ] M4-T5.1 集成：`ConnectionTenantBinder.acquire_with_tenant` 路径上先 `QuotaEnforcer.check_quota` → 再 `Pool.acquire` → 再 `RlsPolicyEnhancer.enhance_query` → 最后 `TenantAuditLogger.log`
- [ ] M4-T5.2 异常处理 1：配额检查失败 → 按 `QuotaEnforceStrategy` 处理，记录日志
- [ ] M4-T5.3 异常处理 2：RLS 策略匹配冲突 → 按策略优先级选择最高优先级策略，记录冲突日志
- [ ] M4-T5.4 异常处理 3：审计日志写入失败 → 记录到备用日志（stderr），告警 SRE，不阻断主流程
- [ ] M4-T5.5 异常处理 4：配额值配置错误（负数/超大值）→ 拒绝配置，返回错误"invalid quota value"
- [ ] M4-T5.6 单元测试：集成流程（配额检查 → 连接获取 → RLS 注入 → 审计日志）完整验证
- [ ] M4-T5.7 单元测试：配额与审计日志可追溯（租户 ID + 配额类型 + 当前值 + 限制值 + 操作 + 时间）

**验收标准**：集成正确；四种异常场景处理正确；配额与审计日志可追溯

**依赖**：M4-T2、M4-T3、M4-T4

## M4-T6：M4 集成测试与门禁验证

**任务描述**：M4 里程碑集成测试与门禁验证，确保 REQ-V47-006 全部 9 条验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M4-T6.1 集成测试：`QuotaEnforcer::check_quota` 完整流程（三种资源配额 + 超限拒绝 + FailClose/FailOpen）
- [ ] M4-T6.2 集成测试：`RlsPolicyEnhancer::enhance_query` 完整流程（多条件组合 + 参数化绑定 + 列级脱敏联动 + 防篡改）
- [ ] M4-T6.3 集成测试：`TenantAuditLogger::log` 完整流程（按租户独立 + 追加写入 + 不可篡改 + 脱敏）
- [ ] M4-T6.4 集成测试：配额检查在连接池/查询层强制执行（不可被应用层绕过）
- [ ] M4-T6.5 集成测试：复用 v4.6.0 `ConnectionTenantBinder` `packages/sz-orm-core/src/connection_tenant.rs:133` + 既有 `RowLevelSecurityPolicy` `packages/sz-orm-core/src/tenant_security.rs:67` + `Pool` `packages/sz-orm-core/src/pool.rs:743`，不新建连接池/多租户逻辑
- [ ] M4-T6.6 集成测试：五方言覆盖（RLS 自动注入 WHERE 全方言支持，参数化绑定）
- [ ] M4-T6.7 运行 `cargo test -p sz-orm-core --features tenant-quota-rls-enhanced`（全部通过）
- [ ] M4-T6.8 `cargo clippy -p sz-orm-core --features tenant-quota-rls-enhanced -- -D warnings`
- [ ] M4-T6.9 `cargo fmt -p sz-orm-core -- --check`
- [ ] M4-T6.10 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-core/src/tenant_quota_rls.rs` 无占位实现
- [ ] M4-T6.11 扫描 `grep -rn 'unsafe' packages/sz-orm-core/src/tenant_quota_rls.rs` 无 unsafe 块
- [ ] M4-T6.12 验证默认 feature 行为与 v4.6.0 一致（`cargo build -p sz-orm-core` 无配额与 RLS 增强）
- [ ] M4-T6.13 验证 `tenant-quota-rls-enhanced` 与既有 `connection-level-tenant`/`forward-compat-sandbox`/`cache-warmup-protection` feature 组合编译通过

**验收标准**：M4 集成测试通过；门禁通过；默认行为不变；配额 + RLS 增强 + 审计日志全部验证

**依赖**：M4-T1、M4-T2、M4-T3、M4-T4、M4-T5

---

# 七、M5：缓存预热与穿透防护（REQ-V47-007，P1，2.5 天）

**目标**：扩展既有 `sz-orm-core` 缓存，新增 `CacheWarmer` 缓存预热器 + `BloomFilter` 布隆过滤器 + `PenetrationGuard` 穿透防护器 + `SingleFlight` 击穿防护器，复用 v4.6.0 `ProcessL1Cache` + 既有 `L2Cache`/`CacheCoherenceProtocol`，补齐缓存预热 + 穿透防护 + 击穿防护。
**对应需求**：REQ-V47-007（spec.md §5.7，design.md §2.1.3.7 + §2.2.2.7）
**预期工作量**：2.5 天
**依赖**：无（M5 为 P1 独立需求，复用 v4.6.0 ProcessL1Cache + 既有 L2Cache，扩展包可与 M1~M4、M6~M7 并行）

## M5-T1：cache-warmup-protection feature gate + 配置结构

**任务描述**：在 `sz-orm-core` 完善 `cache-warmup-protection` feature gate 隔离（M0-T2 已创建占位），定义 `WarmupStrategy` 枚举 + `WarmupProtectionConfig` 配置。

**涉及文件**：
- `packages/sz-orm-core/Cargo.toml`（完善：`cache-warmup-protection` feature 依赖 `process-l1-cache`）
- `packages/sz-orm-core/src/lib.rs`（扩展：模块声明 `mod cache_warmup_protection;`，`#[cfg(feature = "cache-warmup-protection")]` 门控）
- `packages/sz-orm-core/src/cache_warmup_protection.rs`（新建，缓存预热防护配置 + 枚举）

**复用标注**：v4.6.0 `ProcessL1Cache` `packages/sz-orm-core/src/process_l1_cache.rs:169`；v4.6.0 `CrossSessionIdentityMap` `packages/sz-orm-core/src/process_l1_cache.rs:366`；v4.6.0 `ProcessL1Config` `packages/sz-orm-core/src/process_l1_cache.rs:44`；既有 `L2Cache` `packages/sz-orm-core/src/l2_cache.rs:517`；既有 `CacheKey` `packages/sz-orm-core/src/l2_cache.rs:143`；既有 `CacheCoherenceProtocol` `packages/sz-orm-core/src/cache_coherence.rs:103`；既有 `MesiState` `packages/sz-orm-core/src/cache_coherence.rs:12`

**feature gate 隔离**：`cache-warmup-protection = ["process-l1-cache"]`，默认关闭

**子任务**：
- [ ] M5-T1.1 `packages/sz-orm-core/Cargo.toml` 确认 `cache-warmup-protection = ["process-l1-cache"]` feature（M0-T2 已创建）
- [ ] M5-T1.2 `src/lib.rs` 声明 `mod cache_warmup_protection;`，`#[cfg(feature = "cache-warmup-protection")]` 门控
- [ ] M5-T1.3 `src/cache_warmup_protection.rs` 定义 `pub enum WarmupStrategy { HotspotTable(String), HotspotKey(Vec<String>), CustomQuery(String), Disabled }`（预热策略）
- [ ] M5-T1.4 定义 `pub struct WarmupConfig { strategy: WarmupStrategy, batch_size: usize }`（预热配置）
- [ ] M5-T1.5 定义 `pub struct WarmupProtectionConfig { warmup_strategy, warmup_batch_size, bloom_filter_capacity, bloom_filter_fpp, singleflight_timeout_ms, penetration_guard_enabled, stampede_guard_enabled }`（预热与防护配置）
- [ ] M5-T1.6 实现 `impl WarmupProtectionConfig { pub fn new() -> Self }`（默认：Disabled, 1000, 1000000, 0.01, 5000, false, false）
- [ ] M5-T1.7 实现链式配置方法：`with_warmup_strategy` / `with_bloom_filter` / `with_singleflight_timeout` / `with_penetration_guard` / `with_stampede_guard`
- [ ] M5-T1.8 单元测试：`WarmupProtectionConfig::new()` 默认值正确（Disabled, 1000, 1000000, 0.01, 5000, false, false）
- [ ] M5-T1.9 验证 `cargo check -p sz-orm-core --features cache-warmup-protection` 编译通过

**验收标准**：feature gate 定义；WarmupStrategy/WarmupProtectionConfig 定义完整；复用 v4.6.0 `ProcessL1Cache`

**依赖**：M0-T2

## M5-T2：BloomFilter 布隆过滤器

**任务描述**：实现 `BloomFilter` 布隆过滤器，多哈希函数 + 位数组查询，不漏判（不存在的 key 一定返回 false），误判率可配（默认 1%）。

**涉及文件**：
- `packages/sz-orm-core/src/cache_warmup_protection.rs`（扩展：布隆过滤器核心）

**子任务**：
- [ ] M5-T2.1 定义 `pub struct BloomFilter { bit_array: RwLock<Vec<u64>>, hash_functions: Vec<HashFn>, capacity: usize, fpp: f64 }`（布隆过滤器，`RwLock` 保护位数组）
- [ ] M5-T2.2 实现 `impl BloomFilter { pub fn new(capacity: usize, fpp: f64) -> Self }`：计算最优哈希函数数 `k = m/n * ln2` 与位数组大小 `m = -n*ln(p)/(ln2)^2`
- [ ] M5-T2.3 实现 `pub fn add(&self, key: &str)`：多哈希函数计算位位置 → 设置位数组
- [ ] M5-T2.4 实现 `pub fn might_contain(&self, key: &str) -> bool`：多哈希函数计算位位置 → 查询位数组（不漏判：不存在一定返回 false）
- [ ] M5-T2.5 实现 `pub fn add_keys(&self, keys: &[String])`：批量添加 key
- [ ] M5-T2.6 实现容量超限处理：按配置策略处理（Rebuild 重建 / Evict 淘汰 / Degrade 降级为全查 DB），记录日志
- [ ] M5-T2.7 实现按租户隔离：key 含租户前缀（`tenant_id:key`），按租户隔离
- [ ] M5-T2.8 单元测试：添加 key "user:1" → `might_contain("user:1")` 返回 true
- [ ] M5-T2.9 单元测试：未添加 key "user:999" → `might_contain("user:999")` 返回 false（不漏判）
- [ ] M5-T2.10 单元测试：误判率验证（100000 key + 1% 误判率 → 误判率接近 1%）
- [ ] M5-T2.11 边界测试：容量超限 → 按策略处理（Rebuild/Evict/Degrade），记录日志
- [ ] M5-T2.12 性能测试：查询开销 ≤ 100ns/次（含哈希计算 + 位数组查询）

**验收标准**：布隆过滤器正确；不漏判；误判率可配；按租户隔离；性能 ≤ 100ns/次

**依赖**：M5-T1

## M5-T3：CacheWarmer 缓存预热器

**任务描述**：实现 `CacheWarmer` 缓存预热器，按预热策略（HotspotTable/HotspotKey/CustomQuery）异步预加载热点数据到 L1+L2，复用 v4.6.0 `ProcessL1Cache.put` + 既有 `L2Cache.put`，不阻塞服务启动。

**涉及文件**：
- `packages/sz-orm-core/src/cache_warmup_protection.rs`（扩展：缓存预热器核心）

**复用标注**：v4.6.0 `ProcessL1Cache` `packages/sz-orm-core/src/process_l1_cache.rs:169`（`put` 方法复用）；既有 `L2Cache` `packages/sz-orm-core/src/l2_cache.rs:517`（`put` 方法复用）；既有 `CacheCoherenceProtocol` `packages/sz-orm-core/src/cache_coherence.rs:103`（一致性同步复用）；既有 `tenant_cache_key` `packages/sz-orm-core/src/process_l1_cache.rs:421`（租户缓存键复用）

**子任务**：
- [ ] M5-T3.1 定义 `pub struct CacheWarmer { l1: Arc<ProcessL1Cache<Value>>, l2: Arc<L2Cache>, bloom: Option<Arc<BloomFilter>>, pool: Arc<Pool> }`（缓存预热器，复用 v4.6.0 `ProcessL1Cache` + 既有 `L2Cache`）
- [ ] M5-T3.2 实现 `impl CacheWarmer { pub fn new(l1: Arc<ProcessL1Cache<Value>>, l2: Arc<L2Cache>, pool: Arc<Pool>) -> Self }`
- [ ] M5-T3.3 实现 `pub async fn warmup(&self, config: &WarmupConfig) -> Result<WarmupResult, CacheError>`：按策略识别热点数据 → 异步查询 DB → 批量加载到 L1+L2 → 记录预热日志
- [ ] M5-T3.4 实现 `HotspotTable` 策略：按配置的热点表预加载全部/部分数据
- [ ] M5-T3.5 实现 `HotspotKey` 策略：按配置的热点主键列表预加载
- [ ] M5-T3.6 实现 `CustomQuery` 策略：按自定义 SQL 查询预加载（参数化绑定，复用 `Connection::execute_with_params` `pool.rs:82`）
- [ ] M5-T3.7 实现异步预加载：`tokio::spawn` 异步查询 DB 热点数据，批量加载到 L1（`ProcessL1Cache.put` `process_l1_cache.rs:169`）+ L2（`L2Cache.put` `l2_cache.rs:517`），同时加入 `BloomFilter`，不阻塞服务启动
- [ ] M5-T3.8 实现预热失败处理：预热失败（DB 查询失败/热点识别失败）记录日志，不影响服务启动，启动后按需加载
- [ ] M5-T3.9 实现一致性同步：预热期间 DB 数据变更时通过 `CacheCoherenceProtocol` `cache_coherence.rs:103` 同步失效
- [ ] M5-T3.10 实现按租户隔离：预热时使用 `tenant_cache_key` `process_l1_cache.rs:421` 按租户隔离缓存键
- [ ] M5-T3.11 定义 `pub struct WarmupResult { loaded_count: usize, elapsed: Duration, hit_rate_improvement: f64 }`（预热结果）
- [ ] M5-T3.12 单元测试：预热策略 HotspotTable "users" + 启动 → 异步预加载 users 表热点数据到 L1+L2，不阻塞启动，启动后查询 users 命中缓存
- [ ] M5-T3.13 单元测试：预热 key "user:1" → 同时加载到 L1+L2，查询命中 L1
- [ ] M5-T3.14 边界测试：预热失败（DB 查询失败）→ 记录预热失败日志，不影响服务启动，启动后按需加载
- [ ] M5-T3.15 边界测试：预热数据与 DB 不一致（预热期间 DB 数据变更）→ 通过 `CacheCoherenceProtocol` 同步失效
- [ ] M5-T3.16 性能测试：缓存预热开销 ≤ 30 秒（含热点数据识别 + 预加载到 L1+L2）

**验收标准**：缓存预热正确；异步不阻塞启动；复用 v4.6.0 `ProcessL1Cache` + 既有 `L2Cache`；按租户隔离；性能 ≤ 30 秒

**依赖**：M5-T1、M5-T2

## M5-T4：PenetrationGuard 穿透防护器

**任务描述**：实现 `PenetrationGuard` 穿透防护器，查询前 `BloomFilter.might_contain` 判断，不存在直接返回 None 不查 DB，误判回退 DB 查询，复用既有 L1/L2 查询路径。

**涉及文件**：
- `packages/sz-orm-core/src/cache_warmup_protection.rs`（扩展：穿透防护器核心）

**复用标注**：v4.6.0 `ProcessL1Cache` `packages/sz-orm-core/src/process_l1_cache.rs:169`（`get` 方法复用）；既有 `L2Cache` `packages/sz-orm-core/src/l2_cache.rs:517`（`get` 方法复用）

**子任务**：
- [ ] M5-T4.1 定义 `pub struct PenetrationGuard { bloom: Arc<BloomFilter>, l1: Arc<ProcessL1Cache<Value>>, l2: Arc<L2Cache>, single_flight: Option<Arc<SingleFlight>> }`（穿透防护器，包装 `BloomFilter` + 既有 L1/L2）
- [ ] M5-T4.2 实现 `impl PenetrationGuard { pub fn new(bloom: Arc<BloomFilter>, l1: Arc<ProcessL1Cache<Value>>, l2: Arc<L2Cache>) -> Self }`
- [ ] M5-T4.3 实现 `pub async fn get(&self, key: &str) -> Option<Value>`：`BloomFilter.might_contain` → 不存在返回 None 不查 DB → 可能存在查 L1/L2/DB
- [ ] M5-T4.4 实现布隆判断不存在分支：返回 false → 直接返回 None 不查 DB（不存在的 key 一定返回 None，不漏判）
- [ ] M5-T4.5 实现布隆判断可能存在分支：返回 true → 查 L1（`ProcessL1Cache.get`）→ L2（`L2Cache.get`）→ DB
- [ ] M5-T4.6 实现误判回退：布隆误判存在 + DB 不存在 → 回退到 DB 查询返回 None，更新 `BloomFilter`（标记 key 不存在）
- [ ] M5-T4.7 实现与 `SingleFlight` 集成：L1/L2 未命中时通过 `SingleFlight.get_or_rebuild` 重建缓存
- [ ] M5-T4.8 单元测试：查询不存在的 key "user:999" + 布隆过滤器判断不存在 → 直接返回 None 不查 DB
- [ ] M5-T4.9 单元测试：布隆过滤器误判存在 + DB 不存在 → 回退到 DB 查询返回 None，更新布隆过滤器
- [ ] M5-T4.10 单元测试：存在的 key + 布隆判断可能存在 → 查 L1 命中返回 Value
- [ ] M5-T4.11 边界测试：布隆过滤器容量超限 → 按策略处理，不返回错误

**验收标准**：穿透防护正确；不漏判；误判回退 DB；复用既有 L1/L2 查询路径

**依赖**：M5-T2、M5-T3

## M5-T5：SingleFlight 击穿防护器

**任务描述**：实现 `SingleFlight` 击穿防护器，对同一 key 的并发重建请求只执行一次，其他请求等待结果复用，超时释放锁不死锁，复用 v4.6.0 `ProcessL1Cache` 重建逻辑。

**涉及文件**：
- `packages/sz-orm-core/src/cache_warmup_protection.rs`（扩展：击穿防护器核心）

**复用标注**：v4.6.0 `ProcessL1Cache` `packages/sz-orm-core/src/process_l1_cache.rs:169`（重建逻辑复用）

**子任务**：
- [ ] M5-T5.1 定义 `pub struct SingleFlight { inflight: Mutex<HashMap<String, Shared<OnceCell<Result<Value, CacheError>>>>>, timeout_ms: u64 }`（击穿防护器，`tokio::sync` 同步原语）
- [ ] M5-T5.2 定义 `pub struct StampedeGuard { timeout_ms: u64, concurrency_limit: usize }`（击穿防护配置）
- [ ] M5-T5.3 实现 `impl SingleFlight { pub fn new(timeout_ms: u64) -> Self }`
- [ ] M5-T5.4 实现 `pub async fn get_or_rebuild<F, Fut>(&self, key: &str, rebuild: F) -> Result<Value, CacheError> where F: FnOnce() -> Fut, Fut: Future<Output = Result<Value, CacheError>>`：同一 key 并发重建只执行一次 → 其他等待复用 → 超时释放锁
- [ ] M5-T5.5 实现首个请求分支：执行 `rebuild_fn`（查 DB 重建缓存），其他请求等待结果复用
- [ ] M5-T5.6 实现并发请求分支：等待首个请求结果复用（`tokio::sync::Notify` 或 `OnceCell`）
- [ ] M5-T5.7 实现超时释放锁：重建超时（默认 5 秒）释放锁，等待请求重试，不死锁不长期阻塞
- [ ] M5-T5.8 实现 key 锁清理：重建完成后移除 key 锁（避免内存泄漏）
- [ ] M5-T5.9 单元测试：热点 key "user:1" 过期 + 100 并发查询 → 只执行 1 次 DB 查询重建缓存，其他 99 等待复用结果
- [ ] M5-T5.10 单元测试：重建超时 5 秒 + 等待请求 → 5 秒后释放锁，等待请求重试
- [ ] M5-T5.11 边界测试：重建超时 → 释放锁，等待请求重试，日志标注"singleflight rebuild timeout, retried"
- [ ] M5-T5.12 性能测试：singleflight 开销 ≤ 1ms/次（含 key 锁定 + 等待/复用结果）

**验收标准**：击穿防护正确；并发重建只执行一次；超时释放锁不死锁；性能 ≤ 1ms/次

**依赖**：M5-T1

## M5-T6：M5 集成测试与门禁验证

**任务描述**：M5 里程碑集成测试与门禁验证，确保 REQ-V47-007 全部 10 条验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M5-T6.1 集成测试：`CacheWarmer::warmup` 完整流程（按策略识别热点 → 异步预加载 → L1+L2 命中）
- [ ] M5-T6.2 集成测试：`PenetrationGuard::get` 完整流程（布隆判断 → 不存在返回 None / 可能存在查 L1/L2/DB）
- [ ] M5-T6.3 集成测试：`SingleFlight::get_or_rebuild` 完整流程（并发重建只执行一次 + 超时释放锁）
- [ ] M5-T6.4 集成测试：预热与 L1/L2 协同（预热数据同时加载到 L1+L2，查询命中 L1）
- [ ] M5-T6.5 集成测试：布隆过滤器不漏判（不存在的 key 一定返回 None，误判回退 DB）
- [ ] M5-T6.6 集成测试：复用 v4.6.0 `ProcessL1Cache` `packages/sz-orm-core/src/process_l1_cache.rs:169` + 既有 `L2Cache` `packages/sz-orm-core/src/l2_cache.rs:517` + `CacheCoherenceProtocol` `packages/sz-orm-core/src/cache_coherence.rs:103`，不新建缓存逻辑
- [ ] M5-T6.7 集成测试：真实 DB 集成测试（预热真实 DB 热点数据 + L1+L2 命中验证）
- [ ] M5-T6.8 运行 `cargo test -p sz-orm-core --features cache-warmup-protection`（全部通过）
- [ ] M5-T6.9 `cargo clippy -p sz-orm-core --features cache-warmup-protection -- -D warnings`
- [ ] M5-T6.10 `cargo fmt -p sz-orm-core -- --check`
- [ ] M5-T6.11 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-core/src/cache_warmup_protection.rs` 无占位实现
- [ ] M5-T6.12 扫描 `grep -rn 'unsafe' packages/sz-orm-core/src/cache_warmup_protection.rs` 无 unsafe 块
- [ ] M5-T6.13 验证默认 feature 行为与 v4.6.0 一致（`cargo build -p sz-orm-core` 无预热与防护）
- [ ] M5-T6.14 验证 `cache-warmup-protection` 与既有 `process-l1-cache`/`forward-compat-sandbox`/`tenant-quota-rls-enhanced` feature 组合编译通过

**验收标准**：M5 集成测试通过；门禁通过；默认行为不变；预热 + 穿透防护 + 击穿防护全部验证

**依赖**：M5-T1、M5-T2、M5-T3、M5-T4、M5-T5

---

# 八、M6：异常自愈与根因分析（REQ-V47-004，P2，2 天）

**目标**：扩展既有 `sz-orm-observability` 包，新增 `AutoRemediator` 异常自愈器 + `RootCauseAnalyzer` 根因分析器 + `AnomalyCorrelator` 异常关联器，复用 v4.6.0 `AnomalyDetector` + 既有 `MetricsRegistry`/`QueryLogger`，补齐异常自愈 + 根因分析 + 异常关联分析。
**对应需求**：REQ-V47-004（spec.md §5.4，design.md §2.1.3.4 + §2.2.2.4）
**预期工作量**：2 天
**依赖**：无（M6 为 P2 独立需求，复用 v4.6.0 AnomalyDetector + 既有 MetricsRegistry/QueryLogger，扩展包可与 M1~M5、M7 并行）

## M6-T1：anomaly-remediation-rca feature gate + 配置结构

**任务描述**：在 `sz-orm-observability` 完善 `anomaly-remediation-rca` feature gate 隔离（M0-T2 已创建占位），定义 `RemediationAction` 修复动作枚举 + `RemediationConfig` 配置。

**涉及文件**：
- `packages/sz-orm-observability/Cargo.toml`（完善：`anomaly-remediation-rca` feature 依赖 `anomaly-detection`）
- `packages/sz-orm-observability/src/lib.rs`（扩展：模块声明 `mod remediation_rca;`，`#[cfg(feature = "anomaly-remediation-rca")]` 门控）
- `packages/sz-orm-observability/src/remediation_rca.rs`（新建，异常自愈 RCA 配置 + 枚举）

**复用标注**：v4.6.0 `AnomalyDetector` `packages/sz-orm-observability/src/anomaly.rs:254`；v4.6.0 `AnomalyAlert` `packages/sz-orm-observability/src/anomaly.rs:206`；v4.6.0 `Anomaly` `packages/sz-orm-observability/src/anomaly.rs:154`；v4.6.0 `AnomalyConfig` `packages/sz-orm-observability/src/anomaly.rs:74`；既有 `MetricsRegistry` `packages/sz-orm-observability/src/lib.rs:262`；既有 `QueryLogger` `packages/sz-orm-observability/src/query_logger.rs:73`

**feature gate 隔离**：`anomaly-remediation-rca = ["anomaly-detection"]`，默认关闭

**子任务**：
- [ ] M6-T1.1 `packages/sz-orm-observability/Cargo.toml` 确认 `anomaly-remediation-rca = ["anomaly-detection"]` feature（M0-T2 已创建）
- [ ] M6-T1.2 `src/lib.rs` 声明 `mod remediation_rca;`，`#[cfg(feature = "anomaly-remediation-rca")]` 门控
- [ ] M6-T1.3 `src/remediation_rca.rs` 定义 `pub enum RemediationAction { RestartConnection, ClearCache, ScaleOut, CustomAction(Box<dyn Fn() -> BoxFuture<()> + Send + Sync>) }`（四种修复动作）
- [ ] M6-T1.4 定义 `pub struct RemediationConfig { auto_execute_whitelist: Vec<RemediationAction>, rca_confidence_threshold: f64, correlation_window_ms: u64, remediation_timeout_ms: u64 }`（自愈配置）
- [ ] M6-T1.5 实现 `impl RemediationConfig { pub fn new() -> Self }`（默认：空白名单, 0.7, 300000ms, 30000ms）
- [ ] M6-T1.6 实现链式配置方法：`with_whitelist` / `with_confidence_threshold` / `with_correlation_window` / `with_remediation_timeout`
- [ ] M6-T1.7 单元测试：`RemediationConfig::new()` 默认值正确（空白名单, 0.7, 300000ms, 30000ms）
- [ ] M6-T1.8 验证 `cargo check -p sz-orm-observability --features anomaly-remediation-rca` 编译通过

**验收标准**：feature gate 定义；RemediationAction/RemediationConfig 定义完整；复用 v4.6.0 `AnomalyDetector`

**依赖**：M0-T2

## M6-T2：RootCauseAnalyzer 根因分析器

**任务描述**：实现 `RootCauseAnalyzer` 根因分析器，收集异常上下文（`MetricsRegistry` 指标 + `QueryLogger` 日志 + 拓扑）→ 推断根因组件/SQL → 计算置信度 → 构建证据链，生成 `RootCause`。

**涉及文件**：
- `packages/sz-orm-observability/src/remediation_rca.rs`（扩展：根因分析器核心）

**复用标注**：既有 `MetricsRegistry` `packages/sz-orm-observability/src/lib.rs:262`（指标历史数据复用）；既有 `QueryLogger` `packages/sz-orm-observability/src/query_logger.rs:73`（查询日志复用）；v4.6.0 `Anomaly` `packages/sz-orm-observability/src/anomaly.rs:154`

**子任务**：
- [ ] M6-T2.1 定义 `pub struct RootCauseAnalyzer { metrics: Arc<MetricsRegistry>, query_logger: Arc<QueryLogger>, confidence_threshold: f64 }`（根因分析器，复用既有 `MetricsRegistry` + `QueryLogger`）
- [ ] M6-T2.2 实现 `impl RootCauseAnalyzer { pub fn new(metrics: Arc<MetricsRegistry>, query_logger: Arc<QueryLogger>, threshold: f64) -> Self }`
- [ ] M6-T2.3 实现 `pub async fn analyze_root_cause(&self, anomaly: &Anomaly) -> Result<RootCause, RemediationError>`：收集上下文 → 推断根因 → 构建证据链 → 计算置信度
- [ ] M6-T2.4 实现上下文收集：从 `MetricsRegistry` `lib.rs:262` 获取指标历史 + 从 `QueryLogger` `query_logger.rs:73` 获取查询日志 + 拓扑信息
- [ ] M6-T2.5 实现根因推断：基于上下文推断根因组件（DB/Cache/Connection/...）+ 根因 SQL
- [ ] M6-T2.6 实现置信度计算：基于证据充分度计算置信度（0.0~1.0）
- [ ] M6-T2.7 实现证据链构建：构建证据链（指标 + 日志 + 拓扑）
- [ ] M6-T2.8 实现低置信度标注：置信度低于阈值（默认 0.7）标注"根因不确定，需人工排查"
- [ ] M6-T2.9 实现敏感信息脱敏：证据链脱敏敏感信息（SQL 参数值、连接凭据）
- [ ] M6-T2.10 定义 `pub struct RootCause { component: String, sql: Option<String>, confidence: f64, evidence_chain: Vec<Evidence> }`（根因结果）
- [ ] M6-T2.11 单元测试：异常"查询超时" + 根因分析 → 生成 `RootCause`，根因组件"DB"、根因 SQL"SELECT * FROM large_table"、置信度 0.85、证据链[指标+日志+拓扑]
- [ ] M6-T2.12 单元测试：置信度阈值 0.7 + 根因置信度 0.5 → 标注"根因不确定，需人工排查"
- [ ] M6-T2.13 边界测试：根因分析证据不足（指标/日志缺失）→ 标注"根因不确定，证据不足"，建议人工排查
- [ ] M6-T2.14 性能测试：根因分析开销 ≤ 2 秒/异常

**验收标准**：根因分析正确；附证据链与置信度；复用既有 `MetricsRegistry`/`QueryLogger`；敏感信息脱敏；性能 ≤ 2 秒/异常

**依赖**：M6-T1

## M6-T3：AnomalyCorrelator 异常关联器

**任务描述**：实现 `AnomalyCorrelator` 异常关联器，分析多个异常事件的时间/空间关联性，识别同根因异常集群，生成 `CorrelationResult`。

**涉及文件**：
- `packages/sz-orm-observability/src/remediation_rca.rs`（扩展：异常关联器核心）

**复用标注**：v4.6.0 `AnomalyDetector` `packages/sz-orm-observability/src/anomaly.rs:254`；v4.6.0 `Anomaly` `packages/sz-orm-observability/src/anomaly.rs:154`

**子任务**：
- [ ] M6-T3.1 定义 `pub struct AnomalyCorrelator { correlation_window_ms: u64 }`（异常关联器）
- [ ] M6-T3.2 实现 `impl AnomalyCorrelator { pub fn new(window_ms: u64) -> Self }`
- [ ] M6-T3.3 实现 `pub async fn correlate(&self, anomaly: &Anomaly, history: &[Anomaly]) -> Result<CorrelationResult, RemediationError>`：分析时间/空间关联性 → 识别同根因异常集群
- [ ] M6-T3.4 实现时间关联性分析：分析异常事件时间窗口（默认 5 分钟）内的关联性
- [ ] M6-T3.5 实现空间关联性分析：分析异常事件指标/组件的关联性
- [ ] M6-T3.6 实现关联性评分：计算关联性评分，低评分关联标注"weak correlation"
- [ ] M6-T3.7 定义 `pub struct CorrelationResult { correlated_anomalies: Vec<Anomaly>, correlation_score: f64, root_cause_cluster: Option<String> }`（关联结果）
- [ ] M6-T3.8 单元测试：异常 A"查询超时" + 异常 B"连接池耗尽" + 时间关联 → 识别为同根因异常集群，生成 `CorrelationResult` 标注关联性
- [ ] M6-T3.9 单元测试：无关联异常 → 关联性评分低，标注"weak correlation, please verify"
- [ ] M6-T3.10 边界测试：异常关联误关联 → 按关联性评分标注，低评分关联标注"weak correlation"

**验收标准**：异常关联正确；时间/空间关联性分析；关联性评分；复用 v4.6.0 `AnomalyDetector`

**依赖**：M6-T1

## M6-T4：AutoRemediator 异常自愈器 + 审计日志

**任务描述**：实现 `AutoRemediator` 异常自愈器，检测到异常后选择修复动作 → 白名单判断 → 自动执行/请求人工确认 → 记录审计日志，复用 v4.6.0 `AnomalyDetector` 异常事件订阅。

**涉及文件**：
- `packages/sz-orm-observability/src/remediation_rca.rs`（扩展：异常自愈器核心）

**复用标注**：v4.6.0 `AnomalyDetector` `packages/sz-orm-observability/src/anomaly.rs:254`（异常事件订阅复用）；v4.6.0 `AnomalyAlert` `packages/sz-orm-observability/src/anomaly.rs:206`

**子任务**：
- [ ] M6-T4.1 定义 `pub struct AutoRemediator { detector: Arc<AnomalyDetector>, rca: Arc<RootCauseAnalyzer>, correlator: Arc<AnomalyCorrelator>, config: RemediationConfig, audit_logger: Mutex<AuditWriter> }`（异常自愈器，复用 v4.6.0 `AnomalyDetector`）
- [ ] M6-T4.2 实现 `impl AutoRemediator { pub fn new(detector: Arc<AnomalyDetector>, rca: Arc<RootCauseAnalyzer>, correlator: Arc<AnomalyCorrelator>, config: RemediationConfig) -> Self }`
- [ ] M6-T4.3 实现 `pub async fn select_action(&self, anomaly: &Anomaly, root_cause: &RootCause) -> RemediationAction`：根据异常类型 + 根因选择修复动作
- [ ] M6-T4.4 实现 `pub async fn execute_action(&self, action: RemediationAction) -> Result<RemediationResult, RemediationError>`：白名单判断 → 自动执行/请求人工确认 → 记录审计日志
- [ ] M6-T4.5 实现白名单判断：动作在 `auto_execute_whitelist` 内则自动执行；非白名单则请求人工确认（通知$通知 SRE），不静默执行
- [ ] M6-T4.6 实现修复动作执行：`RestartConnection` 重启连接 / `ClearCache` 清缓存 / `ScaleOut` 扩容 / `CustomAction` 自定义
- [ ] M6-T4.7 实现审计日志记录：记录审计日志（异常 ID + 修复动作 + 执行人 + 执行时间 + 结果），追加写入不可篡改
- [ ] M6-T4.8 实现自愈动作执行超时：超时（默认 30 秒）中止，通知 SRE 人工干预
- [ ] M6-T4.9 实现链式配置方法：`with_whitelist`
- [ ] M6-T4.10 单元测试：检测到异常"连接池耗尽" + 自愈动作 RestartConnection + 自动执行白名单 → 自动执行 RestartConnection，记录审计日志
- [ ] M6-T4.11 单元测试：非白名单动作 → 等待人工确认，不自动执行
- [ ] M6-T4.12 单元测试：白名单 [ClearCache] + 异常"缓存不一致" + 动作 ClearCache → 自动执行 ClearCache
- [ ] M6-T4.13 边界测试：自愈动作执行失败 → 记录自愈失败日志，通知 SRE 人工干预，不静默忽略
- [ ] M6-T4.14 边界测试：白名单配置了不存在的动作 → 跳过无效动作，记录日志"invalid remediation action in whitelist: X"
- [ ] M6-T4.15 性能测试：异常自愈响应时间 ≤ 5 秒（含根因分析 + 修复动作选择 + 人工确认等待，白名单内无需等待）

**验收标准**：异常自愈正确；白名单判断；审计日志可追溯；复用 v4.6.0 `AnomalyDetector`；性能 ≤ 5 秒

**依赖**：M6-T1、M6-T2、M6-T3

## M6-T5：M6 集成测试与门禁验证

**任务描述**：M6 里程碑集成测试与门禁验证，确保 REQ-V47-004 全部 8 条验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M6-T5.1 集成测试：`AutoRemediator` 完整流程（异常检测 → 根因分析 → 异常关联 → 修复动作选择 → 白名单判断 → 执行 + 审计）
- [ ] M6-T5.2 集成测试：`RootCauseAnalyzer::analyze_root_cause` 完整流程（收集上下文 → 推断根因 → 证据链 → 置信度）
- [ ] M6-T5.3 集成测试：`AnomalyCorrelator::correlate` 完整流程（时间/空间关联性 → 异常集群 → 关联性评分）
- [ ] M6-T5.4 集成测试：四种修复动作（RestartConnection/ClearCache/ScaleOut/CustomAction）+ 白名单/人工确认 + 审计日志
- [ ] M6-T5.5 集成测试：复用 v4.6.0 `AnomalyDetector` `packages/sz-orm-observability/src/anomaly.rs:254` + 既有 `MetricsRegistry` `packages/sz-orm-observability/src/lib.rs:262` + `QueryLogger` `packages/sz-orm-observability/src/query_logger.rs:73`，不新建异常检测逻辑
- [ ] M6-T5.6 运行 `cargo test -p sz-orm-observability --features anomaly-remediation-rca`（全部通过）
- [ ] M6-T5.7 `cargo clippy -p sz-orm-observability --features anomaly-remediation-rca -- -D warnings`
- [ ] M6-T5.8 `cargo fmt -p sz-orm-observability -- --check`
- [ ] M6-T5.9 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-observability/src/remediation_rca.rs` 无占位实现
- [ ] M6-T5.10 扫描 `grep -rn 'unsafe' packages/sz-orm-observability/src/remediation_rca.rs` 无 unsafe 块
- [ ] M6-T5.11 验证默认 feature 行为与 v4.6.0 一致（`cargo build -p sz-orm-observability` 无异常自愈与根因分析）
- [ ] M6-T5.12 验证 `anomaly-remediation-rca` 与既有 `anomaly-detection`/`query-logging`/`service-mesh` feature 组合编译通过

**验收标准**：M6 集成测试通过；门禁通过；默认行为不变；异常自愈 + 根因分析 + 异常关联全部验证

**依赖**：M6-T1、M6-T2、M6-T3、M6-T4

---

# 九、M7：多云成本对比与容量预测（REQ-V47-005，P2，2.5 天）

**目标**：扩展既有 `sz-orm-storage` 包，新增 `MultiCloudCostComparator` 多云成本对比器 + `CapacityForecaster` 容量预测器 + `AutoOptimizer` 自动优化执行器，复用 v4.6.0 `CostAnalyzer` + 既有 `Storage`/`StorageProvider`，补齐多云成本对比 + 容量预测 + 成本自动优化执行。
**对应需求**：REQ-V47-005（spec.md §5.5，design.md §2.1.3.5 + §2.2.2.5）
**预期工作量**：2.5 天
**依赖**：无（M7 为 P2 独立需求，复用 v4.6.0 CostAnalyzer + 既有 Storage/StorageProvider，扩展包可与 M1~M6 并行）

## M7-T1：multicloud-cost-forecast feature gate + 配置结构

**任务描述**：在 `sz-orm-storage` 完善 `multicloud-cost-forecast` feature gate 隔离（M0-T2 已创建占位），定义 `ForecastAlgorithm` 预测算法枚举 + `MultiCloudForecastConfig` 配置。

**涉及文件**：
- `packages/sz-orm-storage/Cargo.toml`（完善：`multicloud-cost-forecast` feature 依赖 `cost-analysis`）
- `packages/sz-orm-storage/src/lib.rs`（扩展：模块声明 `mod multicloud_forecast;`，`#[cfg(feature = "multicloud-cost-forecast")]` 门控）
- `packages/sz-orm-storage/src/multicloud_forecast.rs`（新建，多云预测配置 + 枚举）

**复用标注**：v4.6.0 `CostAnalyzer` `packages/sz-orm-storage/src/cost.rs:231`；v4.6.0 `CostReport` `packages/sz-orm-storage/src/cost.rs:213`；v4.6.0 `ProviderCost` `packages/sz-orm-storage/src/cost.rs:202`；v4.6.0 `BucketCost` `packages/sz-orm-storage/src/cost.rs:181`；v4.6.0 `CostOptimizationSuggestion` `packages/sz-orm-storage/src/cost.rs:55`；v4.6.0 `CostConfig` `packages/sz-orm-storage/src/cost.rs:96`；既有 `Storage` `packages/sz-orm-storage/src/storage.rs:14`；既有 `StorageProvider` `packages/sz-orm-storage/src/storage.rs:287`

**feature gate 隔离**：`multicloud-cost-forecast = ["cost-analysis"]`，默认关闭

**子任务**：
- [ ] M7-T1.1 `packages/sz-orm-storage/Cargo.toml` 确认 `multicloud-cost-forecast = ["cost-analysis"]` feature（M0-T2 已创建）
- [ ] M7-T1.2 `src/lib.rs` 声明 `mod multicloud_forecast;`，`#[cfg(feature = "multicloud-cost-forecast")]` 门控
- [ ] M7-T1.3 `src/multicloud_forecast.rs` 定义 `pub enum ForecastAlgorithm { LinearRegression, ExponentialSmoothing, HoltWinters }`（三种预测算法）
- [ ] M7-T1.4 定义 `pub struct MultiCloudForecastConfig { comparison_interval_ms, forecast_algorithm, forecast_horizon_days, confidence_level, auto_optimize_whitelist }`（多云预测配置）
- [ ] M7-T1.5 实现 `impl MultiCloudForecastConfig { pub fn new() -> Self }`（默认：604800000ms 每周, LinearRegression, 7 天, 0.95, 空白名单）
- [ ] M7-T1.6 实现链式配置方法：`with_comparison_interval` / `with_forecast_algorithm` / `with_forecast_horizon` / `with_confidence_level` / `with_whitelist`
- [ ] M7-T1.7 单元测试：`MultiCloudForecastConfig::new()` 默认值正确（每周, LinearRegression, 7 天, 0.95, 空白名单）
- [ ] M7-T1.8 验证 `cargo check -p sz-orm-storage --features multicloud-cost-forecast` 编译通过

**验收标准**：feature gate 定义；ForecastAlgorithm/MultiCloudForecastConfig 定义完整；复用 v4.6.0 `CostAnalyzer`

**依赖**：M0-T2

## M7-T2：MultiCloudCostComparator 多云成本对比器

**任务描述**：实现 `MultiCloudCostComparator` 多云成本对比器，对每 provider 调用 `CostAnalyzer.analyze_cost` 获取 `ProviderCost`，计算成本差异，生成 `CostComparisonReport` 含迁移建议。

**涉及文件**：
- `packages/sz-orm-storage/src/multicloud_forecast.rs`（扩展：多云成本对比器核心）

**复用标注**：v4.6.0 `CostAnalyzer` `packages/sz-orm-storage/src/cost.rs:231`（`analyze_cost` 方法复用）；v4.6.0 `ProviderCost` `packages/sz-orm-storage/src/cost.rs:202`；既有 `StorageProvider` `packages/sz-orm-storage/src/storage.rs:287`

**子任务**：
- [ ] M7-T2.1 定义 `pub struct MultiCloudCostComparator { analyzer: Arc<CostAnalyzer> }`（多云成本对比器，复用 v4.6.0 `CostAnalyzer`）
- [ ] M7-T2.2 实现 `impl MultiCloudCostComparator { pub fn new(analyzer: Arc<CostAnalyzer>) -> Self }`
- [ ] M7-T2.3 实现 `pub async fn compare_providers(&self, capacity: u64, providers: &[StorageProvider]) -> Result<CostComparisonReport, StorageError>`：对每 provider 调用 `CostAnalyzer.analyze_cost` → 计算差异 → 生成迁移建议
- [ ] M7-T2.4 实现成本差异计算：计算同容量在不同 provider 的成本差异
- [ ] M7-T2.5 实现 provider 迁移建议生成：基于成本差异生成 `MigrationSuggestion`（源 provider + 目标 provider + 迁移成本 + 迁移风险 + 预期节省）
- [ ] M7-T2.6 定义 `pub struct CostComparisonReport { provider_costs: Vec<ProviderCost>, differences: Vec<CostDifference>, migration_suggestions: Vec<MigrationSuggestion> }`（成本对比报表）
- [ ] M7-T2.7 定义 `pub struct MigrationSuggestion { from_provider: StorageProvider, to_provider: StorageProvider, migration_cost: f64, migration_risk: f64, expected_saving_percent: f64 }`（provider 迁移建议）
- [ ] M7-T2.8 实现不暴露存储凭据：复用既有 `StorageBuilder` `storage.rs:22` 凭据管理
- [ ] M7-T2.9 单元测试：对比 100GB 在 S3/Aliyun/Tencent 的成本 → 生成 `CostComparisonReport`，含每 provider 成本 + 差异 + 迁移建议
- [ ] M7-T2.10 单元测试：S3 成本高于 Aliyun 30% → 生成迁移建议"从 S3 迁移到 Aliyun，预期节省 30%，附迁移成本与风险"
- [ ] M7-T2.11 边界测试：provider API 不可用 → 跳过该 provider 对比，记录日志"provider X API unavailable, skipped"
- [ ] M7-T2.12 性能测试：多云成本对比开销 ≤ 10 秒（含 7 provider 成本统计 + 对比分析 + 报表生成）

**验收标准**：多云成本对比正确；复用 v4.6.0 `CostAnalyzer`；迁移建议附成本/风险/节省；不暴露凭据；性能 ≤ 10 秒

**依赖**：M7-T1

## M7-T3：CapacityForecaster 容量预测器

**任务描述**：实现 `CapacityForecaster` 容量预测器，基于历史容量数据（时间序列）按算法（LinearRegression/ExponentialSmoothing/HoltWinters）预测未来容量，计算置信区间，生成 `CapacityForecast`。

**涉及文件**：
- `packages/sz-orm-storage/src/multicloud_forecast.rs`（扩展：容量预测器核心）

**子任务**：
- [ ] M7-T3.1 定义 `pub struct CapacityForecaster`（容量预测器）
- [ ] M7-T3.2 实现 `impl CapacityForecaster { pub fn new() -> Self }`
- [ ] M7-T3.3 实现 `pub async fn forecast(&self, history: &[CapacityPoint], algorithm: ForecastAlgorithm, horizon_days: u32, confidence: f64) -> Result<CapacityForecast, StorageError>`：按算法拟合历史趋势 → 预测未来 → 计算置信区间
- [ ] M7-T3.4 实现 `LinearRegression` 算法：最小二乘法拟合 `y = ax + b`，预测未来容量
- [ ] M7-T3.5 实现 `ExponentialSmoothing` 算法：`S_t = α * y_t + (1-α) * S_{t-1}`，指数衰减权重平滑
- [ ] M7-T3.6 实现 `HoltWinters` 算法：三指数平滑（水平 + 趋势 + 季节性），支持周期性数据
- [ ] M7-T3.7 实现置信区间计算：基于残差标准差 + 置信水平（默认 95%，正态分布 z=1.96）计算上下界
- [ ] M7-T3.8 定义 `pub struct CapacityForecast { predicted: Vec<CapacityPoint>, confidence_interval: (f64, f64), algorithm: ForecastAlgorithm }`（容量预测结果）
- [ ] M7-T3.9 定义 `pub struct CapacityPoint { timestamp: DateTime<Utc>, capacity: f64 }`（容量数据点）
- [ ] M7-T3.10 单元测试：历史容量数据 30 天 + 预测 7 天 + LinearRegression → 生成 `CapacityForecast`，含未来 7 天预测容量 + 95% 置信区间
- [ ] M7-T3.11 单元测试：预测 7 天容量 + 95% 置信区间 → 预测结果含上下界，不单点预测
- [ ] M7-T3.12 单元测试：配置对比每周 + 预测 ExponentialSmoothing + 置信 99% → 每周对比，按 ExponentialSmoothing 预测，99% 置信区间
- [ ] M7-T3.13 边界测试：历史数据不足（少于 7 天）→ 拒绝预测，返回错误"insufficient history data for forecasting"
- [ ] M7-T3.14 边界测试：预测算法不适用（LinearRegression 对周期性数据预测偏差大）→ 按配置算法预测，标注预测偏差，建议切换算法
- [ ] M7-T3.15 性能测试：容量预测开销 ≤ 5 秒（含历史数据加载 + 预测算法计算 + 置信区间生成）

**验收标准**：容量预测正确；三种算法；附置信区间；性能 ≤ 5 秒

**依赖**：M7-T1

## M7-T4：AutoOptimizer 自动优化执行器

**任务描述**：实现 `AutoOptimizer` 自动优化执行器，接收 `CostOptimizationSuggestion`，白名单内自动执行（如降级 tier），非白名单请求人工确认，生成 `OptimizationExecutionResult`，复用既有 `Storage` trait 执行优化。

**涉及文件**：
- `packages/sz-orm-storage/src/multicloud_forecast.rs`（扩展：自动优化执行器核心）

**复用标注**：v4.6.0 `CostOptimizationSuggestion` `packages/sz-orm-storage/src/cost.rs:55`（优化建议复用）；既有 `Storage` trait `packages/sz-orm-storage/src/storage.rs:14`（执行优化复用）

**子任务**：
- [ ] M7-T4.1 定义 `pub struct AutoOptimizer { storage: Arc<dyn Storage>, whitelist: Vec<CostOptimizationSuggestion> }`（自动优化执行器，复用既有 `Storage`）
- [ ] M7-T4.2 实现 `impl AutoOptimizer { pub fn new(storage: Arc<dyn Storage>) -> Self }`
- [ ] M7-T4.3 实现 `pub async fn execute_suggestion(&self, suggestion: &CostOptimizationSuggestion) -> Result<OptimizationExecutionResult, StorageError>`：白名单判断 → 自动执行/请求人工确认 → 执行优化
- [ ] M7-T4.4 实现白名单判断：建议在 `auto_optimize_whitelist` 内则自动执行；非白名单则请求人工确认（通知 FinOps）
- [ ] M7-T4.5 实现优化执行：通过 `Storage` trait `storage.rs:14` 执行优化动作（如 `TierDowngrade` 降级 tier）
- [ ] M7-T4.6 定义 `pub struct OptimizationExecutionResult { success: bool, details: String, elapsed: Duration }`（优化执行结果）
- [ ] M7-T4.7 实现链式配置方法：`with_whitelist`
- [ ] M7-T4.8 单元测试：优化建议 TierDowngrade + 白名单 → 自动执行降级，记录执行结果
- [ ] M7-T4.9 单元测试：非白名单建议 → 等待人工确认
- [ ] M7-T4.10 边界测试：自动优化执行失败（降级 tier 失败）→ 记录执行失败日志，通知 FinOps 人工干预

**验收标准**：自动优化执行正确；白名单判断；复用既有 `Storage` trait；执行失败通知人工干预

**依赖**：M7-T1

## M7-T5：M7 集成测试与门禁验证

**任务描述**：M7 里程碑集成测试与门禁验证，确保 REQ-V47-005 全部 8 条验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M7-T5.1 集成测试：`MultiCloudCostComparator::compare_providers` 完整流程（7 provider 成本对比 + 差异 + 迁移建议）
- [ ] M7-T5.2 集成测试：`CapacityForecaster::forecast` 完整流程（三种算法 + 置信区间）
- [ ] M7-T5.3 集成测试：`AutoOptimizer::execute_suggestion` 完整流程（白名单/人工确认 + 执行优化 + 执行结果）
- [ ] M7-T5.4 集成测试：复用 v4.6.0 `CostAnalyzer` `packages/sz-orm-storage/src/cost.rs:231` + 既有 `Storage` `packages/sz-orm-storage/src/storage.rs:14` + `StorageProvider` `:287`，不新建成本分析逻辑
- [ ] M7-T5.5 集成测试：成本数据准确（使用 provider API 计费数据，不估算）
- [ ] M7-T5.6 运行 `cargo test -p sz-orm-storage --features multicloud-cost-forecast`（全部通过）
- [ ] M7-T5.7 `cargo clippy -p sz-orm-storage --features multicloud-cost-forecast -- -D warnings`
- [ ] M7-T5.8 `cargo fmt -p sz-orm-storage -- --check`
- [ ] M7-T5.9 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-storage/src/multicloud_forecast.rs` 无占位实现
- [ ] M7-T5.10 扫描 `grep -rn 'unsafe' packages/sz-orm-storage/src/multicloud_forecast.rs` 无 unsafe 块
- [ ] M7-T5.11 验证默认 feature 行为与 v4.6.0 一致（`cargo build -p sz-orm-storage` 无多云成本对比与容量预测）
- [ ] M7-T5.12 验证 `multicloud-cost-forecast` 与既有 `cost-analysis`/`storage-lifecycle`/`real-cloud` feature 组合编译通过

**验收标准**：M7 集成测试通过；门禁通过；默认行为不变；多云对比 + 容量预测 + 自动优化全部验证

**依赖**：M7-T1、M7-T2、M7-T3、M7-T4

---

# 十、M8：集成验证与文档同步（P0，0.5 天）

**目标**：M1~M7 全部完成后，进行 workspace 全量集成测试、feature 全组合编译验证、文档同步与版本号更新，确保 v4.7.0 整体交付质量。
**对应需求**：全局（集成验证与文档同步，非功能需求）
**预期工作量**：0.5 天
**依赖**：M0、M1、M2、M3、M4、M5、M6、M7 全部完成

## M8-T1：workspace 全量集成测试

**任务描述**：运行 workspace 全量测试 + 14 道门禁全量验证，确保 v4.7.0 七项需求集成后整体通过，v4.6.0 测试基线不回退。

**涉及文件**：`Cargo.toml`（workspace 全量）、`packages/sz-orm-queue/`、`packages/sz-orm-core/`、`packages/sz-orm-batch/`、`packages/sz-orm-observability/`、`packages/sz-orm-storage/`

**子任务**：
- [ ] M8-T1.1 运行 `cargo test --workspace -j 2 --no-fail-fast`（全量测试通过，v4.6.0 基线不回退）
- [ ] M8-T1.2 运行 7 项需求 feature 测试：`cargo test -p sz-orm-queue --features delayed-priority-queue` + `cargo test -p sz-orm-core --features forward-compat-sandbox` + `cargo test -p sz-orm-batch --features copy-parallel-shard` + `cargo test -p sz-orm-core --features tenant-quota-rls-enhanced` + `cargo test -p sz-orm-core --features cache-warmup-protection` + `cargo test -p sz-orm-observability --features anomaly-remediation-rca` + `cargo test -p sz-orm-storage --features multicloud-cost-forecast`
- [ ] M8-T1.3 门禁 1：`cargo fmt --all -- --check`（fmt 格式检查）
- [ ] M8-T1.4 门禁 2：`cargo check --workspace --all-targets`（编译检查）
- [ ] M8-T1.5 门禁 3：`cargo clippy --workspace --all-targets -- -D warnings`（clippy 静态分析）
- [ ] M8-T1.6 门禁 4：`cargo test --workspace`（单元/集成测试）
- [ ] M8-T1.7 门禁 5：`cargo doc --workspace --no-deps --all-features`（文档构建）
- [ ] M8-T1.8 门禁 8：扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs' packages/sz-orm-queue/src/delayed_priority.rs packages/sz-orm-core/src/forward_compat_sandbox.rs packages/sz-orm-batch/src/copy_parallel_shard.rs packages/sz-orm-core/src/tenant_quota_rls.rs packages/sz-orm-core/src/cache_warmup_protection.rs packages/sz-orm-observability/src/remediation_rca.rs packages/sz-orm-storage/src/multicloud_forecast.rs` 无占位实现
- [ ] M8-T1.9 门禁 10：`cargo check --workspace --all-targets --all-features`（feature 全组合编译）
- [ ] M8-T1.10 验证 v4.6.0 测试基线不回退（v4.7.0 测试数 ≥ v4.6.0 测试数）
- [ ] M8-T1.11 验证五方言覆盖（COPY 协议/RLS 注入/沙箱预演影子表五方言适配）

**验收标准**：workspace 全量测试通过；14 道门禁全通过；v4.6.0 测试基线不回退；7 项需求 feature 测试通过

**依赖**：M1-T6、M2-T6、M3-T6、M4-T6、M5-T6、M6-T5、M7-T5

## M8-T2：feature 全组合编译验证

**任务描述**：验证 v4.7.0 7 个新 feature 与既有 feature（v4.3.0 7 个 + v4.4.0 6 个 + v4.5.0 3 个 + v4.6.0 7 个）任意组合编译通过，确保无 feature 冲突。

**涉及文件**：`packages/sz-orm-queue/Cargo.toml`、`packages/sz-orm-core/Cargo.toml`、`packages/sz-orm-batch/Cargo.toml`、`packages/sz-orm-observability/Cargo.toml`、`packages/sz-orm-storage/Cargo.toml`

**子任务**：
- [ ] M8-T2.1 验证默认（无 feature）编译通过，行为与 v4.6.0 一致：`cargo build --workspace`
- [ ] M8-T2.2 验证 7 个新 feature 单独编译通过（7 条 `cargo build -p <package> --features <feature>` 命令）
- [ ] M8-T2.3 验证 v4.7.0 7 feature 全组合编译：`cargo build --features sz-orm-queue/delayed-priority-queue,sz-orm-core/forward-compat-sandbox,sz-orm-batch/copy-parallel-shard,sz-orm-observability/anomaly-remediation-rca,sz-orm-storage/multicloud-cost-forecast,sz-orm-core/tenant-quota-rls-enhanced,sz-orm-core/cache-warmup-protection`
- [ ] M8-T2.4 验证 v4.7.0 + v4.6.0 feature 组合编译通过
- [ ] M8-T2.5 验证 v4.7.0 + v4.5.0 feature 组合编译通过
- [ ] M8-T2.6 验证 v4.7.0 + v4.4.0 feature 组合编译通过
- [ ] M8-T2.7 验证 v4.7.0 + v4.3.0 feature 组合编译通过
- [ ] M8-T2.8 验证 `delayed-priority-queue` + 既有 `dlx-auto-redelivery`/`cdc`/`message-tracing` 组合编译通过
- [ ] M8-T2.9 验证 `copy-parallel-shard` + 既有 `batch-atomic`/`batch-v2`/`batch-stream` 组合编译通过
- [ ] M8-T2.10 验证 `tenant-quota-rls-enhanced` + 既有 `connection-level-tenant`/`multi-tenant-enhanced` 组合编译通过
- [ ] M8-T2.11 验证 `cache-warmup-protection` + 既有 `process-l1-cache`/`l1-cache`/`cache-coherence`/`dist-cache` 组合编译通过
- [ ] M8-T2.12 验证全 feature 组合编译：`cargo build --workspace --all-features`

**验收标准**：所有 feature 组合编译通过；无 feature 冲突；默认行为与 v4.6.0 一致

**依赖**：M8-T1

## M8-T3：文档同步与版本号更新

**任务描述**：更新 v4.7.0 相关文档（AGENTS.md feature gate 列 + 版本号）、运行文档一致性门禁，确保文档与代码同步。

**涉及文件**：`AGENTS.md`（版本 4.6.0 → 4.7.0，新增 7 feature）、`docs/spec/v4.7.0/tasks.md`（本文件，标记任务完成）、`CHANGELOG.md`（新增 v4.7.0 变更记录）

**子任务**：
- [ ] M8-T3.1 更新 `AGENTS.md` 版本号 4.6.0 → 4.7.0，新增 7 feature（delayed-priority-queue/forward-compat-sandbox/copy-parallel-shard/anomaly-remediation-rca/multicloud-cost-forecast/tenant-quota-rls-enhanced/cache-warmup-protection）
- [ ] M8-T3.2 更新 `AGENTS.md` v4.7.0 新增能力说明（智能化运维深化 + 性能深化层）
- [ ] M8-T3.3 更新 `CHANGELOG.md` 新增 v4.7.0 变更记录（7 项需求 + 7 feature gate）
- [ ] M8-T3.4 运行 `python scripts/check-doc-consistency.py`（门禁 12，文档与代码一致）
- [ ] M8-T3.5 运行 `python scripts/check-doc-sync.py --diff HEAD`（门禁 14，文档与 HEAD 同步）
- [ ] M8-T3.6 确认 `docs/spec/v4.7.0/tasks.md` 46 任务 / 290 子任务全部标记 `[x]`
- [ ] M8-T3.7 运行 `bash scripts/audit-verify.sh docs/spec/v4.7.0/tasks.md`（门禁 13），验证所有 file:line 引用真实存在
- [ ] M8-T3.8 验证 sz-pay 兼容性（sz-pay 拉取 v4.7.0 编译运行，既有功能不受影响）

**验收标准**：AGENTS.md 更新（版本 4.7.0 + 7 feature）；文档一致性门禁通过；tasks.md 全部完成；审计证据验证通过；sz-pay 兼容

**依赖**：M8-T2

---

# 十一、任务依赖关系图

```
M0（P0，文档基线，立即）
  ├─ M0-T1（v4.6.0 基线锁定）
  ├─ M0-T2（v4.7.0 环境准备，7 feature gate + 版本号）
  └─ M0-T3（基线验证）

M1（P1，延迟队列与优先级调度，独立扩展 sz-orm-queue，M0-T2 后启动）
  ├─ M1-T1（delayed-priority-queue feature + Config）← M0-T2
  ├─ M1-T2（PriorityQueue + aging 机制）← M1-T1
  ├─ M1-T3（DelayScheduler + 定时调度）← M1-T1, M1-T2
  ├─ M1-T4（调度日志 + 异常处理）← M1-T3
  ├─ M1-T5（消息脱敏 + 多租户隔离）← M1-T3
  └─ M1-T6（集成测试与门禁）← M1-T1~M1-T5

M2（P1，前向兼容与沙箱预演，独立扩展 sz-orm-core migration，M0-T2 后启动）
  ├─ M2-T1（forward-compat-sandbox feature + Config）← M0-T2
  ├─ M2-T2（ForwardCompatChecker 兼容性检查）← M2-T1
  ├─ M2-T3（SandboxDryRunner 沙箱预演）← M2-T1
  ├─ M2-T4（DependencyAnalyzer 依赖图 + 拓扑排序）← M2-T1
  ├─ M2-T5（检查与预演日志 + 异常处理）← M2-T2, M2-T3, M2-T4
  └─ M2-T6（集成测试与门禁）← M2-T1~M2-T5

M3（P1，COPY 协议与并行分片，独立扩展 sz-orm-batch，M0-T2 后启动）
  ├─ M3-T1（copy-parallel-shard feature + Config）← M0-T2
  ├─ M3-T2（CopyProtocolAdapter 方言适配）← M3-T1
  ├─ M3-T3（ParallelShardExecutor 并行分片）← M3-T1, M3-T2
  ├─ M3-T4（加载日志 + 异常处理）← M3-T2, M3-T3
  ├─ M3-T5（COPY 数据租户隔离 + 参数化）← M3-T2
  └─ M3-T6（集成测试与门禁）← M3-T1~M3-T5

M4（P1，租户配额与 RLS 增强，独立扩展 sz-orm-core tenant，M0-T2 后启动）
  ├─ M4-T1（tenant-quota-rls-enhanced feature + Config）← M0-T2
  ├─ M4-T2（QuotaEnforcer 配额执行器）← M4-T1
  ├─ M4-T3（RlsPolicyEnhancer RLS 策略增强）← M4-T1
  ├─ M4-T4（TenantAuditLogger 租户审计日志）← M4-T1
  ├─ M4-T5（配额与 RLS 集成 + 异常处理）← M4-T2, M4-T3, M4-T4
  └─ M4-T6（集成测试与门禁）← M4-T1~M4-T5

M5（P1，缓存预热与穿透防护，独立扩展 sz-orm-core cache，M0-T2 后启动）
  ├─ M5-T1（cache-warmup-protection feature + Config）← M0-T2
  ├─ M5-T2（BloomFilter 布隆过滤器）← M5-T1
  ├─ M5-T3（CacheWarmer 缓存预热器）← M5-T1, M5-T2
  ├─ M5-T4（PenetrationGuard 穿透防护器）← M5-T2, M5-T3
  ├─ M5-T5（SingleFlight 击穿防护器）← M5-T1
  └─ M5-T6（集成测试与门禁）← M5-T1~M5-T5

M6（P2，异常自愈与根因分析，独立扩展 sz-orm-observability，M0-T2 后启动）
  ├─ M6-T1（anomaly-remediation-rca feature + Config）← M0-T2
  ├─ M6-T2（RootCauseAnalyzer 根因分析器）← M6-T1
  ├─ M6-T3（AnomalyCorrelator 异常关联器）← M6-T1
  ├─ M6-T4（AutoRemediator 异常自愈器 + 审计日志）← M6-T1, M6-T2, M6-T3
  └─ M6-T5（集成测试与门禁）← M6-T1~M6-T4

M7（P2，多云成本对比与容量预测，独立扩展 sz-orm-storage，M0-T2 后启动）
  ├─ M7-T1（multicloud-cost-forecast feature + Config）← M0-T2
  ├─ M7-T2（MultiCloudCostComparator 多云对比）← M7-T1
  ├─ M7-T3（CapacityForecaster 容量预测）← M7-T1
  ├─ M7-T4（AutoOptimizer 自动优化执行）← M7-T1
  └─ M7-T5（集成测试与门禁）← M7-T1~M7-T4

M8（P0，集成验证与文档同步，M1~M7 全部完成后）
  ├─ M8-T1（workspace 全量集成测试）← M1-T6, M2-T6, M3-T6, M4-T6, M5-T6, M6-T5, M7-T5
  ├─ M8-T2（feature 全组合编译验证）← M8-T1
  └─ M8-T3（文档同步与版本号更新）← M8-T2
```

> **并行开发说明**：M1~M7 七项需求主体相互独立（design.md 依赖关系图明确声明），可并行开发。M0 完成后，M1~M7 同时启动；M1~M7 全部完成后，M8 启动。七项需求无强依赖，可并行推进。

---

# 十二、风险与缓解措施

| 风险 ID | 风险描述 | 影响 | 概率 | 缓解措施 | 责任任务 |
|---------|---------|------|------|---------|---------|
| R-001 | 延迟队列调度循环导致 CPU 空转 | 中 | 中 | 调度周期可配（默认 100ms）+ `CancellationToken` 优雅关闭 | M1-T3 |
| R-002 | 优先级队列 Strict 策略下低优先级消息饿死 | 中 | 中 | aging 机制（默认 5 分钟提升优先级）+ Weighted/FairShare 策略可选 | M1-T2 |
| R-003 | Cron 表达式无效导致定时调度失败 | 低 | 低 | 发布时校验 Cron 表达式，无效则拒绝发布 | M1-T3 |
| R-004 | 沙箱预演影子表残留导致存储浪费 | 中 | 低 | 预演完成后（无论成功失败）清理影子表，不残留 | M2-T3 |
| R-005 | 沙箱预演修改真实数据 | 高 | 低 | 影子表隔离 + 独立事务 + 预演后回滚 + 参数化绑定 | M2-T3 |
| R-006 | 迁移依赖图循环依赖导致死锁 | 中 | 低 | Kahn 算法循环检测，标注循环并返回错误 | M2-T4 |
| R-007 | COPY 协议加载大数据导致 DB 锁竞争 | 中 | 中 | 并行分片 + 批次大小上限 + 冲突解决策略 | M3-T3 |
| R-008 | 并行分片原子性违反导致部分加载 | 高 | 低 | 复用 v4.6.0 `BatchTransactionCoordinator` + `AtomicityGuarantee` 三级别 | M3-T3 |
| R-009 | 分片键不均匀导致负载倾斜 | 中 | 中 | 记录分片数据量供优化 + 可配再平衡策略 | M3-T3 |
| R-010 | 异常自愈误修复导致更大故障 | 高 | 中 | 须人工确认（可配白名单）+ 审计日志 + 不静默执行 | M6-T4 |
| R-011 | 根因分析证据不足导致误判 | 中 | 中 | 附证据链 + 置信度可配阈值 + 低置信度标注"根因不确定" | M6-T2 |
| R-012 | 异常关联误关联导致误导 | 中 | 中 | 关联性评分 + 低评分标注"weak correlation" | M6-T3 |
| R-013 | 多云成本对比 provider API 不可用 | 低 | 中 | 跳过该 provider 对比，记录日志 | M7-T2 |
| R-014 | 容量预测历史数据不足 | 低 | 低 | 拒绝预测，返回错误"insufficient history data" | M7-T3 |
| R-015 | 成本自动优化执行失败 | 中 | 低 | 记录执行失败日志，通知 FinOps 人工干预 | M7-T4 |
| R-016 | 租户配额检查导致性能下降 | 中 | 中 | `AtomicU32` 无锁原子计数器 + 检查开销 ≤ 0.1ms/次 | M4-T2 |
| R-017 | RLS 自动注入 SQL 注入 | 高 | 低 | 参数化绑定（复用 `Connection::execute_with_params`）+ tenant_id 防篡改 | M4-T3 |
| R-018 | 审计日志写入失败导致丢失 | 中 | 低 | 记录到备用日志（stderr）+ 告警 SRE + 不阻断主流程 | M4-T4 |
| R-019 | 缓存预热阻塞服务启动 | 中 | 低 | 异步预热（`tokio::spawn`）+ 预热失败不影响启动 | M5-T3 |
| R-020 | 布隆过滤器误判导致穿透 | 低 | 中 | 误判时回退到 DB 查询 + 不漏判（不存在一定返回 None） | M5-T2 |
| R-021 | singleflight 死锁导致长期阻塞 | 高 | 低 | 重建超时释放锁（默认 5 秒）+ 其他请求可重试 | M5-T5 |
| R-022 | 7 项需求并行开发导致合并冲突 | 中 | 中 | 每项需求独立 feature gate + 模块边界清晰 + 分支策略（每需求一分支） | 全局 |
| R-023 | 新增 feature 与既有 feature 组合编译失败 | 中 | 低 | 门禁 10 全组合编译 + feature 依赖关系验证（M8-T2） | M8-T2 |
| R-024 | sz-pay 既有代码因 API 变更破坏 | 高 | 低 | 无 Breaking Change，feature gate 隔离默认关闭，既有公开 API 完全向后兼容 | M8-T1 |
| R-025 | 沙箱预演 SQL 注入（参数拼接） | 高 | 低 | 复用既有 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82` 参数化绑定 | M2-T3 |

---

# 十三、验收标准总览

## 13.1 REQ-V47-001 消息延迟队列与优先级调度（P1，M1）

1. `DelayedMessage` 延迟投递（按 deliver_at 投递，到期前不可消费）← M1-T3
2. `PriorityQueue` + `PriorityPolicy` 优先级队列（Strict/Weighted/FairShare，默认 Strict）← M1-T2
3. `ScheduledMessage` 定时调度（Cron 表达式或间隔，复用 v4.6.0 `RedeliveryScheduler`）← M1-T3
4. 延迟与优先级组合（延迟到期后按优先级投递）← M1-T3
5. 优先级队列不饿死低优先级（aging 机制，默认 5 分钟）← M1-T2
6. 复用既有 `MessageQueue`/`Message`/`RedeliveryScheduler`，不重复实现 ← M1-T3
7. 调度日志可追溯（消息 ID + 延迟时间 + 优先级 + 投递时间 + 结果）← M1-T4
8. `delayed-priority-queue` feature gate 隔离，默认关闭，既有 `MessageQueue` 与 v4.6.0 `RedeliveryScheduler` 保留 ← M1-T1/T6

## 13.2 REQ-V47-002 迁移前向兼容性检查与沙箱预演（P1，M2）

1. `ForwardCompatChecker` 前向兼容性检查（识别删除列/改列类型/改列约束等破坏性变更）← M2-T2
2. `SandboxDryRunner` 沙箱预演（影子表预执行 + 数据完整性/查询兼容性/性能影响校验）← M2-T3
3. `MigrationDependencyGraph` 迁移依赖图分析（依赖关系 + 执行顺序 + 循环检测）← M2-T4
4. 前向兼容性规则可配置（破坏性变更类型 + 检查严格度）← M2-T2
5. 沙箱预演验证项可配置（数据完整性/查询兼容性/性能影响）← M2-T3
6. 复用既有 `DryRunMigration`/`ImpactReport`/`RollbackExecutor`，不重复实现 ← M2-T2/T3
7. 检查与预演日志可追溯（迁移版本 + 检查结果 + 预演结果 + 耗时）← M2-T5
8. `forward-compat-sandbox` feature gate 隔离，默认关闭，既有 `Migration`/`DryRunMigration`/`RollbackExecutor` 保留 ← M2-T1/T6

## 13.3 REQ-V47-003 批量 COPY 协议与并行分片执行（P1，M3）

1. `CopyProtocolAdapter` COPY 协议方言适配（PostgreSQL COPY / MySQL LOAD DATA / Oracle SQL*Loader / MSSQL BULK INSERT，其他降级 multi-value INSERT）← M3-T2
2. `ParallelShardExecutor` 并行分片执行（按分片键拆分并行，吞吐量 N 倍提升）← M3-T3
3. `ConflictResolution` 冲突解决策略（Upsert/Ignore/Merge/Replace，默认 Upsert）← M3-T2
4. COPY 协议与并行分片组合（每分片 COPY 并行加载）← M3-T3
5. 并行分片原子性（复用 v4.6.0 `AtomicityGuarantee`，AllOrNothing/BestEffort/SagaCompensation）← M3-T3
6. 复用既有 `CopyProtocolExecutor`/`BatchExecutor`/`BatchTransactionCoordinator`，不重复实现 ← M3-T2/T3
7. 加载日志可追溯（加载行数 + 分片列表 + 冲突解决 + 耗时 + 结果）← M3-T4
8. `copy-parallel-shard` feature gate 隔离，默认关闭，既有 `CopyProtocolExecutor`/`BatchExecutor` 保留 ← M3-T1/T6

## 13.4 REQ-V47-004 异常自愈与根因分析（P2，M6）

1. `AutoRemediator` 异常自愈（RemediationAction: RestartConnection/ClearCache/ScaleOut/CustomAction，须人工确认 + 白名单）← M6-T4
2. `RootCauseAnalyzer` 根因分析（根因组件 + SQL + 置信度 + 证据链）← M6-T2
3. `AnomalyCorrelator` 异常关联分析（跨指标关联，识别同根因异常集群）← M6-T3
4. 自愈动作白名单可配置（白名单内自动执行，非白名单须人工确认，默认空）← M6-T4
5. 根因分析置信度阈值可配置（默认 0.7，低于阈值标注"根因不确定"）← M6-T2
6. 复用 v4.6.0 `AnomalyDetector`/`MetricsRegistry`/`QueryLogger`，不重复实现 ← M6-T2/T4
7. 自愈审计日志可追溯（异常 ID + 动作 + 执行人 + 时间 + 结果，追加写入不可篡改）← M6-T4
8. `anomaly-remediation-rca` feature gate 隔离，默认关闭，既有 `AnomalyDetector` 保留 ← M6-T1/T5

## 13.5 REQ-V47-005 多云成本对比与容量预测（P2，M7）

1. `MultiCloudCostComparator` 多云成本对比（跨 provider 成本差异 + 迁移建议）← M7-T2
2. `CapacityForecaster` 容量预测（LinearRegression/ExponentialSmoothing/HoltWinters，附置信区间）← M7-T3
3. `AutoOptimizer` 成本自动优化执行（自动执行优化建议，须人工确认 + 白名单）← M7-T4
4. provider 迁移建议（附迁移成本 + 迁移风险 + 预期节省）← M7-T2
5. 容量预测附置信区间（默认 95%，不单点预测）← M7-T3
6. 复用 v4.6.0 `CostAnalyzer`/`Storage`/`StorageProvider`，不重复实现 ← M7-T2/T4
7. 对比与预测可配置（周期 + 算法 + 置信水平 + 白名单）← M7-T1
8. `multicloud-cost-forecast` feature gate 隔离，默认关闭，既有 `CostAnalyzer`/`Storage` 保留 ← M7-T1/T5

## 13.6 REQ-V47-006 租户资源配额与行级安全增强（P1，M4）

1. `TenantResourceQuota` 租户资源配额（max_connections/max_qps/max_storage，`QuotaEnforcer` 执行）← M4-T2
2. `RlsPolicyEnhancer` RLS 策略增强（多条件组合 + 复杂谓词 + 列级脱敏联动，自动注入 WHERE 参数化）← M4-T3
3. `TenantAuditLogger` 租户级审计日志（连接/查询/配额超限/RLS 命中，追加写入不可篡改）← M4-T4
4. 配额检查在连接池/查询层强制执行（不可被应用层绕过）← M4-T2
5. RLS 自动注入参数化绑定（tenant_id 不可篡改，由可信路径设置）← M4-T3
6. 配额与 RLS 可配置（默认无配额限制，向后兼容）← M4-T1
7. 复用 v4.6.0 `ConnectionTenantBinder` + 既有 `RowLevelSecurityPolicy`/`Pool`，不重复实现 ← M4-T2/T3
8. 配额与审计日志可追溯（租户 ID + 配额类型 + 当前值 + 限制值 + 操作）← M4-T4/T5
9. `tenant-quota-rls-enhanced` feature gate 隔离，默认关闭，既有 `ConnectionTenantBinder`/`RowLevelSecurityPolicy`/`Pool` 保留 ← M4-T1/T6

## 13.7 REQ-V47-007 缓存预热与穿透防护（P1，M5）

1. `CacheWarmer` 缓存预热（HotspotTable/HotspotKey/CustomQuery，异步不阻塞启动）← M5-T3
2. `BloomFilter` + `PenetrationGuard` 缓存穿透防护（不存在的 key 直接返回 None，误判率可配默认 1%）← M5-T2/T4
3. `SingleFlight` 缓存击穿防护（并发重建只执行一次，其他等待复用）← M5-T5
4. 预热与 L1/L2 协同（预热数据同时加载到 L1+L2）← M5-T3
5. 布隆过滤器不漏判（不存在的 key 一定返回 None，误判回退 DB）← M5-T2
6. singleflight 不死锁（重建超时释放锁，默认 5 秒）← M5-T5
7. 预热与防护可配置（策略 + 数据量 + 布隆容量/误判率 + singleflight 超时）← M5-T1
8. 复用 v4.6.0 `ProcessL1Cache`/`L2Cache`/`CacheCoherenceProtocol`，不重复实现 ← M5-T3/T4
9. 预热与防护日志可追溯（策略 + 数据量 + 命中率 + 过滤次数）← M5-T3
10. `cache-warmup-protection` feature gate 隔离，默认关闭，既有 `ProcessL1Cache`/`L2Cache` 保留 ← M5-T1/T6

## 13.8 全局验收

1. 无 Breaking Change，feature gate 隔离默认全关闭，既有公开 API 完全向后兼容 ← M1-T6~M7-T5
2. v4.6.0 测试基线不回退（仅增不减）← M8-T1
3. 14 道门禁全通过 ← M8-T1
4. feature 全组合编译通过 ← M8-T2
5. 五方言覆盖（MySQL/PostgreSQL/SQLite/Oracle/MSSQL）← M2-T6/M3-T6/M4-T6
6. 禁止占位实现（todo!/unimplemented!/unreachable!）← M1-T6~M7-T5
7. unsafe 零容忍 ← M1-T6~M7-T5
8. 参数化查询强制（复用 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82`）← M2-T3/M3-T5/M4-T3/M5-T3
9. 审计证据（每项结论附 file:line 证据）← 全任务
10. 与 v4.6.0 零重叠（智能化运维深化+性能深化层 vs 可靠性+运维智能化层）← 全任务
11. 不新增包（workspace 成员保持 60）← M0-T2

---

# 十四、feature gate 总览

| feature gate | 所属包 | 控制能力 | 默认 | 对应需求 | 测试命令 |
|-------------|--------|---------|------|---------|---------|
| `delayed-priority-queue` | sz-orm-queue（扩展） | 消息延迟队列与优先级调度（延迟投递 + 优先级队列 + 定时调度） | 关闭 | REQ-V47-001 | `cargo test -p sz-orm-queue --features delayed-priority-queue` |
| `forward-compat-sandbox` | sz-orm-core（扩展） | 迁移前向兼容性检查与沙箱预演（兼容检查 + 沙箱预演 + 依赖图） | 关闭 | REQ-V47-002 | `cargo test -p sz-orm-core --features forward-compat-sandbox` |
| `copy-parallel-shard` | sz-orm-batch（扩展） | 批量 COPY 协议与并行分片执行（COPY 方言适配 + 并行分片 + 冲突解决） | 关闭 | REQ-V47-003 | `cargo test -p sz-orm-batch --features copy-parallel-shard` |
| `anomaly-remediation-rca` | sz-orm-observability（扩展） | 异常自愈与根因分析（自愈 + RCA + 关联分析） | 关闭 | REQ-V47-004 | `cargo test -p sz-orm-observability --features anomaly-remediation-rca` |
| `multicloud-cost-forecast` | sz-orm-storage（扩展） | 多云成本对比与容量预测（成本对比 + 容量预测 + 自动优化） | 关闭 | REQ-V47-005 | `cargo test -p sz-orm-storage --features multicloud-cost-forecast` |
| `tenant-quota-rls-enhanced` | sz-orm-core（扩展） | 租户资源配额与行级安全增强（配额 + RLS 增强 + 审计日志） | 关闭 | REQ-V47-006 | `cargo test -p sz-orm-core --features tenant-quota-rls-enhanced` |
| `cache-warmup-protection` | sz-orm-core（扩展） | 缓存预热与穿透防护（预热 + 布隆过滤器 + singleflight） | 关闭 | REQ-V47-007 | `cargo test -p sz-orm-core --features cache-warmup-protection` |

---

# 十五、复用点清单

## 15.1 复用统计

| 需求 | 复用点数 | 新增点数 | 复用率 |
|------|---------|---------|--------|
| REQ-V47-001 延迟队列与优先级调度 | 8 | 6 | 57.1% |
| REQ-V47-002 前向兼容与沙箱预演 | 9 | 8 | 52.9% |
| REQ-V47-003 COPY 协议与并行分片 | 8 | 7 | 53.3% |
| REQ-V47-004 异常自愈与根因分析 | 5 | 7 | 41.7% |
| REQ-V47-005 多云成本对比与容量预测 | 7 | 7 | 50.0% |
| REQ-V47-006 租户配额与 RLS 增强 | 10 | 6 | 62.5% |
| REQ-V47-007 缓存预热与穿透防护 | 8 | 7 | 53.3% |
| **合计** | **55** | **48** | **53.4%** |

> **复用率说明**：v4.7.0 整体复用率 53.4%，优先复用既有能力，不重复实现，符合 spec.md §1.4 复用优先约束。复用率较 v4.6.0（57.3%）略低，因为 v4.7.0 是"智能化运维深化+性能深化"层（新增延迟调度器/沙箱预演器/并行分片执行器/根因分析器/容量预测器/布隆过滤器等新逻辑），而 v4.6.0 是"可靠性+运维智能化"层。详见 design.md §1.1。

## 15.2 关键复用点（附 file:line 证据）

| 复用项 | 复用位置 | 用途 | 对应需求 |
|--------|---------|------|---------|
| `MessageQueue` trait | `packages/sz-orm-queue/src/queue.rs:18` | 既有消息队列 trait，延迟调度器复用 publish/consume | REQ-V47-001 |
| `Message` | `packages/sz-orm-queue/src/queue.rs:57` | 既有消息结构，`DelayedMessage` 包装复用 | REQ-V47-001 |
| `RedeliveryScheduler` | `packages/sz-orm-queue/src/dlx.rs:216` | v4.6.0 重投递调度器，`DelayScheduler` 复用调度循环基线 | REQ-V47-001 |
| `BackoffPolicy` | `packages/sz-orm-queue/src/dlx.rs:47` | v4.6.0 退避策略，延迟消息投递失败重试复用 | REQ-V47-001 |
| `DryRunMigration` | `packages/sz-orm-core/src/migration_dry_run.rs:11` | 既有 dry-run 迁移，`ForwardCompatChecker` 复用 analyze_impact | REQ-V47-002 |
| `ImpactReport` | `packages/sz-orm-core/src/migration_dry_run.rs:80` | 既有影响报告，前向兼容性检查复用 | REQ-V47-002 |
| `RollbackExecutor` | `packages/sz-orm-core/src/rollback_zero_downtime.rs:305` | v4.6.0 回滚执行器，`SandboxDryRunner` 复用影子表能力 | REQ-V47-002 |
| `CopyProtocolExecutor` | `packages/sz-orm-batch/src/copy.rs:14` | 既有 PG COPY 执行器，`CopyProtocolAdapter` 复用 PG COPY 实现 | REQ-V47-003 |
| `BatchTransactionCoordinator` | `packages/sz-orm-batch/src/atomic.rs:216` | v4.6.0 批量事务协调器，`ParallelShardExecutor` 复用原子性 | REQ-V47-003 |
| `AtomicityGuarantee` | `packages/sz-orm-batch/src/atomic.rs:20` | v4.6.0 原子性保证级别，并行分片复用 | REQ-V47-003 |
| `SagaCompensator` | `packages/sz-orm-batch/src/atomic.rs:436` | v4.6.0 Saga 补偿器，`SagaCompensation` 模式复用 | REQ-V47-003 |
| `AnomalyDetector` | `packages/sz-orm-observability/src/anomaly.rs:254` | v4.6.0 异常检测器，`AutoRemediator` 复用异常事件订阅 | REQ-V47-004 |
| `MetricsRegistry` | `packages/sz-orm-observability/src/lib.rs:262` | 既有指标注册表，`RootCauseAnalyzer` 复用指标历史 | REQ-V47-004 |
| `QueryLogger` | `packages/sz-orm-observability/src/query_logger.rs:73` | 既有查询日志器，`RootCauseAnalyzer` 复用日志证据链 | REQ-V47-004 |
| `CostAnalyzer` | `packages/sz-orm-storage/src/cost.rs:231` | v4.6.0 成本分析器，`MultiCloudCostComparator` 复用 analyze_cost | REQ-V47-005 |
| `CostOptimizationSuggestion` | `packages/sz-orm-storage/src/cost.rs:55` | v4.6.0 成本优化建议，`AutoOptimizer` 复用 | REQ-V47-005 |
| `Storage` trait | `packages/sz-orm-storage/src/storage.rs:14` | 既有存储抽象，`AutoOptimizer` 复用执行优化 | REQ-V47-005 |
| `ConnectionTenantBinder` | `packages/sz-orm-core/src/connection_tenant.rs:133` | v4.6.0 连接租户绑定器，`QuotaEnforcer` 复用 acquire_with_tenant 路径 | REQ-V47-006 |
| `RowLevelSecurityPolicy` | `packages/sz-orm-core/src/tenant_security.rs:67` | 既有行级安全策略，`RlsPolicyEnhancer` 复用 WHERE 注入 | REQ-V47-006 |
| `ColumnMaskingRule` | `packages/sz-orm-core/src/tenant_security.rs:155` | 既有列级脱敏，`RlsPolicyEnhancer` 联动复用 | REQ-V47-006 |
| `TenantAuditContext` | `packages/sz-orm-core/src/tenant_security.rs:244` | 既有审计上下文，`TenantAuditLogger` 复用 | REQ-V47-006 |
| `ProcessL1Cache` | `packages/sz-orm-core/src/process_l1_cache.rs:169` | v4.6.0 进程级 L1 缓存，`CacheWarmer`/`PenetrationGuard` 复用 | REQ-V47-007 |
| `L2Cache` | `packages/sz-orm-core/src/l2_cache.rs:517` | 既有 L2 缓存，`CacheWarmer`/`PenetrationGuard` 复用 | REQ-V47-007 |
| `CacheCoherenceProtocol` | `packages/sz-orm-core/src/cache_coherence.rs:103` | 既有缓存一致性协议，预热一致性复用 | REQ-V47-007 |
| `tenant_cache_key` | `packages/sz-orm-core/src/process_l1_cache.rs:421` | 既有租户缓存键，预热按租户隔离复用 | REQ-V47-007 |
| `Connection::execute_with_params` | `packages/sz-orm-core/src/pool.rs:82` | 参数化绑定执行，防 SQL 注入（全局复用） | 全部 |

---

# 十六、与 v4.6.0 的关系

## 16.1 零重叠声明

v4.7.0 与 v4.6.0 零重叠：

| v4.6.0 能力（可靠性 + 运维智能化层） | v4.7.0 能力（智能化运维深化 + 性能深化层） | 关系 |
|-------------------------------|-------------------------|------|
| 消息死信队列自动重投递（`sz-orm-queue` dlx-auto-redelivery） | 消息延迟队列与优先级调度（`sz-orm-queue` delayed-priority-queue） | v4.7.0 延迟队列与优先级调度复用 v4.6.0 `RedeliveryScheduler`（`packages/sz-orm-queue/src/dlx.rs:216`）调度器基线，扩展延迟/优先级/定时维度，不修改既有 DLX 自动重投递逻辑 |
| 迁移回滚自动化（`sz-orm-core` zero-downtime-rollback） | 迁移前向兼容性检查与沙箱预演（`sz-orm-core` forward-compat-sandbox） | v4.7.0 沙箱预演复用 v4.6.0 `RollbackExecutor`（`packages/sz-orm-core/src/rollback_zero_downtime.rs:305`）影子表能力 + 既有 `DryRunMigration`（`packages/sz-orm-core/src/migration_dry_run.rs:11`），扩展前向兼容性检查与依赖图，不修改既有回滚/dry-run 逻辑 |
| 批量事务原子性保证（`sz-orm-batch` batch-atomic） | 批量 COPY 协议与并行分片执行（`sz-orm-batch` copy-parallel-shard） | v4.7.0 并行分片执行复用 v4.6.0 `BatchTransactionCoordinator`（`packages/sz-orm-batch/src/atomic.rs:216`）原子性 + 既有 `CopyProtocolExecutor`（`packages/sz-orm-batch/src/copy.rs:14`），扩展 COPY 方言适配与并行分片，不修改既有原子性/COPY 逻辑 |
| 异常检测（`sz-orm-observability` anomaly-detection） | 异常自愈与根因分析（`sz-orm-observability` anomaly-remediation-rca） | v4.7.0 异常自愈与根因分析复用 v4.6.0 `AnomalyDetector`（`packages/sz-orm-observability/src/anomaly.rs:254`）异常检测，扩展自愈/RCA/关联，不修改既有异常检测逻辑 |
| 存储成本分析与优化建议（`sz-orm-storage` cost-analysis） | 多云成本对比与容量预测（`sz-orm-storage` multicloud-cost-forecast） | v4.7.0 多云成本对比与容量预测复用 v4.6.0 `CostAnalyzer`（`packages/sz-orm-storage/src/cost.rs:231`）成本分析，扩展跨 provider 对比/容量预测/自动优化，不修改既有成本分析逻辑 |
| 连接级多租户隔离（`sz-orm-core` connection-level-tenant） | 租户资源配额与行级安全增强（`sz-orm-core` tenant-quota-rls-enhanced） | v4.7.0 租户配额与 RLS 增强复用 v4.6.0 `ConnectionTenantBinder`（`packages/sz-orm-core/src/connection_tenant.rs:133`）连接级隔离 + 既有 `RowLevelSecurityPolicy`（`packages/sz-orm-core/src/tenant_security.rs:67`），扩展配额/RLS 增强/审计，不修改既有连接级隔离/RLS 逻辑 |
| 进程级 L1 缓存（`sz-orm-core` process-l1-cache） | 缓存预热与穿透防护（`sz-orm-core` cache-warmup-protection） | v4.7.0 缓存预热与穿透防护复用 v4.6.0 `ProcessL1Cache`（`packages/sz-orm-core/src/process_l1_cache.rs:169`）进程级 L1 + 既有 `L2Cache`（`packages/sz-orm-core/src/l2_cache.rs:517`），扩展预热/布隆/singleflight，不修改既有进程级 L1/L2 逻辑 |

## 16.2 依赖关系

```
v4.6.0 已验收基线（7 个 feature gate: dlx-auto-redelivery / zero-downtime-rollback / batch-atomic / anomaly-detection / cost-analysis / connection-level-tenant / process-l1-cache）
  │
  ├─ dlx-auto-redelivery ───→ REQ-V47-001 延迟队列与优先级调度（复用 RedeliveryScheduler 调度器基线）
  ├─ zero-downtime-rollback ─→ REQ-V47-002 前向兼容与沙箱预演（复用 RollbackExecutor 影子表能力）
  ├─ batch-atomic ──────────→ REQ-V47-003 COPY 协议与并行分片（复用 BatchTransactionCoordinator 原子性）
  ├─ anomaly-detection ─────→ REQ-V47-004 异常自愈与根因分析（复用 AnomalyDetector 异常检测）
  ├─ cost-analysis ─────────→ REQ-V47-005 多云成本对比与容量预测（复用 CostAnalyzer 成本分析）
  ├─ connection-level-tenant→ REQ-V47-006 租户配额与 RLS 增强（复用 ConnectionTenantBinder 连接级隔离）
  └─ process-l1-cache ─────→ REQ-V47-007 缓存预热与穿透防护（复用 ProcessL1Cache 进程级 L1）

v4.7.0 七项需求相互独立，可并行开发：
  ├─ REQ-V47-001 延迟队列与优先级调度（扩展 sz-orm-queue，复用既有 MessageQueue + v4.6.0 RedeliveryScheduler）
  ├─ REQ-V47-002 前向兼容与沙箱预演（扩展 sz-orm-core migration，复用既有 DryRunMigration + v4.6.0 RollbackExecutor）
  ├─ REQ-V47-003 COPY 协议与并行分片（扩展 sz-orm-batch，复用既有 CopyProtocolExecutor + v4.6.0 BatchTransactionCoordinator）
  ├─ REQ-V47-004 异常自愈与根因分析（扩展 sz-orm-observability，复用 v4.6.0 AnomalyDetector）
  ├─ REQ-V47-005 多云成本对比与容量预测（扩展 sz-orm-storage，复用 v4.6.0 CostAnalyzer）
  ├─ REQ-V47-006 租户配额与 RLS 增强（扩展 sz-orm-core tenant，复用 v4.6.0 ConnectionTenantBinder + 既有 RowLevelSecurityPolicy）
  └─ REQ-V47-007 缓存预热与穿透防护（扩展 sz-orm-core cache，复用 v4.6.0 ProcessL1Cache + 既有 L2Cache）
```

## 16.3 扩展包

| 包名 | 对应需求 | 扩展内容 |
|------|---------|---------|
| `sz-orm-queue` | REQ-V47-001 | 消息延迟队列与优先级调度（延迟投递 + 优先级队列 + 定时调度，`delayed-priority-queue` feature） |
| `sz-orm-core` | REQ-V47-002 / REQ-V47-006 / REQ-V47-007 | 前向兼容与沙箱预演（`forward-compat-sandbox` feature）+ 租户配额与 RLS 增强（`tenant-quota-rls-enhanced` feature）+ 缓存预热与穿透防护（`cache-warmup-protection` feature） |
| `sz-orm-batch` | REQ-V47-003 | 批量 COPY 协议与并行分片执行（COPY 方言适配 + 并行分片 + 冲突解决，`copy-parallel-shard` feature） |
| `sz-orm-observability` | REQ-V47-004 | 异常自愈与根因分析（自愈 + RCA + 关联，`anomaly-remediation-rca` feature） |
| `sz-orm-storage` | REQ-V47-005 | 多云成本对比与容量预测（成本对比 + 容量预测 + 自动优化，`multicloud-cost-forecast` feature） |

## 16.4 新增包

本版本不新增包，所有能力通过既有包扩展实现（sz-orm-queue / sz-orm-core / sz-orm-batch / sz-orm-observability / sz-orm-storage），workspace 成员保持 60 个。

## 16.5 版本号变更

| 项目 | v4.6.0 | v4.7.0 | 变更类型 |
|------|--------|--------|---------|
| workspace.package.version | 4.6.0 | 4.7.0 | minor 版本号升级 |
| workspace 成员数 | 60 | 60 | 0（不新增包） |
| feature gate 数 | v4.6.0 7 个 + 既有 | v4.7.0 7 个 + v4.6.0 7 个 + 既有 | 新增 7 feature |
| sz-orm-queue feature | cdc / message-tracing / dlx-auto-redelivery | cdc / message-tracing / dlx-auto-redelivery / delayed-priority-queue | 扩展 1 feature |
| sz-orm-core feature | 既有 40+ / zero-downtime-rollback / connection-level-tenant / process-l1-cache | 既有 40+ / zero-downtime-rollback / connection-level-tenant / process-l1-cache / forward-compat-sandbox / tenant-quota-rls-enhanced / cache-warmup-protection | 扩展 3 feature |
| sz-orm-batch feature | batch-stream / batch-v2 / batch-atomic | batch-stream / batch-v2 / batch-atomic / copy-parallel-shard | 扩展 1 feature |
| sz-orm-observability feature | query-logging / service-mesh / anomaly-detection | query-logging / service-mesh / anomaly-detection / anomaly-remediation-rca | 扩展 1 feature |
| sz-orm-storage feature | storage-lifecycle / real-cloud / cost-analysis | storage-lifecycle / real-cloud / cost-analysis / multicloud-cost-forecast | 扩展 1 feature |

---

# 十七、14 道门禁清单

| # | 门禁 | 命令 | 责任任务 |
|---|------|------|---------|
| 1 | fmt 格式检查 | `cargo fmt --all -- --check` | M8-T1.3 |
| 2 | check 编译检查 | `cargo check --workspace --all-targets` | M8-T1.4 |
| 3 | clippy 静态分析 | `cargo clippy --workspace --all-targets -- -D warnings` | M8-T1.5 |
| 4 | test 单元/集成测试 | `cargo test --workspace` | M8-T1.6 |
| 5 | doc 文档构建 | `cargo doc --workspace --no-deps --all-features` | M8-T1.7 |
| 6 | audit 安全审计 | `cargo audit` + `cargo deny check` | M8-T1 |
| 7 | integration 真实服务集成 | `cargo test --workspace -- --ignored` | M8-T1 |
| 8 | 禁止占位实现检查 | `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` | M8-T1.8 |
| 9 | SQL 注入扫描 | `scripts/check-sql-injection.ps1` | M8-T1 |
| 10 | Feature 全组合编译 | `cargo check --workspace --all-targets --all-features` | M8-T1.9 / M8-T2 |
| 11 | 上游仓库未修改检查 | `git diff --name-only HEAD`（ADR-0001） | M8-T1 |
| 12 | 文档与代码一致性检查 | `python scripts/check-doc-consistency.py` | M8-T3.4 |
| 13 | 审计证据验证 | `bash scripts/audit-verify.sh <审计报告.md>` | M8-T3.7 |
| 14 | 文档同步更新检查 | `python scripts/check-doc-sync.py --diff HEAD` | M8-T3.5 |

---

> 文档生成依据：`docs/spec/v4.7.0/spec.md`（需求规格，1218 行，7 项 EARS 需求）+ `docs/spec/v4.7.0/design.md`（技术设计，1523 行）+ `docs/spec/v4.6.0/tasks.md`（v4.6.0 任务规划，46 任务 / 264 子任务已完成）+ 2026-08-13 逐项代码验证（所有 file:line 证据均已实测存在）
> 审计合规：本文档所有 file:line 证据均引用真实存在的代码，遵循 AGENTS.md 审计合规铁律
> 任务约束：46 任务 / 290 子任务，每项任务附输入/输出/验收标准/复用点（file:line 证据），任务粒度 0.5-1 天可完成，依赖关系清晰，里程碑划分合理（M0 文档基线 → M1~M7 七项需求并行 → M8 集成验证）
> 下一阶段：编码实施（按 tasks.md 任务顺序执行，M0 → M1~M7 并行 → M8）





