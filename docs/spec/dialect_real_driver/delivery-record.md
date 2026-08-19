# TASK-003 Informix/SAP HANA/Firebird 真实驱动集成交付记录

> 任务编号：TASK-003
> 交付日期：2026-08-19
> 版本基线：v4.9.0
> 对应需求：REQ-DIA-001 ~ REQ-DIA-014
> 调研报告：`docs/spec/dialect_real_driver/driver-survey.md`

---

## 1. 决策结果

| 方言 | 决策 | 驱动 crate | 版本 | feature |
|------|------|-----------|------|---------|
| Informix | SQL_GENERATION_ONLY | — | — | — |
| SAP HANA | INTEGRATED | hdbconnect_async | 0.32.0 | `dialect-saphana-driver` |
| Firebird | SQL_GENERATION_ONLY | — | — | — |

---

## 2. 调研证据（客观）

### 2.1 Informix（SQL_GENERATION_ONLY）

| 证据 | 来源 |
|------|------|
| 唯一候选 informix_rust v0.0.4 alpha | https://crates.io/api/v1/crates?q=informix |
| 最后 GitHub push 2024-10-21（>1 年） | https://api.github.com/repos/berrytern/informix-rust |
| 下载量 4049，recent 17 | crates.io API |
| 依赖 CSDK（cc + libc） | https://crates.io/api/v1/crates/informix_rust/0.0.4/dependencies |
| stars 3，forks 0 | GitHub API |

### 2.2 SAP HANA（INTEGRATED）

| 证据 | 来源 |
|------|------|
| hdbconnect_async v0.32.0 成熟 | https://crates.io/crates/hdbconnect_async |
| 下载量 92347，recent 17898 | crates.io API |
| docs.rs 100% 文档 | https://docs.rs/hdbconnect_async/0.32.0/ |
| 最后 GitHub push 2026-08-05（活跃） | https://api.github.com/repos/emabee/rust-hdbconnect |
| async + bb8 连接池 + tokio | https://crates.io/api/v1/crates/hdbconnect_async/0.32.0/dependencies |
| stars 43，forks 8 | GitHub API |
| Windows MSVC 编译通过 | `cargo check -p sz-orm-sqlx --features dialect-saphana-driver`（2026-08-19） |

### 2.3 Firebird（SQL_GENERATION_ONLY）

| 证据 | 来源 |
|------|------|
| 主流 rsfbclient v0.27.0 同步（无 async） | https://crates.io/api/v1/crates/rsfbclient/0.27.0/dependencies |
| sqlx-firebirdsql v0.1.0 不成熟（下载量 26） | https://crates.io/crates/sqlx-firebirdsql |
| sqlx-firebird v0.1.0-beta.1 已停更（2023-06-29） | https://crates.io/crates/sqlx-firebird |
| firebird-wire v0.1.11 同步 | https://crates.io/crates/firebird-wire |

---

## 3. 修改文件清单

### 3.1 SAP HANA 集成（INTEGRATED）

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `packages/sz-orm-sqlx/Cargo.toml` | 修改 | 添加 `hdbconnect_async` optional 依赖 + `dialect-saphana-driver` feature + `saphana_e2e` test 配置 |
| `packages/sz-orm-sqlx/src/saphana_adapter.rs` | 新增 | SAP HANA 驱动桥接层，实现 `sz_orm_core::Connection` trait（connect/query/execute/事务） |
| `packages/sz-orm-sqlx/src/lib.rs` | 修改 | 添加 `#[cfg(feature = "dialect-saphana-driver")] pub mod saphana_adapter;` |
| `packages/sz-orm-sqlx/tests/saphana_e2e.rs` | 新增 | 3 个 E2E 测试（连接查询/CRUD+事务/ping），标记 `#[ignore]`（需真实 SAP HANA） |

### 3.2 Informix + Firebird 标注（SQL_GENERATION_ONLY）

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `packages/sz-orm-core/src/db_type.rs:67` | 修改 | Informix 枚举注释：`SQL generation only: 仅 SQL 生成，无真实驱动连接` |
| `packages/sz-orm-core/src/db_type.rs:73` | 修改 | Firebird 枚举注释：`SQL generation only: 仅 SQL 生成，无真实驱动连接` |
| `packages/sz-orm-core/src/db_type.rs:70` | 修改 | SAP HANA 枚举注释：`已集成真实驱动 hdbconnect_async v0.32.0` |
| `packages/sz-orm-core/src/dialect.rs:2614-2631` | 修改 | 三方言注释块：调研决策 + 客观依据 |
| `packages/sz-orm-core/src/dialect.rs:2623` | 修改 | InformixDialect 文档注释 |
| `packages/sz-orm-core/src/dialect.rs:2851` | 修改 | SapHanaDialect 文档注释 |
| `packages/sz-orm-core/src/dialect.rs:3084` | 修改 | FirebirdDialect 文档注释 |
| `README.md:521-523` | 修改 | 支持的数据库表新增 Informix/SAP HANA/Firebird 行 |
| `docs/sz-orm与同类产品对比分析.md:146-149` | 修改 | 2.3 节标注更新（三方言决策结果） |
| `docs/sz-orm与同类产品对比分析.md:276` | 修改 | 综合对比矩阵方言数标注（Informix/Firebird 2 种无驱动，SAP HANA 已集成） |
| `docs/sz-orm与同类产品对比分析.md:312` | 修改 | 5.1 独特优势方言枚举标注 |
| `docs/sz-orm与同类产品对比分析.md:360` | 修改 | 6.2 技术弱点表：Informix/Firebird 无真实驱动（SAP HANA 已集成） |
| `docs/sz-orm与同类产品对比分析.md:403` | 修改 | 7.3 路线图 P2 项标记 ✅ 已完成 |
| `docs/sz-orm与同类产品对比分析.md:485` | 修改 | 文档修订说明第 6 条更新 |

