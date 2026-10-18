# ADR-0010: Workspace 包数量冻结在 72

**状态**：Accepted
**日期**：2026-10-18

## 背景

sz-orm 工作空间自 v5.1.0 起稳定在 72 个成员（70 个 sz-orm-* lib 包 + cli + examples）。随着功能持续扩展（v6.0.0 → v10.10.0），多次面临"新增功能是否需要新建 crate"的决策。

新建 crate 的诱惑在于：
- 模块隔离更清晰
- 可独立发布
- 编译并行度更高

但代价是：
- 工作空间膨胀，维护成本上升
- 依赖图复杂化，循环依赖风险增加
- 发布周期拉长（crates.io 逐包发布）
- 下游消费者依赖管理复杂化

## 决策

**冻结 workspace 包数量在 72，不新增 crate。**

新功能通过以下方式纳入既有包：
1. **既有包内新增模块**（如 `packages/sz-orm-core/src/failover_controller.rs`）
2. **feature gate 控制编译**（如 `auto-failover = ["rw-split-enhanced"]`）
3. **复用既有依赖**（不引入新外部依赖）

例外情况（需 PR 审查批准）：
- 绑定层新语言支持（如新增 sz-orm-ruby）
- 重大架构重构需拆分巨型包

## 后果

**正面**：
- 工作空间稳定，下游 sz-pay/sz-rust 依赖管理简单
- 编译缓存复用率高
- 发布流程可预测（72 包固定拓扑顺序）
- v10.10.0 在既有 72 包内完成 24 子任务交付，零新增 crate

**负面**：
- 部分包体积较大（如 sz-orm-core 504,898 行）
- feature gate 数量增长（517 个），需文档明确启用条件

**缓解**：
- 大包内按模块组织，保持单一职责
- feature gate 全部默认关闭，按需启用
- PHANTOM-2 扫描监控未启用的 feature gate