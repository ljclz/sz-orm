# Changelog

本文件记录 SZ-ORM 项目的所有重要变更。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
并遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [2.0.0] — 2026-08-06

### Breaking Changes

- **移除 deprecated `where_cond` / `or_where`**：`FindWithRelatedBuilder::where_cond()`、`QueryBuilderExt::or_where()` 及相关 `where_conds` 字段已删除。迁移至 `where_eq` / `or_where_eq` 等参数化方法（自 v1.2.0 起已标记 deprecated，v2.0.0 正式移除）

### Added

- **Oracle 23ai 真实集成测试**：`integration_oracle.rs` 追加 7 类场景（CRUD / 事务 / 乐观锁 / 软删除 / 分页 / 聚合 / 批量），10 测试通过
- **SQL Server 真实集成测试**：`integration_mssql.rs` 新建 8 类场景（CRUD / 事务 / 乐观锁 / 软删除 / 分页 / 聚合 / 批量 / INSERT OR IGNORE 回退）
- **Python 绑定 (PyO3)**：`sz-orm-python` 0.1.0 发布到 crates.io，支持 Model / QueryBuilder / Pool / Transaction Python API
- **JavaScript 绑定 (napi-rs)**：`sz-orm-js` 0.1.0 发布到 crates.io，支持 Node.js 原生绑定
- **安全审计报告**：`docs/assessment/2026-08-05-security-audit-report.md`，7 维度覆盖（SQL 注入 / 连接池 / 密码 / 权限 / 输入校验 / 信息泄露 / 依赖安全）

### Changed

- **crates.io 批量发布**：42 个包发布 **2.0.0**（sz-orm-python / sz-orm-js 保持 0.1.0）
- **测试规模**：全 workspace 4,947 passed, 0 failed（lib 测试）
- **内部依赖版本对齐**：所有 `version + path` 格式的内部依赖统一至 2.0.0

## [1.5.0] — 2026-08-05

### Added

- **连接池 Prometheus 统计指标 (sz-orm-core)**：`Pool::pool_metrics()` 返回 `PoolMetrics`（acquire_count / acquire_failed_count / acquire_wait_time / release_count / connection_created_count / connection_closed_count），基于无锁 `AtomicU64`，热路径开销可忽略；`average_acquire_wait_time()` 计算平均获取等待时长
- **ClickHouse 行锁支持 (sz-orm-core)**：`supports_lock_for_update()` / `supports_lock_shared()` 显式返回 `false`（无事务无行锁）；`build_insert_or_ignore_prefix()` 回退普通 `INSERT INTO`
- **SQL Server INSERT OR IGNORE 回退 (sz-orm-core)**：`build_insert_or_ignore_prefix()` 回退普通 `INSERT INTO`（SQL Server 无等价前缀语法，应用层可捕获 2601/2627 冲突或使用 MERGE）
- **DuckDB 真实集成测试**：`integration_duckdb.rs` 7 个真实 DB 测试（duckdb bundled 特性）
- **向量/时序真实实现集成测试**：sz-orm-vector 3 个 `#[ignore]` 真实 pgvector 测试；sz-orm-timeseries 5 个内存集成测试 + 2 个 `#[ignore]` 真实 TimescaleDB 测试
- **Redis 后端默认启用 (sz-orm-core)**：`redis` feature 加入 `default`，`RedisBackend` 开箱即用

### Changed

- **crates.io**：sz-orm-core 发布 **1.5.0**（依赖 sz-orm-sql-validator 1.4.0 / sz-orm-macros 1.4.0）
- **测试规模**：全 workspace 5,809 passed, 0 failed

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
  - sz-orm-cache：多级缓存（注：实现在 sz-orm-core/src/cache.rs 与 l2_cache.rs 内，非独立 crate）
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
- **测试体系**：2,271 个单元/集成测试（1,635 `#[test]` + 636 `#[tokio::test]`）、proptest 属性测试、fuzz 模糊测试、chaos 混沌测试（16 项）、6h soak test
- **CI/CD**：GitHub Actions 多 workflow（CI/安全/soak test/依赖更新）
- **文档**：15 份中文文档 + README.en.md 英文文档 + CONTRIBUTING.md 贡献指南

### Security

- cargo audit 通过（1 allowed warning: paste unmaintained）
- cargo deny check advisories bans licenses sources 全部通过
- 6h Linux CI Soak Test（2026-07-21 立即触发）

### Performance

- 1h soak test：13.8 亿 operations，0 errors，1.16% throughput decay，43μs→41μs P99 latency
- 7 组 criterion 基准测试

## [Unreleased]

### Added

- **API 稳定性承诺文档**：新增 `docs/API-STABILITY.md`，明确 SemVer 承诺、API 稳定性三层分级（Stable/Experimental/Internal）、废弃流程（2 个 MINOR 版本保留期）、破坏性变更条件
- **端到端真实 DB 示例**：新增 `examples/src/bin/real_db_crud.rs`，使用 SQLite 内存数据库演示完整连接池 + CRUD + 事务（提交/回滚）流程
- **Prometheus 告警规则**：新增 `monitoring/alerts.yml`，覆盖错误率/延迟/连接池/SLO 燃烧率告警
- **文档清理**：删除 33 份开发期文档（审计报告/调研文档/重复副本），保留 19 份核心文档

### Fixed

- **sz-orm-search unreachable!() 消除**：将 `TokenizerType::Keyword` 的 `unreachable!()` 替换为正确的 `vec![text]`（整个文本作为单个 token）
- **README 测试数字不一致**：统一 README/README.en.md 中测试数从 2,271/4,959 → 5,404，版本号从 1.0.0 → 1.2.0
- **CI minio:latest 可变标签**：固定为 `minio/minio:RELEASE.2024-10-13T13-34-11Z`

