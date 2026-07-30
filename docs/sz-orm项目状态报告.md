# sz-orm 项目状态报告（统一版）

> **报告版本**：v1.0（合并版）
> **合并日期**：2026-07-30
> **当前 crate 版本**：v1.2.1
> **数据基准**：以最新数据为准（v1.2.1 / 43 包 / 5442+ 测试 / 6h soak / 6 个 fuzz targets / crates.io 已发布 41 包）
> **报告说明**：本报告由 5 份历史审查报告合并去重而成，不创建新的分析结论，仅整合已有内容并标注数据来源

---

## 一、项目概览

### 1.1 核心定位

**SZ-ORM（鲜视达 ORM）** 是一个 Rust 实现的企业级 ORM 框架，核心定位为「SQL 生成器 + 抽象连接池框架」：

- `sz-orm-core` 保持纯粹抽象层（0 处使用 sqlx），提供方言/模型/查询/连接池/事务/迁移/缓存/钩子等核心能力
- `sz-orm-sqlx` 提供 sqlx 适配器，让抽象 Pool/Connection 层端到端连接真实 MySQL/PG/SQLite/Oracle
- 通过 feature flag 隔离编译：默认编译保留内存实现，启用 feature 时引入真实 SDK

*数据来源：成熟度评估报告 §3.1、生产就绪报告 §1.2*

### 1.2 项目规模

| 维度 | 当前数值 | 数据来源 |
|------|---------|---------|
| **crate 版本** | **v1.2.1** | 任务基准（最新） |
| **工作空间成员数** | **43**（41 个 sz-orm-* lib + cli + examples） | 成熟度评估报告 §1（已含 sz-orm-vector） |
| **Rust 源文件数** | 135+ | 成熟度评估报告 §1 |
| **总代码行数（src/）** | ~115,000 LOC | 成熟度评估报告 §1 |
| **总代码行数（含测试）** | ~139,000 LOC | 成熟度评估报告 §1 |
| **核心包 sz-orm-core 模块数** | 18+ | 成熟度评估报告 §1 |
| **数据库方言数** | 7 独立 + 13 协议兼容 | 真实审查报告 §1 |
| **DbError 变体数** | 21（DB001-DB021） | 成熟度评估报告 §1 |
| **Value 类型变体数** | 20 | 成熟度评估报告 §1 |
| **中文文档** | 14 份完整 | 真实审查报告 §1 |
| **英文文档** | README.en.md 完整 | 真实审查报告 §1 |

**7 独立方言**：MySQL / PostgreSQL / SQLite / Oracle / SqlServer / ClickHouse / DB2
**13 协议兼容**：MariaDB/TiDB/PolarDB/GaussDB（MySQL 协议）+ 达梦/人大金仓/GBase/Sybase（PG/SqlServer 协议）

### 1.3 内部生产使用情况

**已有内部生产案例**：sz-rust 项目依赖 18 个 sz-orm 包（包括 sz-orm-core / sz-orm-auth / sz-orm-crypto / sz-orm-storage / sz-orm-queue / sz-orm-mqtt / sz-orm-websocket / sz-orm-scheduler / sz-orm-tracing / sz-orm-logger / sz-orm-audit / sz-orm-health / sz-orm-masking / sz-orm-swagger / sz-orm-limit / sz-orm-config / sz-orm-macros / sz-orm-sql-validator），业务代码实际使用 `use sz_orm_core::{Model, ModelExt, TimestampFields}`。

*数据来源：真实审查报告 §2*

### 1.4 外部试点

**sz-pay 已确认作为外部试点项目**（v1.2.1 已发布到 crates.io，外部项目可直接通过 cargo 引入）。

*数据来源：任务基准（最新状态）*

---

## 二、质量基线

### 2.1 编译与静态分析

| 项 | 命令 | 结果 | 数据来源 |
|----|------|------|---------|
| 编译 | `cargo build --workspace` | ✅ 通过，**0 errors** | 生产就绪报告 §2.1 |
| 严格 lint | `cargo clippy --workspace --all-targets` | ✅ **0 warnings**, 0 errors | 生产就绪报告 §2.1 |
| API 文档 | `cargo doc --workspace --no-deps` | ✅ 全部包文档生成 | 生产就绪报告 §2.1 |
| 格式化 | `cargo fmt --all -- --check` | ✅ 通过 | 生产就绪报告 §2.1 |
| `unimplemented!` / `todo!` / `FIXME` | — | **0 处** | 成熟度评估报告 §1 |
| 生产代码 `panic!` | — | **0 处** | 成熟度评估报告 §1 |
| 生产代码 `unwrap()` / `expect()` | — | **0 处**（已全部降级为 match/Result） | 成熟度评估报告 §1 |

### 2.2 测试规模

| 测试维度 | 数值 | 数据来源 |
|---------|------|---------|
| 测试套件数 | **112** | 成熟度评估报告 §2 |
| 通过测试 | **5,442** | 成熟度评估报告 §2 |
| 失败测试 | **0** | 成熟度评估报告 §2 |
| 忽略测试 | 79（真实云服务/外部凭证环境依赖，CI 默认不运行，非代码问题） | 成熟度评估报告 §2 |
| 文档测试通过 | 8+ | 成熟度评估报告 §2 |
| Fuzz targets | **6 个** | 任务基准（最新） |

### 2.3 测试维度覆盖

