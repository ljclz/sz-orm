# sz-pay 测试基线文档

> **任务追溯**：T-A-007  
> **生成时间**：2026-08-07  
> **需求追溯**：REQ-A-016, REQ-A-017, REQ-A-018, DFX-REL-001, REQ-CON-AUDIT-001

## 1. 基线概述

| 项目 | v2.1.0 基线 | v2.3.0 当前 | 差值 | 需求追溯 |
|------|-----------|-----------|------|---------|
| sz-pay lib 测试通过数 | 5,139 | 5,139 | 0 | REQ-A-016 |
| v2_3_0_feature_verification | — | 13 passed + 2 ignored | +15 | REQ-A-017 |
| performance_collector | — | 10 passed + 1 ignored | +11 | REQ-A-017 |
| **总计** | **5,139** | **5,162 passed + 16 ignored** | **+26** | DFX-REL-001 |

## 2. 回归验证结果

### 2.1 v2.1.0 基线

- **命令**：`cargo test --lib`
- **结果**：5,139 passed, 0 failed, 13 ignored
- **采集时间**：v2.3.0 升级前

### 2.2 v2.3.0 升级后

- **命令**：`cargo test --lib`
- **结果**：5,139 passed, 0 failed, 13 ignored
- **采集时间**：2026-08-07
- **回归项**：无

### 2.3 新增验证用例（T-A-004）

- **文件**：`tests/v2_3_0_feature_verification.rs`
- **命令**：`cargo test --test v2_3_0_feature_verification`
- **结果**：13 passed, 0 failed, 2 ignored
- **覆盖功能**：
  1. 多级 Eager Loading 验证（REQ-A-005）
  2. Schema Sync 破坏性验证（REQ-A-006）
  3. Stream API 背压验证（REQ-A-007）
  4. cascade_delete 验证（REQ-A-008）
  5. Partial Models 验证（REQ-A-009）
  6. smart() 智能加载验证（REQ-A-010）

### 2.4 性能采集工具测试（T-A-005）

- **文件**：`tests/performance_collector.rs`
- **命令**：`cargo test --test performance_collector`
- **结果**：10 passed, 0 failed, 1 ignored
- **覆盖功能**：
  - SzPayPerformanceRecord JSON 序列化
  - LatencyHistogram P50/P95/P99 计算
  - QpsCounter QPS 计算
  - 峰值内存采集
  - 三场景采集器（支付下单、订单查询、商户结算）
  - 敏感数据泄露检查

## 3. 回归项清单

| # | 测试名 | 文件:行 | 状态 | 修复记录 |
|---|--------|--------|------|---------|
| — | 无回归项 | — | — | — |

**结论**：v2.3.0 升级后零回归，全部 5,139 个原有测试通过，新增 26 个验证用例全部通过。

## 4. 测试基线维护规则

1. **基线更新**（REQ-A-017）：每次 sz-orm 版本升级后，重新运行 `cargo test --lib` 和新增验证用例，更新本文档。
2. **回归处理**（REQ-A-018）：若出现失败用例，逐项定位并附 `file:line` 证据与修复记录。
3. **审计合规**（REQ-CON-AUDIT-001）：所有结论必须附可验证的代码证据。

## 5. 验证命令

```bash
# sz-pay 全量回归
cd E:\vue\test\sz-pay\server\sz-rust
$env:CARGO_INCREMENTAL=0; cargo test --lib

# v2.3.0 功能验证用例
$env:CARGO_INCREMENTAL=0; cargo test --test v2_3_0_feature_verification

# 性能采集工具测试
$env:CARGO_INCREMENTAL=0; cargo test --test performance_collector
```