## [1.2.0] — 2026-07-26

### Added

- **Oracle 独立适配器包 (sz-orm-oracle)**：基于 `oracle` crate (ODPI-C 绑定) 实现 `Connection` trait，支持 Oracle 12c+；阻塞池隔离、占位符自动转换、完整类型映射
- **SQL Server 独立适配器包 (sz-orm-mssql)**：基于 `tiberius` crate (纯 Rust TDS 协议) 实现 `Connection` trait，支持 SQL Server 2008+；占位符自动转换为 `@PN` 格式
- **axum Web 框架集成 (sz-orm-axum)**：提供 `PoolState`、`JsonRows`、`JsonResp<T>`、`transaction_layer` 组件
- **actix-web Web 框架集成 (sz-orm-actix)**：提供 `PoolState`、`JsonRows`、`JsonResp<T>`、`TransactionMiddleware` 组件
- **独立查询构建器 (sz-orm-query-builder)**：提供与 core `QueryBuilder` 不同的 fluent API，支持 SELECT/INSERT/UPDATE/DELETE 及 UNION/INTERSECT/EXCEPT 集合操作
- **DI 容器 (container.rs)**：依赖注入容器，支持构造函数注入和单例注册
- **ORM 迁移集成 (migrate.rs)**：sz-orm-mig 与 sz-orm-core 的集成层
- **Whoops 调试页面 (debug_page.rs)**：开发环境调试信息展示
- **API 版本管理 (api_version.rs)**：API 版本协商与路由
- **缓存预热 (cache_warmer.rs)**：启动时预加载热点数据到缓存
- **迁移历史表 (migration_history.rs)**：迁移执行记录持久化

### Changed

- **MSRV 升级**：1.80 → 1.81（trait_variant dyn compatibility 要求）
- **workspace lints 强制**：新增 `[workspace.lints]` 配置，全 workspace clippy 零警告强制执行
- **测试数增长**：4,959 → 5,404（+445），新增 soak test/Jepsen/kill-9 崩溃恢复测试

### Fixed

- **clippy writeln_empty_string**：修复 `sz-orm-audit` 中的 `writeln!(file, "")` 警告
- **clippy unnecessary_cast**：修复 `sz-orm-sqlx` 中的 `as i64` 不必要转换
- **hydration_plugin unwrap**：修复 `chars().next().unwrap()` 为安全错误处理
- **postgis partial_cmp unwrap**：替换为 `total_cmp` 实现 NaN 安全比较

## [1.1.0] — 2026-07-22

### Added

- **位置式查询优化 (query_values / query_values_with_params)**：为 `Connection` trait 新增两个高性能查询方法，绕过 HashMap 行映射开销。SQLite 提升 34.4%，Oracle 提升 57.4%
- **真实 MQ 客户端 (sz-orm-queue)**：新增 5 种真实消息队列客户端 — RabbitMQ/NATS/Kafka/ActiveMQ Artemis/Pulsar
- **全部 37 扩展包深度优化**：测试数从 2,271 增至 4,959（+2,688），每个包补充 200-500 行高级特性代码与 15-30 个单元测试
- **Connection trait 参数绑定**：新增 `execute_with_params`/`query_with_params`，MySQL/PostgreSQL/SQLite 实现真实 prepared statement 绑定
- **编译时类型推断完善**：`SqlType` 扩展至 13 种变体，`InferSqlType` trait 覆盖 14 种 Rust 类型
- **编译时 SQL schema 生成（`schema!` 宏）**：接受 SQL CREATE TABLE 语句，编译期自动生成类型安全查询代码
- **英文文档**：新增 `README.en.md` + `CONTRIBUTING.md`
- **ADR 体系**：9 个 ADR + `ADR与生产Bug定位规范.md`
- **SeaORM 迁移指南**：547 行，10 章 + 检查清单
- **Fuzz Testing**：3 个 fuzz target（query_builder/value_escape/pool_config）
- **PooledConnection Drop 修复**：连接在 drop 时自动归还池中
- **core 包 tracing 可观测性**：关键路径添加 `#[tracing::instrument]` 注解
- **学习路线图**：面向 PHP/ThinkPHP 工程师的 17 章学习教程

### Changed

- **Rust 工具链升级**：升级至 Rust 1.97.1
- **sqlx 升级**：0.8.6 → 0.9.0，消除 rsa Marvin Attack 漏洞

### Security

- **Critical 修复 (C-2/C-3)**：修复 2 个 Critical 安全漏洞
- **反向审计全量修复**：H-1 至 H-9（9 项 High）、M-1 至 M-17（17 项 Medium）、L-1 至 L-5（5 项 Low）全部修复
- **cargo audit / cargo deny 全通过**

### Fixed

- **hook 测试锁毒化**：替换为 `AtomicU32` 无锁计数器
- **SQLite 集成测试磁盘 I/O 错误**：改用 `open_in_memory()`
- **unreachable!() 消除**：简化 `sz-orm-postgis` `st_union` 的冗余嵌套 match

### CI

- **CI 基础设施非阻塞**：4 类外部依赖 job 设为 `continue-on-error: true`
- **integration.yml 独立工作流**：手动触发 + 每日定时
- **test job 解耦**：test 不再依赖 build

[1.2.0]: https://github.com/ljclz/sz-orm/releases/tag/v1.2.0
[1.1.0]: https://github.com/ljclz/sz-orm/releases/tag/v1.1.0
[1.0.0]: https://github.com/ljclz/sz-orm/releases/tag/v1.0.0