- **单元测试**：核心包 146+ + 各扩展包单元测试 + P3+ 新增 ~280+
- **集成测试**：fuzz 11 + jepsen 29 + stress 12 + chaos 16 + formal 14 + core 13
- **真实云 DB 集成**：SQLite 11 + MySQL 12 + PG 12 + sqlx 适配器 16
- **真实云 DB Jepsen**：10（MySQL 5 + PG 5，云端实测）
- **真实云 DB Pool/Tx**：12（MySQL 5 + PG 5 + SQLite 2）
- **真实云服务测试**：MQTT 4 + WebSocket 3 + RabbitMQ 4 + S3 5（共 16 可运行 + 9 ignored）
- **真实 AI/gRPC/GraphQL 测试**：sz-orm-ai real 4 通过 + 2 ignored；sz-orm-grpc real 4 ignored；sz-orm-graphql real 4 通过
- **P3+ 新增测试维度**：typed_ast 25+ / dynamic_sql 30 / find_with_related 20+ / json_query 30+ / hooks 40+ / Saga 25+ / TCC 32 / 跨分片 22 / enhanced 66
- **Soak Test**：3 单元 + 1 主 soak（24h）+ 1 冒烟（10s）
- **可观测性测试**：MetricsRegistry 5 + SloMonitor 5 + 2 doctest + tracing OTLP 83
- **生态扩展测试**：postgis 35 + timeseries 24 + search 50 = 109
- **AI 增强测试**：sz-orm-vector pgvector + NL→SQL + SQL 安全验证

*数据来源：成熟度评估报告 §2*

### 2.4 性能基准（超大数据量）

| 数据库 | 操作 | 吞吐量 | 环境 |
|--------|------|--------|------|
| SQLite | 10 万行批量 INSERT | 72 万行/s | 本机 |
| PostgreSQL 18 | 10 万行批量 INSERT | 26.8 万行/s | 本机 |
| MySQL 9.6 | 10 万行批量 INSERT | 14.5 万行/s | 本机 |
| Oracle 23ai | — | 1.91 万行/s | 本机 |
| PostgreSQL | 10 万行批量 INSERT | 4.11 万行/s | 远程云 |
| MySQL 8.x | 10 万行批量 INSERT | 2.57 万行/s | 远程云 |

并发压测：8 任务 × 1 万次连接池 acquire/release，无泄漏、无死锁。

*数据来源：成熟度评估报告 §3.2、生产就绪报告 §2.4*

---

## 三、安全审计汇总

### 3.1 AI 安全审计（22 项发现）

**审计基准**：v1.2.0（Cargo.lock 779 个 crate 依赖）
**审计日期**：2026-07-30
**审计方法**：源代码逐行扫描（Grep/Read）+ `cargo audit` 依赖漏洞扫描 + 配置文件审查

| 级别 | 数量 | 占比 | 编号 |
|------|------|------|------|
| 🔴 Critical | 3 | 14.3% | C-1 / C-2 / C-3 |
| 🟠 High | 3 | 14.3% | H-1 / H-2 / H-3 |
| 🟡 Medium | 7 | 33.3% | M-1 ~ M-7 |
| 🟢 Low | 5 | 23.8% | L-1 ~ L-5 |
| ℹ️ Info | 4 | 19.0% | I-1 ~ I-4 |
| **合计** | **22** | 100% | |

**Critical 详情（3 项，全部为 PRNG 不安全）**：

| 编号 | 文件 | 问题 | 状态 |
|------|------|------|------|
| C-1 | `sz-orm-auth/src/mfa.rs:263-279` | MFA 密钥使用 `DefaultHasher` + 纳秒时间戳种子（可预测，绕过 TOTP） | ✅ 已修复（替换为 OsRng） |
| C-2 | `sz-orm-auth/src/oauth2.rs:264-272` | OAuth2 授权码使用 `DefaultHasher`（违反 RFC 6749 §10.10） | ✅ 已修复（替换为 OsRng） |
| C-3 | `sz-orm-auth/src/token_store.rs:466-474` | 令牌家族 ID 使用 `DefaultHasher`（重放检测绕过风险） | ✅ 已修复（替换为 OsRng） |

**High 详情（3 项）**：

| 编号 | 文件 | 问题 | 状态 |
|------|------|------|------|
| H-1 | `sz-orm-auth/src/auth.rs:155-180` | JwtAuthenticator 未配置验证器时接受任意凭证（不安全默认配置） | ✅ 已修复 |
| H-2 | Cargo.lock（传递依赖） | quick-xml 0.38.4 DoS（RUSTSEC-2026-0194/0195，仅 s3-sdk feature 引入） | ⚠ 待上游升级（feature-gated） |
| H-3 | `sz-orm-batch/src/lib.rs:170-172` | `quote()` 未转义反引号（潜在 SQL 注入） | ✅ 已修复 |

**Medium（7 项）**：M-1 rsa Marvin Attack（传递依赖）/ M-2 rustls-webpki 证书缺陷（传递依赖）/ M-3 rand 0.7.3 unsound（传递依赖）/ M-4 GraphQL 无深度限制 / M-5 graphql RwLock `.expect()` / M-6 JWT HMAC `.expect()` / M-7 AI HTTP 无超时/SSRF 防护

**Low（5 项）**：L-1 JWT user_id 回退为 0 / L-2 MFA 错误泄露 user_id / L-3 lc generate_ddl 未校验模型名 / L-4 AI HTTP 无超时 / L-5 TOTP 使用 HMAC-SHA1

**Info（4 项）**：I-1 paste 未维护 / I-2 rustls-pemfile 未维护 / I-3 令牌存储内存实现 / I-4 配置中心内存实现

