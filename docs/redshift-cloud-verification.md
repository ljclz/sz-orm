# Redshift 云验证报告（v3.7.0 M5）

> 验证日期：2026-08-10
> 状态：**cloud verification pending: no accessible instance**

## 1. 验证环境评估

### 1.1 云实例可用性

| 检查项 | 结果 |
|--------|------|
| AWS Redshift Serverless 实例 | 无 |
| AWS Redshift Provisioned 实例 | 无 |
| 环境变量 `REDSHIFT_URL` | 未设置 |
| 环境变量 `AWS_REDSHIFT_URL` | 未设置 |
| AWS CLI 配置 | 未配置 Redshift |

**结论**：Redshift 云实例**不可用**。

### 1.2 Rust 驱动成熟度

| 驱动 | 版本 | 下载量 | 评估 |
|------|------|--------|------|
| sqlx (postgres) | 0.9 | - | Redshift 为 PG 兼容，可通过 sqlx postgres 驱动连接 |

RedshiftDialect 委派 PostgreSqlDialect + COPY/UNLOAD 特性扩展。理论上可通过 sqlx postgres 驱动连接 Redshift（PG 兼容协议）。

## 2. 验证缺口

由于无可用 Redshift 云实例，以下验证项**待验证**：

| 验证项 | 状态 | 说明 |
|--------|------|------|
| COPY 行为一致性 | ⏳ 待验证 | `COPY INTO table FROM 's3://bucket/path'` 需真 DB 验证 |
| UNLOAD 行为一致性 | ⏳ 待验证 | `UNLOAD ('SELECT ...') TO 's3://bucket/path'` 需真 DB 验证 |
| PG 兼容性行为一致性 | ⏳ 待验证 | PG 兼容 SQL 语法需真 DB 验证 |

## 3. 替代方案

### 3.1 方案 A：AWS Redshift Serverless 免费试用

AWS Redshift Serverless 提供 $500 免费信用额度（30 天）。
- 注册后配置 `REDSHIFT_URL` 环境变量
- 使用 sqlx postgres 驱动连接（Redshift PG 兼容协议）
- 运行 `cargo test --features dialect-redshift --test e2e_redshift_cloud`（待实现）

### 3.2 方案 B：SQL 生成 + 人工审核

当前采用方案。RedshiftDialect 生成的 SQL 由人工审核与 AWS Redshift 官方文档对比：
- COPY：`COPY table FROM 's3://bucket/path' IAM_ROLE 'arn:aws:iam::...' FORMAT AS CSV`
- UNLOAD：`UNLOAD ('SELECT * FROM table') TO 's3://bucket/path' IAM_ROLE 'arn:aws:iam::...'`
- PG 兼容性：RedshiftDialect 委派 PostgreSqlDialect，PG 兼容 SQL 语法一致

### 3.3 方案 C：本地 PG 模拟 + Redshift 特性标注

使用本地 PostgreSQL 验证 PG 兼容性部分，COPY/UNLOAD 特性仅 SQL 生成 + 人工审核。

## 4. 现有 SQL 生成测试通过证据

RedshiftDialect 的 SQL 生成测试全部通过（15 测试用例）：

```
running 15 tests
test test_redshift_auto_increment ... ok
test test_redshift_clone_box ... ok
test test_redshift_db_type ... ok
test test_redshift_copy ... ok
test test_redshift_escape_string ... ok
test test_redshift_get_dialect ... ok
test test_redshift_concat ... ok
test test_redshift_json_extract ... ok
test test_redshift_json_type ... ok
test test_redshift_pagination ... ok
test test_redshift_pg_consistency ... ok
test test_redshift_build_create_table ... ok
test test_redshift_quote ... ok
test test_redshift_supports_returning ... ok
test test_redshift_unload ... ok

test result: ok. 15 passed; 0 failed; 0 ignored
```

**证据文件**：`packages/sz-orm-core/tests/dialect_redshift_test.rs`
**RedshiftDialect 实现**：`packages/sz-orm-core/src/dialect.rs` RedshiftDialect

## 5. 结论

| 项目 | 结论 |
|------|------|
| 云实例可用性 | 不可用 |
| 真实云验证 | 待验证（无可用实例） |
| SQL 生成测试 | 15 测试全通过 |
| 替代方案 | 方案 B（SQL 生成 + 人工审核） |
| 后续计划 | 获取 AWS Redshift Serverless 免费试用后补齐真实云验证 |

**v3.7.0 状态**：Redshift 方言 SQL 生成验证通过，真实云验证待后续补齐。