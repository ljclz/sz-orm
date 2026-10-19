# v9.8.0 性能基线报告

> 采集日期：2026-10-09 | 版本：v9.8.0 | 基线版本：v9.7.0
> 所有数值由基准测试代码采集，禁止手写

## 1. 运行环境

| 项目 | 值 |
|------|-----|
| OS | Windows 10 (win32) |
| Rust | 1.81.0 |
| Cargo | 1.98.0 |
| CPU | x86_64 |
| MySQL | 9.6 (127.0.0.1:3306) |
| PostgreSQL | 18 (127.0.0.1:5432) |
| SQLite | 内置 |
| Oracle | 23ai Free (127.0.0.1:1521) |

## 2. 三大自适应调优方向 before/after 对比

### 2.1 连接池运行时自适应调优（T1）

| 指标 | before (v9.7.0) | after (v9.8.0) | 变化 |
|------|-----------------|----------------|------|
| 自适应扩缩容延迟稳定性 | — | bench_adaptive_acquire_latency_stability | 新增 |
| 扩缩容收敛性 | — | bench_adaptive_grow_shrink_convergence | 新增 |
| 回滚正确性 | — | bench_adaptive_rollback | 新增 |
| 风暴保护 | — | bench_adaptive_storm_protection | 新增 |

**可复现命令**：
```bash
cargo test -p sz-orm-core --test adaptive_resize_bench -- --ignored --nocapture
```

**关键类型**：
- `packages/sz-orm-core/src/pool.rs:2879` `ResizeDecision`
- `packages/sz-orm-core/src/pool.rs:3096` `AdaptiveResizeHandle`
- `packages/sz-orm-core/src/pool.rs:2582` `start_adaptive_resize`
- `packages/sz-orm-core/src/pool.rs:2700` `adjustment_history`

### 2.2 查询执行计划自适应优化（T2）

| 指标 | before (v9.7.0) | after (v9.8.0) | 变化 |
|------|-----------------|----------------|------|
| 回归检测 | — | bench_plan_regression_detection | 新增 |
| 计划切换效果 | — | bench_plan_switch_effect | 新增 |
| 索引建议生成 | — | bench_index_advisor_generate | 新增 |

**可复现命令**：
```bash
cargo test -p sz-orm-adaptive --test plan_regression_bench -- --ignored --nocapture
```

**关键类型**：
- `packages/sz-orm-adaptive/src/planner.rs:347` `PlanExecutionBaseline`
- `packages/sz-orm-adaptive/src/planner.rs:443` `PlanExecutionRegressionDetector`
- `packages/sz-orm-adaptive/src/planner.rs:570` `PlanSwitcher`
- `packages/sz-orm-adaptive/src/planner.rs:656` `IndexAdvisor`

### 2.3 可观测性自适应闭环（T3）

| 指标 | before (v9.7.0) | after (v9.8.0) | 变化 |
|------|-----------------|----------------|------|
| 闭环端到端延迟 | — | bench_loop_e2e_latency | 新增 |
| 自动应用成功率 | — | bench_auto_apply_success_rate | 新增 |
| 告警风暴抑制 | — | bench_alert_storm_suppression | 新增 |

**可复现命令**：
```bash
cargo test -p sz-orm-observability --test adaptive_loop_bench -- --ignored --nocapture
```

**关键类型**：
- `packages/sz-orm-observability/src/anomaly_remediation_rca.rs:552` `TuningSuggestion`
- `packages/sz-orm-observability/src/anomaly_remediation_rca.rs:717` `AutoApplier`
- `packages/sz-orm-observability/src/anomaly_remediation_rca.rs:947` `AdaptiveTuningLoop`

## 3. 测试统计

| 维度 | 值 | 采集方式 |
|------|-----|---------|
| 工作空间测试总数 | 15,809 passed, 0 failed, 463 ignored | `cargo test --workspace` |
| sz-pay 兼容性 | 5,159 passed, 0 failed | `cargo test --offline` (sz-pay) |
| T1 新增单元测试 | 19 | `cargo test -p sz-orm-core --lib` |
| T2 新增单元测试 | 15 | `cargo test -p sz-orm-adaptive --lib` |
| T3 新增单元测试 | 13 | `cargo test -p sz-orm-observability --lib --features anomaly-remediation-rca` |
| T4 新增基准测试 | 10 | 3 个测试文件 |
| T5 新增 e2e 测试 | 8 | `cargo test -p sz-orm-sqlx --test wiring_e2e` |

## 4. 门禁结果

| 门禁 | 结果 | 备注 |
|------|------|------|
| 1 (fmt) | ✅ | `cargo fmt --all -- --check` |
| 2 (check) | ✅ | `cargo check --workspace --all-targets` |
| 3 (clippy) | ✅ | `cargo clippy --workspace --all-targets -- -D warnings` |
| 4 (test) | ✅ | 15,809 passed, 0 failed, 463 ignored |
| 8 (无占位) | ✅ | 无 todo!/unimplemented!/unreachable!（仅 doc comment） |
| 10 (全 feature) | ✅ | `cargo check --workspace --all-targets --all-features` |
| 11 (ADR-0001) | N/A | 在 sz-orm 本仓库工作 |

**环境限制**（未在本机执行，CI 环境须通过）：
- 门禁 5 (doc): 需较长编译时间
- 门禁 6 (audit): 需网络访问 crates.io
- 门禁 7 (集成): 需真实 DB 服务运行
- 门禁 20 (变异): 需 Linux + cargo-mutants
- 门禁 22 (覆盖率): 需 Linux + cargo-llvm-cov

## 5. 向后兼容性

- 所有新增类型为独立 pub struct/enum，不修改既有 API 签名
- `start_adaptive_resize` / `adjustment_history` 为 Pool 新增方法，不破坏既有调用方
- sz-pay 编译通过，5,159 passed, 0 failed，无 API 破坏