### 3.2 安全正面发现

| 项目 | 状态 |
|------|------|
| 0 unsafe 生产代码 | ✅ 优秀（unsafe 仅出现在注释/doc-test） |
| 0 process::exit | ✅ 优秀 |
| 0 Command::new | ✅ 优秀 |
| 0 from_utf8_unchecked/transmute/raw pointer | ✅ 优秀 |
| 0 硬编码生产凭证 | ✅ 优秀（"secret"/"minioadmin" 均在测试代码中） |
| 0 .env 文件 | ✅ 优秀 |
| JWT 常量时间签名比较 | ✅ 优秀（`subtle::ConstantTimeEq`） |
| JWT 算法校验 | ✅ 优秀（拒绝非 HS256 算法，防 alg confusion） |
| AES-256-GCM + OsRng nonce | ✅ 优秀 |
| PBKDF2 密码哈希 | ✅ 良好（100k 迭代） |
| OAuth2 redirect_uri 精确匹配 | ✅ 优秀 |
| 令牌轮换 + 重放检测 | ✅ 优秀 |
| cargo audit + cargo deny CI 配置 | ✅ 优秀（忽略项有文档说明） |
| 数据脱敏 | ✅ 优秀（masking 包覆盖 10 种敏感类型） |
| 限流器 | ✅ 良好（滑动窗口 + max_keys 防护） |

*数据来源：AI 安全审计报告 §3、§4*

### 3.3 CI 安全工具链

| 工具 | 内容 | 状态 |
|------|------|------|
| **Semgrep** | `.semgrep/rust-security.yaml` 16 条 Rust 安全规则 | ✅ 已完成 |
| **CodeQL** | `.github/workflows/codeql.yml` | ✅ 已完成 |
| **cargo-audit** | 漏洞公告扫描（RustSec advisory-db，1173 条公告），7 个 RUSTSEC 已忽略并记录原因 | ✅ CI 集成 |
| **cargo-deny** | advisories / bans / licenses / sources 四维度 | ✅ CI 集成 |

*数据来源：真实审查报告 §3、AI 安全审计报告 §1.3*

### 3.4 安全评级

**总体评级**：B-（良好，需修复 Critical 问题）

| 维度 | 评分 | 说明 |
|------|------|------|
| 核心 ORM 安全 | A | 0 unsafe、参数化查询、标识符校验 |
| 认证模块安全 | D | 3 个 Critical（PRNG 不安全）、1 个 High（默认配置不安全） |
| 加密模块安全 | A | AES-256-GCM、PBKDF2、OsRng、常量时间比较 |
| 依赖安全 | B | 10 个漏洞但多为传递依赖且 feature-gated，忽略配置有文档 |
| 配置安全 | A | 0 硬编码凭证、0 .env、cargo audit/deny CI 集成 |
| 并发安全 | B+ | 大部分 RwLock 已修复，graphql/extensions.rs 仍有 expect |
| 错误处理 | B | 0 生产 panic!，少量 .expect() 残留 |

*数据来源：AI 安全审计报告 §6*

---

## 四、功能完成度

### 4.1 P0-P4 所有问题状态

#### 第一轮审查（24 项修复，2026-07-20）

| 优先级 | 编号 | 问题 | 状态 |
|--------|------|------|------|
| **P0** | P0-1 ~ P0-8 | 6 Critical SQL 注入 + ES/OpenSearch `_source` 字段名 bug + RealPg/RealTimescale `new()` 编译问题 + JWT 时序攻击（H-3） | ✅ 全部已解决 |
| **P1** | P1-1 ~ P1-5 | H-1 escape_string 不一致 + H-2 data_permission bracket depth + RealPg ST_Union/ST_Buffer 真实实现 + Meilisearch index_doc _id + CI real-* feature 编译 | ✅ 全部已解决 |
| **P2** | P2-1 ~ P2-5 | SloMonitor 4 窗口 + SzTracer W3C TraceContext + SoakSnapshot thread_count + CI Linux soak test + jwt.rs audited crate | ✅ 全部已解决 |
| **P3** | P3-1 ~ P3-4 | 对比文档 SeaORM 错误 + 统一包数/LOC 数据 + lib.rs 倒排索引注释 + 评估报告 Soak 平台限制 | ✅ 全部已解决 |

#### 第二轮审查（16 个新发现，2026-07-20）

| 优先级 | 编号 | 问题 | 状态 |
|--------|------|------|------|
| **P0** | P0-9 | timeseries continuous_aggregate query 参数 SQL 注入（新发现） | ✅ 已解决（新增 `validate_continuous_aggregate_query()` + 23 个测试） |
| **P1** | P1-4-fix | Meilisearch `create_index` primary_key 遗漏 | ✅ 已解决 |
| **P2/P3** | 14 项 | 注释/文档/数据一致性问题（P3-2-cmp/P3-2-ready/P3-2-arch/P3-2-mature/P3-2-sec/P3-2-progress/P3-2-api/P3-2-tech/P3-4-fix/P3-3-doc 等） | ✅ 全部已解决 |

#### v1.0.0 阶段十九（sqlx 0.9.0 升级）

- **rsa Marvin Attack (RUSTSEC-2023-0071) 已彻底消除**：rsa 从依赖树中完全移除
- 剩余 9 个漏洞均来自可选 feature（s3-sdk/real-broker/real-es），默认编译不受影响
- sqlx 0.8.6 → 0.9.0 升级（100 处代码适配）
- Rust 工具链 1.90.0 → 1.97.1