### 3.3 文档

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `docs/spec/dialect_real_driver/driver-survey.md` | 新增 | 调研报告（三方言 9 项评估 + 决策 + 依据） |
| `docs/spec/dialect_real_driver/delivery-record.md` | 新增 | 本交付记录 |

---

## 4. 验证结果

### 4.1 编译验证

| 验证项 | 命令 | 结果 | 时间 |
|--------|------|------|------|
| sz-orm-sqlx 默认（不启用 feature） | `cargo check -p sz-orm-sqlx` | ✅ 通过 | 2026-08-19 |
| sz-orm-sqlx 启用 SAP HANA 驱动 | `cargo check -p sz-orm-sqlx --features dialect-saphana-driver` | ✅ 通过 | 2026-08-19 |
| sz-orm-sqlx 全 targets + SAP HANA | `cargo check -p sz-orm-sqlx --features dialect-saphana-driver --all-targets` | ✅ 通过 | 2026-08-19 |
| sz-orm-core 三方言 feature | `cargo check -p sz-orm-core --features dialect-informix,dialect-saphana,dialect-firebird` | ✅ 通过 | 2026-08-19 |
| sz-orm-sqlx SAP HANA 测试编译 | `cargo test -p sz-orm-sqlx --features dialect-saphana-driver --no-run` | ✅ 通过（含 saphana_e2e.rs） | 2026-08-19 |

### 4.2 标注一致性验证

| 标注位置 | Informix | SAP HANA | Firebird |
|---------|----------|----------|---------|
| db_type.rs | ✅ `SQL generation only: 仅 SQL 生成，无真实驱动连接` | ✅ `已集成真实驱动 hdbconnect_async v0.32.0` | ✅ `SQL generation only: 仅 SQL 生成，无真实驱动连接` |
| dialect.rs | ✅ 同上 | ✅ 同上 | ✅ 同上 |
| README.md | ✅ 同上 | ✅ 同上 | ✅ 同上 |
| 对比文档 | ✅ 同上 | ✅ 同上 | ✅ 同上 |

### 4.3 E2E 测试

| 测试 | 状态 | 说明 |
|------|------|------|
| `saphana_connect_and_query_dummy` | #[ignore] | 需真实 SAP HANA，设置 `SAP_HANA_URL` 后运行 `--ignored` |
| `saphana_create_insert_select_transaction` | #[ignore] | CRUD + 事务提交/回滚往返 |
| `saphana_ping_and_is_connected` | #[ignore] | ping + is_connected |

E2E 测试需真实 SAP HANA 数据库（本机未安装），标记 `#[ignore]`，编译验证通过（`cargo test --no-run` 成功，saphana_e2e 可执行文件生成）。

### 4.4 feature 门控验证

- 默认不启用 `dialect-saphana-driver`：`cargo check -p sz-orm-sqlx` 成功，无 hdbconnect_async 依赖
- 启用 `dialect-saphana-driver`：`cargo check -p sz-orm-sqlx --features dialect-saphana-driver` 成功

---

## 5. 五维审查

| 维度 | 结论 | 证据 |
|------|------|------|
| 正确性 | ✅ 三方言决策基于客观证据；SAP HANA 集成编译通过；Informix/Firebird 标注三处一致 | 本记录 §2、§4 |
| 可读性 | ✅ driver-survey.md 结构清晰，每 crate 9 项字段完整 | driver-survey.md |
| 架构 | ✅ SAP HANA 集成复用既有 Connection trait；标注方案不修改 SQL 生成逻辑 | saphana_adapter.rs 实现 Connection trait |
| 安全性 | ✅ hdbconnect_async license MIT OR Apache-2.0；SQL 参数化（dml/query 方法） | saphana_adapter.rs |
| 性能 | ✅ hdbconnect_async async + bb8 连接池；复用 sz-orm-core 连接池 | hdbconnect_async 依赖 bb8 ^0.9 |

---

## 6. 约束遵守

| 约束 | 遵守 | 证据 |
|------|------|------|
| 不自行编写数据库驱动 | ✅ | 仅集成 hdbconnect_async v0.32.0（crates.io 发布） |
| 调研基于客观证据 | ✅ | crates.io API + GitHub API（见 §2） |
| 代码精简 | ✅ | saphana_adapter.rs 95 行，仅实现 Connection trait 核心方法 |
| 标注三处一致 | ✅ | db_type.rs + dialect.rs + README + 对比文档（见 §4.2） |
| feature 门控默认不启用 | ✅ | `dialect-saphana-driver` 默认关闭（见 §4.4） |
| 未新增 workspace 成员 | ✅ | 仅在 sz-orm-sqlx 添加 feature + 模块 |
| 未修改既有 25 种其他方言 | ✅ | 仅修改三方言注释 + SAP HANA 集成 |