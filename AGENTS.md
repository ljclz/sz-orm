# sz-orm 项目 AI 工作指南

- 版本：3.6.0（workspace.package.version 集中管理）
- 语言：Rust 2021 Edition（rust-version = "1.81"）
- 工作空间：43 个成员（41 个 lib 包 + cli + examples）
- 核心依赖：tokio（异步运行时）、sqlx（DB 驱动）、crossbeam-queue（连接池无锁队列）、serde/serde_json（序列化）
- 连接池：自研（AtomicU32 + crossbeam-queue ArrayQueue + Notify），非 deadpool（deadpool-postgres 仅 dev-dependency 用于 chaos-pool 测试）
- 模块路径：`packages/sz-orm-core/src/{query,model,pool,migration,transaction,hooks,repository,...}.rs`（扁平模块，非嵌套目录）
- 已发布：sz-orm-core 1.0.0 已发布到 crates.io（2026-07-23），当前代码版本 3.4.0
- 外部生产试点：sz-pay 项目（`E:\vue\test\sz-pay\server\sz-rust`）已使用 sz-orm-core/sqlx/config/auth/macros/queue 6 个包
- 约束：任何 WHERE 条件必须参数化（`where_eq`/`or_where_eq` 等），`where_cond`/`or_where` 已标记 deprecated；默认禁止 `SELECT *`；N+1 检测自动拦截（N1QueryDetector）。

## 工程化审查规范

**每次开发必须严格遵守** [docs/sz-orm-engineering-practices.md](docs/sz-orm-engineering-practices.md)，核心要点：

### ADR-0001（铁律）

**严禁下游项目修改上游 sz-orm / sz-rust 仓库的任何文件。** 任何改动必须通过 PR 贡献到上游。违反此原则会导致审计记录与事实不符，直接红牌拒绝入库。

### 14 道门禁（提交前必过）

| # | 门禁 | 命令 |
|---|------|------|
| 1 | fmt 格式检查 | `cargo fmt --all -- --check` |
| 2 | check 编译检查 | `cargo check --workspace --all-targets` |
| 3 | clippy 静态分析 | `cargo clippy --workspace --all-targets -- -D warnings` |
| 4 | test 单元/集成测试 | `cargo test --workspace` |
| 5 | doc 文档构建 | `cargo doc --workspace --no-deps --all-features` |
| 6 | audit 安全审计 | `cargo audit` + `cargo deny check` |
| 7 | integration 真实服务集成 | `cargo test --workspace -- --ignored` |
| 8 | 禁止占位实现检查 | `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` |
| 9 | SQL 注入扫描 | `scripts/check-sql-injection.ps1` |
| 10 | Feature 全组合编译 | `cargo check --workspace --all-targets --all-features` |
| 11 | 上游仓库未修改检查 | `git diff --name-only HEAD`（ADR-0001） |
| 12 | 文档与代码一致性检查 | `python scripts/check-doc-consistency.py` |
| 13 | 审计证据验证 | `bash scripts/audit-verify.sh <审计报告.md>` |
| 14 | 文档同步更新检查 | `python scripts/check-doc-sync.py --diff HEAD` |

### 五维审查（每次 PR 必做）

正确性 → 可读性 → 架构 → 安全性 → 性能

### AI 辅助开发 10 条硬约束

1. 禁止占位实现（todo!/unimplemented!/unreachable!）
2. 强制参数化查询（禁止 SQL 字符串拼接）
3. API 兼容性（签名变更必须同步更新所有调用方和测试）
4. 五维审查必过
5. unsafe 零容忍（必须有 // SAFETY: 注释）
6. 禁止 mock 逃逸
7. 门禁前置（主动运行 gate.ps1）
8. 跨平台意识
9. Feature 隔离
10. 教训记忆（阅读防御追溯表）

### 审计合规铁律（生死线）

**任何审计/审查结论必须附带可验证的代码证据：**

- ❌ 禁止：`已修复`、`应该没问题`、`参见其他文档`
- ✅ 必须：`[packages/sz-orm-core/src/query.rs:127](file:///.../query.rs#L127) 已修复，cargo test 输出：43 passed`
- 每条结论必须有 `file:line` 证据，且该文件行必须真实存在
- 修复后必须运行 `cargo test` 并附输出，禁止未验证即标记 ✅
- 多项修复必须逐项验证，禁止批量声称"全部通过"
- 违反本条视为审计无效，必须重新执行

**审计后必须运行验证脚本：**

```bash
# 验证审计报告中所有 file:line 引用是否真实存在
bash scripts/audit-verify.sh <审计报告.md>
# 或 Windows：
.\scripts\audit-verify.ps1 <审计报告.md>
```

脚本会逐项验证报告中所有 `file:line` 引用：
- ✅ 文件存在且行号在范围内
- ❌ 文件不存在或行号超出范围（编造证据）

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