#### AI 安全审计 P0/P1 修复（2026-07-30）

| 优先级 | 编号 | 问题 | 状态 |
|--------|------|------|------|
| **P0** | C-1/C-2/C-3 | MFA 密钥/OAuth2 授权码/令牌家族 ID PRNG 不安全 | ✅ 已修复（替换为 OsRng） |
| **P0** | H-1 | JwtAuthenticator 不安全默认配置 | ✅ 已修复 |
| **P1** | H-3 | batch `quote()` 未转义反引号 | ✅ 已修复 |
| **P1** | M-4/M-5/M-6/M-7 | GraphQL 深度限制/RwLock expect/JWT HMAC expect/AI HTTP 超时 | ✅ 已修复 |
| **P2** | H-2/M-1/M-2/M-3 | 传递依赖漏洞（feature-gated） | ⚠ 待上游升级 |
| **P2** | L-1 ~ L-5 | 提权/用户枚举/代码生成注入/超时/HMAC-SHA1 | ⚠ 待修复 |
| **P3** | I-1 ~ I-4 | 未维护依赖/内存实现 | ℹ️ 信息性，无需修复 |

*数据来源：全面审查报告 v1 §5-7、AI 安全审计报告 §7、真实审查报告 §4*

### 4.2 假实现清零

**所有虚假/伪实现问题已全部修复**：

| 编号 | 问题 | 修复版本 | 修复说明 |
|------|------|---------|---------|
| V-1 | RealPg `st_union` 完全不执行 SQL | v0.2.2 (2026-07-20) | 改为参数化查询 + `Geometry::from_ewkt()` 真实解析 |
| V-2 | RealPg `st_buffer` SQL 执行但丢弃结果 | v0.2.2 | 改为 `query_ewkt()` 调用，真实解析 EWKT 返回 |
| V-3 | ES/OpenSearch `get_doc` 字段名 bug（`source` → `_source`） | v0.2.2 | 两处 `.get("source")` 改为 `.get("_source")` |
| V-4 | RealPg/RealTimescale `new()` 疑似无法编译 | v0.2.2 | 改用 `tokio::sync::OnceCell<Client>` 延迟连接 |
| V-5 | Meilisearch `index_doc` 忽略 `_id` 参数 | v0.2.2 | 注入 id 到文档 + 显式 primary_key="id" |
| V-6 | SloMonitor 仅 2 窗口，非声称 4 窗口 | v0.2.2 | 完全重写为 4 窗口 Google SRE 标准 |
| V-7 | SoakSnapshot 缺失 `thread_count` 字段 | v0.2.2 | 新增字段 + `read_thread_count()` 函数 + 3 个新测试 |
| V-8 | 3 包 real-* feature 从未在 CI 编译/测试 | v0.2.2 | ci.yml 添加 `real-features-compile` job |
| S-1 | Search Memory 自称"倒排索引"实为线性扫描 | P3-3 | 注释统一改为"线性扫描 + 子串匹配（无倒排索引）" |
| S-6 | SzTracer 使用自定义头（非 W3C） | v0.2.2 | 改为 W3C `traceparent` header，保留 legacy 向后兼容 |

**简化/名实不符项**：S-2 ~ S-5 / S-8 共 5 项为生态包 Memory 实现的功能限制（如 Memory `st_union` 仅 Point-Point、`parse_bucket` 仅支持单字符单位），已明确标注为简化实现，非虚假声明。

*数据来源：全面审查报告 v1 §2*

### 4.3 核心功能清单

#### 核心包能力（sz-orm-core）

| 能力 | 状态 | 说明 |
|------|------|------|
| SQL 生成（dialect/query/migration） | ✅ 100% | SQL 注入防护 fuzz 验证 + 真实 DB 集成 |
| 连接池（pool） | ✅ 100% | sz-orm-sqlx 适配器端到端验证 |
| 事务（transaction） | ✅ 100% | 真实 DB savepoint 20 层嵌套 + 隔离级别测试 |
| Dialect 矩阵 | ✅ 100% | MySQL/PG/SQLite/Oracle 23ai 4 种方言完整 |
| 钩子系统（hooks） | ✅ 100% | 16 事件 + HookDispatcher + 40+ 测试 |
| 强类型 AST（typed_ast） | ✅ 100% | Diesel 风格 ZST + 编译期类型约束 + 25+ 测试 |
| 动态 SQL（dynamic_sql） | ✅ 100% | rbatis 风格 XML 模板（if/where/set/foreach/choose/trim）+ 30 测试 |
| find_with_related | ✅ 100% | SeaORM 风格关联查询（JOIN/子查询/eager load）+ 20+ 测试 |
| JSON 查询（json_query） | ✅ 100% | MySQL/PG/SQLite 三方言 + 30+ 测试 |

#### 分布式事务（sz-orm-dtx）

| 模式 | 状态 | 测试 |
|------|------|------|
| 2PC | ✅ 100% | 22 单元测试 |
| TCC | ✅ 100% | 32 单元 + 1 doctest（7 状态机 + retry_confirm/retry_cancel） |
| Saga | ✅ 100% | 20+ 单元 + 1 doctest（Orchestration + 反向补偿） |
| 跨分片 ACID | ✅ 100% | 22 单元 + 1 doctest（2PC 协调 + 按 shard 分组合并） |

#### 扩展包生态（共 41 个 sz-orm-* lib）

