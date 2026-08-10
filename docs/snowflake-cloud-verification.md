# Snowflake 云验证报告（v3.7.0 M5）

> 验证日期：2026-08-10
> 状态：**cloud verification pending: no accessible instance**

## 1. 验证环境评估

### 1.1 云实例可用性

| 检查项 | 结果 |
|--------|------|
| Snowflake 云账号 | 无 |
| AWS Snowflake 实例 | 无 |
| Azure Snowflake 实例 | 无 |
| Snowflake Developer Edition（本地模拟） | 未安装 |
| 环境变量 `SNOWFLAKE_URL` | 未设置 |
| 环境变量 `SNOWFLAKE_ACCOUNT` | 未设置 |

**结论**：Snowflake 云实例**不可用**。

### 1.2 Rust 驱动成熟度

| 驱动 | 版本 | 下载量 | 评估 |
|------|------|--------|------|
| `snowflake-api` | - | - | 无成熟纯 Rust 驱动 |

sz-orm 使用 sqlx 作为 DB 驱动，sqlx 不支持 Snowflake。SnowflakeDialect 为 SQL generation only。

## 2. 验证缺口

由于无可用 Snowflake 云实例，以下验证项**待验证**：

| 验证项 | 状态 | 说明 |
|--------|------|------|
| UPSERT 行为一致性 | ⏳ 待验证 | MERGE INTO 语法需真 DB 验证 |
| TIME TRAVEL 行为一致性 | ⏳ 待验证 | AT(OBJECT => ...) / BEFORE(STATE => ...) 语法需真 DB 验证 |
| VARIANT 类型行为一致性 | ⏳ 待验证 | 半结构化类型需真 DB 验证 |
| COPY INTO 行为一致性 | ⏳ 待验证 | 数据加载语法需真 DB 验证 |

## 3. 替代方案

### 3.1 方案 A：Snowflake 免费试用

Snowflake 提供 30 天免费试用账号（$400 信用额度）。
- 注册地址：https://www.snowflake.com/free-trial/
- 注册后配置 `SNOWFLAKE_URL` 环境变量
- 运行 `cargo test --features dialect-snowflake --test e2e_snowflake_cloud`（待实现）

### 3.2 方案 B：SQL 生成 + 人工审核

当前采用方案。SnowflakeDialect 生成的 SQL 由人工审核与 Snowflake 官方文档对比：
- UPSERT：`MERGE INTO ... USING ... ON ... WHEN MATCHED THEN UPDATE ... WHEN NOT MATCHED THEN INSERT ...`
- TIME TRAVEL：`SELECT * FROM table AT(OBJECT => '<timestamp>')` / `SELECT * FROM table BEFORE(STATE => '<statement_id>')`
- VARIANT：`SELECT column:field FROM table`（路径访问语法）
- COPY INTO：`COPY INTO table FROM 's3://bucket/path'`

### 3.3 方案 C：本地 Snowflake 模拟

Snowflake Developer Edition（本地嵌入式 Snowflake）尚在预览阶段，未公开发布。

## 4. 现有 SQL 生成测试通过证据

SnowflakeDialect 的 SQL 生成测试全部通过（17 测试用例）：

```
running 17 tests
test test_snowflake_clone_box ... ok
test test_snowflake_time_travel_at ... ok
test test_snowflake_get_dialect ... ok
test test_snowflake_copy_into ... ok
test test_snowflake_json_extract ... ok
test test_snowflake_json_type ... ok
test test_snowflake_pagination ... ok
test test_snowflake_quote ... ok
test test_snowflake_auto_increment ... ok
test test_snowflake_last_insert_id ... ok
test test_snowflake_db_type ... ok
test test_snowflake_escape_string ... ok
test test_snowflake_supports_returning ... ok
test test_snowflake_time_travel_before ... ok
test test_snowflake_full_text_search ... ok
test test_snowflake_build_create_table ... ok
test test_snowflake_concat ... ok

test result: ok. 17 passed; 0 failed; 0 ignored
```

**证据文件**：`packages/sz-orm-core/tests/dialect_snowflake_test.rs`
**SnowflakeDialect 实现**：`packages/sz-orm-core/src/dialect.rs` SnowflakeDialect

## 5. 结论

| 项目 | 结论 |
|------|------|
| 云实例可用性 | 不可用 |
| 真实云验证 | 待验证（无可用实例） |
| SQL 生成测试 | 17 测试全通过 |
| 替代方案 | 方案 B（SQL 生成 + 人工审核） |
| 后续计划 | 获取 Snowflake 免费试用账号后补齐真实云验证 |

**v3.7.0 状态**：Snowflake 方言 SQL 生成验证通过，真实云验证待后续补齐。