# sz-pay 29 包接线实例

> 版本：v9.0.0 | 更新日期：2026-09-26 | 依据：M11-M14 接线代码

## 概述

sz-pay 项目（`E:\vue\test\sz-pay\server\sz-rust`）是 sz-orm 的首个生产试点下游项目。本文档记录 sz-pay 接入 sz-orm 29 个包的接线实例，每项附生产调用点证据（file:line）。

## 接线层架构

```
sz-pay-server
├── src/sz_orm_ai.rs      # v9.0.0 M11: AI 接线（NL2SQL + 自动调优 + 索引建议）
├── src/sz_orm_dtx.rs     # v9.0.0 M12: 分布式事务接线（Saga + 支付 Saga 构建）
├── src/sz_orm_obs.rs     # v9.0.0 M13: 可观测接线（Prometheus 指标 + 结构化日志）
├── src/ai/               # v5.1.0: AI 服务层（nl2sql_service / index_advisor_service / nl_query_service）
├── src/v71/              # v7.1.0: 生产接线模块（auto_tuning / dist_coordination / cdc_sync 等）
└── src/db.rs             # 核心 DB 连接管理
```

## 29 包接线清单

### 核心层（6 包）

| 包 | 用途 | 接线文件 | Feature Gate | 生产调用点 |
|---|------|---------|-------------|-----------|
| sz-orm-core | 核心 ORM | `src/db.rs` | 默认 | `src/db.rs:1` |
| sz-orm-sqlx | SQLite/MySQL/PG 驱动 | `src/db.rs` | 默认 | `src/db.rs:1` |
| sz-orm-query-builder | 查询构建器 | `src/repositories/` | 默认 | `src/repositories/` |
| sz-orm-config | 配置管理 | `src/config.rs` | 默认 | `src/config.rs:1` |
| sz-orm-auth | 认证授权 | `src/middleware/` | `auth` | `src/middleware/auth.rs` |
| sz-orm-macros | 过程宏 | 编译期 | 默认 | `#[derive(Model)]` 等 |

### AI 层（3 包）

| 包 | 用途 | 接线文件 | Feature Gate | 生产调用点 |
|---|------|---------|-------------|-----------|
| sz-orm-ai | NL2SQL + 索引建议 | `src/sz_orm_ai.rs` | `ai-nl2sql` / `ai-index-advisor` | `src/sz_orm_ai.rs:nl2sql_query` |
| sz-orm-nl-query | NL 查询管线 | `src/ai/nl_query_service.rs` | `ai-nl-query` | `src/ai/nl_query_service.rs:1` |
| sz-orm-agent | AI Agent | `src/ai/` | `ai-agent` | `src/ai/` |

### 分布式层（2 包）

| 包 | 用途 | 接线文件 | Feature Gate | 生产调用点 |
|---|------|---------|-------------|-----------|
| sz-orm-dtx | Saga/TCC 分布式事务 | `src/sz_orm_dtx.rs` | `v71-dist-coordination` | `src/sz_orm_dtx.rs:saga_execute` |
| sz-orm-advisor | 查询自调优 | `src/v71/auto_tuning.rs` | `v71-query-auto-tuning` | `src/v71/auto_tuning.rs:1` |

### 可观测层（1 包）

| 包 | 用途 | 接线文件 | Feature Gate | 生产调用点 |
|---|------|---------|-------------|-----------|
| sz-orm-observability | Prometheus 指标 + SLO | `src/sz_orm_obs.rs` | `anomaly-remediation-rca` | `src/sz_orm_obs.rs:export_metrics` |

### 扩展层（17 包）

| 包 | 用途 | Feature Gate |
|---|------|-------------|
| sz-orm-graph | 图查询 | `v71-graph-traversal` |
| sz-orm-vector | 向量搜索 | `vector` |
| sz-orm-audit | 审计日志 | `audit` |
| sz-orm-crypto | 加密 | `crypto` |
| sz-orm-masking | 数据脱敏 | `masking` |
| sz-orm-governance | 数据治理 | `governance` |
| sz-orm-multimodal | 多模态 | `multimodal` |
| sz-orm-mcp | MCP 协议 | `mcp` |
| sz-orm-queue | 消息队列 | `delayed-priority-queue` |
| sz-orm-batch | 批处理 | `batch` |
| sz-orm-storage | 对象存储 | `storage` |
| sz-orm-parallel | 并行查询 | `parallel` |
| sz-orm-stream | 流式处理 | `stream` |
| sz-orm-cdc | CDC 同步 | `v71-cdc-sync` |
| sz-orm-rbac | RBAC/ABAC | `v71-rbac-abac` |
| sz-orm-olap | OLAP 向量化 | `v71-olap-vectorized` |
| sz-orm-explain | 执行计划分析 | `explain` |

## v9.0.0 新增接线详解

### M11: AI 接线（`src/sz_orm_ai.rs`）

```rust
// NL2SQL 自然语言查询
let sql = sz_orm_ai::nl2sql_query("show all orders where status = 'paid'").await?;

// 查询自动调优
let analysis = sz_orm_ai::auto_tune("SELECT * FROM orders WHERE merchant_id = 1")?;

// 索引建议
let suggestions = sz_orm_ai::index_advise(&slow_queries).await?;
```

### M12: 分布式事务接线（`src/sz_orm_dtx.rs`）

```rust
// 创建支付 Saga
let saga = sz_orm_dtx::create_payment_saga(
    "payment-123",
    || deduct_balance(),      // 前向：扣余额
    || refund_balance(),      // 补偿：退余额
    || create_order(),        // 前向：创建订单
    || cancel_order(),        // 补偿：取消订单
    || post_ledger(),         // 前向：记账
    || reverse_ledger(),      // 补偿：冲账
);

// 执行 Saga
let result = sz_orm_dtx::saga_execute(saga)?;
```

### M13: 可观测接线（`src/sz_orm_obs.rs`）

```rust
// 记录查询耗时
sz_orm_obs::record_query_metric(0.025);

// 导出 Prometheus 指标
let metrics_text = sz_orm_obs::export_metrics();

// 结构化查询日志
sz_orm_obs::log_query("SELECT * FROM orders", 25);
```

## 端到端验证

16 个端到端测试全通过（`tests/v900_wiring_e2e.rs`）：

| 测试 | 覆盖路径 |
|------|---------|
| test_nl2sql_query_normal | 正常 NL2SQL 转换 |
| test_nl2sql_query_empty_input | 空输入边界 |
| test_nl2sql_query_whitespace_only | 纯空白边界 |
| test_nl2sql_query_payment_table | payments 表查询 |
| test_auto_tune_normal | 正常查询分析 |
| test_auto_tune_empty | 空查询边界 |
| test_index_advise_empty_input | 空慢查询边界 |
| test_index_advise_with_slow_query | 正常索引建议 |
| test_saga_execute_success | 单步 Saga 成功 |
| test_saga_execute_with_compensation | 多步 Saga + 补偿 |
| test_create_payment_saga | 支付 Saga 构建 |
| test_saga_state_not_found | Saga 不存在边界 |
| test_export_metrics_not_empty | 指标导出非空 |
| test_record_query_metric | 指标记录 |
| test_export_metrics_format | Prometheus 格式 |
| test_log_query | 结构化日志 |