| 分类 | 包名 | 状态 |
|------|------|------|
| 适配器 | sz-orm-sqlx | ✅ MySQL/PG/SQLite/Oracle |
| 校验 | sz-orm-sql-validator | ✅ 12 种注入模式 + 23 测试 |
| 宏 | sz-orm-macros | ✅ `sql_string!` + `query!` 编译时 SQL 检查（5 种数据库连真 DB 验证） |
| AI | sz-orm-ai | ✅ embedding + vector + RAG + OpenAI 兼容 API |
| AI | sz-orm-vector | ✅ pgvector 向量数据库（cosine/euclidean/dot）+ NL→SQL |
| 实时通信 | sz-orm-mqtt / sz-orm-websocket | ✅ rumqttc 0.25 + tokio-tungstenite 0.30 |
| 存储 | sz-orm-storage | ✅ 7 provider + S3SdkStorage（rust-s3 0.37） |
| 队列 | sz-orm-queue | ✅ 6 provider + LapinRabbitmqQueue（lapin 4.10） |
| 认证 | sz-orm-auth | ✅ RustCrypto JWT HS256 + OAuth2 + MFA + TokenStore |
| 加密 | sz-orm-crypto | ✅ RustCrypto（sha2/hmac/aes-gcm/pbkdf2/subtle/OsRng） |
| 调度 | sz-orm-scheduler | ✅ Cron（秒级支持）+ 76 测试 |
| 可观测 | sz-orm-tracing / sz-orm-observability | ✅ OTLP + P50/P95/P99 + SLO 燃烧率 + MetricsRegistry + SloMonitor 4 窗口 |
| 限流 | sz-orm-limit | ✅ 令牌桶/滑动窗口/漏桶 |
| 分布式 | sz-orm-dtx | ✅ 2PC + TCC + Saga + 跨分片 ACID |
| 高可用 | sz-orm-rw / sz-orm-sharding | ✅ 读写分离 + 分片（Hash/Range/Date + 一致性哈希 + 复合分片） |
| 生态扩展 | sz-orm-postgis / sz-orm-timeseries / sz-orm-search | ✅ PostGIS 空间 + TimescaleDB 时序 + ES/OS/Meilisearch 全文搜索 |
| 工具 | cli | ✅ 8 命令 + generate entity 反向工程 |
| 示例 | examples | ✅ 8 个示例（含 production_app + production_dtx 两大生产案例） |

**总体完成度**：**100%**（全部 43 个 workspace 成员均已达到生产可用基线）

*数据来源：成熟度评估报告 §5、生产就绪报告 §1.2/§3*

---

## 五、生产就绪评估

### 5.1 成熟度评分

**综合成熟度评分**：**4.98 / 5（CMMI 5 级 — 持续优化级）**

| 维度 | 评分 | 说明 |
|------|------|------|
| 代码质量 | 5.0/5 | clippy 0 警告 + fmt + 0 panic + 0 expect + RustCrypto + SQL 编译时检查 + ActiveRecord + hooks 16 事件 + 强类型 AST + 五维审查修复 11 个 Critical |
| 测试覆盖 | 4.97/5 | 5,442 测试（含单元/集成/Jepsen/Fuzz/Stress/Chaos/Contract/真实云 DB/1h Soak/Property-Based 22 个），112 个测试套件全部通过。扣分：7×24h soak test 待跑 |
| 功能完成度 | 5.0/5 | 43 workspace 成员全部 100% + sqlx + SQL 验证 + 4 云服务 + Oracle + ActiveRecord + hooks 16 事件 + AI/gRPC/GraphQL real 实现 + 强类型 AST + 动态 SQL + Saga/TCC/跨分片 + JSON 查询 + find_with_related + 分片增强 + SoakMonitor + MetricsRegistry + SloMonitor + OTLP + Grafana 仪表盘 + PostGIS + TimescaleDB + 多 provider Search + sz-orm-vector（pgvector + NL→SQL + SQL 安全验证） |
| 安全性 | 4.97/5 | RustCrypto + constant_time_eq + SQL 注入检测（12 种模式）+ cargo-audit/deny + 修复 S-1/S-2/S-3 三个安全 Critical + 门禁 9 修复 8 处 SQL 注入 + sqlx 0.9.0 升级消除 rsa Marvin Attack |
| 文档 | 5.0/5 | 43 包 cargo doc + ~400 行 lib.rs doc + README + 使用指南 + API 参考 + 架构设计 + 性能基准 + 进度表 + 评估报告 + 对比文档 + 5 个规范文档 |
| 生态完整度 | 5.0/5 | 43 包（41 sz-orm-* lib + cli + examples） |
| 性能 | 5.0/5 | SQLite 72 万行/s、PG 26.8 万行/s、MySQL 14.5 万行/s、Oracle 1.91 万行/s |

**扣分项**（共 -0.02，总分 5.0 - 0.02 = 4.98）：
- **-0.01**：无生产案例（唯一非环境依赖项短板，v1.2.1 已发布 crates.io + sz-pay 外部试点待累积数据）
- **-0.005**：Soak Test 1h 实际运行已通过，24h/6h 长稳态 CI 任务待积累完整周期数据
- **-0.0025**：3 个安全 Critical（JWT 时序攻击 / RateLimiter OOM / TCC 数据一致性）需生产环境持续验证（rsa Marvin Attack 已消除，扣分项从 -0.005 减半至 -0.0025）

*数据来源：成熟度评估报告 §8*

### 5.2 CMMI 成熟度等级

```
[1 初始] ─── [2 受控] ─── [3 已定义] ─── [4 量化管理] ─── [5 持续优化]
                                                              ▲ 当前
```

**等级：5 级（持续优化级）**

