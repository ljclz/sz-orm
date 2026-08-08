# sz-orm v3.2.0 任务分解文档

> 版本：v3.2.0（性能深度优化）
> 基线：v3.1.0（已完成：GraphPool 连接池改进 + WASM TypeScript 定义 + rdkafka-sys 可选化 + OpenTelemetry 集成 + 全部 10 项交付），workspace 2.3.0 已发布 crates.io
> 日期：2026-08-08
> 文档定位：任务分解（可执行任务清单），对应需求规格 `docs/spec/v3.2.0/spec.md`（20 条 EARS 需求，4 组 REQ-ZC/REQ-SIMD/REQ-PW/REQ-PC）与技术设计 `docs/spec/v3.2.0/design.md`（1347 行，4 大模块、5 里程碑、12 风险）
> 任务总数：5 个里程碑 / 50 个子任务，覆盖全部 20 条需求 + 8 总体验收标准
> 工程化基线：AGENTS.md 10 道门禁 + 五维审查 + 审计合规铁律（file:line 证据）
> 任务 ID 规范：`T-M{里程碑}-{序号}`，如 T-M1-001

---

## 任务规划原则

1. **垂直切割**：按业务功能分组（连接池预热增强 / 查询计划缓存 / 零拷贝序列化 / SIMD 加速 / 集成发布），非按技术层次分组
2. **可验收**：每个子任务标注对应需求编号、验收标准（含具体命令 + 期望结果）、门禁命令，可独立判定完成
3. **原子性**：一个子任务只做一件事，标注涉及文件路径，工作量 0.5~2 天（design.md 里程碑总工期 16 天）
4. **有序性**：被依赖任务在前，按 design.md §2.7 里程碑顺序（M1 预热 → M2 计划缓存 → M3 零拷贝 → M4 SIMD → M5 集成发布）；M1~M4 互相独立可并行
5. **门禁对齐**：每个任务验收标准必须包含门禁命令（参考 AGENTS.md 10 道门禁）
6. **审计合规**：每条结论附 file:line 证据，修复后必须运行 `cargo test` 验证
7. **Feature 隔离**：四项新能力通过 `zero-copy` / `simd` / `auto-prewarm` / `plan-cache` 四个 feature gate 隔离，默认关闭，无 Breaking Change
8. **unsafe 零容忍**：SIMD 通过 `wide` crate 安全抽象，禁止 `unsafe`（除 `// SAFETY:` 论证注释）

---

## 1. 任务总览

### 1.1 任务统计

| 里程碑 | 名称 | 任务数 | 需求组 | 周期 | 风险 |
|--------|------|--------|--------|------|------|
| M1 | 连接池预热增强 | 10 | REQ-PW-001~005 | 3 天 | 低 |
| M2 | 查询计划缓存 | 12 | REQ-PC-001~005 | 4 天 | 中 |
| M3 | 零拷贝序列化 | 10 | REQ-ZC-001~005 | 4 天 | 高 |
| M4 | SIMD 加速 | 9 | REQ-SIMD-001~005 | 3 天 | 高 |
| M5 | 集成验证 + 发布 | 9 | 全部（AC-ALL-1~8） | 2 天 | 中 |
| **合计** | | **50** | **20 条需求 + 8 总体验收** | **16 天** | |

### 1.2 任务依赖关系图

```mermaid
graph TD
    M1[M1: 连接池预热增强<br/>10 任务 / 3 天 / 低风险]
    M2[M2: 查询计划缓存<br/>12 任务 / 4 天 / 中风险]
    M3[M3: 零拷贝序列化<br/>10 任务 / 4 天 / 高风险]
    M4[M4: SIMD 加速<br/>9 任务 / 3 天 / 高风险]
    M5[M5: 集成验证 + 发布<br/>9 任务 / 2 天]

    M1 --> M5
    M2 --> M5
    M3 --> M5
    M4 --> M5

    M1 -.可并行.-> M2
    M2 -.可并行.-> M3
    M3 -.可并行.-> M4
```

**ASCII 版本**：

```
┌─────────────────────────────────┐
│ M1: 连接池预热增强 (10 任务/3天) │ ─┐
└─────────────────────────────────┘  │
┌─────────────────────────────────┐  │
│ M2: 查询计划缓存 (12 任务/4天)   │ ─┤
└─────────────────────────────────┘  │  M1~M4 互相独立可并行
┌─────────────────────────────────┐  │
│ M3: 零拷贝序列化 (10 任务/4天)   │ ─┤
└─────────────────────────────────┘  │
┌─────────────────────────────────┐  │
│ M4: SIMD 加速 (9 任务/3天)       │ ─┘
└─────────────────────────────────┘
                 ▼
┌─────────────────────────────────┐
│ M5: 集成验证 + 发布 (9 任务/2天) │
└─────────────────────────────────┘
```

### 1.3 与 design.md 里程碑映射关系

| 本文档里程碑 | design.md §2.7 里程碑 | design.md 交付物 | 一致性 |
|-------------|----------------------|-----------------|--------|
| M1: 连接池预热增强 | M1: 连接池预热增强 | `prewarm.rs` + `Pool::new_async` + `PoolConfig` 扩展 + `PrewarmProgress` + `UnifiedPool::unified_prewarm` + 渐进式分批 + telemetry 集成 | ✅ |
| M2: 查询计划缓存 | M2: 查询计划缓存 | `plan_cache.rs` + `SqlNormalizer` + `PlanCache` + LRU 淘汰 + 表级失效 + 命中率统计 + `UnifiedQueryOptimizer::with_plan_cache` | ✅ |
| M3: 零拷贝序列化 | M3: 零拷贝序列化 | `value_borrowed.rs` + `columnar.rs` + `BorrowedValue` + `BorrowedRowData` + `ColumnarResultSet` + `apply_result_map_borrowed` | ✅ |
| M4: SIMD 加速 | M4: SIMD 加速 | `simd.rs` + `SimdAvailability` 检测 + `batch_decode_integers` + `batch_compare_*` + 标量降级 + wide crate 集成 | ✅ |
| M5: 集成验证 + 发布 | M5: 集成验证 + 发布 | Feature 全组合编译 + 五方言集成测试 + sz-pay 5139 回归 + v2.4.0 基准不回退 + 性能基准报告 + CHANGELOG | ✅ |

---

## 2. 逐里程碑任务分解

## 里程碑 M1：连接池预热增强

### 目标
在 sz-orm-core 内增强既有预热能力（复用 `Pool::prewarm()` 语义），新增自动预热触发、渐进式分批策略、预热进度可观测；在 sz-orm-sqlx 的 UnifiedPool 扩展多池统一预热。通过 feature gate `auto-prewarm` 隔离自动预热行为，手动预热 API 保持不变（向后兼容）。

### 需求覆盖
REQ-PW-001（自动预热）, REQ-PW-002（多池统一预热）, REQ-PW-003（预热进度可观测）, REQ-PW-004（渐进式预热策略）, REQ-PW-005（禁止预热静默吞错）

### 任务列表

#### 任务 T-M1-001：配置 auto-prewarm feature gate 与 Cargo.toml
- **描述**：在 `packages/sz-orm-core/Cargo.toml` 新增 `auto-prewarm` feature gate（默认关闭，无新增依赖），在 `packages/sz-orm-sqlx/Cargo.toml` 新增 `auto-prewarm = ["sz-orm-core/auto-prewarm"]` 转发 feature；在 `packages/sz-orm-core/src/lib.rs` 新增 `#[cfg(feature = "auto-prewarm")] pub mod prewarm;` 条件导出模块骨架
- **输入**：[packages/sz-orm-core/Cargo.toml:13](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L13)（既有 features）、[packages/sz-orm-core/src/lib.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs)
- **输出**：`packages/sz-orm-core/Cargo.toml`（新增 auto-prewarm feature）+ `packages/sz-orm-sqlx/Cargo.toml`（转发 feature）+ `packages/sz-orm-core/src/prewarm.rs`（模块骨架）+ `lib.rs`（条件导出）
- **验收标准**：
  - `auto-prewarm` feature 默认关闭，默认 feature 不引入额外依赖
  - 命令：`cargo build -p sz-orm-core`（默认 feature）成功，无新依赖引入
  - 命令：`cargo build -p sz-orm-core --features auto-prewarm` 成功
  - 命令：`cargo build -p sz-orm-sqlx --features auto-prewarm` 成功
- **依赖**：无
- **预估**：低（0.5 天）
- **门禁**：`cargo build -p sz-orm-core` + `cargo build -p sz-orm-core --features auto-prewarm` + `cargo build -p sz-orm-sqlx --features auto-prewarm`

#### 任务 T-M1-002：实现 PrewarmConfig + ProgressiveConfig 配置扩展
- **描述**：在 `packages/sz-orm-core/src/prewarm.rs` 新增 `PrewarmConfig`（auto_prewarm: bool 默认 false + progressive: Option<ProgressiveConfig>）与 `ProgressiveConfig`（batch_size: u32 默认 2 + interval: Duration 默认 10ms + total_timeout: Duration 默认 30s）；在 `PoolConfig` 新增 `auto_prewarm` + `progressive_prewarm` 字段（向后兼容，Default 不变）；在 `PoolConfigBuilder` 新增 `auto_prewarm(bool)` + `progressive_prewarm(ProgressiveConfig)` 链式方法
- **输入**：[packages/sz-orm-core/src/pool.rs:545](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L545)（PoolConfig）、[packages/sz-orm-core/src/pool.rs:684](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L684)（PoolConfigBuilder::prewarm）
- **输出**：`packages/sz-orm-core/src/prewarm.rs`（PrewarmConfig + ProgressiveConfig）+ `packages/sz-orm-core/src/pool.rs`（PoolConfig/Builder 扩展字段）
- **验收标准**：
  - `PoolConfig::default()` 不含 auto_prewarm 字段（向后兼容，auto_prewarm 默认 false）
  - `PoolConfigBuilder::default().auto_prewarm(true).progressive_prewarm(ProgressiveConfig{batch_size:5,..}).build()` 配置生效
  - 命令：`cargo test -p sz-orm-core --features auto-prewarm prewarm_config` 通过
  - 命令：`cargo clippy -p sz-orm-core --features auto-prewarm -- -D warnings` 零警告
- **依赖**：T-M1-001
- **预估**：低（0.5 天）
- **门禁**：`cargo test -p sz-orm-core --features auto-prewarm prewarm_config` + `cargo clippy -p sz-orm-core --features auto-prewarm -- -D warnings`

#### 任务 T-M1-003：实现 PrewarmProgress 进度指标 + PoolStatus 扩展
- **描述**：在 `packages/sz-orm-core/src/prewarm.rs` 新增 `PrewarmProgress`（warmed: AtomicU32 + target: u32 + failed: AtomicU32 + elapsed: AtomicU64 + is_completed: AtomicBool）与 `PrewarmProgressSnapshot`（快照结构体）；实现 `snapshot()` 方法读取原子计数器快照；在 `PoolStatus` 新增 `prewarm_progress: Option<PrewarmProgressSnapshot>` 字段（None 向后兼容）；在 `Pool` 新增 `prewarm_progress(&self) -> Option<PrewarmProgressSnapshot>` 查询方法
- **输入**：[packages/sz-orm-core/src/pool.rs:551](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L551)（PoolStatus）、[packages/sz-orm-core/src/pool.rs:578](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L578)（PoolMetrics AtomicU64 模式参考）
- **输出**：`packages/sz-orm-core/src/prewarm.rs`（PrewarmProgress + PrewarmProgressSnapshot）+ `packages/sz-orm-core/src/pool.rs`（PoolStatus 扩展 + Pool::prewarm_progress）
- **验收标准**：
  - `PrewarmProgress::new(target=10)` 初始化 warmed=0/failed=0/is_completed=false
  - `snapshot()` 返回四项指标 + completed 状态
  - `PoolStatus::default().prewarm_progress == None`（向后兼容）
  - 约束：warmed + failed ≤ target（测试验证）
  - 命令：`cargo test -p sz-orm-core --features auto-prewarm prewarm_progress` 通过
