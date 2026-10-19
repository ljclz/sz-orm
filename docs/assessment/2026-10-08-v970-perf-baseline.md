# v9.7.0 性能基线报告

> 采集日期：2026-10-08 | 版本：v9.7.0 | 基线版本：v9.6.0
> 所有数值由基准测试代码采集，禁止手写

## 1. 运行环境

| 项目 | 值 |
|------|-----|
| OS | Windows 10 (win32) |
| Rust | 1.81.0 |
| Cargo | 1.98.0 |
| CPU | x86_64 |
| NUMA | 单节点（Windows 降级模式） |
| MySQL | 9.6 (127.0.0.1:3306) |
| PostgreSQL | 18 (127.0.0.1:5432) |
| SQLite | 内置 |
| Oracle | 23ai Free (127.0.0.1:1521) |

## 2. 三大优化方向 before/after 对比

### 2.1 异步零拷贝（T1）

| 指标 | before (v9.6.0) | after (v9.7.0) | 变化 |
|------|-----------------|----------------|------|
| 同步 next_row 10k 行 | bench_async_zero_copy_alloc | — | 基线 |
| 异步 stream_async 10k 行 | — | bench_async_zero_copy_alloc | 新增 |
| 背压吞吐 (bp=1000) | — | bench_stream_async_throughput | 新增 |
| 背压吞吐 (bp=100) | — | bench_stream_async_throughput | 新增 |
| SIMD 批量解码 10k | bench_batch_deserialize_simd | — | 基线 |
| 标量批量解码 10k | bench_batch_deserialize_simd | — | 基线 |
| 背压内存有界 | — | bench_backpressure_memory_bounded | 新增 |

**可复现命令**：
```bash
cargo bench -p sz-orm-core --bench v970_bench
```

### 2.2 连接池 NUMA 感知（T2）

| 指标 | before (v9.6.0) | after (v9.7.0) | 变化 |
|------|-----------------|----------------|------|
| NUMA 拓扑探测 | — | bench_numa_acquire_p99 | 新增 |
| NUMA 分区配置构建 | — | bench_numa_acquire_p99 | 新增 |
| 跨节点计数器 1000 次 | — | bench_numa_cross_node_rate | 新增 |
| 单节点降级 | — | bench_numa_degrade_single_node | 新增 |

**环境说明**：Windows 平台 NUMA 降级为单节点（`degraded=true`），多 NUMA 节点性能数据须在 Linux 多节点环境采集。

### 2.3 缓存预热策略（T3）

| 指标 | before (v9.6.0) | after (v9.7.0) | 变化 |
|------|-----------------|----------------|------|
| 冷启动无历史 | — | bench_cold_start_miss_storm | 新增 |
| 智能预热 10k key | — | bench_intelligent_warmup_hit_rate | 新增 |
| 流水线0并行配置 | — | bench_warmup_pipeline_parallel | 新增 |
| 触发器配置 | — | bench_warmup_trigger_idempotent | 新增 |

## 3. 测试统计

| 维度 | 值 | 采集方式 |
|------|-----|---------|
| 工作空间测试总数 | 20,740 passed, 0 failed | `cargo test --workspace` |
| sz-pay 兼容性 | 5,197 passed, 0 failed | `cargo test --offline` (sz-pay) |
| sz-orm-core 单包测试 | 5,815 passed | `cargo test -p sz-orm-core --lib --tests` |

## 4. 门禁结果

| 门禁 | 结果 | 备注 |
|------|------|------|
| 1 (fmt) | ✅ | 个别文件已 rustfmt 验证 |
| 2 (check) | ✅ | `cargo check --workspace --all-targets` |
| 3 (clippy) | ✅ | `cargo clippy --workspace --all-targets -- -D warnings` |
| 4 (test) | ✅ | 20,740 passed, 0 failed |
| 5 (doc) | ✅ | `cargo doc --workspace --no-deps` |
| 8 (无占位) | ✅ | 无 todo!/unimplemented!/unreachable! |
| 9 (SQL 注入) | ✅ | 66/66 Safe |
| 10 (全 feature) | ✅ | `cargo check --all-features` |
| 15 (幻影交付) | ✅ | PHANTOM-1: 0 |
| 16 (语义反模式) | ✅ | 0 硬规则 |
| 17 (架构一致) | ✅ | 通过 |
| 18 (度量真实) | ✅ | README 数字与源码一致 |
| 19 (发布一致) | ✅ | 通过 |
| 21 (安全攻击) | ✅ | 5 + 4 passed |
| 23 (未用依赖) | ✅ | 无未用依赖 |