### 5.3 生产就绪结论

**就绪度**：**原型阶段 → 接近 GA**（v1.2.1 已发布 crates.io + 外部试点 sz-pay 确认）

#### 适合的应用场景

- ✅ 互联网应用后端（CMS、电商、社交）
- ✅ 内部企业系统（ERP、CRM、OA）
- ✅ 中等规模数据分析与报表
- ✅ IoT 设备数据接入（MQTT 真实对接）
- ✅ 实时通信后端（WebSocket 真实对接）
- ✅ 对象存储场景（S3/阿里云/腾讯云/华为云/七牛/又拍云/本地）
- ✅ 消息队列场景（RabbitMQ 真实对接 + Kafka/NATS/ActiveMQ/RocketMQ/Pulsar 抽象）
- ✅ 医疗记录系统（RustCrypto 审计栈 + 审计日志 + 脱敏 + cargo-audit/deny）
- ✅ 涉及秒级调度的业务（scheduler bug 已修复）
- ✅ 跨分片订单系统（CrossShardCoordinator 2PC 协调）
- ✅ 多步骤长流程业务（Saga 补偿模式）
- ✅ 多租户 SaaS 系统（TenantModel 全局作用域 + 16 种 HookEvent 审计）
- ✅ 复杂动态查询场景（动态 SQL XML 模板 + JSON 字段查询 + 强类型 AST）
- ⚠️ 金融交易系统：相关模块已实现（灾备 + SLA + Chaos + Formal + TCC/Saga）但未经生产验证，不建议直接使用

#### 谨慎应用的场景

- ⚠️ 直接替换 Diesel/SQLx 的存量生产系统（建议先在试点项目如 sz-pay 验证）
- ⚠️ 大规模分布式数据库（分片仅做内存实现，sz-orm-sharding 可作为 future enhancement）

### 5.4 剩余短板

| 短板 | 状态 | 影响 |
|------|------|------|
| crates.io 发布 | ✅ **已解决**（v1.2.1 已发布 41 包） | 外部用户可直接通过 cargo 引入 |
| 外部生产案例 | ⏳ 进行中（sz-pay 试点确认） | 0.01 扣分项待恢复 |
| 6h Soak 长稳态数据 | ⏳ 进行中（GitHub Actions 已改为 6h） | 0.005 扣分项待恢复 |
| cargo-fuzz 覆盖率 | ⏳ 待增加（当前 6 个 fuzz targets） | 非扣分项 |
| 安全 Critical 生产验证 | ⏳ 待生产流量验证 | 0.0025 扣分项待恢复 |
| 传递依赖漏洞（H-2/M-1/M-2/M-3） | ⚠ 待上游升级（feature-gated，默认编译不受影响） | 非扣分项 |
| Low 级别安全问题（L-1 ~ L-5） | ⚠ 待修复 | 非扣分项 |

### 5.5 扩展能力模块清单（金融级能力）

| 能力 | 状态 | 说明 |
|------|------|------|
| 灾备演练 | ✅ | sz-orm-back 备份恢复 + 降级预案 + 64 测试 |
| SLA 监控 | ✅ | sz-orm-tracing P50/P95/P99 + SLO 燃烧率 + 83 测试 |
| 混沌工程 | ✅ | Chaos 16 项（网络分区/磁盘满/时钟漂移/主从切换） |
| 形式化验证 | ✅ | Formal 14 项不变量 |
| 安全审计 | ✅ | cargo-audit + cargo-deny + security.yml CI，0 个未忽略漏洞 |
| 真实 DB Jepsen | ✅ | MySQL 5 + PG 5 共 10 项 |
| 真实云服务 | ✅ | MQTT + WebSocket + RabbitMQ + S3 |
| 生产代码 0 panic | ✅ | sharding 改为 Result，13 处 lock poisoned 改为降级 |
| RustCrypto 审计栈 | ✅ | sz-orm-crypto + sz-orm-auth 均使用 RustCrypto |
| SQL 注入检测 | ✅ | sz-orm-sql-validator + 12 种注入模式 + `sql_string!` 编译时检查 |

*数据来源：成熟度评估报告 §8、生产就绪报告 §5/§6*

---

## 六、Soak 测试状态

### 6.1 当前状态

**当前 soak 测试周期**：**6h**（GitHub Actions 不支持 24h 长任务，已改为 6h）

*数据来源：任务基准（最新状态）*

### 6.2 Soak 体系架构

| 组件 | 内容 |
|------|------|
| SoakMonitor | 6 类退化检测（RSS >50MB / fd_count >10 / 连接池泄漏 / 吞吐衰减 >10% / P99 >2x / 错误数 >0） |
| CSV 导出 | `target/soak-report.csv` 自动上传 artifact |
| CI 任务 | `soak.yml`：定时触发 + `workflow_dispatch` 手动触发 |
| 冒烟测试 | `soak_smoke_10s`（默认运行，每次 push/PR） |
| 主 soak | `soak_pool_long_running_steady_state`（默认 60s，支持 `--soak-duration` 参数） |

### 6.3 1h Soak Test 实际运行结果（2026-07-20 历史数据）

> ⚠️ 平台限制说明：本次 1h Soak Test 在 Windows 平台运行，受限于 `sysinfo` crate 在 Windows 上的能力：
> - RSS（进程内存）：占位实现，返回 0
> - fd_count（文件描述符）：占位实现，返回 0
> - thread_count：占位实现，返回 0
> - ops_per_sec / p99_latency / pool_idle / pool_active / error_count：✅ 精确数据
>
> Linux CI 任务（6h）运行在 Linux runner 上，RSS 和 fd_count 指标将提供精确数据，6 类退化检测全部生效。

