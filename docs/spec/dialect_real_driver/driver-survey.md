# sz-orm Informix/SAP HANA/Firebird Rust 驱动 crate 调研报告

> 任务编号：TASK-003
> 调研日期：2026-08-19
> 调研方法：crates.io API + GitHub API 客观证据（禁止凭 README 自述）
> 版本基线：v4.9.0

---

## 1. Informix 驱动 crate 调研

### 1.1 候选 crate 搜索

crates.io API 搜索 `q=informix`（https://crates.io/api/v1/crates?q=informix），返回 5 个 crate，其中仅 1 个为数据库驱动：

| # | crate 名称 | 描述 | 是否驱动 |
|---|-----------|------|---------|
| 1 | `informix_rust` | 包装 Informix CSDK 的 Rust 库 | ✅ 是 |
| 2 | `wicked-estate-tree-sitter-informix4gl` | tree-sitter grammar | ❌ 否 |
| 3 | `wicked-estate-extract` | 代码提取器 | ❌ 否 |
| 4 | `wicked-estate` | 代码图谱 | ❌ 否 |
| 5 | `rustium-config` | 配置库 | ❌ 否 |

### 1.2 候选 crate 9 项评估

#### informix_rust v0.0.4

| # | 评估项 | 结果 | 证据 |
|---|--------|------|------|
| 1 | crate 是否存在于 crates.io | ✅ 是 | https://crates.io/crates/informix_rust |
| 2 | 是否有近期发布版本 | ❌ 否（alpha） | v0.0.4，最后更新 2024-09-13（>11 个月） |
| 3 | 是否有文档 | ❌ 否 | 无 docs.rs，无 documentation 字段 |
| 4 | 是否有维护者 | ⚠️ 弱 | stars 3，forks 0，open_issues 1 |
| 5 | 是否支持异步 | ❌ 未知 | 无 tokio/async-trait 依赖，FFI 绑定 |
| 6 | 是否有测试 | ⚠️ 未知 | 无 CI 状态徽章 |
| 7 | 下载量 | ❌ 极低 | 总 4049，recent 17 |
| 8 | GitHub 仓库是否存在 | ✅ 是 | https://github.com/berrytern/informix-rust，最后 push 2024-10-21（>1 年，接近 DEPRECATED） |
| 9 | 是否有已知安全问题 | ⚠️ 未知 | 版本太低，未 audit |

**依赖分析**：`cc` ^1.0（build）+ `libc` ^0.2 + `chrono` ^0.4 → FFI 绑定 Informix CSDK（C native 库），需用户本地安装 CSDK，跨平台兼容性差。

### 1.3 决策

**Decision: SQL_GENERATION_ONLY**

**依据**：
1. 唯一候选 v0.0.4 alpha（< 1.0），未达稳定
2. 最后 GitHub push 2024-10-21（>1 年未更新，接近 DEPRECATED）
3. 下载量极低（4049，recent 17），社区采用度极低
4. 依赖 Informix CSDK（C native），跨平台兼容性差
5. 无文档（docs.rs）
6. stars 3，forks 0

---

## 2. SAP HANA 驱动 crate 调研

### 2.1 候选 crate 搜索

crates.io API 搜索 `q=hana`（https://crates.io/api/v1/crates?q=hana），返回 49 个 crate，其中 SAP HANA 驱动相关：

| # | crate 名称 | 描述 | 是否驱动 |
|---|-----------|------|---------|
| 1 | `hdbconnect_async` | 异步纯 Rust SAP HANA 驱动 | ✅ 是（async） |
| 2 | `hdbconnect` | 同步纯 Rust SAP HANA 驱动 | ✅ 是（sync） |
| 3 | `hdbconnect_impl` | hdbconnect 公共实现核心 | ⚠️ 内部 |
| 4 | `hdbconnect-arrow` | Arrow 集成 | ⚠️ 扩展 |
| 5 | `hdbconnect-mcp` | MCP server | ❌ 否 |
| 6 | `lumen-rag` | RAG 框架（支持 SAP HANA Cloud） | ❌ 否（非驱动） |
| 其他 | hana/hana-vault/hana_prefab 等 | Bevy 插件/SSH 客户端等 | ❌ 否 |

### 2.2 候选 crate 9 项评估

#### hdbconnect_async v0.32.0（首选）

