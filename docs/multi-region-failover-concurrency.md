# 多区域多活：故障转移并发设计（v7.0.0 multi-region）

> 状态：现行 | 关联代码：`packages/sz-orm-fusion/src/{region_topology,region_failover,replication_lag,global_router}.rs`
> 评审来源：2026-09-13 全量审查 AI 评审遗留项 #3（并发策略不明确）；并发守卫于 2026-09-14 补齐（本次评审同批提交）

## 1. 组件与锁清单

| 组件 | 共享状态 | 并发原语 | 说明 |
|------|----------|----------|------|
| `RegionTopology` | 节点健康表 | `parking_lot::RwLock` | `update_health` / `health` / `candidates_by_priority` 均为短临界区，无嵌套获取 |
| `RegionFailoverCoordinator` | `failover_start` | `RwLock<Option<(String, Instant)>>` | 仅记录最近一次切换元数据，供诊断；非互斥用途 |
| `RegionFailoverCoordinator` | `failover_in_progress` | `AtomicBool` + `compare_exchange`（AcqRel/Acquire/Release） | **切换互斥的权威机制**，见 §2 |
| `RegionFailoverCoordinator` | `audit_logs` | `RwLock<Vec<FailoverAuditLog>>` | append-only；持有 `failover_in_progress` 期间写入，无与其他锁交叉 |
| `ReplicationLagTracker` | `lags` / `thresholds` | 各自独立的 `RwLock<HashMap>` | 两把锁不嵌套；`LagWindow` 内部为固定容量环形采样，每次读写整体克隆短临界区 |
| `GlobalRouter` | `latency_stats` | `Arc<parking_lot::RwLock<LatencyStats>>` | 路由决策为只读快照 + 延迟统计更新，短临界区 |

**锁序约定**：以上锁均不嵌套获取（临界区内不再获取第二把锁），不存在跨组件锁序问题；新增代码必须维持此约定。

## 2. 故障切换状态机（2026-09-14 修订）

```
空闲 ── CAS(false→true) 抢占 failover_in_progress ──> 切换中
切换中：update_health(Unavailable) → 按优先级选目标 → 写审计日志 → 标志复位(true→false) → 空闲
抢占失败 ──> 返回 Err(FailoverError::FailoverInProgress)
```

- **互斥保证**：`on_region_failure` 入口以 CAS 抢占 `failover_in_progress`，同一时刻至多一个线程在执行切换，防止并发双切换导致的路由漂移与脑裂窗口。
- **标志复位保证**：`do_failover` 的所有返回路径（含 `NoAvailableTarget`）都经过统一的标志复位点（`Release` 序），不会出现标志卡死导致的永久拒绝。并发测试 `concurrent_failover_cas_guard` 验证：8 线程 × 200 次争用后，末次切换必须成功。
- **与 `failover_start` 的关系**：`failover_start` 只做 RTO 计量与诊断展示，不参与互斥（避免读写锁竞争成为切换热路径）。

## 3. 脑裂防护

- `check_split_brain`：统计当前健康的 Primary 数量，>1 即判定脑裂（调用方策略：告警并人工介入，协调器不做自动仲裁）。
- CAS 互斥从源头抑制"两个区域同时被切换为主"的竞态窗口；跨进程/跨节点层面的脑裂（网络分区下多活写入）由底层复制拓扑（`ReplicationMode::Sync/Async` + `RegionRole`）与运维流程兜底，本组件职责为**单进程内的切换决策互斥**。

## 4. 副本延迟与路由联动

- `ReplicationLagTracker` 按 `(主区域, 从区域)` 维护滑动窗口延迟；`thresholds` 独立锁存配置。
- 路由侧（`GlobalRouter::route`）读取延迟快照做权重决策，写路径与读路径不共享可变状态，无数据竞争。

## 5. 已知边界（明确不保证）

1. **多进程部署**：CAS 守卫只在单进程内有效；多实例部署需外部队主（如数据库租约/分布式锁），本组件不负责。
2. **降级期间的新切换**：容忍期（默认 24h）概念属 TDE KMS 降级管理器（`sz-orm-crypto::KmsDegradeManager`），与区域切换互斥无关，两者不可混用。
3. **审计日志无界增长**：`audit_logs` 为 append-only Vec，长期运行需定期归档（调用方职责）。
4. `KmsDegradeManager` / `CachedKmsClient` 目前为独立组件（需调用方接线），未自动接入路由路径——使用时须显式构造（依据 AGENTS.md 幻影交付规则，此处明确标注"提供组件，需手动接入"）。

## 6. 验证证据

- `concurrent_failover_cas_guard`（8 线程 × 200 次 CAS 争用 + 复位不变量）：`cargo test -p sz-orm-fusion --features multi-region --lib concurrent_failover` → 1 passed（2026-09-14）。
- 修复前缺陷记录：AI 评审指出"故障转移决策需要原子性，否则可能出现脑裂"（2026-09-13 审查报告补充信息节），经代码核证属实并修复。
