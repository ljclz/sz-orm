# 集成 FAQ

> 版本：v9.0.0 | 更新日期：2026-09-26

## Q1: 如何启用 AI 能力？

在 `Cargo.toml` 中添加：

```toml
[dependencies]
sz-orm-ai = { version = "9.0.0", optional = true }

[features]
ai-nl2sql = ["dep:sz-orm-ai", "sz-orm-ai/ai-nl2sql-enhanced"]
ai-index-advisor = ["dep:sz-orm-ai", "sz-orm-ai/ai-index-advisor"]
```

然后 `cargo build --features ai-nl2sql,ai-index-advisor`。

## Q2: NL2SQL 支持哪些方言？

通过 `SqlDialect` 枚举指定：`PostgreSQL`、`MySQL`、`SQLite`、`Oracle`、`Mssql`。默认使用 `PostgreSQL`。

## Q3: Saga 事务失败后如何处理？

`SagaResult` 有三种状态：
- `Success`：所有步骤成功
- `Compensated`：步骤失败，已成功步骤已自动补偿
- `CompensationFailed`：补偿也失败，需人工介入

## Q4: Prometheus 指标如何导出？

调用 `sz_orm_obs::export_metrics()` 获取 Prometheus 文本格式，或调用 `sz_orm_obs::start_metrics_http("0.0.0.0:9090")` 启动 HTTP 服务。

## Q5: 如何处理 sz-orm 版本升级？

1. 修改 `Cargo.toml` 中 sz-orm-* 版本号
2. 运行 `cargo check` 验证编译
3. 如有 API 不兼容，在接线层添加兼容适配
4. 运行 `cargo test` 确认无退化

## Q6: feature gate 冲突怎么办？

sz-orm 的 feature gate 设计为正交独立，不会互相冲突。如遇编译错误，检查是否同时启用了互斥的 feature（如 `real` 和 mock 模式）。

## Q7: 如何自定义 Schema 用于 NL2SQL？

构建 `SchemaContext`：

```rust
let schema = SchemaContext {
    tables: vec![
        TableInfo {
            name: "my_table".into(),
            columns: vec![
                ColumnInfo { name: "id".into(), data_type: "BIGINT".into(), nullable: false, is_primary_key: true },
            ],
        },
    ],
};
```

## Q8: 接线层应该放在哪里？

建议放在 `src/sz_orm_*.rs`，在 `lib.rs` 中通过 feature gate 注册：

```rust
#[cfg(feature = "ai-nl2sql")]
pub mod sz_orm_ai;
```