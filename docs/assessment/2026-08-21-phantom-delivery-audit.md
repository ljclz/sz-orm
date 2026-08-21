# sz-orm 工作空间幻影交付排查报告

**日期**：2026-08-21  
**排查范围**：sz-orm 工作空间全部 58 个 lib 包  
**方法**：依赖图分析 + feature 启用点搜索 + stub 模式扫描 + 生产调用链验证

---

## 排查方法

1. 解析根 `Cargo.toml` 工作空间成员（58 个 lib 包 + cli + examples）
2. 解析每个包的 `Cargo.toml` 依赖关系，构建依赖图
3. 检查 cli/examples 的实际 `use sz_orm_*` 导入，确认生产调用点
4. 检查每个包 `lib.rs` 的 `#[cfg(feature = ...)]` gate
5. 全工作空间搜索 feature 启用点（`features = ["..."]`）
6. 搜索 `todo!`/`unimplemented!`/`unreachable!`/`#![allow(dead_code)]`/stub 返回

## 全局事实

- **`todo!`/`unimplemented!`/`unreachable!`**：仅 4 处，全部是文档示例或测试字符串，无真实 stub 实现
- **crate 级 `#![allow(dead_code)]`**：3 处，其中 1 处违规（macros/src/diagnostic.rs:6）

## P0 整包零调用 + feature 无启用（4 个）

| # | 包 | 证据 file:line | 说明 |
|---|---|---|---|
| 1 | sz-orm-advisor | `packages/sz-orm-advisor/src/lib.rs:8` `#[cfg(feature = "query-advisor")]` | 无任何包依赖；全部 pub 项在 `query-advisor`/`query-intelligence-loop` feature 后，两 feature 全工作空间无启用点 |
| 2 | sz-orm-fusion | `packages/sz-orm-fusion/src/lib.rs:25` `#[cfg(feature = "db-fusion-v2")]` | 无任何包依赖；全部 pub 项在 `db-fusion`/`db-fusion-v2` feature 后，无启用点。lib.rs:1 自述 "POC, optional experiment" |
| 3 | sz-orm-stream | `packages/sz-orm-stream/src/lib.rs:8` `#[cfg(feature = "stream-resultset")]` | 无任何包依赖；核心模块在 `stream-resultset` feature 后，无启用点 |
| 4 | sz-orm-parallel | `packages/sz-orm-parallel/src/lib.rs:16` `#[cfg(feature = "parallel-query")]` | 无任何包依赖；scheduler 模块在 `parallel-query` feature 后，无启用点 |

## P1 整包零调用（14 个）

| # | 包 | 证据 file:line | 说明 |
|---|---|---|---|
| 5 | sz-orm-graphql | `packages/sz-orm-graphql/src/lib.rs:35` `#[cfg(feature = "real")]` | 无任何包依赖；`real` feature 无启用点，默认 `execute_query` 返回 mock JSON |
| 6 | sz-orm-es | `packages/sz-orm-es/src/lib.rs:1` 自述 "MOCK-ONLY" | 无任何包依赖；默认 `InMemoryEsSync` 是 mock，`real` feature 无启用点 |
| 7 | sz-orm-logger | `packages/sz-orm-logger/src/lib.rs:1` | 无任何包依赖；`prod-log-level` feature 无启用点 |
| 8 | sz-orm-postgis | `packages/sz-orm-postgis/src/lib.rs:59` `#[cfg(feature = "real-postgis")]` | 无任何包依赖；默认仅 Memory/Stub 实现 |
| 9 | sz-orm-timeseries | `packages/sz-orm-timeseries/src/lib.rs:53` `#[cfg(feature = "real-timescale")]` | 无任何包依赖；默认仅 Memory/Stub |
| 10 | sz-orm-search | `packages/sz-orm-search/src/lib.rs:50` `#[cfg(feature = "real-es")]` | 无任何包依赖；`real-es`/`real-opensearch`/`real-meilisearch` 全无启用点 |
| 11 | sz-orm-wasm | `packages/sz-orm-wasm/src/lib.rs:14` `#[cfg(feature = "js")]` | 无任何包依赖；`wasm-real-db`/`js`/`persistence` 全无启用点 |
| 12 | sz-orm-lc | `packages/sz-orm-lc/src/lib.rs:1` | 无任何包依赖；`lc-bidirectional-sync` feature 无启用点 |
| 13 | sz-orm-back | `packages/sz-orm-back/src/lib.rs:17` `#[cfg(feature = "backup-verify")]` | 无任何包依赖 |
| 14 | sz-orm-mig | `packages/sz-orm-mig/src/lib.rs:1` | 无任何包依赖 |
| 15 | sz-orm-rw | `packages/sz-orm-rw/src/lib.rs:22` `#[cfg(feature = "auto-failover")]` | 无任何包依赖 |
| 16 | sz-orm-config | `packages/sz-orm-config/src/lib.rs:12` `#[cfg(feature = "real-consul")]` | 无任何包依赖；`real-consul`/`real-nacos`/`prod-config-masking` 全无启用点 |
| 17 | sz-orm-mqtt | `packages/sz-orm-mqtt/src/lib.rs:33` `#[cfg(feature = "real-broker")]` | 无任何包依赖 |
| 18 | sz-orm-websocket | `packages/sz-orm-websocket/src/lib.rs:28` `#[cfg(feature = "server")]` | 无任何包依赖 |

