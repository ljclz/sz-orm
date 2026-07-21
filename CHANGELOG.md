# Changelog

本文件记录 SZ-ORM 项目的所有重要变更。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
并遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [1.0.0] — 2026-07-19

### Added

- **核心引擎 (sz-orm-core)**：Model trait、QueryBuilder、多数据库方言（MySQL/PostgreSQL/SQLite/Oracle 23ai）、异步连接池、ACID 事务、文件迁移系统、多级缓存、统一值类型（20 种变体）、错误类型体系
- **数据库适配器**：sz-orm-sqlx（MySQL/PostgreSQL/SQLite/Oracle）、sz-orm-sql-validator（SQL 注入检测）
- **扩展生态包 (18 个)**：
  - sz-orm-crypto：AES-256-GCM、PBKDF2、HMAC-SHA256
  - sz-orm-auth：认证与授权
  - sz-orm-batch：批量 INSERT/UPDATE/UPSERT
  - sz-orm-dtx：分布式事务
  - sz-orm-mig：迁移工具
  - sz-orm-sharding：分库分表
  - sz-orm-cache：多级缓存
  - sz-orm-queue：消息队列
  - sz-orm-scheduler：任务调度
  - sz-orm-graphql：GraphQL 接口
  - sz-orm-grpc：gRPC 接口
  - sz-orm-ai：NL→SQL（自然语言转 SQL）
  - sz-orm-vector：pgvector 向量搜索
  - sz-orm-search：Meilisearch/Elasticsearch/OpenSearch 集成
  - sz-orm-storage：S3 兼容对象存储
  - sz-orm-postgis：PostGIS 地理空间
  - sz-orm-timeseries：时序数据
  - sz-orm-observability：Prometheus 指标 + OpenTelemetry tracing
  - sz-orm-tracing：分布式追踪（W3C TraceContext）
- **CLI (sz-orm-cli)**：命令行工具
- **DevTools**：sz-orm-swagger（OpenAPI）、sz-orm-health（健康检查）
- **测试体系**：3003 个单元/集成测试、proptest 属性测试、fuzz 模糊测试、chaos 混沌测试（16 项）、24h soak test
- **CI/CD**：GitHub Actions 多 workflow（CI/安全/soak test/依赖更新）
- **文档**：15 份中文文档 + README.en.md 英文文档 + CONTRIBUTING.md 贡献指南

### Security

- cargo audit 通过（1 allowed warning: paste unmaintained）
- cargo deny check advisories bans licenses sources 全部通过
- 24h Linux CI Soak Test（2026-07-21 立即触发）

### Performance

- 1h soak test：13.8 亿 operations，0 errors，1.16% throughput decay，43μs→41μs P99 latency
- 7 组 criterion 基准测试

## [Unreleased]

_暂无未发布变更。_

[1.0.0]: https://github.com/ljclz/sz-orm/releases/tag/v1.0.0