- **依赖**：T-M1-001
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features auto-prewarm prewarm_progress`

#### 任务 T-M1-004：实现 Pool::new_async 异步构造（await prewarm）
- **描述**：在 `packages/sz-orm-core/src/pool.rs` 新增 `Pool::new_async(config: PoolConfig, factory: Arc<dyn ConnectionFactory>) -> Result<Pool>` 异步构造方法；当 `config.auto_prewarm == true` 时内部 await prewarm（阻塞至预热完成，池就绪后返回空闲 ≥ min_idle）；当 `auto_prewarm == false` 时等同 `Pool::new`（向后兼容）；预热过程更新 PrewarmProgress，失败不阻断池创建（复用 [pool.rs:866](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L866) 语义）
- **输入**：[packages/sz-orm-core/src/pool.rs:712](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L712)（Pool::new 同步）、[packages/sz-orm-core/src/pool.rs:879](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L879)（Pool::prewarm）
- **输出**：`packages/sz-orm-core/src/pool.rs`（Pool::new_async）
- **验收标准**：
  - `auto_prewarm=true` 时 `new_async` 返回后 `pool.status().idle >= min_idle`（DB 可达时）
  - `auto_prewarm=false` 时 `new_async` 等同 `new`（行为一致）
  - 预热失败（DB 不可达）时 `new_async` 仍返回 Ok（不 Err），日志含失败原因
  - 命令：`cargo test -p sz-orm-core --features auto-prewarm pool_new_async` 通过
- **依赖**：T-M1-002, T-M1-003
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features auto-prewarm pool_new_async`

#### 任务 T-M1-005：实现 Pool::new 内 tokio::spawn 后台预热路径
- **描述**：在 `packages/sz-orm-core/src/pool.rs` 的 `Pool::new` 同步构造方法中，当 `config.auto_prewarm == true` 时通过 `tokio::spawn` 后台触发 prewarm（不阻塞构造，池立即可用冷启动）；后台预热更新 PrewarmProgress；未启用 auto-prewarm 时行为完全不变（向后兼容）
- **输入**：[packages/sz-orm-core/src/pool.rs:712](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L712)（Pool::new）、T-M1-003（PrewarmProgress）
- **输出**：`packages/sz-orm-core/src/pool.rs`（Pool::new 内 `#[cfg(feature = "auto-prewarm")]` 后台 spawn 分支）
- **验收标准**：
  - `auto_prewarm=true` 时 `Pool::new` 立即返回（不阻塞），后台异步预热
  - 后台预热完成后 `pool.prewarm_progress().is_completed == true`
  - `auto_prewarm=false` 时 `Pool::new` 行为与既有完全一致（无 spawn）
  - 命令：`cargo test -p sz-orm-core --features auto-prewarm pool_new_spawn` 通过
  - 命令：`cargo test -p sz-orm-core`（默认 feature，无 auto-prewarm）既有 pool 测试全部通过
- **依赖**：T-M1-003
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features auto-prewarm pool_new_spawn` + `cargo test -p sz-orm-core pool`

#### 任务 T-M1-006：实现渐进式分批预热 progressive_prewarm
- **描述**：在 `packages/sz-orm-core/src/prewarm.rs` 新增 `progressive_prewarm(pool: &Pool, config: &ProgressiveConfig, progress: &PrewarmProgress)` 函数；分批创建连接（每批 batch_size 个，批间隔 interval，总时间不超 total_timeout）；每批并行建连（tokio::join），瞬时建连数 ≤ batch_size；每批后更新 PrewarmProgress（AtomicU32）；超时或达 min_idle 时停止；未达 min_idle 时 tracing::warn! 记录（不 panic 不 Err）
- **输入**：[packages/sz-orm-core/src/pool.rs:879](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L879)（Pool::prewarm 建连语义）、[packages/sz-orm-core/src/pool.rs:1563](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L1563)（Pool::warmup CAS 防并发）、T-M1-002（ProgressiveConfig）、T-M1-003（PrewarmProgress）
- **输出**：`packages/sz-orm-core/src/prewarm.rs`（progressive_prewarm 函数）
- **验收标准**：
  - 配置 `min_idle=50, batch_size=2, interval=10ms` → 连接分批创建，瞬时建连数 ≤ 2
  - 总预热时间 ≤ total_timeout（超时停止，已预热连接保留）
  - 最终 `progress.warmed + progress.failed <= 50`，`progress.is_completed == true`
  - 未达 min_idle 时日志含 `tracing::warn!` 记录（不 panic）
  - 命令：`cargo test -p sz-orm-core --features auto-prewarm progressive_prewarm` 通过
- **依赖**：T-M1-002, T-M1-003
- **预估**：中（1.5 天）
- **门禁**：`cargo test -p sz-orm-core --features auto-prewarm progressive_prewarm`

#### 任务 T-M1-007：实现 UnifiedPool::unified_prewarm + MultiPoolRegistry 多池统一预热
- **描述**：在 `packages/sz-orm-sqlx/src/unified_pool.rs` 新增 `UnifiedPool::unified_prewarm(&self) -> PrewarmSummary`（委托内部 Pool prewarm，返回汇总结果）；新增 `MultiPoolRegistry` 结构体（注册多个 UnifiedPool）+ `unified_prewarm_all(&self) -> PrewarmSummary`（并行 tokio::join_all 各后端 prewarm，部分失败不阻断其它）；在 `packages/sz-orm-core/src/prewarm.rs` 新增 `PrewarmSummary`（results: Vec<BackendPrewarmResult>）+ `BackendPrewarmResult`（backend + warmed + failed + elapsed + errors）+ 聚合查询方法 `total_warmed/total_failed/all_succeeded`
- **输入**：[packages/sz-orm-sqlx/src/unified_pool.rs:48](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/unified_pool.rs#L48)（UnifiedPool）、T-M1-003（PrewarmProgress）
- **输出**：`packages/sz-orm-sqlx/src/unified_pool.rs`（unified_prewarm + MultiPoolRegistry）+ `packages/sz-orm-core/src/prewarm.rs`（PrewarmSummary + BackendPrewarmResult）
- **验收标准**：
  - `MultiPoolRegistry` 注册 MySQL + PG 两池，`unified_prewarm_all` 并行预热两池
  - 某后端预热失败（如 Oracle 不可达）不阻断其它后端（MySQL/PG 正常预热）
  - `PrewarmSummary` 含各后端 `BackendPrewarmResult`（backend + warmed + failed + elapsed + errors）
  - `total_warmed()` + `total_failed()` 聚合正确
  - 命令：`cargo test -p sz-orm-sqlx --features auto-prewarm unified_prewarm` 通过
- **依赖**：T-M1-004, T-M1-006
- **预估**：中（1.5 天）
- **门禁**：`cargo test -p sz-orm-sqlx --features auto-prewarm unified_prewarm`

#### 任务 T-M1-008：telemetry 集成 prewarm 计数器
- **描述**：在 `packages/sz-orm-core/src/telemetry.rs` 的 `TelemetryMetrics` 新增 prewarm 相关原子计数器（prewarm_count: AtomicU64 + prewarm_failed_count: AtomicU64 + prewarm_duration_ns: AtomicU64）；预热过程通过 telemetry 上报进度；既有字段不变（向后兼容）
- **输入**：[packages/sz-orm-core/src/telemetry.rs:83](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/telemetry.rs#L83)（TelemetryMetrics AtomicU64 模式）、T-M1-003（PrewarmProgress）
- **输出**：`packages/sz-orm-core/src/telemetry.rs`（新增 prewarm 计数器）+ `packages/sz-orm-core/src/prewarm.rs`（telemetry 上报集成）
- **验收标准**：
  - `TelemetryMetrics` 新增 3 个 prewarm 原子计数器，既有字段不变
  - 预热完成后 `telemetry.prewarm_count >= min_idle`（成功时）
  - 预热失败时 `telemetry.prewarm_failed_count > 0`
  - 命令：`cargo test -p sz-orm-core --features auto-prewarm telemetry_prewarm` 通过
- **依赖**：T-M1-003
- **预估**：低（0.5 天）
- **门禁**：`cargo test -p sz-orm-core --features auto-prewarm telemetry_prewarm`

#### 任务 T-M1-009：编写 M1 单元测试
- **描述**：在 `packages/sz-orm-core/src/prewarm.rs` `#[cfg(test)]` 模块新增单元测试，覆盖：PrewarmConfig 默认值与向后兼容、PrewarmProgress 原子计数器读写、ProgressiveConfig 边界（batch_size=1/interval=0/total_timeout=0）、progressive_prewarm 分批逻辑（mock factory）、预热失败不阻断（mock factory 返回 Err）、PrewarmSummary 聚合查询
- **输入**：T-M1-001~T-M1-008
- **输出**：`packages/sz-orm-core/src/prewarm.rs`（#[cfg(test)] 单元测试模块）
- **验收标准**：
  - 测试覆盖 PrewarmConfig/ProgressiveConfig/PrewarmProgress/PrewarmSummary 全部公开 API
  - 边界用例：batch_size=1（每批 1 个）、interval=0（无间隔）、total_timeout=0（立即超时）
  - mock factory 模拟 DB 不可达 → 预热失败但不 panic
  - 命令：`cargo test -p sz-orm-core --features auto-prewarm prewarm::tests` 全部通过
  - 命令：`cargo clippy -p sz-orm-core --features auto-prewarm -- -D warnings` 零警告
- **依赖**：T-M1-001~T-M1-008
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features auto-prewarm prewarm::tests` + `cargo clippy -p sz-orm-core --features auto-prewarm -- -D warnings`

#### 任务 T-M1-010：编写 M1 集成测试（真实 DB + 冷启动 P95）
- **描述**：在 `packages/sz-orm-core/tests/prewarm_integration.rs` 新增集成测试（`#[ignore]` 标注真实服务测试），覆盖：自动预热真实 DB（MySQL/PG）后空闲 ≥ min_idle、渐进式预热大池（min_idle=50）分批建连、多池统一预热（MySQL+PG）汇总结果、DB 不可达时预热失败不阻断池创建、冷启动首次查询 P95 ≤ 20ms（对比未预热 ≤ 100ms）
- **输入**：T-M1-001~T-M1-009、本机 MySQL（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）+ PostgreSQL（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）
- **输出**：`packages/sz-orm-core/tests/prewarm_integration.rs`（含 6+ 测试函数）
- **验收标准**：
  - 自动预热 MySQL 池后 `pool.status().idle >= min_idle`
  - 渐进式预热 min_idle=50 → 瞬时建连数 ≤ batch_size，总时间 ≤ total_timeout
  - 多池统一预热 MySQL+PG → `PrewarmSummary.total_warmed() == min_idle_mysql + min_idle_pg`
  - DB 不可达（错误 DSN）→ 池创建成功，日志含预热失败原因，首次查询冷启动建连
  - 冷启动首次查询 P95 ≤ 20ms（自动预热）vs ≤ 100ms（未预热），附计时证据
  - 命令：`cargo test -p sz-orm-core --features auto-prewarm --test prewarm_integration -- --ignored` 全部通过
- **依赖**：T-M1-009
- **预估**：高（2 天）
- **门禁**：`cargo test -p sz-orm-core --features auto-prewarm --test prewarm_integration -- --ignored`

---

## 里程碑 M2：查询计划缓存

### 目标
在 sz-orm-core 内新增查询计划缓存模块 `PlanCache`，缓存 SQL 解析结果（AST）与查询优化结果（UnifiedQueryAnalysis），与既有 L2Cache（数据缓存）职责分离。通过 feature gate `plan-cache` 隔离，启用后相同 SQL 模板第二次起跳过解析/优化（≤1μs），含 schema 变更精确失效、LRU 淘汰、命中率统计。

