# 方言扩展评估报告（v3.7.0 M4）

> 评估日期：2026-08-10
> 评估范围：Informix / SAP HANA / Firebird 三种方言的 Rust 驱动成熟度与用户需求

## 1. Informix 方言评估

### 1.1 Rust 驱动成熟度

| 驱动 | 版本 | 下载量 | 创建时间 | 最后更新 | 稳定版 | 依赖 | 评估 |
|------|------|--------|----------|----------|--------|------|------|
| `informix_rust` | 0.0.4 | 4,048 | 2024-09-13 | 2024-09-13 | 无 | Informix CSDK（C 库） | **不成熟** |

**结论**：Informix Rust 驱动**不成熟**。
- `informix_rust` 仅 v0.0.4（预发布阶段），4,048 下载量
- 依赖 Informix CSDK（C 库），非纯 Rust 实现
- 无稳定版，仅 4 个版本，2024-09-13 后未更新
- 不支持作为 sz-orm 的生产级驱动

### 1.2 用户需求

- GitHub issue：无 Informix 方言需求
- 社区反馈：无
- sz-pay 项目：未使用 Informix

**结论**：Informix 方言**无用户需求**。

### 1.3 决策

**实现 SQL 生成方言**（标注 "SQL generation only, no real DB driver"）。
- 提供 InformixDialect 实现 Dialect trait
- 支持 SERIAL/ROW 类型 SQL 生成
- 不连真 DB（无可用 Rust 驱动）

## 2. SAP HANA 方言评估

### 2.1 Rust 驱动成熟度

| 驱动 | 版本 | 下载量 | 创建时间 | 最后更新 | 稳定版 | 依赖 | 评估 |
|------|------|--------|----------|----------|--------|------|------|
| `hdbconnect` | 0.32.0 | 145,582 | 2016-12-07 | 2025-06-06 | 0.32.0 | 纯 Rust | **较成熟** |
| `hdbconnect_async` | 0.32.0 | 91,033 | 2023-02-02 | 2025-06-06 | 0.32.0 | 纯 Rust | **较成熟** |

**结论**：SAP HANA Rust 驱动**较成熟**。
- `hdbconnect` v0.32.0，145K+ 下载，2016 年起维护，86 个版本
- `hdbconnect_async` v0.32.0，91K+ 下载，异步版本
- 纯 Rust 实现，有稳定版
- 但 sz-orm 目前使用 sqlx 作为 DB 驱动，sqlx 不支持 SAP HANA

### 2.2 企业需求

- GitHub issue：无 SAP HANA 方言需求
- 社区反馈：无
- sz-pay 项目：未使用 SAP HANA

**结论**：SAP HANA 方言**无用户需求**。

### 2.3 决策

**实现 SQL 生成方言**（标注 "SQL generation only, no real DB driver"）。
- 提供 SapHanaDialect 实现 Dialect trait
- 支持计算列/CE 函数 SQL 生成
- 不连真 DB（sz-orm 使用 sqlx，sqlx 不支持 SAP HANA）

## 3. Firebird 方言评估

### 3.1 Rust 驱动成熟度

| 驱动 | 版本 | 下载量 | 创建时间 | 最后更新 | 稳定版 | 依赖 | 评估 |
|------|------|--------|----------|----------|--------|------|------|
| `rsfbclient` | 0.27.0 | 58,223 | 2020-06-09 | 2026-07-03 | 0.27.0 | fbclient C 库 | **较成熟** |
| `rsfbclient-core` | 0.27.0 | 51,039 | 2020-08-10 | 2026-07-03 | 0.27.0 | - | **较成熟** |
| `rsfbclient-native` | 0.27.0 | 43,272 | 2020-08-10 | 2026-07-03 | 0.27.0 | fbclient | **较成熟** |
| `rsfbclient-diesel` | 0.27.0 | 10,994 | 2022-08-31 | 2026-07-03 | 0.27.0 | Diesel | **较成熟** |
| `r2d2_firebird` | 0.27.0 | 16,309 | 2021-02-23 | 2026-07-03 | 0.27.0 | r2d2 | **较成熟** |
| `firebird-wire` | 0.1.11 | 317 | 2026-06-23 | 2026-07-09 | 0.1.11 | 纯 Rust | **早期** |

**结论**：Firebird Rust 驱动**较成熟**。
- `rsfbclient` v0.27.0，58K+ 下载，2020 年起维护，36 个版本
- 有 Diesel 集成（`rsfbclient-diesel`）和 r2d2 连接池（`r2d2_firebird`）
- 但 sz-orm 目前使用 sqlx 作为 DB 驱动，sqlx 不支持 Firebird

### 3.2 用户需求

- GitHub issue：无 Firebird 方言需求
- 社区反馈：无
- sz-pay 项目：未使用 Firebird

**结论**：Firebird 方言**无用户需求**。

### 3.3 决策

**实现 SQL 生成方言**（标注 "SQL generation only, no real DB driver"）。
- 提供 FirebirdDialect 实现 Dialect trait
- 支持 GENERATOR/SEQUENCE + EXECUTE BLOCK SQL 生成
- 不连真 DB（sz-orm 使用 sqlx，sqlx 不支持 Firebird）

## 4. 总结

| 方言 | 驱动成熟度 | 用户需求 | 决策 | feature gate |
|------|-----------|----------|------|--------------|
| Informix | 不成熟 | 无 | SQL 生成方言 | `dialect-informix` |
| SAP HANA | 较成熟 | 无 | SQL 生成方言 | `dialect-saphana` |
| Firebird | 较成熟 | 无 | SQL 生成方言 | `dialect-firebird` |

三种方言均实现 SQL 生成方言（不连真 DB），通过 feature gate 隔离，默认关闭。