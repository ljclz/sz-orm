# 门禁阻断报告（2026-09-30）

- 分支 / commit：`main` @ `0d09d4a0`
- 失败门禁：**G1 格式检查（cargo fmt）**
- 状态机：`G0 未启用 → G1 → FAILED`
- 失败命令：
  ```bash
  cargo fmt --all -- --check
  ```
- 触发方式：`/sz-orm-review 全面审计`（全量 23 关），G1 即红牌，后续门禁未执行。

## 失败输出

### 1) `cargo fmt --all -- --check` 在 Windows 上报告 os error 206

```
文件名或扩展名太长。 (os error 206)
This utility formats all bin and lib files of the current crate using rustfmt.
...
```

定位结论：`cargo fmt --all` 将 `cli` + `examples` 两个成员包的全部 .rs 文件合并为**一条** rustfmt 命令行，实测该命令行长度为 **59,610 字符**，超过 Windows `CreateProcessW` 的 32767 字符限制，返回 `ERROR_FILENAME_EXCED_RANGE`（os error 206）。属于本机环境限制，非代码格式缺陷。

### 2) 逐包运行 `cargo fmt -p <pkg> -- --check` 发现 7 个包存在真实格式漂移

| 包 | diff 行数 | 首个证据位置 |
|----|-----------|--------------|
| sz-orm-core | 3153 | `packages/sz-orm-core/tests/common/pool_mock.rs:67` |
| sz-orm-health | 17 | `packages/sz-orm-health/tests/endpoint_test.rs:44` |
| sz-orm-sqlx | 124 | `packages/sz-orm-sqlx/tests/any_mysql_test.rs:157` |
| sz-orm-mssql | 154 | `packages/sz-orm-mssql/tests/mssql_conn_info_test.rs:4` |
| sz-orm-oracle | 206 | `packages/sz-orm-oracle/tests/oracle_conn_info_test.rs:31` |
| sz-orm-logger | 33 | `packages/sz-orm-logger/tests/log_pipeline_test.rs:1` |
| sz-orm-query-builder | 100 | `packages/sz-orm-query-builder/tests/qb_advanced_test.rs:17` |

代表性 diff（sz-orm-core）：

```
Diff in packages/sz-orm-core/tests/common/pool_mock.rs:67:
-    fn close<'a>(
-        &'a mut self,
-    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
+    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
```

代表性 diff（sz-orm-oracle，文件尾缺少换行）：

```
Diff in packages/sz-orm-oracle/tests/oracle_conn_info_test.rs:31:
     assert_eq!(info.as_connect_string(), "dbhost:1522/svc");
 }
+
```

## 证据（file:line，均真实存在）

- `packages/sz-orm-core/tests/common/pool_mock.rs:67` — `fn close` 签名应折叠为单行
- `packages/sz-orm-core/tests/common/pool_mock.rs:171` — 文件尾缺少换行
- `packages/sz-orm-core/tests/dialect_advanced_test.rs:3` — 行首 import 顺序
- `packages/sz-orm-core/tests/dialect_deep_test.rs:1` — 行首 import 顺序
- `packages/sz-orm-health/tests/endpoint_test.rs:44` — `HealthEndpointConfig::new` 多行参数应折叠
- `packages/sz-orm-sqlx/tests/any_mysql_test.rs:157` — `CREATE TABLE` 多行字符串应折叠
- `packages/sz-orm-sqlx/tests/any_pg_test.rs:12` — 行首 import 顺序
- `packages/sz-orm-mssql/tests/mssql_conn_info_test.rs:4` — `parse_conn_str` 参数应折叠
- `packages/sz-orm-mssql/tests/mssql_connection_test.rs:5` — 行首 import 顺序
- `packages/sz-orm-oracle/tests/oracle_conn_info_test.rs:31` — 文件尾缺少换行
- `packages/sz-orm-oracle/tests/oracle_connection_test.rs:4` — 行首 import 顺序
- `packages/sz-orm-oracle/tests/oracle_dialect_test.rs:1` — 行首 import 顺序
- `packages/sz-orm-logger/tests/log_pipeline_test.rs:1` — `use` 顺序：`std::collections` 应在 crate use 之前
- `packages/sz-orm-query-builder/tests/qb_advanced_test.rs:17` — `with_recursive_cte` 应折叠为单行
- `packages/sz-orm-query-builder/tests/qb_coverage_test.rs:1` — 行首 import 顺序

## 建议修复

1. 因 `cargo fmt --all` 在 Windows 上会触发命令行超长（os error 206），逐包执行格式化：
   ```bash
   cargo fmt -p sz-orm-core
   cargo fmt -p sz-orm-health
   cargo fmt -p sz-orm-sqlx
   cargo fmt -p sz-orm-mssql
   cargo fmt -p sz-orm-oracle
   cargo fmt -p sz-orm-logger
   cargo fmt -p sz-orm-query-builder
   ```
2. `cli` / `examples` 包虽无格式漂移，也要单独跑一次避免合并调用超长：
   ```bash
   cargo fmt -p sz-orm-cli
   cargo fmt -p sz-orm-examples
   ```
3. 提交格式化改动后，复跑 G1：
   ```bash
   # Linux CI / 逐包校验：
   cargo fmt --all -- --check   # 在 Linux runner 上执行
   # Windows 本机因命令行超长无法执行 --all，改用逐包校验：
   for p in $(cargo metadata --format-version 1 --no-deps | python -c "import sys,json;[print(p['name']) for p in json.load(sys.stdin)['packages']]"); do cargo fmt -p "$p" -- --check || exit 1; done
   ```

## 复跑要求

修复后必须**从 G1 重跑**，禁止跳过失败关。G1 全绿后方可进入 G2 及后续门禁。