### 需求覆盖
REQ-PC-001（SQL 解析结果缓存）, REQ-PC-002（查询优化结果缓存）, REQ-PC-003（Schema 变更缓存失效）, REQ-PC-004（缓存容量与淘汰策略）, REQ-PC-005（禁止缓存污染错误计划）

### 任务列表

#### 任务 T-M2-001：配置 plan-cache feature gate + Cargo.toml 依赖
- **描述**：在 `packages/sz-orm-core/Cargo.toml` 新增 `plan-cache = ["dep:sqlparser", "dep:xxhash-rust"]` feature gate；将 sqlparser 从 dev-dependency 提升为 optional dependency（`sqlparser = { version = "0.47", optional = true }`）；新增 `xxhash-rust = { version = "0.8", optional = true, features = ["xxh64"] }`；在 `packages/sz-orm-ai/Cargo.toml` 新增 `plan-cache = ["sz-orm-core/plan-cache"]` 转发 feature；在 `lib.rs` 新增 `#[cfg(feature = "plan-cache")] pub mod plan_cache;` 条件导出
- **输入**：[packages/sz-orm-core/Cargo.toml:13](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L13)（既有 features）、[packages/sz-orm-core/Cargo.toml:52](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L52)（dev-dependencies sqlparser）
- **输出**：`packages/sz-orm-core/Cargo.toml`（plan-cache feature + sqlparser/xxhash-rust optional dep）+ `packages/sz-orm-ai/Cargo.toml`（转发 feature）+ `packages/sz-orm-core/src/plan_cache.rs`（模块骨架）+ `lib.rs`（条件导出）
- **验收标准**：
  - `plan-cache` feature 默认关闭，默认 feature 不引入 sqlparser/xxhash-rust
  - 命令：`cargo build -p sz-orm-core`（默认 feature）成功，无 sqlparser 依赖引入
  - 命令：`cargo build -p sz-orm-core --features plan-cache` 成功
  - 命令：`cargo build -p sz-orm-ai --features plan-cache` 成功
- **依赖**：无
- **预估**：低（0.5 天）
- **门禁**：`cargo build -p sz-orm-core` + `cargo build -p sz-orm-core --features plan-cache` + `cargo build -p sz-orm-ai --features plan-cache`

#### 任务 T-M2-002：实现 SqlNormalizer（SQL 归一化）
- **描述**：在 `packages/sz-orm-core/src/plan_cache.rs` 新增 `SqlNormalizer`；实现 `normalize(sql: &str) -> (String, Vec<String>)` 返回（归一化 SQL，参数占位符列表）；通过 sqlparser 解析 SQL → AST → 规范化（忽略空白/注释/参数顺序，参数值替换为 $1/$2 占位符）→ 重新生成归一化 SQL 文本；相同语义不同写法（如 `SELECT * FROM t WHERE id=?` vs `select * from t where id = ?`）归一化后相同
- **输入**：sqlparser 0.47 API（`sqlparser::parser::Parser::parse_sql` + `sqlparser::ast::Statement`）
- **输出**：`packages/sz-orm-core/src/plan_cache.rs`（SqlNormalizer + normalize 方法）
- **验收标准**：
  - `normalize("SELECT * FROM t WHERE id = ?")` 与 `normalize("select * from t where id = ?")` 返回相同归一化 SQL
  - 参数值不进入归一化 SQL（替换为 $1/$2 占位符）
  - 敏感信息（密码/token）不进入归一化 SQL
  - 命令：`cargo test -p sz-orm-core --features plan-cache sql_normalizer` 通过
- **依赖**：T-M2-001
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features plan-cache sql_normalizer`

#### 任务 T-M2-003：实现 PlanCacheKey + PlanCacheKeyHasher（xxHash 64bit）
- **描述**：在 `packages/sz-orm-core/src/plan_cache.rs` 新增 `PlanCacheKey`（hash: u64 + sql_normalized: String）与 `PlanCacheKeyHasher`；使用 xxhash-rust xxh64 对归一化 SQL 哈希生成 64bit 键；提供 `from_sql(sql: &str) -> PlanCacheKey`（归一化 + 哈希）；可选 SQL 文本二次校验（防哈希碰撞）
- **输入**：T-M2-002（SqlNormalizer）、xxhash-rust xxh64 API
- **输出**：`packages/sz-orm-core/src/plan_cache.rs`（PlanCacheKey + PlanCacheKeyHasher）
- **验收标准**：
  - 相同 SQL 模板 → 相同 hash；不同参数相同模板 → 相同 hash
  - 不同语义 SQL → 不同 hash（无碰撞，差分测试验证）
  - 缓存键不含参数值/敏感信息
  - 命令：`cargo test -p sz-orm-core --features plan-cache plan_cache_key` 通过
- **依赖**：T-M2-002
- **预估**：低（0.5 天）
- **门禁**：`cargo test -p sz-orm-core --features plan-cache plan_cache_key`

#### 任务 T-M2-004：实现 PlanCacheEntry + PlanCacheStats
- **描述**：在 `packages/sz-orm-core/src/plan_cache.rs` 新增 `PlanCacheEntry`（ast: Option<Statement> + analysis: Option<UnifiedQueryAnalysis> + created_at: Instant + tables: Vec<String> + ttl: Option<Duration>）与 `PlanCacheStats`（parse_hits/parse_misses/optimize_hits/optimize_misses/evictions: AtomicU64 + size: usize）；实现 `parse_hit_rate()` / `optimize_hit_rate()` 方法
- **输入**：[packages/sz-orm-core/src/l2_cache.rs:214](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/l2_cache.rs#L214)（L2CacheStats 思路参考）、sqlparser::ast::Statement
- **输出**：`packages/sz-orm-core/src/plan_cache.rs`（PlanCacheEntry + PlanCacheStats）
- **验收标准**：
  - `PlanCacheEntry` 含 ast/analysis/created_at/tables/ttl 五字段
  - `PlanCacheStats` 原子计数器无锁，`parse_hit_rate()` 返回 0.0~1.0
  - 命令：`cargo test -p sz-orm-core --features plan-cache plan_cache_entry_stats` 通过
- **依赖**：T-M2-001
- **预估**：低（0.5 天）
- **门禁**：`cargo test -p sz-orm-core --features plan-cache plan_cache_entry_stats`

#### 任务 T-M2-005：实现 PlanCache 主体（双缓存 + LruOrder + table_index）
- **描述**：在 `packages/sz-orm-core/src/plan_cache.rs` 新增 `PlanCache` 主体结构体：`parse_cache: RwLock<HashMap<u64, PlanCacheEntry>>` + `optimize_cache: RwLock<HashMap<u64, PlanCacheEntry>>` + `access_order: RwLock<LruOrder>`（复用 [l2_cache.rs:359](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/l2_cache.rs#L359)）+ `table_index: RwLock<HashMap<String, Vec<u64>>>`（表级失效索引）+ `stats: RwLock<PlanCacheStats>` + `max_size: usize` + `default_ttl: Option<Duration>`；使用 parking_lot::RwLock 防毒化；锁顺序约定：parse_cache → access_order → table_index → stats（避免死锁）
- **输入**：[packages/sz-orm-core/src/l2_cache.rs:359](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/l2_cache.rs#L359)（LruOrder arena 双向链表）、[packages/sz-orm-core/src/l2_cache.rs:521](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/l2_cache.rs#L521)（table_index 思路）、T-M2-003（PlanCacheKey）、T-M2-004（PlanCacheEntry/Stats）
- **输出**：`packages/sz-orm-core/src/plan_cache.rs`（PlanCache 主体）
- **验收标准**：
  - `PlanCache::new(max_size=1024, default_ttl=None)` 初始化空缓存
  - 复用既有 `LruOrder`（不修改 LruOrder，直接使用 pub API）
  - 锁顺序约定文档化（注释说明）
  - 命令：`cargo test -p sz-orm-core --features plan-cache plan_cache_new` 通过
- **依赖**：T-M2-003, T-M2-004
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features plan-cache plan_cache_new`

#### 任务 T-M2-006：实现 get_or_parse / get_or_optimize 命中跳过逻辑
- **描述**：在 `PlanCache` 实现 `get_or_parse(&self, sql: &str) -> Arc<Statement>`（解析缓存查找：命中返回 AST + stats.parse_hits++ + LRU touch；未命中 stats.parse_misses++ + sqlparser 解析 + 提取依赖表列表 + 存入 parse_cache + table_index + LRU 淘汰）与 `get_or_optimize(&self, sql: &str) -> Option<Arc<UnifiedQueryAnalysis>>`（优化缓存查找，逻辑类似）；命中时耗时 ≤ 1μs
- **输入**：T-M2-005（PlanCache 主体）、T-M2-002（SqlNormalizer）、T-M2-003（PlanCacheKeyHasher）
- **输出**：`packages/sz-orm-core/src/plan_cache.rs`（get_or_parse + get_or_optimize 方法）
- **验收标准**：
  - 相同 SQL 模板第二次 `get_or_parse` 命中缓存，耗时 ≤ 1μs（基准验证）
  - 不同参数相同模板命中同一缓存
  - 命中时 `stats.parse_hits` 递增；未命中时 `stats.parse_misses` 递增
  - 命中时 LRU touch（移到 MRU 端）
  - 命令：`cargo test -p sz-orm-core --features plan-cache get_or_parse` 通过
- **依赖**：T-M2-005
- **预估**：中（1.5 天）
- **门禁**：`cargo test -p sz-orm-core --features plan-cache get_or_parse`

