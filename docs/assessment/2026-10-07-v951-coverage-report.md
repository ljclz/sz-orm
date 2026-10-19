# sz-orm v9.5.1 覆盖率报告

> 采集日期：2026-10-07 | 工具：cargo-llvm-cov 0.9.1 | 采集对象：sz-orm-core 单包

## 1. 总览

| 维度 | 数值 | 门槛 | 结果 |
|------|------|------|------|
| 行覆盖率 | **94.10%** | ≥ 92% | ✅ |
| 分支覆盖率 | **91.37%** | — | — |
| 函数覆盖率 | **93.27%** | — | — |

- 总行数：104,463
- 已覆盖行：98,298
- 未覆盖行：6,165
- 总分支：8,076（已覆盖 7,379）
- 总函数：60,513（已覆盖 56,440）

## 2. 采集命令

```bash
export RUST_MIN_STACK=67108864 CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/f/cargo-target
cargo llvm-cov -p sz-orm-core --summary-only
```

## 3. 关键文件覆盖率

| 文件 | 行数 | 未覆盖 | 覆盖率 |
|------|------|--------|--------|
| repository.rs | 3,578 | 80 | 97.76% |
| pool.rs | 4,638 | 586 | 87.37% |
| query.rs | 7,959 | 813 | 89.79% |
| transaction.rs | 2,632 | 317 | 87.96% |
| eager_loader.rs | — | — | — |
| relation_trait.rs | 119 | 0 | 100.00% |
| value.rs | 2,300 | 62 | 97.30% |
| result_map.rs | 3,516 | 124 | 96.47% |
| schema_sync.rs | 1,765 | 122 | 93.09% |
| smart_eager_loader.rs | 1,599 | 269 | 83.18% |

## 4. v9.5.1 改动影响分析

### T2 EagerLoader 集成（repository.rs）

- 修改前覆盖率：~97.8%（v9.5.0 基线）
- 修改后覆盖率：97.76%
- 影响：新增 9 个测试覆盖 `with_eager_loader`/`find_all_with_eager`/`find_one_with_eager`/`RelationLoaderNotSet` 错误路径，覆盖率未降级

### T3 AutoHealer 端到端（pool.rs）

- pool.rs 覆盖率：87.37%（AutoHealer 相关代码由 e2e 测试覆盖，非单元测试）
- 影响：`test_auto_healer_evict_e2e` 验证 `AutoHealer::start` → warmup → acquire → stop 全链路

## 5. 对比基线

| 版本 | 行覆盖率 | 采集日期 |
|------|----------|---------|
| v8.7.0 | 88.59% | 2026-09-23 |
| v9.0.0 | 90.2% | 2026-09-26 |
| **v9.5.1** | **94.10%** | **2026-10-07** |

v9.5.1 较 v9.0.0 提升 +3.9%，较 v8.7.0 提升 +5.51%。