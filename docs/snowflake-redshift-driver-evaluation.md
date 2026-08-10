# Snowflake + Redshift Rust 驱动评估

> 版本：v3.6.0 | 日期：2026-08-10 | 对应需求：REQ-DIALECT-005

## 1. Snowflake Rust 驱动评估

### 1.1 现状

Snowflake 官方不提供原生 Rust 驱动。社区可选方案：

| 方案 | 类型 | 成熟度 | 维护状态 |
|------|------|--------|----------|
| ODBC + odbc-rs | ODBC 桥接 | 中 | 活跃 |
| HTTP SQL API | REST API | 低 | 社区原型 |
| snowflake-api-rs | HTTP API 封装 | 低 | 个人项目 |

### 1.2 推荐方案

**ODBC 桥接**：通过 Snowflake 官方 ODBC 驱动 + Rust `odbc` crate 连接。

```rust
// 示例连接字符串
// "Driver={Snowflake};Server=xxx.snowflakecomputing.com;Account=xxx;User=xxx;Password=xxx;Database=xxx;Schema=xxx;Warehouse=xxx"
```

### 1.3 sz-orm 集成策略

- v3.6.0：方言实现完成（SQL 生成正确），标注"需用户自备驱动（ODBC/HTTP API）"
- 集成测试标注 `#[ignore]`，需真实 Snowflake 云实例
- 未来版本可考虑封装 `snowflake-api-rs` 或 ODBC 桥接

## 2. Redshift Rust 驱动评估

### 2.1 现状

Redshift 兼容 PostgreSQL wire protocol，可直接用 `sqlx::Postgres` 驱动连接：

| 方案 | 类型 | 成熟度 | 维护状态 |
|------|------|--------|----------|
| sqlx::Postgres | PG wire protocol | 高 | 活跃 |
| ODBC + Amazon Redshift ODBC | ODBC 桥接 | 高 | 官方维护 |
| rusoto_redshift (AWS SDK) | 管理 API | 中 | 活跃 |

### 2.2 推荐方案

**sqlx::Postgres**：Redshift 兼容 PG 协议，可直接用 sqlx PG 驱动。

```rust
// 示例连接字符串
// "postgres://user:password@redshift-cluster-xxx.region.redshift.amazonaws.com:5439/db"
```

### 2.3 sz-orm 集成策略

- v3.6.0：方言实现委派 PG，SQL 生成与 PG 一致
- 可直接复用 sz-orm-core 的 PG 连接池和查询执行
- COPY/UNLOAD 特有操作通过 `RedshiftDialect::build_copy()` / `build_unload()` 生成 SQL，再通过连接执行
- 集成测试标注 `#[ignore]`，需真实 Redshift 云实例

## 3. 结论

| 方言 | 驱动方案 | 集成难度 | v3.6.0 状态 |
|------|----------|----------|-------------|
| Snowflake | ODBC 桥接 | 中 | 方言实现完成，驱动待社区生态 |
| Redshift | sqlx::Postgres | 低 | 方言实现完成，可直接用 PG 驱动 |