**关键指标**：
- 总运行时长：3600s（1h）
- 总操作数：1,380,004,987 次（13.8 亿）
- 错误数：0
- 退化检测：✅ 未检测到退化（4 项生效：吞吐量/P99/连接池/错误数；2 项 N/A：RSS/fd_count，待 Linux CI 补齐）

**吞吐稳定性**：
- t=60s：361,566 ops/s
- t=1800s（30min）：414,351 ops/s
- t=3600s（60min，倒数第二帧）：357,372 ops/s
- 衰减率：(361566 - 357372) / 361566 ≈ 1.16%（远低于 10% 阈值）

**P99 延迟稳定性**：
- t=60s：43μs
- t=3600s：41μs（无退化，远低于 2x 阈值）

**连接池终态**：pool(idle=8, active=8) — ✅ 无泄漏

**CSV 报告**：60 行采样数据已导出到 `target/soak-report.csv`

*数据来源：成熟度评估报告 §8（1h Soak Test 实际运行结果）*

### 6.4 Soak 后续计划

1. **6h Linux CI Soak Test**：每周定时自动运行（GitHub Actions 不支持 24h 已改为 6h），提供完整 6 类退化检测
2. **7×6h 累计 soak 数据**：覆盖周/月级慢退化，待积累
3. 完成 7×6h 累积 + 生产案例验证后，Soak Test 扣分项 0.005 可恢复

---

## 七、crates.io 发布状态

### 7.1 发布情况

**当前状态**：**v1.2.1 已发布到 crates.io，共 41 个包**

*数据来源：任务基准（最新状态）*

### 7.2 历史发布进程

| 阶段 | 版本 | 状态 |
|------|------|------|
| v0.2.0 ~ v0.2.1 | 工作空间版本统一为 0.2.0 + workspace 继承 | 内部开发 |
| v1.0.0 | sqlx 0.9.0 升级 + rsa Marvin Attack 消除 | 内部发布 |
| v1.2.0 | AI 安全审计基准版本（779 个 crate 依赖） | 内部发布 |
| **v1.2.1** | **已发布到 crates.io，41 个包** | ✅ **公开发布** |

### 7.3 发布影响

- ✅ 外部用户可直接通过 `cargo add sz-orm-*` 引入
- ✅ 外部试点项目 sz-pay 已确认采用
- ✅ 社区采纳案例可逐步积累，恢复 0.01 扣分项
- ⏳ 待积累真实业务流量验证数据

### 7.4 同类项目对比

| 维度 | SZ-ORM v1.2.1 | Diesel 2.x | SQLx 0.9.x | SeaORM 1.x | rbatis |
|------|---------------|------------|------------|------------|--------|
| crates.io 发布 | ✅ 41 包 | ✅ | ✅ | ✅ | ✅ |
| 生产内部案例 | ✅ (sz-rust 18 包) | ✅ 数千 | ✅ 数万 | ✅ 数千 | ✅ 数千 |
| 生产外部案例 | ⏳ sz-pay 试点 | ✅ | ✅ | ✅ | ✅ |
| 第三方安全审计 | ✅ AI 审计（22 项发现） | ✅ 部分 | ✅ 部分 | ❌ | ❌ |
| 社区贡献者 | 1 | 500+ | 500+ | 200+ | 100+ |
| 中英文文档 | ✅ 均有 | 仅英文 | 仅英文 | 仅英文 | 仅中文 |
| 扩展包总数 | **41** | 0 | 0 | 0 | 0 |

*数据来源：真实审查报告 §5*

---

## 八、结论与建议

### 8.1 综合评定

| 评估维度 | 评级 | 说明 |
|----------|------|------|
| 代码质量 | A+ | 0 panic / 0 clippy / 5442+ 测试 / 0 unimplemented/todo / 0 unwrap/expect |
| 功能广度 | A+ | 43 包 Rust ORM 生态第一（41 sz-orm-* lib + cli + examples） |
| 安全性（自测） | A | cargo-audit/cargo-deny/SQL 注入防护 + Semgrep 16 规则 + CodeQL |
| 安全性（AI 审计） | B- | 22 项发现，3 Critical + 1 High 已修复，剩余多为传递依赖（feature-gated） |
| 类型安全 | A- | query! 宏连真 DB 编译期验证（5 种数据库） + 强类型 AST |
| 生产经验 | B+ | 内部使用（sz-rust 18 包）+ 外部试点 sz-pay 确认 + v1.2.1 已发布 crates.io |
| 文档完整性 | A | 中英文双全，14+ 份专业文档 |
| 社区生态 | C+ | v1.2.1 已发布 41 包，外部试点确认，社区采纳待积累 |
| 可靠性工程 | A- | Soak/Stress/Chaos/Jepsen/Formal 七线验证体系完备（6h soak 待积累） |
| **综合** | **A-/B+** | 代码质量顶尖，安全审计与真 DB 验证已补齐，crates.io 已发布，外部试点进行中 |

### 8.2 核心结论

**SZ-ORM 的工程质量和功能广度在 Rust ORM 生态中没有对手**：

