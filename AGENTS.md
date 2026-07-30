# sz-orm 项目 AI 工作指南

- 版本：1.2.0（workspace.package.version 集中管理）
- 语言：Rust 2021 Edition（rust-version = "1.81"）
- 工作空间：43 个成员（41 个 lib 包 + cli + examples）
- 核心依赖：tokio（异步运行时）、sqlx（DB 驱动）、crossbeam-queue（连接池无锁队列）、serde/serde_json（序列化）
- 连接池：自研（AtomicU32 + crossbeam-queue ArrayQueue + Notify），非 deadpool（deadpool-postgres 仅 dev-dependency 用于 chaos-pool 测试）
- 模块路径：`packages/sz-orm-core/src/{query,model,pool,migration,transaction,hooks,repository,...}.rs`（扁平模块，非嵌套目录）
- 已发布：sz-orm-core 1.0.0 已发布到 crates.io（2026-07-23），当前代码版本 1.2.0
- 外部生产试点：sz-pay 项目（`E:\vue\test\sz-pay\server\sz-rust`）已使用 sz-orm-core/sqlx/config/auth/macros/queue 6 个包
- 约束：任何 WHERE 条件必须参数化（`where_eq`/`or_where_eq` 等），`where_cond`/`or_where` 已标记 deprecated；默认禁止 `SELECT *`；N+1 检测自动拦截（N1QueryDetector）。

## 编译时 SQL 验证（db-verify feature）

sz-orm-macros 提供 `query!` 宏，支持连真 DB 验证（类似 SQLx 的 `query!` 宏）：

```bash
# 启用连真 DB 验证（支持 MySQL/PostgreSQL/SQLite）
export DATABASE_URL="mysql://root:test123@127.0.0.1:3306/sz_orm_test"
export SZ_ORM_QUERY_VERIFY=1
cargo build --features sz-orm-macros/db-verify
```

- 默认仅语法校验（`validate_sql_content`）
- 启用 `db-verify` feature + `SZ_ORM_QUERY_VERIFY=1` 后，编译时连真 DB 执行 `EXPLAIN` 验证
- 支持 MySQL（`EXPLAIN`）、PostgreSQL（`EXPLAIN`）、SQLite（`EXPLAIN QUERY PLAN`）

## 质量官智能体（sz-orm-qa）

**系统提示词**：
```text
你是 sz-orm 项目的首席质量官（CQO），专注数据访问层健壮性。
工作流（严格顺序）：
1. 执行 SQL 生成变异测试。
2. 执行结果集差分测试。
3. 执行池混沌测试。
4. 执行 API 反向审查。
任一环节红牌即生成《阻断报告》并拒绝入库。
你拥有最终否决权。
```

**绑定 Skills**：全部 5 个（mutation-testing / sql-differential / chaos-pool / api-review / shadow-traffic）

## 本机数据库（用于测试和 db-verify）

- MySQL 9.6：`mysql://root:test123@127.0.0.1:3306/sz_orm_test`
- PostgreSQL 18：`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`
- Oracle 23ai Free：`127.0.0.1:1521/freepdb1`（用户 sys，密码 test123，Sysdba 权限）