## P2 间接零调用 — 依赖链断裂（6 个）

| # | 包 | 证据 file:line | 说明 |
|---|---|---|---|
| 19 | sz-orm-diagnosis | `packages/sz-orm-diagnosis/src/lib.rs:28` | 仅被 sz-orm-advisor(optional) 依赖，advisor 是 P0 幻影 |
| 20 | sz-orm-adaptive | `packages/sz-orm-adaptive/src/lib.rs:1` | 被 sz-orm-parallel 依赖（P0 幻影） |
| 21 | sz-orm-flamegraph | `packages/sz-orm-flamegraph/src/lib.rs:16` | 被 observability(optional, query-logging) 依赖，query-logging 无启用点 |
| 22 | sz-orm-tracing | `packages/sz-orm-tracing/src/lib.rs:1` | 仅被 sz-orm-flamegraph(optional) 依赖 |
| 23 | sz-orm-ai | `packages/sz-orm-ai/src/lib.rs:1` | 被 sz-orm-vector 依赖（vector 仅被 fusion 幻影依赖） |
| 24 | sz-orm-vector | `packages/sz-orm-vector/src/lib.rs:33` | 仅被 sz-orm-fusion(optional, db-fusion-v2) 依赖 |

## P3 模块级幻影 / 违规（5 个）

| # | 包/模块 | 证据 file:line | 说明 |
|---|---|---|---|
| 25 | observability::query_logger | `packages/sz-orm-observability/src/lib.rs:62` | `query-logging` feature 无启用点 |
| 26 | sz-orm-grpc | `packages/sz-orm-grpc/src/lib.rs:1` | 仅被 sz-orm-dtx(optional, cross-lang-dtx) 依赖，feature 无启用 |
| 27 | sz-orm-oracle | `packages/sz-orm-oracle/src/lib.rs:1` | 仅被 sz-orm-sqlx(optional, oracle) 依赖，feature 无启用 |
| 28 | sz-orm-mssql | `packages/sz-orm-mssql/src/lib.rs:1` | 仅被 sz-orm-sqlx(optional, mssql) 依赖，feature 无启用 |
| 29 | macros/src/diagnostic.rs | `packages/sz-orm-macros/src/diagnostic.rs:6` | `#![allow(dead_code)]` 违规 |

## 已确认非幻影的包

sz-orm-core, sz-orm-sqlx, sz-orm-macros, sz-orm-sql-validator, sz-orm-n1-lint, sz-orm-observability, sz-orm-storage, sz-orm-batch, sz-orm-queue, sz-orm-designer, sz-orm-swagger, sz-orm-crypto, sz-orm-auth, sz-orm-limit, sz-orm-scheduler, sz-orm-audit, sz-orm-dtx, sz-orm-sharding, sz-orm-health, sz-orm-masking, sz-orm-anomaly, sz-orm-graph（已修复）

## 严重程度汇总

| 严重程度 | 数量 | 包 |
|---|---|---|
| P0 整包零调用+feature无启用 | 4 | advisor, fusion, stream, parallel |
| P1 整包零调用 | 14 | graphql, es, logger, postgis, timeseries, search, wasm, lc, back, mig, rw, config, mqtt, websocket |
| P2 间接零调用 | 6 | diagnosis, adaptive, flamegraph, tracing, ai, vector |
| P3 模块级/违规 | 5 | observability::query_logger, grpc, oracle, mssql, macros/diagnostic.rs |
| 总计 | 29 | |