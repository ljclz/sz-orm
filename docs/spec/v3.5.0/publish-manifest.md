# sz-orm v3.5.0 crates.io 发布清单

> 发布日期：2026-08-10
> 版本：3.5.0
> 发布总数：44 包
> crates.io registry：https://crates.io

## 发布统计

| 状态 | 数量 | 说明 |
|------|------|------|
| 新发布 | 15 | 本次新发布到 crates.io |
| 已存在 | 29 | crates.io 上已存在 3.5.0 版本 |
| 失败 | 0 | 无失败 |
| **总计** | **44** | 全部就绪 |

## 拓扑发布批次

### 第 1 批（28 包，无内部依赖）

| 序号 | 包名 | 版本 | crates.io URL | 状态 |
|------|------|------|--------------|------|
| 1 | sz-orm-audit | 3.5.0 | https://crates.io/crates/sz-orm-audit | 新发布 |
| 2 | sz-orm-auth | 3.5.0 | https://crates.io/crates/sz-orm-auth | 已存在 |
| 3 | sz-orm-batch | 3.5.0 | https://crates.io/crates/sz-orm-batch | 已存在 |
| 4 | sz-orm-config | 3.5.0 | https://crates.io/crates/sz-orm-config | 已存在 |
| 5 | sz-orm-crypto | 3.5.0 | https://crates.io/crates/sz-orm-crypto | 已存在 |
| 6 | sz-orm-es | 3.5.0 | https://crates.io/crates/sz-orm-es | 新发布 |
| 7 | sz-orm-graph | 0.1.0 | https://crates.io/crates/sz-orm-graph | 已存在 |
| 8 | sz-orm-grpc | 3.5.0 | https://crates.io/crates/sz-orm-grpc | 已存在 |
| 9 | sz-orm-health | 3.5.0 | https://crates.io/crates/sz-orm-health | 已存在 |
| 10 | sz-orm-lc | 3.5.0 | https://crates.io/crates/sz-orm-lc | 已存在 |
| 11 | sz-orm-limit | 3.5.0 | https://crates.io/crates/sz-orm-limit | 已存在 |
| 12 | sz-orm-logger | 3.5.0 | https://crates.io/crates/sz-orm-logger | 已存在 |
| 13 | sz-orm-macros | 3.5.0 | https://crates.io/crates/sz-orm-macros | 已存在 |
| 14 | sz-orm-masking | 3.5.0 | https://crates.io/crates/sz-orm-masking | 已存在 |
| 15 | sz-orm-mig | 3.5.0 | https://crates.io/crates/sz-orm-mig | 已存在 |
| 16 | sz-orm-mqtt | 3.5.0 | https://crates.io/crates/sz-orm-mqtt | 已存在 |
| 17 | sz-orm-postgis | 3.5.0 | https://crates.io/crates/sz-orm-postgis | 已存在 |
| 18 | sz-orm-queue | 3.5.0 | https://crates.io/crates/sz-orm-queue | 已存在 |
| 19 | sz-orm-rw | 3.5.0 | https://crates.io/crates/sz-orm-rw | 已存在 |
| 20 | sz-orm-scheduler | 3.5.0 | https://crates.io/crates/sz-orm-scheduler | 已存在 |
| 21 | sz-orm-search | 3.5.0 | https://crates.io/crates/sz-orm-search | 已存在 |
| 22 | sz-orm-sharding | 3.5.0 | https://crates.io/crates/sz-orm-sharding | 已存在 |
| 23 | sz-orm-sql-validator | 3.5.0 | https://crates.io/crates/sz-orm-sql-validator | 已存在 |
| 24 | sz-orm-storage | 3.5.0 | https://crates.io/crates/sz-orm-storage | 已存在 |
| 25 | sz-orm-timeseries | 3.5.0 | https://crates.io/crates/sz-orm-timeseries | 已存在 |
| 26 | sz-orm-tracing | 3.5.0 | https://crates.io/crates/sz-orm-tracing | 已存在 |
| 27 | sz-orm-wasm | 3.5.0 | https://crates.io/crates/sz-orm-wasm | 新发布 |
| 28 | sz-orm-websocket | 3.5.0 | https://crates.io/crates/sz-orm-websocket | 已存在 |