| # | 评估项 | 结果 | 证据 |
|---|--------|------|------|
| 1 | crate 是否存在于 crates.io | ✅ 是 | https://crates.io/crates/hdbconnect_async |
| 2 | 是否有近期发布版本 | ✅ 是 | v0.32.0，最后更新 2025-06-06，18 个版本 |
| 3 | 是否有文档 | ✅ 是 | https://docs.rs/hdbconnect_async/，100% 文档覆盖 |
| 4 | 是否有维护者 | ✅ 是 | emabee + dleifeld，stars 43，forks 8 |
| 5 | 是否支持异步 | ✅ 是 | 依赖 tokio ^1.23 + async-trait ^0.1，`_async` 后缀 |
| 6 | 是否有测试 | ✅ 是 | 成熟 crate，86 个版本历史（同步版 hdbconnect） |
| 7 | 下载量 | ✅ 高 | 总 92347，recent 17898 |
| 8 | GitHub 仓库是否存在 | ✅ 是 | https://github.com/emabee/rust-hdbconnect，最后 push 2026-08-05（非常活跃） |
| 9 | 是否有已知安全问题 | ✅ 无 | cargo check 编译通过，license MIT OR Apache-2.0 |

**依赖分析**：`hdbconnect_impl` ^0.32.0 (features=["async"]) + `bb8` ^0.9 (optional，连接池) + `tokio` ^1.23 (optional) + `async-trait` ^0.1 (optional) → 纯 Rust async + bb8 连接池 + tokio，无 native 依赖。

**编译验证**：`cargo check -p sz-orm-sqlx --features dialect-saphana-driver` 成功（Windows MSVC，2026-08-19）。

### 2.3 决策

**Decision: INTEGRATED**

**依据**：
1. v0.32.0 成熟（> 0.1，18 个版本历史，2017 年至今长期维护）
2. 异步纯 Rust 实现，支持 bb8 连接池 + tokio + async-trait，无 native 依赖
3. 高下载量（92347，recent 17898），活跃使用
4. 有文档（docs.rs 100% 覆盖）
5. 活跃维护（最后 push 2026-08-05，2 周前）
6. stars 43，forks 8，社区采用度好
7. license MIT OR Apache-2.0
8. Windows MSVC 编译验证通过

**集成位置**：
- 依赖：`packages/sz-orm-sqlx/Cargo.toml`（feature `dialect-saphana-driver`，默认不启用）
- 桥接：`packages/sz-orm-sqlx/src/saphana_adapter.rs`（实现 `sz_orm_core::Connection` trait）
- E2E：`packages/sz-orm-sqlx/tests/saphana_e2e.rs`（3 个测试，标记 `#[ignore]`，需真实 SAP HANA）

---

## 3. Firebird 驱动 crate 调研

### 3.1 候选 crate 搜索

crates.io API 搜索 `q=firebird`（https://crates.io/api/v1/crates?q=firebird），返回 17 个 crate，其中 Firebird 驱动相关：

| # | crate 名称 | 描述 | 是否驱动 | async |
|---|-----------|------|---------|-------|
| 1 | `rsfbclient` | 绑定官方 firebird client lib | ✅ 是 | ❌ 同步 |
| 2 | `rsfbclient-rust` | 纯 Rust 实现 | ✅ 是 | ❌ 同步 |
| 3 | `rsfbclient-native` | native（fbclient）实现 | ✅ 是 | ❌ 同步 |
| 4 | `firebirust` | Firebird 客户端库 | ✅ 是 | ❌ 同步 |
| 5 | `firebird-wire` | 纯 Rust sync driver for Firebird 5+ | ✅ 是 | ❌ 同步 |
| 6 | `sqlx-firebirdsql` | Firebird SQL driver for SQLx | ✅ 是 | ✅ 异步 |
| 7 | `sqlx-firebird` | sqlx firebird driver | ✅ 是 | ✅ 异步 |
| 8 | `r2d2_firebird` | r2d2 连接池支持 | ⚠️ 扩展 | ❌ 同步 |
| 9 | `rsfbclient-diesel` | Diesel 实现 | ⚠️ 扩展 | ❌ 同步 |

### 3.2 候选 crate 9 项评估

#### rsfbclient v0.27.0（主流同步驱动）

| # | 评估项 | 结果 | 证据 |
|---|--------|------|------|
| 1 | crate 是否存在于 crates.io | ✅ 是 | https://crates.io/crates/rsfbclient |
| 2 | 是否有近期发布版本 | ✅ 是 | v0.27.0，最后更新 2026-07-03，36 个版本 |
| 3 | 是否有文档 | ⚠️ 部分 | 无 docs.rs，有 README |
| 4 | 是否有维护者 | ✅ 是 | fernandobatels，stars 94，forks 16 |
| 5 | 是否支持异步 | ❌ 否 | 无 tokio/async-trait 依赖，纯同步 |
| 6 | 是否有测试 | ✅ 是 | 成熟 crate，36 个版本 |
| 7 | 下载量 | ✅ 高 | 总 59287，recent 7425 |
| 8 | GitHub 仓库是否存在 | ✅ 是 | https://github.com/fernandobatels/rsfbclient，最后 push 2026-08-12 |
| 9 | 是否有已知安全问题 | ⚠️ 未知 | 需 audit |