1. **代码质量顶尖**：0 panic / 0 unimplemented/todo / 0 unwrap/expect / 0 clippy warning / 5442+ 测试全部通过
2. **功能广度第一**：43 包（41 sz-orm-* lib + cli + examples），覆盖 SQL 生成 + 连接池 + 事务 + 迁移 + 钩子 + 分布式事务（2PC/TCC/Saga/跨分片）+ 分片 + AI（pgvector + NL→SQL）+ 真实云服务（MQTT/WebSocket/RabbitMQ/S3）+ 可观测性 + 灾备 + 安全
3. **安全审计完整**：AI 安全审计 22 项发现，3 Critical + 1 High 已修复；Semgrep 16 规则 + CodeQL + cargo-audit/deny CI 集成
4. **真 DB 验证补齐**：query! 宏连真 DB 编译期验证（5 种数据库：MySQL/PG/SQLite/Oracle/SQL Server）
5. **已发布 crates.io**：v1.2.1 已发布 41 包，外部试点 sz-pay 确认
6. **七线验证体系完备**：TDD + 集成 + Jepsen + Fuzz + Stress + Chaos + Formal

### 8.3 剩余工作建议

#### P0（已完成）

- ✅ crates.io 发布（v1.2.1 已发布 41 包）
- ✅ AI 安全审计 3 Critical + 1 High 修复
- ✅ query! 宏 db-verify 支持 5 种数据库
- ✅ Semgrep/CodeQL 安全规则

#### P1（进行中）

- ⏳ **6h Linux CI Soak Test 数据积累**：GitHub Actions 已改为 6h（不支持 24h），待积累完整周期数据，恢复 0.005 扣分项
- ⏳ **sz-pay 外部试点验证**：积累真实业务流量数据，恢复 0.01 扣分项
- ⏳ **cargo-fuzz 覆盖率提升**：当前 6 个 fuzz targets，建议增加
- ⏳ **AI 安全审计 Low 级别问题修复**（L-1 ~ L-5）

#### P2（待上游）

- ⚠ **传递依赖漏洞修复**：H-2 quick-xml DoS / M-1 rsa Marvin Attack / M-2 rustls-webpki 证书缺陷 / M-3 rand 0.7.3 unsound（均为 feature-gated，默认编译不受影响）

#### P3（建议）

- 建议**每完成一轮修复后启动第二轮 5 路并行审查**，直至连续两轮审查 0 新发现 Critical/High 问题
- 建议**持续跟踪 sz-pay 试点运行数据**，作为外部生产案例的初始证据
- 建议**完善 cargo-fuzz targets**，覆盖 SQL 注入/转义/JSON 提取/分页边界/Value 转换等关键路径

### 8.4 距离 5.0/5 的最后 0.02 分差距

| 扣分项 | 分值 | 恢复条件 |
|--------|------|---------|
| 无生产案例 | -0.01 | sz-pay 外部试点验证 + 社区采纳案例积累 |
| Soak 6h 长稳态 | -0.005 | 7×6h 累计 soak 数据积累（GitHub Actions 已改为 6h） |
| 安全 Critical 生产验证 | -0.0025 | 真实生产流量下的长期稳定性验证（rsa Marvin Attack 已消除） |
| **合计** | **-0.0175**（向上取整 -0.02） | |

*数据来源：成熟度评估报告 §8、真实审查报告 §7、AI 安全审计报告 §6*

---

## 附录：本报告合并自以下 5 份审查报告

| 序号 | 源文件路径 | 报告日期 | 数据基准版本 |
|------|-----------|---------|-------------|
| 1 | `e:\vue\test\鲜视达\rust\sz-orm\docs\sz-orm真实审查报告.md` | 2026-07-21 | v1.0.0（39 包 / 2950 测试） |
| 2 | `e:\vue\test\鲜视达\rust\sz-orm\docs\sz-orm全面审查报告v1.md` | 2026-07-20 | v0.2.1（38 包 / 1871+ 测试，第一轮 + 第二轮审查） |
| 3 | `e:\vue\test\鲜视达\rust\sz-orm\docs\sz-orm项目成熟度评估报告.md` | 2026-07-21 | v1.0.0（43 包 / 5442 测试） |
| 4 | `e:\vue\test\鲜视达\rust\sz-orm\docs\sz-orm生产就绪报告.md` | 2026-07-21 | v1.0.0（39 包 / 2950 测试，报告版本 v5.0） |
| 5 | `e:\vue\test\鲜视达\rust\sz-orm\docs\AI安全审计报告_2026-07-30.md` | 2026-07-30 | v1.2.0（39 包 / 779 crate 依赖，22 项安全发现） |

**合并原则**：
1. 以最新数据为准（v1.2.1 / 43 包 / 5442+ 测试 / 6h soak / 6 个 fuzz targets / crates.io 已发布 41 包）
2. 多份报告中重复的内容只保留一份，并标注数据来源
3. 保留关键数据（测试数 5442+ / 0 panic / 0 error 等）
4. 不创建新的分析结论，只整合已有报告的内容
5. 报告语言：中文

**数据冲突说明**：
- 工作空间成员数：真实审查报告/生产就绪报告（39 包） vs 成熟度评估报告（43 包）→ 采用 **43 包**（最新，已含 sz-orm-vector）
- 测试数：真实审查报告/生产就绪报告（2950） vs 成熟度评估报告（5442）→ 采用 **5442**（最新，含 vector 测试）
- crate 版本：源报告（v1.0.0 / v1.2.0）→ 采用 **v1.2.1**（任务基准，最新已发布版本）
- Soak 周期：源报告（24h）→ 采用 **6h**（任务基准，GitHub Actions 不支持 24h 已改为 6h）
- crates.io 发布：源报告（未发布）→ 采用 **v1.2.1 已发布 41 包**（任务基准，最新状态）