#### 任务 T-M2-007：实现 invalidate_table 表级精确失效 + invalidate_all
- **描述**：在 `PlanCache` 实现 `invalidate_table(&self, table: &str) -> usize`（table_index 查找受影响键 → 遍历移除 parse_cache + optimize_cache + access_order 条目 → stats.evictions++ → table_index.remove(table) → 返回失效条目数）与 `invalidate_all(&self)`（全量清空）；精确失效：仅失效受影响表，其它表缓存不受影响
- **输入**：T-M2-005（PlanCache 主体 + table_index）、[packages/sz-orm-core/src/l2_cache.rs:521](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/l2_cache.rs#L521)（invalidate_table 思路参考）
- **输出**：`packages/sz-orm-core/src/plan_cache.rs`（invalidate_table + invalidate_all 方法）
- **验收标准**：
  - 缓存已有 `table_a` + `table_b` 查询计划 → `invalidate_table("table_a")` 仅失效 table_a 条目，table_b 不受影响
  - 返回失效条目数正确
  - `invalidate_all()` 清空所有缓存，`stats.size == 0`
  - 命令：`cargo test -p sz-orm-core --features plan-cache invalidate_table` 通过
- **依赖**：T-M2-005
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features plan-cache invalidate_table`

#### 任务 T-M2-008：实现 LRU 淘汰 + 容量上限 + 命中率统计
- **描述**：在 `PlanCache` 的 `get_or_parse` / `get_or_optimize` 存入新条目时，若 `stats.size >= max_size` 则按 LRU 淘汰最久未用条目（复用 `LruOrder::lru_key()`）；淘汰时同步清理 parse_cache + optimize_cache + table_index；`stats()` 方法返回 PlanCacheStats 快照（命中/未命中/命中率）
- **输入**：T-M2-005（PlanCache 主体）、[packages/sz-orm-core/src/l2_cache.rs:359](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/l2_cache.rs#L359)（LruOrder::lru_key/remove O(1)）
- **输出**：`packages/sz-orm-core/src/plan_cache.rs`（LRU 淘汰逻辑 + stats 方法）
- **验收标准**：
  - `max_size=10`，存入 11 条计划 → 最久未用条目被淘汰，`stats.size == 10`
  - `stats.evictions` 递增正确
  - `stats().parse_hit_rate()` 返回 0.0~1.0
  - 淘汰不影响正确性（被淘汰的 SQL 下次查询重新解析）
  - 命令：`cargo test -p sz-orm-core --features plan-cache lru_eviction` 通过
- **依赖**：T-M2-006
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features plan-cache lru_eviction`

#### 任务 T-M2-009：扩展 UnifiedQueryOptimizer::with_plan_cache
- **描述**：在 `packages/sz-orm-ai/src/query_plan_optimizer.rs` 的 `UnifiedQueryOptimizer` 新增 `with_plan_cache(self, cache: Arc<PlanCache>) -> Self` 方法（注入计划缓存）；修改 `optimize()` 方法内部，在执行规则分析 + LLM 调用前先查 `plan_cache.get_or_optimize(sql)`，命中跳过优化，未命中执行优化后存入缓存；未调用 `with_plan_cache` 时行为完全不变（向后兼容）
- **输入**：[packages/sz-orm-ai/src/query_plan_optimizer.rs:515](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-ai/src/query_plan_optimizer.rs#L515)（UnifiedQueryOptimizer）、T-M2-006（get_or_optimize）
- **输出**：`packages/sz-orm-ai/src/query_plan_optimizer.rs`（with_plan_cache + optimize 内部查缓存）
- **验收标准**：
  - 未调用 `with_plan_cache` 时 `optimize()` 行为与既有完全一致
  - 调用 `with_plan_cache(cache)` 后，相同 SQL 第二次 `optimize` 命中缓存跳过优化
  - 优化建议与首次一致（缓存命中返回相同 UnifiedQueryAnalysis）
  - 命令：`cargo test -p sz-orm-ai --features plan-cache with_plan_cache` 通过
  - 命令：`cargo test -p sz-orm-ai`（默认 feature，无 plan-cache）既有优化器测试全部通过
- **依赖**：T-M2-006
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-ai --features plan-cache with_plan_cache` + `cargo test -p sz-orm-ai`

#### 任务 T-M2-010：QueryBuilder / migration.rs 集成 plan-cache
- **描述**：在 `packages/sz-orm-core/src/query.rs` 的查询执行路径前 `#[cfg(feature = "plan-cache")]` 查 `PlanCache::get_or_parse`（命中跳过解析）；在 `packages/sz-orm-core/src/migration.rs` 的 DDL 执行后 `#[cfg(feature = "plan-cache")]` 调用 `plan_cache.invalidate_table(table)` 触发失效；未启用 plan-cache 时既有行为不变
- **输入**：[packages/sz-orm-core/src/query.rs:36](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L36)（QueryBuilder）、[packages/sz-orm-core/src/migration.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/migration.rs)（迁移工具）、T-M2-006（get_or_parse）、T-M2-007（invalidate_table）
- **输出**：`packages/sz-orm-core/src/query.rs`（查询前查缓存）+ `packages/sz-orm-core/src/migration.rs`（DDL 后触发失效）
- **验收标准**：
  - 启用 plan-cache 后相同 SQL 模板第二次查询跳过解析
  - DDL（如 ALTER TABLE）后受影响表计划缓存自动失效
  - 未启用 plan-cache 时查询与迁移行为完全不变
  - 命令：`cargo test -p sz-orm-core --features plan-cache query_plan_cache_integration` 通过
  - 命令：`cargo test -p sz-orm-core`（默认 feature）既有 query/migration 测试全部通过
- **依赖**：T-M2-006, T-M2-007
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features plan-cache query_plan_cache_integration` + `cargo test -p sz-orm-core`

#### 任务 T-M2-011：编写 M2 单元测试
- **描述**：在 `packages/sz-orm-core/src/plan_cache.rs` `#[cfg(test)]` 模块新增单元测试，覆盖：SqlNormalizer 归一化（相同语义不同写法 → 相同）、PlanCacheKey 无碰撞、PlanCache get_or_parse 命中/未命中、LRU 淘汰（max_size 边界）、invalidate_table 精确失效、invalidate_all 全量清空、PlanCacheStats 命中率统计、TTL 过期
- **输入**：T-M2-001~T-M2-010
- **输出**：`packages/sz-orm-core/src/plan_cache.rs`（#[cfg(test)] 单元测试模块）
- **验收标准**：
  - 测试覆盖 PlanCache 全部公开 API
  - 边界用例：max_size=1（仅 1 条）、TTL=0（立即过期）、空 SQL
  - 命令：`cargo test -p sz-orm-core --features plan-cache plan_cache::tests` 全部通过
  - 命令：`cargo clippy -p sz-orm-core --features plan-cache -- -D warnings` 零警告
- **依赖**：T-M2-001~T-M2-010
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features plan-cache plan_cache::tests` + `cargo clippy -p sz-orm-core --features plan-cache -- -D warnings`

#### 任务 T-M2-012：编写 M2 差分测试（缓存 vs 未缓存结果一致 + 无碰撞 + 并发竞态）
- **描述**：在 `packages/sz-orm-core/tests/plan_cache_differential.rs` 新增差分测试，覆盖：缓存命中 vs 未缓存执行相同查询结果完全一致、缓存键无碰撞（proptest 随机 SQL 对，不同语义 → 不同键）、并发竞态（多线程并发缓存同一 SQL，last-write-wins，结果正确）、schema 变更后缓存失效再查询结果与未缓存一致
- **输入**：T-M2-001~T-M2-011、proptest（既有 dev-dep）
- **输出**：`packages/sz-orm-core/tests/plan_cache_differential.rs`（含 5+ 差分测试函数）
- **验收标准**：
  - 缓存命中 vs 未缓存：查询结果完全一致（行数/字段/值）
  - proptest 随机 1000 对 SQL：不同语义 → 不同 hash（无碰撞）
  - 10 线程并发缓存同一 SQL：最终保留一个条目，结果正确
  - schema 变更后失效再查询：结果与未缓存一致
  - 命令：`cargo test -p sz-orm-core --features plan-cache --test plan_cache_differential` 全部通过
- **依赖**：T-M2-011
- **预估**：高（1.5 天）
- **门禁**：`cargo test -p sz-orm-core --features plan-cache --test plan_cache_differential`

---

## 里程碑 M3：零拷贝序列化

### 目标
在 sz-orm-core 内扩展借用型值类型 `BorrowedValue<'a>` 与借用型行数据 `BorrowedRowData<'a>`，以及列式结果集 `ColumnarResultSet`，通过 feature gate `zero-copy` 隔离。启用后查询结果反序列化路径减少内存拷贝（分配减少 ≥50%，耗时减少 ≥30%），不启用时现有 owned `Value`/`RowData` API 完全不变。

### 需求覆盖
REQ-ZC-001（借用型值类型）, REQ-ZC-002（RowData 列名借用）, REQ-ZC-003（零拷贝反序列化路径）, REQ-ZC-004（列式结果集布局）, REQ-ZC-005（禁止隐式深拷贝）

### 任务列表

#### 任务 T-M3-001：配置 zero-copy feature gate + Cargo.toml
- **描述**：在 `packages/sz-orm-core/Cargo.toml` 新增 `zero-copy = []` feature gate（默认关闭，无新增依赖，仅 std::borrow::Cow）；在 `lib.rs` 新增 `#[cfg(feature = "zero-copy")] pub mod value_borrowed;` + `#[cfg(feature = "zero-copy")] pub mod columnar;` 条件导出
- **输入**：[packages/sz-orm-core/Cargo.toml:13](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L13)（既有 features）、[packages/sz-orm-core/src/lib.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/lib.rs)
- **输出**：`packages/sz-orm-core/Cargo.toml`（zero-copy feature）+ `packages/sz-orm-core/src/value_borrowed.rs`（模块骨架）+ `packages/sz-orm-core/src/columnar.rs`（模块骨架）+ `lib.rs`（条件导出）
- **验收标准**：
  - `zero-copy` feature 默认关闭，无新增依赖
  - 命令：`cargo build -p sz-orm-core`（默认 feature）成功，无新依赖
  - 命令：`cargo build -p sz-orm-core --features zero-copy` 成功
- **依赖**：无
- **预估**：低（0.5 天）
- **门禁**：`cargo build -p sz-orm-core` + `cargo build -p sz-orm-core --features zero-copy`

#### 任务 T-M3-002：实现 BorrowedValue<'a> 借用型值枚举
- **描述**：在 `packages/sz-orm-core/src/value_borrowed.rs` 新增 `BorrowedValue<'a>` 枚举，与 `Value` 变体一一对应：标量变体（Null/Bool/I8..I64/U8..U64/F32/F64）与 Value 相同（Copy 类型）；字符串类变体（Decimal/String/Uuid/Date/DateTime/Time/Json）使用 `Cow<'a, str>` 替代 String；Bytes 变体使用 `Cow<'a, [u8]>` 替代 Vec<u8>；Array 变体 `Vec<BorrowedValue<'a>>`；Object 变体 `HashMap<String, BorrowedValue<'a>>`；生命周期 `'a` 绑定原始行缓冲区；实现 `Debug/Clone/PartialEq`
- **输入**：[packages/sz-orm-core/src/value.rs:13](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/value.rs#L13)（Value 枚举 20 变体）、std::borrow::Cow
- **输出**：`packages/sz-orm-core/src/value_borrowed.rs`（BorrowedValue 枚举）
- **验收标准**：
  - `BorrowedValue` 变体与 `Value` 一一对应（20 变体）
  - 字符串类变体使用 `Cow<'a, str>`，Cow::Borrowed 引用原始缓冲区零额外分配
  - `#[non_exhaustive]` 允许未来扩展
  - 命令：`cargo build -p sz-orm-core --features zero-copy` 成功
  - 命令：`cargo clippy -p sz-orm-core --features zero-copy -- -D warnings` 零警告
- **依赖**：T-M3-001
- **预估**：中（1 天）
- **门禁**：`cargo build -p sz-orm-core --features zero-copy` + `cargo clippy -p sz-orm-core --features zero-copy -- -D warnings`

#### 任务 T-M3-003：实现 BorrowedValue <-> Value 转换 + 等价方法
- **描述**：在 `BorrowedValue` 实现 `to_owned(&self) -> Value`（Cow::Borrowed 时 to_owned，Cow::Owned 时 clone）与 `from(value: &Value) -> BorrowedValue<'_>`（引用 Value 内部数据构造 Cow::Borrowed）；实现 `as_str(&self) -> Option<&str>` / `as_bytes(&self) -> Option<&[u8]>` / `eq(&self, other: &Value) -> bool` 等与 Value 等价方法；行为等价：BorrowedValue 与 Value 在比较/序列化/类型转换场景结果一致
- **输入**：T-M3-002（BorrowedValue）、[packages/sz-orm-core/src/value.rs:525](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/value.rs#L525)（Value::to_param Cow 模式参考）
- **输出**：`packages/sz-orm-core/src/value_borrowed.rs`（to_owned + from + as_str/as_bytes/eq 方法）
- **验收标准**：
  - `BorrowedValue::from(&value).to_owned() == value`（往返一致）
  - `BorrowedValue::from(&Value::String("hello".into())).as_str() == Some("hello")`（零拷贝引用）
  - `BorrowedValue::from(&v).eq(&v2) == (v == v2)`（比较等价）
  - 命令：`cargo test -p sz-orm-core --features zero-copy borrowed_value_eq` 通过
- **依赖**：T-M3-002
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features zero-copy borrowed_value_eq`

#### 任务 T-M3-004：实现 BorrowedRowData<'a> 借用型行数据
- **描述**：在 `packages/sz-orm-core/src/value_borrowed.rs` 新增 `BorrowedRowData<'a>` 结构体：`columns: HashMap<&'a str, BorrowedValue<'a>>`（键引用 schema 元数据列名，值借用行缓冲区）；`'a` 绑定 schema 列名元数据 + 行缓冲区；实现 `new(schema: &'a [&'a str]) -> Self` / `get(&self, col: &str) -> Option<&BorrowedValue<'a>>` / `set(&'a str, BorrowedValue)` / `iter() -> impl Iterator<Item = (&'a str, &BorrowedValue<'a>)>` / `to_owned(&self) -> RowData`
- **输入**：[packages/sz-orm-core/src/result_map.rs:397](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/result_map.rs#L397)（RowData HashMap<String, Value>）、T-M3-003（BorrowedValue）
- **输出**：`packages/sz-orm-core/src/value_borrowed.rs`（BorrowedRowData）
- **验收标准**：
  - `BorrowedRowData::new(&["id","name"])` 初始化空行，列名引用 schema
  - `get("id")` 返回 `Option<&BorrowedValue>`（零拷贝引用）
  - `to_owned()` 转换为 RowData，列名 to_string，值 to_owned
  - 命令：`cargo test -p sz-orm-core --features zero-copy borrowed_row_data` 通过
- **依赖**：T-M3-003
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features zero-copy borrowed_row_data`

#### 任务 T-M3-005：实现 ColumnarResultSet + ColumnarSchema 列式结果集
- **描述**：在 `packages/sz-orm-core/src/columnar.rs` 新增 `ColumnarSchema`（names: Vec<String> + types: Vec<DbType>）与 `ColumnarResultSet`（columns: Vec<Vec<Value>> 按列连续存储 + schema: ColumnarSchema + row_count: usize）；实现 `column(&self, name: &str) -> Option<&Vec<Value>>`（按列名取列，批量遍历缓存友好）+ `row_count(&self) -> usize`
- **输入**：[packages/sz-orm-core/src/result_map.rs:397](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/result_map.rs#L397)（RowData）、Value
- **输出**：`packages/sz-orm-core/src/columnar.rs`（ColumnarSchema + ColumnarResultSet）
- **验收标准**：
  - `ColumnarResultSet` 每列连续存储（Vec<Vec<Value>> 外层每元素为一列）
  - `column("id")` 返回该列所有行的值（缓存友好批量遍历）
  - 每列长度 == row_count
  - 命令：`cargo test -p sz-orm-core --features zero-copy columnar_result_set` 通过
- **依赖**：T-M3-001
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features zero-copy columnar_result_set`

#### 任务 T-M3-006：实现 RowData <-> ColumnarResultSet 互转
- **描述**：在 `ColumnarResultSet` 实现 `from_row_data(rows: &[RowData], schema: ColumnarSchema) -> Self`（行式转列式：按列拆分到独立 Vec）与 `to_row_data(&self) -> Vec<RowData>`（列式转行式：按行组装 HashMap）；行列互转数据一致
- **输入**：T-M3-005（ColumnarResultSet）、RowData
- **输出**：`packages/sz-orm-core/src/columnar.rs`（from_row_data + to_row_data）
- **验收标准**：
  - `ColumnarResultSet::from_row_data(rows, schema).to_row_data() == rows`（往返一致）
  - 列顺序与 schema 一致
  - 命令：`cargo test -p sz-orm-core --features zero-copy columnar_convert` 通过
- **依赖**：T-M3-005
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features zero-copy columnar_convert`

#### 任务 T-M3-007：实现 apply_result_map_borrowed 零拷贝反序列化路径
- **描述**：在 `packages/sz-orm-core/src/result_map.rs` 新增 `apply_result_map_borrowed(registry, map_id, &BorrowedRowData) -> Result<HashMap<String, BorrowedValue>>` 与 `apply_result_map_many_borrowed`；复用既有 `apply_result_map` 逻辑但消除 `v.clone()`（[result_map.rs:545,550](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/result_map.rs#L545)）与 `attrs.clone()`（[result_map.rs:685](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/result_map.rs#L685)），改为借用或 move；association prefix 模式列名 strip_prefix 引用而非 clone；既有 `apply_result_map` 保持不变
- **输入**：[packages/sz-orm-core/src/result_map.rs:514](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/result_map.rs#L514)（apply_result_map）、[packages/sz-orm-core/src/result_map.rs:641](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/result_map.rs#L641)（apply_result_map_many）、T-M3-004（BorrowedRowData）
- **输出**：`packages/sz-orm-core/src/result_map.rs`（apply_result_map_borrowed + apply_result_map_many_borrowed，`#[cfg(feature = "zero-copy")]`）
- **验收标准**：
  - `apply_result_map_borrowed` 结果与 `apply_result_map` 行为完全一致（数据等价）
  - 借用型路径消除 `v.clone()`（代码审查无 clone 调用）
  - 既有 `apply_result_map` 不修改（向后兼容）
  - 命令：`cargo test -p sz-orm-core --features zero-copy apply_result_map_borrowed` 通过
  - 命令：`cargo test -p sz-orm-core`（默认 feature）既有 result_map 测试全部通过
- **依赖**：T-M3-004
- **预估**：高（2 天）
- **门禁**：`cargo test -p sz-orm-core --features zero-copy apply_result_map_borrowed` + `cargo test -p sz-orm-core result_map`

#### 任务 T-M3-008：编写 M3 等价性测试（BorrowedValue vs Value + 混用场景）
- **描述**：在 `packages/sz-orm-core/src/value_borrowed.rs` `#[cfg(test)]` 模块新增等价性测试，覆盖：BorrowedValue 与 Value 在比较/序列化/类型转换场景行为等价、to_owned 往返一致、BorrowedRowData 与 RowData 行为等价、借用型与 owned 混用类型不符返回明确错误（非 panic）、生命周期安全（编译期静态检查，悬垂引用编译失败）
- **输入**：T-M3-001~T-M3-007
- **输出**：`packages/sz-orm-core/src/value_borrowed.rs`（#[cfg(test)] 等价性测试）+ `packages/sz-orm-core/src/columnar.rs`（#[cfg(test)] 测试）
- **验收标准**：
  - BorrowedValue 与 Value 在所有公开操作下行为等价（20 变体全覆盖）
  - 混用类型不符返回 `TypeMismatch` 错误（非 panic）
  - 生命周期安全：悬垂引用代码编译失败（trybuild 测试验证编译错误）
  - 命令：`cargo test -p sz-orm-core --features zero-copy borrowed_equivalence` 全部通过
  - 命令：`cargo clippy -p sz-orm-core --features zero-copy -- -D warnings` 零警告
- **依赖**：T-M3-001~T-M3-007
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features zero-copy borrowed_equivalence` + `cargo clippy -p sz-orm-core --features zero-copy -- -D warnings`

#### 任务 T-M3-009：编写 M3 分配追踪基准（分配减少 ≥50%，耗时减少 ≥30%）
- **描述**：在 `packages/sz-orm-core/benches/zero_copy_bench.rs` 新增 criterion 基准测试，对比启用 vs 未启用 zero-copy 的 10000 行结果集反序列化：统计 String 分配次数（分配计数器）+ 耗时；附基准报告证明分配减少 ≥50%、耗时减少 ≥30%；识别可避免的隐式深拷贝（.clone()/to_string()）并消除
- **输入**：T-M3-007（apply_result_map_borrowed）、criterion（既有 dev-dep）、10000 行测试数据
- **输出**：`packages/sz-orm-core/benches/zero_copy_bench.rs`（criterion 基准 + 分配计数器）
- **验收标准**：
  - 10000 行反序列化：启用 zero-copy 内存分配次数较未启用减少 ≥ 50%（附分配次数对比证据）
  - 10000 行反序列化：启用 zero-copy 耗时较未启用减少 ≥ 30%（附 criterion 报告）
  - 可避免的隐式深拷贝为零（分配追踪报告证明）
  - 命令：`cargo bench --features zero-copy --bench zero_copy_bench` 输出报告，收益达标
- **依赖**：T-M3-008
- **预估**：高（2 天）
- **门禁**：`cargo bench --features zero-copy --bench zero_copy_bench`

#### 任务 T-M3-010：编写 M3 列式结果集测试（行列互转一致性 + 缓存局部性）
- **描述**：在 `packages/sz-orm-core/src/columnar.rs` `#[cfg(test)]` 模块新增列式结果集测试，覆盖：from_row_data/to_row_data 往返一致、每列长度等于 row_count、列顺序与 schema 一致、批量列遍历缓存局部性（criterion 基准对比行式 vs 列式批量聚合耗时）
- **输入**：T-M3-005, T-M3-006
- **输出**：`packages/sz-orm-core/src/columnar.rs`（#[cfg(test)] 列式测试）
- **验收标准**：
  - 行列互转往返一致（from_row_data → to_row_data == 原始）
  - 每列长度 == row_count，列顺序与 schema 一致
  - 列式批量聚合（如 SUM/MIN/MAX）耗时较行式减少（缓存局部性，附基准证据）
  - 命令：`cargo test -p sz-orm-core --features zero-copy columnar::tests` 全部通过
- **依赖**：T-M3-006
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features zero-copy columnar::tests`

---

## 里程碑 M4：SIMD 加速

### 目标
在 sz-orm-core 内扩展 SIMD 加速模块，通过 `wide` crate（stable 安全抽象）加速批量行解码与列比较，含运行时自动检测与降级。启用后批量操作（≥1024 元素）吞吐量 ≥2x，列比较耗时减少 ≥40%，不启用时全量标量路径不变。

### 需求覆盖
REQ-SIMD-001（批量行解码 SIMD 加速）, REQ-SIMD-002（列比较批量过滤 SIMD）, REQ-SIMD-003（SIMD 自动检测与降级）, REQ-SIMD-004（SIMD 安全抽象）, REQ-SIMD-005（禁止小数据量 SIMD）

### 任务列表

#### 任务 T-M4-001：配置 simd feature gate + Cargo.toml（wide crate）
- **描述**：在 `packages/sz-orm-core/Cargo.toml` 新增 `simd = ["dep:wide"]` feature gate + `wide = { version = "0.7", optional = true }` optional dependency；在 `lib.rs` 新增 `#[cfg(feature = "simd")] pub mod simd;` 条件导出；可选新增 `simd-nightly = ["simd"]` 用于 std::simd 便携 SIMD（nightly feature 隔离，不强制全项目 nightly）
- **输入**：[packages/sz-orm-core/Cargo.toml:13](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml#L13)（既有 features）
- **输出**：`packages/sz-orm-core/Cargo.toml`（simd feature + wide optional dep）+ `packages/sz-orm-core/src/simd.rs`（模块骨架）+ `lib.rs`（条件导出）
- **验收标准**：
  - `simd` feature 默认关闭，默认 feature 不引入 wide crate
  - 命令：`cargo build -p sz-orm-core`（默认 feature）成功，无 wide 依赖
  - 命令：`cargo build -p sz-orm-core --features simd` 成功
- **依赖**：无
- **预估**：低（0.5 天）
- **门禁**：`cargo build -p sz-orm-core` + `cargo build -p sz-orm-core --features simd`

#### 任务 T-M4-002：实现 SimdAvailability 枚举 + detect() 运行时检测
- **描述**：在 `packages/sz-orm-core/src/simd.rs` 新增 `SimdAvailability` 枚举（Avx2/Avx/Sse2/Neon/None）；实现 `detect() -> SimdAvailability`：编译时 `cfg!(target_feature = "avx2")` 等检测 + 运行时 `is_x86_feature_detected!("avx2")` 宏（x86）；WASM 目标 `cfg!(target_arch = "wasm32")` 直接返回 None；使用 `std::sync::OnceLock` 缓存检测结果（首次检测后缓存，避免重复开销）；实现 `is_available(&self) -> bool`
- **输入**：std::arch::is_x86_feature_detected 宏、std::sync::OnceLock
- **输出**：`packages/sz-orm-core/src/simd.rs`（SimdAvailability + detect + is_available）
- **验收标准**：
  - `SimdAvailability::detect()` 首次检测后缓存（OnceLock，第二次调用无重复检测）
  - x86-64 目标检测 Avx2/Avx/Sse2；aarch64 目标检测 Neon；WASM 目标返回 None
  - `is_available()` 对 Avx2/Avx/Sse2/Neon 返回 true，对 None 返回 false
  - 命令：`cargo test -p sz-orm-core --features simd simd_availability` 通过
- **依赖**：T-M4-001
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features simd simd_availability`

#### 任务 T-M4-003：实现 batch_decode_integers SIMD 批量整数解码
- **描述**：在 `packages/sz-orm-core/src/simd.rs` 实现 `batch_decode_integers(buf: &[u8], count: usize, avail: SimdAvailability) -> Vec<i64>`；count ≥ 1024 且 avail ≠ None 时走 SIMD 路径（wide::i64x4 向量批量解析，每批 4 元素，尾部 count % 4 标量处理）；count < 1024 或 avail == None 时走标量降级；标量降级路径 `scalar_decode_integers` API 签名与 SIMD 路径一致
- **输入**：T-M4-002（SimdAvailability）、wide::i64x4 向量类型
- **输出**：`packages/sz-orm-core/src/simd.rs`（batch_decode_integers + scalar_decode_integers）
- **验收标准**：
  - count ≥ 1024 且 avail ≠ None → SIMD 路径，吞吐量 ≥ 2x（基准验证）
  - count < 1024 → 标量降级（无 SIMD 开销）
  - avail == None → 标量降级
  - SIMD 与标量结果完全一致（差分测试）
  - 尾部 count % 4 元素标量处理正确
  - 命令：`cargo test -p sz-orm-core --features simd batch_decode_integers` 通过
- **依赖**：T-M4-002
- **预估**：中（1.5 天）
- **门禁**：`cargo test -p sz-orm-core --features simd batch_decode_integers`

#### 任务 T-M4-004：实现 batch_compare_eq / batch_compare_in SIMD 列比较
- **描述**：在 `packages/sz-orm-core/src/simd.rs` 实现 `batch_compare_eq(values: &[i64], target: i64, avail: SimdAvailability) -> Vec<bool>`（SIMD i64x4 并行比较 4 元素）与 `batch_compare_in(values: &[i64], set: &[i64], avail: SimdAvailability) -> Vec<bool>`（SIMD 向量比较 + 布尔掩码 IN 过滤）；count ≥ 1024 且 avail ≠ None 走 SIMD，否则标量降级；标量降级路径 API 签名一致
- **输入**：T-M4-002（SimdAvailability）、wide::i64x4 向量比较
- **输出**：`packages/sz-orm-core/src/simd.rs`（batch_compare_eq + batch_compare_in + scalar 降级）
- **验收标准**：
  - 1024+ 元素 IN/批量过滤：SIMD 比较耗时较标量减少 ≥ 40%（基准验证）
  - SIMD 与标量过滤结果完全一致（差分测试）
  - count < 1024 → 标量降级
  - 命令：`cargo test -p sz-orm-core --features simd batch_compare` 通过
- **依赖**：T-M4-002
- **预估**：中（1.5 天）
- **门禁**：`cargo test -p sz-orm-core --features simd batch_compare`

#### 任务 T-M4-005：实现标量降级路径 + < 1024 回退
- **描述**：在 `packages/sz-orm-core/src/simd.rs` 完善 `scalar_decode_integers` / `scalar_compare_eq` / `scalar_compare_in` 标量降级路径；API 签名与 SIMD 路径完全一致（仅性能差异，无行为差异）；< 1024 元素或不适合 SIMD 的类型自动回退标量（无 SIMD 开销）
- **输入**：T-M4-003, T-M4-004
- **输出**：`packages/sz-orm-core/src/simd.rs`（标量降级路径完善）
- **验收标准**：
  - 标量路径与 SIMD 路径 API 签名一致（可透明替换）
  - < 1024 元素 → 回退标量，无 SIMD 向量加载开销
  - 标量路径结果与 SIMD 路径完全一致
  - 命令：`cargo test -p sz-orm-core --features simd scalar_fallback` 通过
- **依赖**：T-M4-003, T-M4-004
- **预估**：低（0.5 天）
- **门禁**：`cargo test -p sz-orm-core --features simd scalar_fallback`

#### 任务 T-M4-006：集成 SIMD 到查询结果处理路径 + WHERE IN 过滤
- **描述**：在 `packages/sz-orm-core/src/result_map.rs` 批量行解码路径 `#[cfg(feature = "simd")]` 调用 `batch_decode_integers`（否则标量）；在 `packages/sz-orm-core/src/query.rs` 的 WHERE col IN (...) 过滤 `#[cfg(feature = "simd")]` 调用 `batch_compare_in`（否则标量）；未启用 simd 时既有标量路径不变
- **输入**：[packages/sz-orm-core/src/result_map.rs:514](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/result_map.rs#L514)（反序列化路径）、[packages/sz-orm-core/src/query.rs:36](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/query.rs#L36)（QueryBuilder IN 条件）、T-M4-003, T-M4-004
- **输出**：`packages/sz-orm-core/src/result_map.rs`（`#[cfg(feature = "simd")]` 批量解码）+ `packages/sz-orm-core/src/query.rs`（`#[cfg(feature = "simd")]` IN 过滤）
- **验收标准**：
  - 启用 simd 后批量行解码走 SIMD 路径
  - 启用 simd 后 WHERE IN 过滤走 SIMD 路径
  - 未启用 simd 时既有标量路径完全不变
  - 命令：`cargo test -p sz-orm-core --features simd simd_integration` 通过
  - 命令：`cargo test -p sz-orm-core`（默认 feature）既有测试全部通过
- **依赖**：T-M4-005
- **预估**：中（1 天）
- **门禁**：`cargo test -p sz-orm-core --features simd simd_integration` + `cargo test -p sz-orm-core`

#### 任务 T-M4-007：编写 M4 差分测试（SIMD vs 标量结果一致 + 边界值）
- **描述**：在 `packages/sz-orm-core/tests/simd_differential.rs` 新增差分测试，覆盖：SIMD vs 标量在所有输入下结果完全一致、边界值（溢出/NaN/空集/极大极小值/全相同/全不同）、proptest 随机输入 1000 轮、count 边界（1023/1024/1025/向量宽度整数倍/非整数倍）
- **输入**：T-M4-001~T-M4-006、proptest（既有 dev-dep）
- **输出**：`packages/sz-orm-core/tests/simd_differential.rs`（含 6+ 差分测试函数）
- **验收标准**：
  - SIMD vs 标量：所有输入结果完全一致（含边界值）
  - 边界值：溢出（i64::MAX/MIN）、空集（count=0）、全相同、全不同
  - count 边界：1023（< 阈值回退标量）、1024（= 阈值 SIMD）、1025（> 阈值 SIMD）、向量宽度整数倍/非整数倍
  - proptest 随机 1000 轮无不一致
  - 命令：`cargo test -p sz-orm-core --features simd --test simd_differential` 全部通过
- **依赖**：T-M4-006
- **预估**：高（1.5 天）
- **门禁**：`cargo test -p sz-orm-core --features simd --test simd_differential`

#### 任务 T-M4-008：编写 M4 基准测试（吞吐量 ≥2x + 比较耗时减少 ≥40%）
- **描述**：在 `packages/sz-orm-core/benches/simd_bench.rs` 新增 criterion 基准测试，对比 SIMD vs 标量：1024+ 行整数列解码吞吐量、1024+ 元素 IN/批量过滤耗时；附基准报告证明吞吐量 ≥ 2x、比较耗时减少 ≥ 40%
- **输入**：T-M4-003, T-M4-004、criterion（既有 dev-dep）
- **输出**：`packages/sz-orm-core/benches/simd_bench.rs`（criterion 基准）
- **验收标准**：
  - 1024+ 行整数列解码：SIMD 吞吐量较标量 ≥ 2x（附 criterion 报告）
  - 1024+ 元素 IN/批量过滤：SIMD 耗时较标量减少 ≥ 40%（附 criterion 报告）
  - 命令：`cargo bench --features simd --bench simd_bench` 输出报告，收益达标
- **依赖**：T-M4-007
- **预估**：中（1 天）
- **门禁**：`cargo bench --features simd --bench simd_bench`

#### 任务 T-M4-009：验证 WASM 目标降级（wasm32 编译 + 自动降级标量）
- **描述**：验证 simd feature 在 wasm32-unknown-unknown 目标自动降级标量；`SimdAvailability::detect()` 在 WASM 目标返回 None；WASM 产物编译成功，SIMD 降级标量无错误；审查 SIMD 实现代码零 unsafe（或每处 // SAFETY: 论证）、零内联汇编
- **输入**：T-M4-002（SimdAvailability WASM 检测）、T-M4-006（集成路径）
- **输出**：WASM 编译验证报告 + unsafe 审查报告
- **验收标准**：
  - 命令：`cargo build --target wasm32-unknown-unknown -p sz-orm-core --features simd` 成功（WASM 目标 simd feature 自动降级）
  - WASM 目标 `SimdAvailability::detect() == None`
  - SIMD 实现代码零 `unsafe`（或每处有 `// SAFETY:` 论证注释）
  - 零内联汇编（grep `asm!` 无结果）
  - 跨 x86-64/aarch64 编译通过
- **依赖**：T-M4-006
- **预估**：中（1 天）
- **门禁**：`cargo build --target wasm32-unknown-unknown -p sz-orm-core --features simd` + grep unsafe/asm 审查

---

## 里程碑 M5：集成验证 + 发布

### 目标
完成 v3.2.0 全部四项性能优化的集成验证与发布：Feature 全组合编译、10 道门禁全量通过、五方言集成测试、sz-pay/sz-rust 下游零回归、v2.4.0 性能基准不回退、v3.2.0 性能基准报告、CHANGELOG + 需求追溯、crates.io 发布。

### 需求覆盖
AC-ALL-1~8（无 Breaking Change + 全测试通过 + clippy 零警告 + feature 隔离 + 下游零回归 + 基准不回退 + 五方言一致 + 20 条需求全满足）

### 任务列表

#### 任务 T-M5-001：Feature 全组合编译验证
- **描述**：验证 4 个新 feature（zero-copy/simd/auto-prewarm/plan-cache）× 既有 feature 组合矩阵全编译通过；纳入门禁 10 Feature 全组合编译；验证 feature 间正交性（任意组合启用/关闭编译通过）
- **输入**：T-M1-001, T-M2-001, T-M3-001, T-M4-001（4 个 feature gate）
- **输出**：Feature 全组合编译报告
- **验收标准**：
  - 命令：`cargo check --workspace --all-targets --all-features` 通过（含 4 新 feature 全组合）
  - 4 新 feature 两两组合编译通过（zero-copy+simd / zero-copy+auto-prewarm / ...）
  - 每个新 feature 单独启用编译通过
- **依赖**：T-M1-010, T-M2-012, T-M3-010, T-M4-009
- **预估**：中（1 天）
- **门禁**：`cargo check --workspace --all-targets --all-features`

#### 任务 T-M5-002：10 道门禁全量通过
- **描述**：运行 AGENTS.md 定义的 10 道门禁全量通过（fmt/check/clippy/test/doc/audit/integration/占位检查/SQL 注入扫描/Feature 全组合/ADR-0001 上游未修改）
- **输入**：T-M1-010, T-M2-012, T-M3-010, T-M4-009
- **输出**：10 道门禁通过报告（每道附命令输出证据）
- **验收标准**：
  - 命令：`cargo fmt --all -- --check` 通过
  - 命令：`cargo check --workspace --all-targets` 通过
  - 命令：`cargo clippy --workspace --all-targets -- -D warnings` 零警告
  - 命令：`cargo test --workspace` 全部通过
  - 命令：`cargo doc --workspace --no-deps --all-features` 通过
  - 命令：`cargo audit` + `cargo deny check` 通过
  - 命令：`cargo test --workspace -- --ignored` 真实服务集成全部通过
  - 命令：`grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` 无结果
  - 命令：`scripts/check-sql-injection.ps1` 通过
  - 命令：`git diff --name-only HEAD` 仅含新增文件，无既有业务代码变更（ADR-0001）
- **依赖**：T-M5-001
- **预估**：中（1 天）
- **门禁**：上述全部命令

#### 任务 T-M5-003：五方言集成测试（行为一致性）
- **描述**：验证四项性能优化在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言下行为一致；复用既有 `tests/smart_eager_integration_*.rs` 等价性测试基础设施；启用 4 新 feature 后五方言集成测试全部通过
- **输入**：[packages/sz-orm-core/tests/](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/)（既有五方言集成测试）、本机 MySQL/PG/SQLite/Oracle/MSSQL
- **输出**：五方言集成测试报告
- **验收标准**：
  - 命令：`cargo test --workspace --features zero-copy,simd,auto-prewarm,plan-cache -- --ignored` 五方言集成测试全部通过
  - 五方言 CRUD/事务/Eager Loading 结果等价（行数/字段/值/嵌套深度一致）
  - 性能优化在 core 层统一，不触碰方言驱动（sz-orm-sqlx/oracle/mssql 既有逻辑不变）
- **依赖**：T-M5-002
- **预估**：中（1 天）
- **门禁**：`cargo test --workspace --features zero-copy,simd,auto-prewarm,plan-cache -- --ignored`

#### 任务 T-M5-004：sz-pay 5139 测试零回归验证
- **描述**：在 sz-pay 项目（`E:\vue\test\sz-pay\server\sz-rust`）验证升级 sz-orm v3.2.0 后 5139 测试零回归；feature gate 默认关闭确保理论上无行为变更，但需实际回归验证
- **输入**：sz-pay 项目（5139 测试基线）、v3.2.0 sz-orm-core/sqlx/config/auth/macros/queue 6 个包
- **输出**：sz-pay 回归测试报告
- **验收标准**：
  - sz-pay 升级 sz-orm v3.2.0 后 `cargo test` 5139 测试全部通过
  - 无 Breaking Change 导致的编译错误
  - feature gate 默认关闭，sz-pay 无需启用新 feature
- **依赖**：T-M5-003
- **预估**：中（1 天）
- **门禁**：sz-pay `cargo test` 5139 测试零回归

#### 任务 T-M5-005：sz-rust 下游零回归验证
- **描述**：在 sz-rust 项目验证升级 sz-orm v3.2.0 后零回归；feature gate 默认关闭确保无行为变更
- **输入**：sz-rust 项目、v3.2.0 sz-orm 包
- **输出**：sz-rust 回归测试报告
- **验收标准**：
  - sz-rust 升级 sz-orm v3.2.0 后全部测试通过
  - 无 Breaking Change 导致的编译错误
- **依赖**：T-M5-004
- **预估**：低（0.5 天）
- **门禁**：sz-rust `cargo test` 零回归

#### 任务 T-M5-006：v2.4.0 性能基准不回退验证
- **描述**：验证 v3.2.0 不得使 v2.4.0 已验收的性能基准回退（decision_latency P99 ≤ 100μs、smart_vs_manual 比 ≤ 1.10、n1_elimination batch 更快、WASM gzip ≤ 1MB）；运行既有 `benches/core_bench.rs` 对比 v2.4.0 基线
- **输入**：[packages/sz-orm-core/benches/core_bench.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/benches/core_bench.rs)（既有基准）、v2.4.0 基线数据
- **输出**：v2.4.0 基准不回退验证报告
- **验收标准**：
  - decision_latency P99 ≤ 100μs（不回退）
  - smart_vs_manual 比 ≤ 1.10（不回退）
  - n1_elimination batch 更快（不回退）
  - WASM gzip ≤ 1MB（不回退）
  - 命令：`cargo bench --bench core_bench` 输出对比 v2.4.0 基线，无回退
- **依赖**：T-M5-002
- **预估**：中（1 天）
- **门禁**：`cargo bench --bench core_bench`

#### 任务 T-M5-007：v3.2.0 性能基准报告（四项优化收益证据）
- **描述**：汇总四项性能优化的基准测试报告：零拷贝（分配减少 ≥50%/耗时减少 ≥30%）、SIMD（吞吐量 ≥2x/比较耗时减少 ≥40%）、预热增强（冷启动 P95 ≤ 20ms）、计划缓存（命中 ≤1μs/命中率 ≥80%）；每项附 criterion 报告 + 命令复现
- **输入**：T-M1-010（预热基准）、T-M2-012（计划缓存基准）、T-M3-009（零拷贝基准）、T-M4-008（SIMD 基准）
- **输出**：`docs/spec/v3.2.0/performance_benchmark_report.md`（性能基准报告）
- **验收标准**：
  - 零拷贝：10000 行反序列化分配减少 ≥ 50%、耗时减少 ≥ 30%（附 criterion 报告）
  - SIMD：1024+ 行解码吞吐量 ≥ 2x、列比较耗时减少 ≥ 40%（附 criterion 报告）
  - 预热增强：冷启动首次查询 P95 ≤ 20ms（对比未预热 ≤ 100ms）
  - 计划缓存：命中耗时 ≤ 1μs、重复 SQL 命中率 ≥ 80%
  - 每项附复现命令（`cargo bench --features ...`）
- **依赖**：T-M3-009, T-M4-008, T-M1-010, T-M2-012
- **预估**：中（1 天）
- **门禁**：性能基准报告审查 + 命令复现

#### 任务 T-M5-008：CHANGELOG + 需求追溯矩阵更新
- **描述**：更新 `CHANGELOG.md` 记录 v3.2.0 四项性能优化（feature gate / 新增 API / 性能收益）；更新需求追溯矩阵（20 条 REQ 编号 → 对应任务 → 覆盖状态 ✅）
- **输入**：T-M1-010, T-M2-012, T-M3-010, T-M4-009
- **输出**：`CHANGELOG.md`（v3.2.0 变更记录）+ 需求追溯矩阵更新
- **验收标准**：
  - CHANGELOG 含 v3.2.0 四项优化（zero-copy/simd/auto-prewarm/plan-cache）
  - 需求追溯矩阵 20 条 REQ 全部标记 ✅（对应任务已完成）
  - 8 总体验收标准 AC-ALL-1~8 全部标记 ✅
- **依赖**：T-M5-007
- **预估**：低（0.5 天）
- **门禁**：CHANGELOG + 需求追溯矩阵审查

#### 任务 T-M5-009：crates.io 发布 + workspace 版本提升
- **描述**：将 workspace.package.version 从 2.3.0 提升到 3.2.0；发布 v3.2.0 到 crates.io（sz-orm-core/sqlx/config/auth/macros/queue 等已发布包）；验证 crates.io 发布成功
- **输入**：T-M5-001~T-M5-008、[Cargo.toml:6](file:///E:/vue/test/鲜视达/rust/sz-orm/Cargo.toml#L6)（workspace.package.version）
- **输出**：crates.io v3.2.0 发布结果
- **验收标准**：
  - workspace.package.version = "3.2.0"
  - crates.io 发布 sz-orm-core v3.2.0 成功
  - 命令：`cargo publish --dry-run -p sz-orm-core` 通过
  - 命令：`cargo publish -p sz-orm-core` 发布成功
- **依赖**：T-M5-008
- **预估**：低（0.5 天）
- **门禁**：`cargo publish --dry-run -p sz-orm-core` + crates.io 发布验证

---

## 3. 里程碑间依赖关系

### 3.1 强依赖（必须前置完成）

| 前置里程碑 | 后续里程碑 | 依赖原因 |
|-----------|-----------|---------|
| M1 | M5 | M5 集成验证需 M1 预热增强完成 |
| M2 | M5 | M5 集成验证需 M2 计划缓存完成 |
| M3 | M5 | M5 集成验证需 M3 零拷贝完成 |
| M4 | M5 | M5 集成验证需 M4 SIMD 完成 |

### 3.2 弱依赖（可并行）

| 里程碑 A | 里程碑 B | 关系 |
|---------|---------|------|
| M1 | M2 | 独立模块，可并行开发 |
| M1 | M3 | 独立模块，可并行开发 |
| M1 | M4 | 独立模块，可并行开发 |
| M2 | M3 | 独立模块，可并行开发 |
| M2 | M4 | 独立模块，可并行开发 |
| M3 | M4 | 独立模块，可并行开发（M4 SIMD 可加速 M3 列式结果集，但非强依赖） |

### 3.3 关键路径

```
M1 (3天) ─┐
M2 (4天) ─┤
M3 (4天) ─┼─→ M5 (2天) = 6 天（最长路径 M2/M3 + M5）
M4 (3天) ─┘
```

- **关键路径**：M2 或 M3（4 天）+ M5（2 天）= 6 天（串行）
- **并行最优**：M1~M4 全并行（4 天）+ M5（2 天）= 6 天
- **总工期**：16 天（串行）/ 6 天（M1~M4 全并行）

---

## 4. 总体进度跟踪表

| 里程碑 | 任务总数 | 已完成 | 进行中 | 待开始 | 完成率 | 状态 |
|--------|---------|--------|--------|--------|--------|------|
| M1 连接池预热增强 | 10 | 0 | 0 | 10 | 0% | ☐ 待开始 |
| M2 查询计划缓存 | 12 | 0 | 0 | 12 | 0% | ☐ 待开始 |
| M3 零拷贝序列化 | 10 | 0 | 0 | 10 | 0% | ☐ 待开始 |
| M4 SIMD 加速 | 9 | 0 | 0 | 9 | 0% | ☐ 待开始 |
| M5 集成验证 + 发布 | 9 | 0 | 0 | 9 | 0% | ☐ 待开始 |
| **合计** | **50** | **0** | **0** | **50** | **0%** | ☐ 待开始 |

> 状态标记：☐ 待开始 / 🔄 进行中 / ✅ 已完成 / ⚠️ 阻塞

---

## 5. 工程化审查清单

### 5.1 各里程碑门禁命令汇总

| 里程碑 | 核心门禁命令 |
|--------|-------------|
| M1 | `cargo test -p sz-orm-core --features auto-prewarm` + `cargo test -p sz-orm-sqlx --features auto-prewarm` + `cargo test -p sz-orm-core --features auto-prewarm --test prewarm_integration -- --ignored` |
| M2 | `cargo test -p sz-orm-core --features plan-cache` + `cargo test -p sz-orm-ai --features plan-cache` + `cargo test -p sz-orm-core --features plan-cache --test plan_cache_differential` |
| M3 | `cargo test -p sz-orm-core --features zero-copy` + `cargo bench --features zero-copy --bench zero_copy_bench` |
| M4 | `cargo test -p sz-orm-core --features simd` + `cargo bench --features simd --bench simd_bench` + `cargo build --target wasm32-unknown-unknown -p sz-orm-core --features simd` |
| M5 | `cargo check --workspace --all-targets --all-features` + `cargo test --workspace -- --ignored` + sz-pay/sz-rust 回归 + `cargo bench --bench core_bench` |

### 5.2 发布前全量门禁（T-M5-002）

| # | 门禁 | 命令 | 期望结果 |
|---|------|------|---------|
| 1 | fmt 格式检查 | `cargo fmt --all -- --check` | 通过 |
| 2 | check 编译检查 | `cargo check --workspace --all-targets` | 通过 |
| 3 | clippy 静态分析 | `cargo clippy --workspace --all-targets -- -D warnings` | 零警告 |
| 4 | test 单元/集成测试 | `cargo test --workspace` | 全部通过 |
| 5 | doc 文档构建 | `cargo doc --workspace --no-deps --all-features` | 通过 |
| 6 | audit 安全审计 | `cargo audit` + `cargo deny check` | 通过 |
| 7 | integration 真实服务集成 | `cargo test --workspace -- --ignored` | 全部通过 |
| 8 | 禁止占位实现检查 | `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` | 无结果 |
| 9 | SQL 注入扫描 | `scripts/check-sql-injection.ps1` | 通过 |
| 10 | Feature 全组合编译 | `cargo check --workspace --all-targets --all-features` | 通过 |
| 11 | 上游仓库未修改检查 | `git diff --name-only HEAD` | 仅含新增文件，无业务代码变更（ADR-0001） |

### 5.3 五维审查（每次 PR 必做）

| 维度 | 检查项 | 验证方式 |
|------|--------|---------|
| 正确性 | 20 条需求全覆盖 + 无占位实现 + 无 Breaking Change + unsafe 零容忍 + 参数化查询铁律 | 需求追溯矩阵 + grep 占位/unsafe + API 兼容性审查 + SQL 注入扫描 |
| 可读性 | 文档结构清晰 + file:line 证据 + 图表辅助 | 文档审查 + `scripts/audit-verify.ps1` |
| 架构 | 模块职责单一 + 复用基础设施（LruOrder/telemetry/Cow）+ Feature 隔离 + 不新增独立包 | 架构审查 + `cargo check --all-features` |
| 安全性 | 参数化查询 + 缓存键无碰撞 + 敏感信息不缓存 + unsafe 零容忍 + 借用型生命周期安全 | `scripts/check-sql-injection.ps1` + grep unsafe + 差分测试 + 编译期生命周期检查 |
| 性能 | 零拷贝分配减少 ≥50% + SIMD 吞吐量 ≥2x + 冷启动 P95 ≤ 20ms + 计划缓存命中率 ≥80% + v2.4.0 基准不回退 | criterion 基准测试 + 集成测试计时 |

---

## 6. 风险标记汇总

| 风险 ID | 关联任务 | 风险描述 | 等级 | 缓解措施 |
|---------|---------|---------|------|---------|
| R-01 | T-M3-002~T-M3-007 | 借用型值生命周期复杂度增加 API 使用难度 | 高 | feature gate 隔离默认 owned；提供清晰文档与示例；编译期静态检查（生命周期 `'a` 编译错误优于运行时） |
| R-02 | T-M4-003~T-M4-009 | SIMD 实现跨平台一致性维护成本 | 高 | 优先 `wide` crate（stable）抽象；差分测试覆盖边界值；WASM 自动降级验证 |
| R-03 | T-M4-001 | `std::simd` 需 nightly 导致全项目 nightly 化力 | 中 | SIMD 独立 feature gate，stable 路径用 `wide` crate，nightly 路径（`simd-nightly`）可选不强制 |
| R-04 | T-M1-004, T-M1-010 | 自动预热在数据库不可达时影响启动体验 | 中 | 预热失败不阻断池创建（复用 [pool.rs:866](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/pool.rs#L866) 语义）；超时可配置；日志明确提示 |
| R-05 | T-M2-003, T-M2-012 | 查询计划缓存键碰撞导致错误计划 | 中 | xxHash 64bit 强哈希 + 可选 SQL 文本二次校验；碰撞时回退解析不返回错误计划；差分测试验证 |
| R-06 | T-M2-007, T-M2-010 | Schema 变更未通过迁移工具导致缓存未失效 | 中 | 提供手动 `invalidate_table` 接口 + 文档提示；迁移工具 DDL 后自动触发失效 |
| R-07 | T-M3-009, T-M4-008 | 零拷贝与 SIMD 优化收益不达预期（基准验证） | 中 | 先行 spike 基准，收益不达预期则降优先级或取消；M3/M4 附基准证据 |
| R-08 | T-M5-003 | 性能优化引入五方言行为差异 | 中 | 五方言集成测试全覆盖；优化在 core 层统一，不触碰方言驱动 |
| R-09 | T-M5-001 | feature 组合矩阵膨胀（4 新 feature × 既有组合） | 低 | 纳入既有门禁 10 Feature 全组合编译；CI 矩阵覆盖 |
| R-10 | T-M5-004, T-M5-005 | 下游 sz-pay/sz-rust 回归 | 中 | v3.2.0 无 Breaking Change（feature gate 隔离），sz-pay 5139 测试基线 + sz-rust 回归验证 |
| R-11 | T-M2-001 | PlanCache 引入 sqlparser 从 dev-dep 提升为 dep 增加编译时间 | 低 | sqlparser 仅在 plan-cache feature 启用时引入（optional dep）；未启用时零影响 |
| R-12 | T-M3-008 | 借用型与 owned 混用类型不符 | 中 | 提供 `BorrowedValue::to_owned` / `Value` 桥接；类型不匹配返回明确错误（非 panic） |

**高严重度风险优先级**：R-01 = R-02 > R-04 = R-05 = R-06 = R-07
**风险应对策略**：R-01/R-02（高风险）需在 M3/M4 启动前先行 spike 基准验证收益，若不达预期则降级或取消；R-04/R-05/R-06/R-07（中风险）在里程碑内通过集成测试覆盖；R-09/R-10/R-11（低风险）在 M5 集成验证阶段统一处理。

---

## 7. 需求覆盖核对表

| 需求编号 | 对应任务 | 覆盖状态 |
|---------|---------|---------|
| REQ-PW-001（自动预热） | T-M1-002 + T-M1-004 + T-M1-005 + T-M1-010 | ☐ |
| REQ-PW-002（多池统一预热） | T-M1-007 + T-M1-010 | ☐ |
| REQ-PW-003（预热进度可观测） | T-M1-003 + T-M1-008 + T-M1-010 | ☐ |
| REQ-PW-004（渐进式预热策略） | T-M1-006 + T-M1-010 | ☐ |
| REQ-PW-005（禁止预热静默吞错） | T-M1-006 + T-M1-009 + T-M1-010 | ☐ |
| REQ-PC-001（SQL 解析结果缓存） | T-M2-002 + T-M2-003 + T-M2-006 + T-M2-011 | ☐ |
| REQ-PC-002（查询优化结果缓存） | T-M2-006 + T-M2-009 + T-M2-011 | ☐ |
| REQ-PC-003（Schema 变更缓存失效） | T-M2-007 + T-M2-010 + T-M2-011 | ☐ |
| REQ-PC-004（缓存容量与淘汰策略） | T-M2-005 + T-M2-008 + T-M2-011 | ☐ |
| REQ-PC-005（禁止缓存污染错误计划） | T-M2-003 + T-M2-012 | ☐ |
| REQ-ZC-001（借用型值类型） | T-M3-002 + T-M3-003 + T-M3-008 | ☐ |
| REQ-ZC-002（RowData 列名借用） | T-M3-004 + T-M3-008 | ☐ |
| REQ-ZC-003（零拷贝反序列化路径） | T-M3-007 + T-M3-009 | ☐ |
| REQ-ZC-004（列式结果集布局） | T-M3-005 + T-M3-006 + T-M3-010 | ☐ |
| REQ-ZC-005（禁止隐式深拷贝） | T-M3-009 | ☐ |
| REQ-SIMD-001（批量行解码 SIMD 加速） | T-M4-003 + T-M4-006 + T-M4-008 | ☐ |
| REQ-SIMD-002（列比较批量过滤 SIMD） | T-M4-004 + T-M4-006 + T-M4-008 | ☐ |
| REQ-SIMD-003（SIMD 自动检测与降级） | T-M4-002 + T-M4-005 + T-M4-009 | ☐ |
| REQ-SIMD-004（SIMD 安全抽象） | T-M4-001 + T-M4-009 | ☐ |
| REQ-SIMD-005（禁止小数据量 SIMD） | T-M4-005 + T-M4-007 | ☐ |

> 覆盖状态：☐ 待完成 / ✅ 已完成（任务执行时更新）
> **20 条需求全覆盖确认**：上表 20 行，每行对应任务非空，覆盖状态完整。

### 总体验收标准覆盖

| 验收标准 | 对应任务 | 覆盖状态 |
|---------|---------|---------|
| AC-ALL-1（无 Breaking Change） | T-M1-001 + T-M2-001 + T-M3-001 + T-M4-001（feature gate 隔离） | ☐ |
| AC-ALL-2（全 workspace cargo test 通过） | T-M5-002 | ☐ |
| AC-ALL-3（clippy 零警告） | T-M5-002 | ☐ |
| AC-ALL-4（四项能力 feature gate 隔离） | T-M1-001 + T-M2-001 + T-M3-001 + T-M4-001 + T-M5-001 | ☐ |
| AC-ALL-5（sz-pay/sz-rust 下游零回归） | T-M5-004 + T-M5-005 | ☐ |
| AC-ALL-6（v2.4.0 性能基准不回退） | T-M5-006 | ☐ |
| AC-ALL-7（五方言行为一致性） | T-M5-003 | ☐ |
| AC-ALL-8（20 条需求全满足） | T-M5-008（需求追溯矩阵） | ☐ |

---

## 8. 里程碑交付物汇总

| 里程碑 | 交付物 | 文件路径 |
|--------|--------|---------|
| M1 | 自动预热 + 渐进式 + 多池统一 + 进度可观测 + telemetry 集成 + 测试 | `packages/sz-orm-core/src/prewarm.rs` + `packages/sz-orm-core/src/pool.rs`（扩展）+ `packages/sz-orm-sqlx/src/unified_pool.rs`（扩展）+ `packages/sz-orm-core/src/telemetry.rs`（扩展）+ `packages/sz-orm-core/tests/prewarm_integration.rs` |
| M2 | PlanCache + SqlNormalizer + LRU 淘汰 + 表级失效 + 命中率统计 + 优化器扩展 + 差分测试 | `packages/sz-orm-core/src/plan_cache.rs` + `packages/sz-orm-core/src/query.rs`（扩展）+ `packages/sz-orm-core/src/migration.rs`（扩展）+ `packages/sz-orm-ai/src/query_plan_optimizer.rs`（扩展）+ `packages/sz-orm-core/tests/plan_cache_differential.rs` |
| M3 | BorrowedValue + BorrowedRowData + ColumnarResultSet + 零拷贝反序列化 + 等价性测试 + 基准 | `packages/sz-orm-core/src/value_borrowed.rs` + `packages/sz-orm-core/src/columnar.rs` + `packages/sz-orm-core/src/result_map.rs`（扩展）+ `packages/sz-orm-core/benches/zero_copy_bench.rs` |
| M4 | SimdAvailability + 批量解码 + 列比较 + 标量降级 + wide crate 集成 + 差分测试 + 基准 + WASM 降级 | `packages/sz-orm-core/src/simd.rs` + `packages/sz-orm-core/src/result_map.rs`（扩展）+ `packages/sz-orm-core/src/query.rs`（扩展）+ `packages/sz-orm-core/tests/simd_differential.rs` + `packages/sz-orm-core/benches/simd_bench.rs` |
| M5 | 全门禁通过 + 五方言集成 + 下游回归 + 基准不回退 + 性能报告 + CHANGELOG + crates.io 发布 | 门禁报告 + 五方言集成报告 + sz-pay/sz-rust 回归报告 + `docs/spec/v3.2.0/performance_benchmark_report.md` + `CHANGELOG.md` + crates.io v3.2.0 发布 |

---

> **文档结束**
>
> **文档版本**：v3.2.0 任务分解（tasks 阶段）
> **对应需求**：`docs/spec/v3.2.0/spec.md`（20 条 EARS 需求，4 组）
> **对应设计**：`docs/spec/v3.2.0/design.md`（1347 行，4 大模块、5 里程碑、12 风险）
> **基线参考**：`docs/spec/v3.0.0/tasks.md`（v3.0.0 任务分解，7 里程碑 / 61 子任务）
> **任务总数**：5 个里程碑 / 50 个子任务，覆盖全部 20 条需求 + 8 总体验收标准
> **关键路径**：T-M2-001 → T-M2-005 → T-M2-006 → T-M2-012 → T-M5-001 → T-M5-002 → T-M5-004 → T-M5-009（8 个任务，6 天）
> **总工期**：16 天（串行）/ 6 天（M1~M4 全并行 + M5），按收益/风险序推进（预热 → 计划缓存 → 零拷贝 → SIMD → 集成发布）