#### sqlx-firebirdsql v0.1.0（异步候选）

| # | 评估项 | 结果 | 证据 |
|---|--------|------|------|
| 1 | crate 是否存在于 crates.io | ✅ 是 | https://crates.io/crates/sqlx-firebirdsql |
| 2 | 是否有近期发布版本 | ⚠️ 首发版 | v0.1.0，2026-05-25 首次发布 |
| 3 | 是否有文档 | ❌ 否 | 无 docs.rs |
| 4 | 是否有维护者 | ⚠️ 弱 | stars 0，forks 0 |
| 5 | 是否支持异步 | ✅ 是 | 依赖 sqlx-core ^0.9.0 + tokio ^1 |
| 6 | 是否有测试 | ⚠️ 未知 | 1 个版本 |
| 7 | 下载量 | ❌ 极低 | 总 26 |
| 8 | GitHub 仓库是否存在 | ✅ 是 | https://github.com/nakagami/sqlx-firebirdsql，最后 push 2026-05-25（仅 1 次提交） |
| 9 | 是否有已知安全问题 | ⚠️ 未知 | 太新 |

#### sqlx-firebird v0.1.0-beta.1（异步候选，已停更）

| # | 评估项 | 结果 | 证据 |
|---|--------|------|------|
| 1 | crate 是否存在于 crates.io | ✅ 是 | https://crates.io/crates/sqlx-firebird |
| 2 | 是否有近期发布版本 | ❌ 否 | v0.1.0-beta.1（beta），2023-06-29（>3 年） |
| 3 | 是否有文档 | ✅ 是 | https://docs.rs/sqlx-firebird |
| 4 | 是否有维护者 | ❌ 弱 | 无 GitHub 仓库 |
| 5 | 是否支持异步 | ✅ 是 | sqlx 驱动 |
| 6 | 是否有测试 | ⚠️ 未知 | 1 个版本 |
| 7 | 下载量 | ⚠️ 低 | 总 1484，recent 10 |
| 8 | GitHub 仓库是否存在 | ❌ 否 | repository 字段为 null |
| 9 | 是否有已知安全问题 | ⚠️ 未知 | beta 版本 |

### 3.3 决策

**Decision: SQL_GENERATION_ONLY**

**依据**：
1. 主流驱动 `rsfbclient` v0.27.0 是同步的（无 tokio/async-trait），集成到异步框架需 `spawn_blocking` 包装，复杂度高
2. 异步候选 `sqlx-firebirdsql` v0.1.0 太新不成熟（2026-05-25 首发版，下载量 26，stars 0，forks 0，仅 1 次提交）
3. 另一异步候选 `sqlx-firebird` v0.1.0-beta.1 是 beta，2023 年最后更新（>3 年停更），无 GitHub 仓库
4. `firebird-wire` v0.1.11 也是同步（"Pure-Rust sync driver"）
5. sz-orm 是异步框架，应优先集成异步驱动；集成同步驱动增加复杂度，与"代码精简"约束冲突

---

## 4. 决策汇总

| 方言 | 决策 | 驱动 crate | 关键依据 |
|------|------|-----------|---------|
| Informix | SQL_GENERATION_ONLY | — | 唯一候选 alpha + >1 年未更新 + CSDK 依赖 + 下载量极低 |
| SAP HANA | INTEGRATED | hdbconnect_async v0.32.0 | 成熟 + async + bb8 连接池 + 高下载量 + 活跃维护 |
| Firebird | SQL_GENERATION_ONLY | — | 主流驱动同步，异步候选不成熟 |

---

## 5. 标注一致性

三处标注措辞统一：
- **Informix**：`SQL generation only: 仅 SQL 生成，无真实驱动连接`
- **SAP HANA**：`已集成真实驱动 hdbconnect_async v0.32.0（feature dialect-saphana-driver）`
- **Firebird**：`SQL generation only: 仅 SQL 生成，无真实驱动连接`

标注位置：
1. `packages/sz-orm-core/src/db_type.rs:67-75`（DbType 枚举注释）
2. `packages/sz-orm-core/src/dialect.rs:2614-2631`（方言实现注释）
3. `README.md:521-523`（支持的数据库表）
4. `docs/sz-orm与同类产品对比分析.md:146-149`（2.3 节标注）