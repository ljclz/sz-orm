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
- **测试体系**：2,271 个单元/集成测试（1,635 `#[test]` + 636 `#[tokio::test]`）、proptest 属性测试、fuzz 模糊测试、chaos 混沌测试（16 项）、24h soak test
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

### Added

- **真实 MQ 客户端 (sz-orm-queue)**：新增 5 种真实消息队列客户端实现 — RabbitMQ (lapin/AMQP 0.9.1)、NATS (async-nats)、Kafka (rdkafka)、ActiveMQ Artemis (AMQP 1.0)、Pulsar (pulsar crate)，覆盖 publish/consume/ack/subscribe 全流程
- **英文文档**：新增 `README.en.md` 英文版 README + `CONTRIBUTING.md` 贡献者指南，支持国际化协作
- **架构决策记录 (ADR)**：新增 ADR 文档、模块文档、生产事故 runbook
- **AI 渗透测试报告**：新增 AI 渗透测试报告 + 自定义 Semgrep SAST 规则，crates.io 发布准备
- **Dependabot 自动升级**：新增 Dependabot 配置，自动升级 GitHub Actions 依赖
- **GitHub Pages 文档**：自动构建并部署 API 文档到 GitHub Pages
- **学习路线图**：新增 `docs/sz-orm学习路线图.md`，L1-L4 分阶段学习指南（含按角色推荐路线和验收标准）
- **学习教程重写**：将学习路线图重写为**面向 PHP/ThinkPHP 工程师的具体学习教程**（17 章 + 3 附录），包含 Rust 速通（对照 PHP）、ThinkPHP ↔ sz-orm 逐概念对照、可运行代码示例、AI 协作开发姿势（提问模板/验证 5 步法/常见陷阱），修复失效文档链接（`sz-orm性能基准.md`、`docs/Security.md` 路径错误）
- **Benchmark 扩展**：新增 3 组 criterion 基准测试 — `query_builder_select`（3 种复杂度 SELECT 构建）、`query_builder_insert_update`（INSERT/UPDATE/DELETE 4 种操作）、`value_batch_to_param`（10/100/1000 批量转换），共 10 组
- **ADR 覆盖率扩展**：新增 4 个 ADR（0006-0009）覆盖关联关系加载三策略、ResultMap 分组聚合、连接池持锁不 await close、QueryBuilder 只生成 SQL 不执行，每个 ADR 含 Bug 定位提示，将 ADR 从决策记忆扩展到 bug 定位辅助
- **core 包 tracing 可观测性**：为 sz-orm-core 关键路径添加 `#[tracing::instrument]` 注解 — 连接池（acquire/release/close_all/reap_idle）、QueryBuilder（build_select/build_insert/build_update/build_delete）、关联加载（load_eager/load_join/find_with_related_eager_sql/find_with_related_subquery）、结果映射（apply_result_map/apply_result_map_many），生产 bug 可通过 tracing span 定位
- **可复用规范提炼**：新增 `docs/ADR与生产Bug定位规范.md`（v1.0），从 SZ-ORM 实测经验提炼 — ADR 写作四段式（含 Bug 定位提示段必填）、ADR 覆盖率标准（密度 ≥ 0.15）、运行时可观测性规范（关键路径必须加 tracing）、四层 bug 定位流程（决策层→运行时层→指标层→代码层）、ADR 有效性验证流程（零上下文子代理测试 + bug 定位命中率测试）、工程化门禁（PR 检查清单 + CI 门禁），含 ADR 模板/Bug 定位报告模板/新项目落地清单 3 个附录

### Changed

- **Rust 工具链升级**：升级至 Rust 1.97.1，同步全面工程化审计
- **sqlx 升级**：sqlx 0.8.6 → 0.9.0，消除 rsa Marvin Attack 漏洞
- **文档数据统一**：统一测试数 2,271 / LOC ~104,000 / 文档数 11，消除文档间数据矛盾（与实测一致）
- **8 项工程改进落地**：基于 2026-07-21 全面审计的 8 项未来改进建议（1-7）全部实施

### Security

- **Critical 修复 (C-2/C-3)**：修复 2 个 Critical 级别安全漏洞，新增 `SECURITY.md` + `CODEOWNERS` 文件
- **反向审计全量修复**：完成 H-1 至 H-9（9 项 High）、M-1 至 M-17（17 项 Medium）、L-1 至 L-5（5 项 Low）全部修复
- **文档敏感信息清除**：清除文档中所有敏感信息（连接字符串、密钥等）
- **cargo audit / cargo deny 全通过**：advisories / bans / licenses / sources 四项全部通过

### Fixed

- **hook 测试锁毒化**：`RwLock<HashSet<String>>` 在 panic 后锁毒化导致静默失败，替换为 `AtomicU32` 无锁计数器
- **SQLite 集成测试磁盘 I/O 错误**：CI Ubuntu runner 磁盘空间不足导致文件模式 `disk I/O error`，改用 `open_in_memory()`
- **CI Feature Matrix 原生依赖缺失**：添加 `protobuf-compiler` (pulsar) + `cmake` (rdkafka) 原生依赖安装
- **CI Semgrep SARIF 上传权限缺失**：添加 `permissions: security-events: write`
- **CI cargo fmt 格式化失败**：修复长行格式问题
- **MySQL 9.7 CI 失败**：减少邮件噪音，修复 MySQL 9.7 兼容性
- **6 个 Cargo.toml description 缺失**：补全 sz-orm-audit/graphql/health/logger/masking/swagger 的 description 字段
- **README 8 个失效文档链接**：文档索引缩减为 6 条（仅 git 跟踪文件）
- **unreachable!() 消除**：简化 `sz-orm-postgis` `st_union` 的冗余嵌套 match，消除 `unreachable!()` panic 风险
- **typed_ast TODO 完成**：为 `Literal<i64>/Literal<String>/Literal<bool>` 分别实现 `TypedExpression`，派生正确的 `SqlType`（Integer/Text/Bool），替换原统一 Text 标记
- **dialect TODO 完成**：将 `build_create_table` 重复代码 TODO 转为正式架构说明注释，记录权衡决策

### CI

- **CI 基础设施非阻塞**：将依赖外部 Docker 镜像/第三方工具的 4 类 job（feature-matrix/integration/mq-integration/coverage）设为 `continue-on-error: true`，使其失败不阻塞核心 CI
- **integration.yml 独立工作流**：移除 push/PR 触发，改为手动触发 + 每日定时，修复 MinIO 标签和健康检查
- **security.yml 修复**：补全 cargo-deny 安装步骤
- **test job 解耦**：test 不再依赖 build，加速 CI 反馈

[1.0.0]: https://github.com/ljclz/sz-orm/releases/tag/v1.0.0