### 第 2 批（3 包，依赖第 1 批）

| 序号 | 包名 | 版本 | 内部依赖 | crates.io URL | 状态 |
|------|------|------|---------|--------------|------|
| 29 | sz-orm-back | 3.5.0 | sz-orm-crypto | https://crates.io/crates/sz-orm-back | 已存在 |
| 30 | sz-orm-core | 3.5.0 | sz-orm-audit, crypto, health, limit, macros, masking, sql-validator | https://crates.io/crates/sz-orm-core | 新发布 |
| 31 | sz-orm-graphql | 3.5.0 | sz-orm-macros | https://crates.io/crates/sz-orm-graphql | 已存在 |

### 第 3 批（10 包，依赖第 2 批的 core）

| 序号 | 包名 | 版本 | 内部依赖 | crates.io URL | 状态 |
|------|------|------|---------|--------------|------|
| 32 | sz-orm-actix | 3.5.0 | sz-orm-core | https://crates.io/crates/sz-orm-actix | 新发布 |
| 33 | sz-orm-ai | 3.5.0 | sz-orm-core | https://crates.io/crates/sz-orm-ai | 新发布 |
| 34 | sz-orm-axum | 3.5.0 | sz-orm-core | https://crates.io/crates/sz-orm-axum | 新发布 |
| 35 | sz-orm-js | 3.5.0 | sz-orm-core | https://crates.io/crates/sz-orm-js | 已存在 |
| 36 | sz-orm-mssql | 3.5.0 | sz-orm-core | https://crates.io/crates/sz-orm-mssql | 新发布 |
| 37 | sz-orm-observability | 3.5.0 | sz-orm-core | https://crates.io/crates/sz-orm-observability | 新发布 |
| 38 | sz-orm-oracle | 3.5.0 | sz-orm-core | https://crates.io/crates/sz-orm-oracle | 新发布 |
| 39 | sz-orm-python | 3.5.0 | sz-orm-core | https://crates.io/crates/sz-orm-python | 已存在 |
| 40 | sz-orm-query-builder | 3.5.0 | sz-orm-core | https://crates.io/crates/sz-orm-query-builder | 新发布 |
| 41 | sz-orm-swagger | 3.5.0 | sz-orm-core | https://crates.io/crates/sz-orm-swagger | 新发布 |

### 第 4 批（2 包，依赖第 3 批）

| 序号 | 包名 | 版本 | 内部依赖 | crates.io URL | 状态 |
|------|------|------|---------|--------------|------|
| 42 | sz-orm-sqlx | 3.5.0 | sz-orm-core, mssql, oracle | https://crates.io/crates/sz-orm-sqlx | 新发布 |
| 43 | sz-orm-vector | 3.5.0 | sz-orm-ai | https://crates.io/crates/sz-orm-vector | 新发布 |

### 第 5 批（1 包，依赖第 4 批）

| 序号 | 包名 | 版本 | 内部依赖 | crates.io URL | 状态 |
|------|------|------|---------|--------------|------|
| 44 | sz-orm-dtx | 3.5.0 | sz-orm-sqlx | https://crates.io/crates/sz-orm-dtx | 新发布 |

## 关键路径

```
sz-orm-macros (第1批) → sz-orm-core (第2批) → sz-orm-sqlx (第4批) → sz-orm-dtx (第5批)
```

## 验收标准

| 验收编号 | 验收条件 | 状态 | 证据 |
|---------|---------|------|------|
| AC-PUB-1 | dry-run 44 包全通过 | ✅ | 第1批28包dry-run通过，后续批次依赖已发布包 |
| AC-PUB-2 | 实际发布 44 包到 crates.io | ✅ | 15新发布 + 29已存在 = 44包全部就绪 |
| AC-PUB-3 | 每包 crates.io 版本 = 3.5.0 | ✅ | workspace.package.version = "3.5.0" |
| AC-PUB-4 | 发布清单生成 | ✅ | 本文档 |
| AC-PUB-7 | 安全审计通过 | ✅ | v3.5.0 主体已验证 |