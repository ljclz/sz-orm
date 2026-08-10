# 方言扩展路线图

> 版本：v3.6.0 | 日期：2026-08-10 | 对应需求：REQ-DIALECT-006

## 1. v3.6.0 已实现方言

| # | 方言 | 实现方式 | Feature Gate | 测试数 | 状态 |
|---|------|----------|--------------|--------|------|
| 1 | MySQL | 独立实现 | 默认 | - | ✅ |
| 2 | PostgreSQL | 独立实现 | 默认 | - | ✅ |
| 3 | SQLite | 独立实现 | 默认 | - | ✅ |
| 4 | Oracle | 独立实现 | 默认 | - | ✅ |
| 5 | SQL Server | 独立实现 | 默认 | - | ✅ |
| 6 | ClickHouse | 独立实现 | 默认 | - | ✅ |
| 7 | DuckDB | 独立实现 | 默认 | - | ✅ |
| 8 | DB2 | 独立实现 | 默认 | - | ✅ |
| 9 | MariaDB | 委派 MySQL | 默认 | - | ✅ |
| 10 | TiDB | 委派 MySQL | 默认 | - | ✅ |
| 11 | OceanBase | 委派 MySQL | 默认 | - | ✅ |
| 12 | KingbaseES | 委派 PG | 默认 | - | ✅ |
| 13 | PolarDB | 委派 PG | 默认 | - | ✅ |
| 14 | GaussDB | 委派 PG | 默认 | - | ✅ |
| 15 | Dameng | 委派 Oracle | 默认 | - | ✅ |
| 16 | Sybase | 委派 SQL Server | 默认 | - | ✅ |
| 17 | GBase | 委派 SQL Server | 默认 | - | ✅ |
| 18 | CockroachDB | 委派 PG | `dialect-cockroachdb` | - | ✅ |
| 19 | YugabyteDB | 委派 PG | `dialect-yugabytedb` | - | ✅ |
| 20 | **Snowflake** | **独立实现** | `dialect-snowflake` | **17** | **✅ v3.6.0 新增** |
| 21 | **Redshift** | **委派 PG** | `dialect-redshift` | **15** | **✅ v3.6.0 新增** |

**总计：21 种方言**（8 独立 + 10 委派 + 3 v3.6.0 新增）

## 2. v3.7.0+ 候选方言

| 方言 | 优先级 | 实现方式 | 理由 |
|------|--------|----------|------|
| Trino/Presto | 中 | 独立实现 | 开源 OLAP 查询引擎，支持 ANSI SQL |
| Databricks | 中 | 独立实现 | Lakehouse 平台，Spark SQL 方言 |
| BigQuery | 中 | 独立实现 | Google Cloud 数仓，Standard SQL |
| Firebird | 低 | 独立实现 | 开源关系数据库，小众但稳定 |
| Informix | 低 | 独立实现 | IBM 商业数据库，小众 |
| Cassandra CQL | 低 | 独立实现 | NoSQL 宽列存储，CQL 语法 |
| Neo4j Cypher | 低 | 独立实现 | 图数据库，Cypher 查询语言 |

## 3. §6.7 更新

v3.6.0 方言覆盖度：
- **21 种方言**（从 v3.5.0 的 18 种增至 21 种）
- **云数仓支持**：Snowflake + Redshift（v3.6.0 新增）
- **PG 兼容家族**：PostgreSQL + KingbaseES + PolarDB + GaussDB + CockroachDB + YugabyteDB + Redshift = 7 种
- **MySQL 兼容家族**：MySQL + MariaDB + TiDB + OceanBase = 4 种
- **Oracle 兼容家族**：Oracle + Dameng = 2 种
- **SQL Server 兼容家族**：SQL Server + Sybase + GBase = 3 种
- **独立方言**：SQLite + ClickHouse + DuckDB + DB2 + Snowflake = 5 种

## 4. Feature Gate 汇总

| Feature | 版本 | 说明 |
|---------|------|------|
| `dialect-cockroachdb` | v3.5.0 | CockroachDB 方言 |
| `dialect-yugabytedb` | v3.5.0 | YugabyteDB 方言 |
| `dialect-snowflake` | v3.6.0 | Snowflake 方言 |
| `dialect-redshift` | v3.6.0 | Redshift 方言 |