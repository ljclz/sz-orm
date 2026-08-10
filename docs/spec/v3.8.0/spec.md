# sz-orm v3.8.0 需求规格说明书

> 版本：v3.8.0（生产部署就绪检查清单 + ORM 库特性适配 + 配置化与验证体系 + 五方言连接安全）
> 基线：v3.7.0（已完成真实数据库端到端测试体系 + 对比分析文档同步 + 探索能力成熟化 + 方言扩展 + 云数仓验证 + 工程规范化）
> 日期：2026-08-10
> 需求格式：EARS（Ubiquitous / Event-driven / State-driven / Optional / Unwanted）
> 文档定位：需求规格（What to build），不含技术设计（How to build）
> 优先级声明：十五项检查按"安全红线类(1,最高,涉及密钥/TLS/注入) → 配置可观测类(2,高,端点/日志/探针) → 阈值调优类(3,高,限流/熔断/连接池) → ORM 特有防护类(4,中,泄漏/N+1/方言安全) → 检查清单工具化(5,低但必须,可执行验证)"的收益/风险序推进
> 需求编号约定：REQ-PROD-xxx（生产部署就绪检查项）
> 缺陷来源：sz-orm 作为 Rust ORM 库工作空间（46 成员），虽非独立服务，但包含 config/auth/limit/health/observability/queue/audit/core 等需生产配置的模块，当前缺乏统一的生产部署就绪验证体系：Redis 连接无 TLS（`packages/sz-orm-queue/src/` grep TLS 无结果）、JWT 无签名密钥轮换 kid 机制（`packages/sz-orm-auth/src/jwt.rs` 单密钥 HS256）、metrics 端点无访问控制（`packages/sz-orm-observability/src/lib.rs:418` start_metrics_server 裸暴露）、优雅关闭超时硬编码 30 秒不可配置（`packages/sz-orm-core/src/pool.rs:1703`）、配置脱敏无统一验证、限流/熔断无生产调优配置、日志级别无生产强制配置
> 兼容性铁律：所有新能力通过 feature gate 隔离（`prod-ready` 总 feature gate），默认 feature 行为不变，既有公开 API 完全向后兼容，v3.7.0 已验收测试基线不回退；sz-pay 生产依赖（从 crates.io 拉取 sz-orm-*）不得被破坏；五方言覆盖：MySQL/PostgreSQL/SQLite/Oracle/MSSQL

---

# 1. 组件定位

## 1.1 核心职责

本组件负责交付 sz-orm v3.8.0 的生产部署就绪检查清单体系：将用户参照 sz-rust 框架提出的 10 项生产部署检查项（配置脱敏验证、Redis TLS、JWT 密钥轮换、限流调优、熔断调优、日志级别、metrics 访问控制、健康检查端点、优雅关闭超时、K8s 探针）适配为 ORM 库工作空间的配置化能力与验证体系，并补充 5 项 ORM 库特有防护（SQL 注入生产验证、连接泄漏检测、N+1 检测调优、连接池参数调优、五方言连接安全验证），实现 sz-orm 在"配置安全、连接加密、密钥管理、流量防护、可观测性、探针就绪、优雅停机、ORM 防护"八个维度的生产就绪，且不破坏现有 API 兼容性与 v3.7.0 已验收基线。

## 1.2 核心输入

1. **v3.7.0 已验收基线**：真实 DB e2e 测试体系、对比分析文档同步、探索能力成熟化等已完成，测试基线不回退，作为本版本基准。
2. **用户 10 项生产部署检查项**：配置文件敏感字段脱敏验证、Redis 连接 TLS 加密、JWT 签名密钥轮换机制、限流阈值生产调优、熔断器阈值生产调优、日志级别生产配置（warn 以上）、Prometheus metrics 端点访问控制、健康检查端点配置、优雅关闭超时配置、K8s readiness/liveness probe 配置，作为核心需求来源。
3. **现有模块能力清单**：
   - `sz-orm-config`：ConfigEncryption 加密（`packages/sz-orm-config/src/lib.rs:1479`）、Consul/Nacos 配置中心，但无统一脱敏验证。
   - `sz-orm-auth`：JWT HS256 RustCrypto 实现（`packages/sz-orm-auth/src/jwt.rs:1`）、刷新令牌轮换（`packages/sz-orm-auth/src/token_store.rs:4`），但无签名密钥轮换 kid 机制。
   - `sz-orm-limit`：令牌桶 + 滑动窗口限流、OOM 防护（`packages/sz-orm-limit/src/lib.rs:21` DEFAULT_MAX_KEYS），但无生产阈值调优配置。
   - `sz-orm-health`：HealthStatus/HealthReport（`packages/sz-orm-health/src/lib.rs:26`）、ProbeManager K8s 探针（`packages/sz-orm-health/src/advanced.rs:9`）、CascadingHealthChecker、TimeoutHealthChecker、HealthCheckCache。
   - `sz-orm-observability`：MetricsRegistry、start_metrics_server（`packages/sz-orm-observability/src/lib.rs:418`，裸暴露无访问控制）、SLO 燃烧率。
   - `sz-orm-queue`：Redis/Kafka/RabbitMQ/NATS/Pulsar/ActiveMQ/RocketMQ 后端，但无 TLS 配置。
   - `sz-orm-audit`：SQL 审计日志 + 敏感关键词脱敏（`packages/sz-orm-audit/src/lib.rs:23` SENSITIVE_KEYWORDS）。
   - `sz-orm-core`：连接池 shutdown() 优雅关闭（`packages/sz-orm-core/src/pool.rs:1695`，超时硬编码 30 秒 `:1703`）、circuit_breaker 熔断器（`packages/sz-orm-core/src/circuit_breaker.rs:3`，feature gate）、configure_circuit_breaker（`packages/sz-orm-core/src/pool.rs:1173`）、acquire_timeout/idle_timeout/connection_timeout/query_timeout。
   - `sz-orm-masking`：数据脱敏 DataMasker（`packages/sz-orm-masking/src/lib.rs:21` MaskingRule，手机/邮箱/身份证/银行卡等）。
   - `sz-orm-logger`：日志模块。
   - `sz-orm-crypto`：加密模块。
4. **缺口证据**：Redis 无 TLS（`packages/sz-orm-queue/src/` 全模块 grep `tls|Tls|TLS|rustls` 无结果）、JWT 单密钥无 kid、metrics 端点无访问控制、shutdown 超时硬编码、配置脱敏无统一验证入口。
5. **本机数据库连接信息**：MySQL 9.6（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）、PostgreSQL 18（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）、Oracle 23ai Free（`127.0.0.1:1521/freepdb1`）。
6. **sz-pay 生产依赖证据**：sz-pay 从 crates.io 拉取 sz-orm-* 6 个包，作为 API 兼容性验证的下游基准。
7. **五方言覆盖约束**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL，连接安全验证须覆盖全部方言。
8. **既有 feature gate 体系**：`packages/sz-orm-core/Cargo.toml` 已有 25+ feature，作为新能力 feature gate 隔离的基础。

## 1.3 核心输出

1. **生产部署就绪检查清单**：覆盖 15 项检查的可执行清单，每项含验证方法、代码证据要求、验收条件。
2. **配置化能力**：Redis TLS 配置、JWT 密钥轮换配置、metrics 访问控制配置、优雅关闭超时配置、日志级别生产配置、限流/熔断生产调优配置，均通过 `prod-ready` feature gate 隔离。
3. **统一配置脱敏验证入口**：整合 sz-orm-config（ConfigEncryption）+ sz-orm-masking（DataMasker）+ sz-orm-audit（SQL 脱敏），提供配置加载后敏感字段已脱敏/加密的验证。
4. **K8s 探针配置化**：基于 sz-orm-health 的 ProbeManager，提供 readiness/liveness 端点配置与 K8s 集成配置。
5. **ORM 特有防护验证**：SQL 注入生产验证、连接泄漏检测配置、N+1 检测调优、连接池参数调优、五方言连接安全验证。
6. **需求追溯矩阵**：本文档第 7 章，建立需求 ↔ 验收条件映射。
7. **验收标准总览**：本文档第 8 章，按检查项汇总验收条件。

## 1.4 职责边界

本组件**不负责**以下事项：

1. **不破坏既有公开 API**：所有新能力通过 `prod-ready` feature gate 隔离，既有公开 API 签名保持完全向后兼容。
2. **不改变既有安全铁律**：任何 WHERE 条件必须参数化，默认禁止 `SELECT *`，N+1 检测自动拦截，沿用既有铁律，本版本仅补生产验证。
3. **不替换既有连接池 shutdown 实现**：既有 `shutdown()`（`packages/sz-orm-core/src/pool.rs:1695`）保留，新增 `shutdown_with_timeout()` 可配置超时版本，两者共存。
4. **不强制启用 TLS**：Redis TLS 通过配置项可选启用，默认行为不变（明文连接），仅在显式配置 TLS 时启用加密。
5. **不强制启用 metrics 访问控制**：默认行为不变（裸暴露），仅在显式配置访问控制时启用，避免破坏既有集成。
6. **不负责 sz-pay / sz-rust 下游代码修改**：ADR-0001 严禁修改下游/上游仓库，仅保证 API 兼容性。
7. **不降低既有测试覆盖**：v3.8.0 不得使 v3.7.0 已验收测试基线回退，仅增不减。
8. **不负责 K8s 集群部署编排**：本组件提供探针配置与端点，不负责 K8s Deployment/YAML 编写。
9. **不负责 Prometheus / Grafana 服务端配置**：本组件仅提供 metrics 端点与访问控制，不负责抓取端配置。
10. **不负责密钥的物理安全管理**：本组件提供密钥轮换机制（kid 多密钥），不负责密钥的 HSM/KMS 物理存储。

---

# 2. 领域术语

**生产部署就绪（Production Readiness）**
: 系统/库在正式上线前须满足的非功能性检查项集合，涵盖安全、配置、可观测性、探针、优雅停机等维度，每项须有可执行的验证方法与代码证据。

**配置敏感字段脱敏验证（Config Sensitive Field Masking Verification）**
: 在配置加载完成后，验证所有标记为敏感的字段（密码、密钥、令牌、连接串等）已被脱敏或加密，不以明文形式存在于运行时配置对象、日志输出、审计记录中。
: 备注：sz-orm 已有 ConfigEncryption（`packages/sz-orm-config/src/lib.rs:1479`）、DataMasker（`packages/sz-orm-masking/src/lib.rs:21`）、SQL 审计脱敏（`packages/sz-orm-audit/src/lib.rs:23`），本版本补统一验证入口。

**Redis 连接 TLS 加密（Redis Connection TLS Encryption）**
: Redis 客户端连接通过 TLS 加密传输，包含 CA 证书、客户端证书/密钥、SNI、证书校验等配置，防止中间人攻击与明文泄露。
: 备注：sz-orm-queue 当前无 TLS 配置（`packages/sz-orm-queue/src/` grep TLS 无结果），本版本新增。

**JWT 签名密钥轮换（JWT Signing Key Rotation）**
: 支持多个签名密钥并存，通过 JWT header 的 `kid`（Key ID）字段标识签发密钥，验证时按 `kid` 查找对应密钥，实现密钥无停机轮换：新令牌用新密钥签发，旧令牌用旧密钥验证直至过期。
: 备注：区别于 sz-orm-auth 已有的刷新令牌轮换（`packages/sz-orm-auth/src/token_store.rs:4`，轮换的是 refresh token 而非签名密钥），本版本新增签名密钥轮换。

**限流阈值生产调优（Rate Limit Threshold Production Tuning）**
: 限流器的阈值（令牌桶容量/速率、滑动窗口请求数/窗口大小）可通过生产配置文件设定，并提供配置验证（阈值合理性检查）与运行时动态调整能力。
: 备注：sz-orm-limit 已有限流器（`packages/sz-orm-limit/src/lib.rs`），本版本补生产配置与验证。

**熔断器阈值生产调优（Circuit Breaker Threshold Production Tuning）**
: 熔断器的阈值（失败次数阈值、恢复超时）可通过生产配置文件设定，并提供配置验证与运行时查询能力。
: 备注：sz-orm-core 已有熔断器（`packages/sz-orm-core/src/circuit_breaker.rs:3`，feature gate）与 configure_circuit_breaker（`packages/sz-orm-core/src/pool.rs:1173`），本版本补生产配置与验证。

**日志级别生产配置（Log Level Production Configuration）**
: 生产环境下日志级别强制配置为 warn 及以上（warn/error），禁止 debug/trace 级别在生产环境输出，避免性能损耗与敏感信息泄露。
: 备注：sz-orm-logger 已有日志模块，本版本补生产级别强制配置与验证。

**Prometheus metrics 端点访问控制（Metrics Endpoint Access Control）**
: Prometheus metrics 端点（`/metrics`）支持访问控制，包含 IP 白名单、Bearer Token 认证、Basic Auth 认证，防止未授权抓取。
: 备注：sz-orm-observability 的 start_metrics_server（`packages/sz-orm-observability/src/lib.rs:418`）当前裸暴露无访问控制，本版本新增。

**健康检查端点配置（Health Check Endpoint Configuration）**
: 健康检查以 HTTP 端点形式暴露，支持配置端点路径、端口、检查资源集合、缓存 TTL，供外部探活系统调用。
: 备注：sz-orm-health 已有 HealthStatus/HealthReport（`packages/sz-orm-health/src/lib.rs:26`），本版本补端点暴露与配置化。

**优雅关闭超时配置（Graceful Shutdown Timeout Configuration）**
: 连接池优雅关闭时，等待在途连接归还的超时时间可配置，超时后强制关闭，避免无限等待。
: 备注：sz-orm-core 的 shutdown()（`packages/sz-orm-core/src/pool.rs:1695`）当前硬编码 30 秒超时（`:1703`），本版本新增可配置版本。

**K8s readiness/liveness probe 配置（Kubernetes Probe Configuration）**
: 提供 Kubernetes 风格的 readiness（就绪可接流量）与 liveness（进程存活）探针端点配置，两者独立管理，支持配置探针路径、端口、检查资源。
: 备注：sz-orm-health 已有 ProbeManager（`packages/sz-orm-health/src/advanced.rs:9`），本版本补端点暴露与 K8s 集成配置。

**SQL 注入防护生产验证（SQL Injection Protection Production Verification）**
: 在生产部署前，验证所有查询路径均使用参数化查询（where_eq/or_where_eq 等），无 SQL 字符串拼接，通过自动化扫描与测试确认防护到位。
: 备注：sz-orm 已有参数化铁律与 `scripts/check-sql-injection.ps1`，本版本补生产验证清单化。

**连接泄漏检测（Connection Leak Detection）**
: 检测连接池中借出后未归还的连接（连接泄漏），提供检测配置（检测开关、告警阈值、泄漏超时）与运行时报告。
: 备注：sz-orm-core 的 pool.rs 已有泄漏防护注释（`packages/sz-orm-core/src/pool.rs:310`），本版本补生产级检测配置。

**N+1 查询检测生产调优（N+1 Query Detection Production Tuning）**
: N+1 查询检测器（N1QueryDetector）的阈值（检测窗口、告警阈值、拦截开关）可通过生产配置设定，并提供运行时统计。
: 备注：sz-orm 已有 N1QueryDetector 自动拦截，本版本补生产调优配置。

**五方言连接安全验证（Five-Dialect Connection Security Verification）**
: 验证 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五种方言的连接均满足生产安全要求（TLS、认证、连接串脱敏、连接池参数），覆盖全部方言。

**prod-ready feature gate**
: 控制生产部署就绪能力的总 feature gate，聚合 Redis TLS、JWT 密钥轮换、metrics 访问控制、优雅关闭超时配置等子能力，默认关闭（避免无配置环境行为变化），生产环境启用。

---

# 3. 角色与边界

## 3.1 核心角色

- **ORM 库维护者**：执行 v3.8.0 生产就绪检查清单的开发、验证、测试操作者，是新增能力的主要使用者与验收人。
- **下游项目开发者（sz-pay）**：关注 API 兼容性与版本平滑升级的下游使用者，v3.8.0 不得破坏其既有代码。
- **运维/SRE 工程师**：在生产环境部署 sz-orm 下游应用时，使用生产就绪检查清单逐项验证，配置 TLS/密钥/阈值/探针等参数。

## 3.2 外部系统

- **MySQL 9.6**：五方言连接安全验证目标之一（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）。
- **PostgreSQL 18**：五方言连接安全验证目标之一（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）。
- **Oracle 23ai Free**：五方言连接安全验证目标之一（`127.0.0.1:1521/freepdb1`）。
- **SQLite**：五方言连接安全验证目标之一（文件型，无需独立服务）。
- **MSSQL**：五方言连接安全验证目标之一（如本机可用）。
- **Redis**：Redis 连接 TLS 加密验证目标。
- **Kubernetes**：K8s readiness/liveness probe 配置的集成目标。
- **Prometheus**：metrics 端点访问控制的抓取方。
- **crates.io**：v3.8.0 发布目标（如发布）。
- **sz-pay 项目**：API 兼容性验证的下游基准。

## 3.3 交互上下文

```plantuml
@startuml
title sz-orm v3.8.0 生产部署就绪检查清单 交互上下文

rectangle "sz-orm v3.8.0" as orm {
  rectangle "配置脱敏验证" as cfg
  rectangle "Redis TLS" as tls
  rectangle "JWT 密钥轮换" as jwt
  rectangle "限流/熔断调优" as limit
  rectangle "日志级别配置" as log
  rectangle "metrics 访问控制" as metrics
  rectangle "健康检查端点" as health
  rectangle "优雅关闭超时" as shutdown
  rectangle "K8s 探针配置" as probe
  rectangle "ORM 防护验证" as guard
}

actor "ORM 库维护者" as dev
actor "运维/SRE" as sre
actor "sz-pay 开发者" as downstream

database "MySQL/PG/SQLite\n/Oracle/MSSQL" as db
database "Redis" as redis
cloud "Kubernetes" as k8s
cloud "Prometheus" as prom

dev --> orm : 开发/验证/测试
sre --> orm : 配置TLS/密钥/阈值/探针
sre --> k8s : 部署探针配置
metrics --> prom : /metrics (访问控制)
probe --> k8s : readiness/liveness
tls --> redis : TLS 加密连接
guard --> db : 五方言安全验证
downstream --> orm : API 兼容性验证

@enduml
```

---

# 4. DFX约束

## 4.1 性能

1. **TLS 握手开销**：Redis TLS 连接首次握手额外开销不超过 50ms，连接池复用后无额外开销。
2. **metrics 访问控制开销**：启用访问控制后，单次 metrics 抓取鉴权开销不超过 1ms。
3. **配置脱敏验证开销**：配置加载后脱敏验证一次性执行，开销不超过 100ms。
4. **连接泄漏检测开销**：泄漏检测周期性检查（默认 60 秒一次），单次检查开销不超过 10ms。
5. **N+1 检测开销**：N+1 检测器启用后，单次查询检测开销不超过 0.1ms。

## 4.2 可靠性

1. **优雅关闭超时保证**：shutdown_with_timeout(timeout) 须在 timeout 内完成或在 timeout 后强制关闭，不无限等待。
2. **密钥轮换无停机**：JWT 密钥轮换期间，旧令牌用旧密钥验证直至过期，新令牌用新密钥签发，不中断服务。
3. **熔断器配置生效保证**：生产配置的熔断阈值须在连接池启动时生效，运行时可查询当前状态。
4. **v3.7.0 测试基线不回退**：v3.8.0 不得使 v3.7.0 已验收测试基线回退，仅增不减。

## 4.3 安全性

1. **Redis TLS 证书校验**：启用 TLS 时须校验服务器证书（CA 证书），禁止跳过校验的生产配置。
2. **metrics 端点禁止裸暴露生产**：生产环境 metrics 端点须启用访问控制（IP 白名单或认证），禁止无访问控制暴露。
3. **日志级别禁止 debug 生产**：生产环境日志级别须为 warn 及以上，禁止 debug/trace。
4. **配置敏感字段禁止明文**：配置加载后敏感字段须脱敏或加密，禁止明文存在于运行时配置对象、日志、审计记录。
5. **JWT 密钥最小长度**：签名密钥长度不少于 32 字节，禁止弱密钥。
6. **连接串脱敏**：数据库连接串在日志/错误/审计中须脱敏（密码字段掩码），禁止明文密码泄露。

## 4.4 可维护性

1. **生产就绪检查清单可执行**：检查清单每项须有可执行的验证命令或测试，附代码证据（file:line）。
2. **配置项可观测**：所有生产配置项（TLS/密钥/阈值/超时/级别）须可通过运行时查询或 metrics 暴露当前值。
3. **审计证据要求**：每项检查结论须附 file:line 证据，遵循 AGENTS.md 审计合规铁律。

## 4.5 兼容性

1. **API 向后兼容**：所有新能力通过 `prod-ready` feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容。
2. **sz-pay 不破坏**：sz-pay 从 crates.io 拉取的 sz-orm-* 6 个包既有用法不受影响。
3. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（TLS/连接安全相关项按方言能力适配）。

---

# 5. 核心能力

## 5.1 配置敏感字段脱敏验证

### 5.1.1 业务规则

1. **统一脱敏验证入口**：系统必须提供统一的配置脱敏验证入口，在配置加载完成后执行，验证所有标记为敏感的字段（密码、密钥、令牌、连接串、API Key 等）已被脱敏或加密。
   a. 验收条件：[配置加载后调用验证入口] → [返回所有敏感字段的脱敏状态报告，未脱敏字段标记为违规]
2. **脱敏规则可配置**：敏感字段识别规则（字段名匹配、字段路径）与脱敏方式（掩码/加密/移除）必须可通过配置文件设定。
   a. 验收条件：[配置指定 `database.password` 为敏感字段，脱敏方式为掩码] → [验证报告中该字段显示为 `***`，非明文]
3. **复用既有脱敏能力**：脱敏验证必须复用 sz-orm-masking（DataMasker）、sz-orm-config（ConfigEncryption）、sz-orm-audit（SQL 审计脱敏）既有能力，不重复实现。
   a. 验收条件：[验证入口对手机号字段脱敏] → [使用 DataMasker::apply(Phone) 规则，结果与既有脱敏一致]
4. **禁止项**：禁止配置加载后敏感字段以明文存在于运行时配置对象、日志输出、审计记录中。
   a. 验收条件：[配置含 `password: "test123"`，加载后打印配置对象] → [日志中 password 字段显示为 `***`，不含 `test123`]

### 5.1.2 交互流程

```plantuml
@startuml
title 配置脱敏验证 交互流程
actor "运维/SRE" as sre
participant "sz-orm-config" as cfg
participant "sz-orm-masking" as mask
database "配置文件" as file

sre -> cfg : 加载配置(含敏感字段)
cfg -> file : 读取配置
cfg -> cfg : 标记敏感字段
cfg -> mask : 对敏感字段应用脱敏规则
mask --> cfg : 返回脱敏后值
cfg -> cfg : 执行脱敏验证(检查无明文残留)
cfg --> sre : 返回脱敏验证报告
@enduml
```

### 5.1.3 异常场景

1. **敏感字段未脱敏**
   a. 触发条件：配置中某敏感字段未匹配任何脱敏规则，以明文存在
   b. 系统行为：验证报告标记该字段为违规，输出告警
   c. 用户感知：验证报告含 `[VIOLATION] field=database.password reason=plaintext_not_masked`
2. **脱敏规则配置错误**
   a. 触发条件：脱敏规则引用了不存在的 MaskingRule 类型
   b. 系统行为：配置加载阶段返回错误，拒绝启动
   c. 用户感知：错误提示 `invalid masking rule: unknown_rule`

## 5.2 Redis 连接 TLS 加密

### 5.2.1 业务规则

1. **TLS 配置项**：Redis 连接必须支持 TLS 配置，包含 CA 证书路径、客户端证书路径、客户端密钥路径、SNI、是否跳过证书校验（生产禁止跳过）。
   a. 验收条件：[配置 TLS 参数并启用] → [Redis 连接通过 TLS 加密建立，传输内容加密]
2. **TLS 可选启用**：TLS 通过配置项可选启用，默认行为不变（明文连接），仅在显式配置 TLS 时启用。
   a. 验收条件：[未配置 TLS] → [Redis 连接为明文，行为与 v3.7.0 一致]
3. **连接池复用 TLS 连接**：TLS 连接建立后进入连接池复用，握手仅首次发生，后续复用无握手开销。
   a. 验收条件：[连接池 acquire 多次] → [仅首次 TLS 握手，后续复用已建立的 TLS 连接]
4. **禁止项**：生产环境禁止跳过 TLS 证书校验（`skip_verify = true`）。
   a. 验收条件：[生产环境配置 `skip_verify = true`] → [配置验证拒绝，返回错误 `TLS skip_verify forbidden in production`]

### 5.2.2 交互流程

```plantuml
@startuml
title Redis TLS 连接 交互流程
participant "sz-orm-queue" as queue
participant "连接池" as pool
database "Redis" as redis

queue -> pool : 配置 TLS(ca_cert, client_cert, client_key)
pool -> redis : TLS 握手(校验 CA 证书)
redis --> pool : 握手成功
pool --> queue : 返回 TLS 加密连接
queue -> redis : 加密传输命令
@enduml
```

### 5.2.3 异常场景

1. **TLS 握手失败**
   a. 触发条件：CA 证书无效或服务器证书不匹配
   b. 系统行为：连接建立失败，返回 TLS 握手错误，连接池记录失败到熔断器
   c. 用户感知：错误提示 `Redis TLS handshake failed: certificate verify error`
2. **证书文件不存在**
   a. 触发条件：配置的 CA 证书路径不存在
   b. 系统行为：配置加载阶段返回错误，拒绝启动
   c. 用户感知：错误提示 `TLS CA cert file not found: /path/to/ca.crt`

## 5.3 JWT 签名密钥轮换机制

### 5.3.1 业务规则

1. **多密钥并存**：JWT 签名必须支持多个密钥并存，每个密钥有唯一 `kid`（Key ID）标识，签发时指定当前活跃 kid，验证时按 token header 的 kid 查找对应密钥。
   a. 验收条件：[配置 kid1=secret1, kid2=secret2，活跃 kid=kid2] → [新令牌 header 含 kid=kid2，用 secret2 签发；旧令牌 header 含 kid=kid1，用 secret1 验证]
2. **无停机轮换**：密钥轮换期间，旧令牌用旧密钥验证直至过期，新令牌用新密钥签发，不中断服务。
   a. 验收条件：[轮换：新增 kid2=secret2 设为活跃，保留 kid1=secret1] → [kid1 签发的未过期令牌仍验证通过，新令牌用 kid2 签发]
3. **过期密钥清理**：已过期密钥（所有用该密钥签发的令牌均已过期）可安全移除，不影响验证。
   a. 验收条件：[kid1 所有令牌已过期，移除 kid1] → [验证不报 kid_not_found，因无有效令牌引用 kid1]
4. **密钥最小长度**：签名密钥长度必须不少于 32 字节，禁止弱密钥。
   a. 验收条件：[配置密钥长度 < 32 字节] → [配置验证拒绝，返回错误 `JWT secret too short: minimum 32 bytes`]
5. **禁止项**：禁止无 kid 的令牌在生产环境通过验证（兼容期除外）。
   a. 验收条件：[生产环境验证无 kid 的令牌] → [验证失败，返回错误 `missing kid in production`]

### 5.3.2 交互流程

```plantuml
@startuml
title JWT 密钥轮换 交互流程
actor "运维/SRE" as sre
participant "sz-orm-auth" as auth
participant "密钥存储" as keys

sre -> auth : 配置密钥集(kid1=secret1, kid2=secret2, active=kid2)
auth -> keys : 存储多密钥
sre -> auth : 签发令牌(claims)
auth -> auth : 用 active kid(kid2) 签发
auth --> sre : 返回 token(header.kid=kid2)
sre -> auth : 验证令牌(token)
auth -> auth : 解析 header.kid=kid2
auth -> keys : 查找 kid2 密钥
keys --> auth : secret2
auth -> auth : 用 secret2 验证签名
auth --> sre : 验证结果
@enduml
```

### 5.3.3 异常场景

1. **kid 不存在**
   a. 触发条件：令牌 header 的 kid 在密钥集中不存在
   b. 系统行为：验证失败，返回 kid_not_found 错误
   c. 用户感知：错误提示 `JWT kid not found: kid3`
2. **密钥已过期但仍有有效令牌**
   a. 触发条件：尝试移除仍有有效令牌引用的密钥
   b. 系统行为：拒绝移除，返回警告
   c. 用户感知：警告 `Cannot remove kid1: 3 active tokens still reference it`

## 5.4 限流阈值生产调优

### 5.4.1 业务规则

1. **生产配置项**：限流器阈值（令牌桶容量/速率、滑动窗口请求数/窗口大小、max_keys）必须可通过生产配置文件设定。
   a. 验收条件：[配置 `rate_limit.capacity=100, rate_limit.rate=10/s`] → [限流器按配置阈值生效，第 101 个请求被拒绝]
2. **阈值合理性验证**：配置加载时必须验证阈值合理性（容量 > 0、速率 > 0、窗口 > 0、max_keys 在合理范围），不合理则拒绝启动。
   a. 验收条件：[配置 `capacity=0`] → [配置验证拒绝，返回错误 `rate limit capacity must be positive`]
3. **运行时动态调整**：限流阈值支持运行时动态调整（不重启），调整后立即生效。
   a. 验收条件：[运行时将容量从 100 调整为 200] → [调整后第 201 个请求才被拒绝]
4. **阈值可观测**：当前限流阈值与统计（通过/拒绝计数）必须可通过运行时查询或 metrics 暴露。
   a. 验收条件：[查询限流器状态] → [返回当前容量、速率、已通过数、已拒绝数]

### 5.4.2 交互流程

```plantuml
@startuml
title 限流阈值调优 交互流程
actor "运维/SRE" as sre
participant "sz-orm-limit" as limit
participant "限流器" as limiter

sre -> limit : 配置阈值(capacity=100, rate=10/s)
limit -> limiter : 创建限流器(配置参数)
sre -> limiter : acquire(key)
limiter --> sre : allowed/rejected
sre -> limit : 动态调整容量(200)
limit -> limiter : 更新容量
limiter --> limit : 生效
@enduml
```

### 5.4.3 异常场景

1. **阈值配置非法**
   a. 触发条件：配置的阈值为 0 或负数或超出合理范围
   b. 系统行为：配置加载阶段返回错误，拒绝启动
   c. 用户感知：错误提示 `invalid rate limit config: capacity must be positive`
2. **max_keys 过小导致误淘汰**
   a. 触发条件：max_keys 配置过小，正常 key 被强制淘汰
   b. 系统行为：记录告警，建议调大 max_keys
   c. 用户感知：告警 `rate limit max_keys too small, normal keys evicted`

## 5.5 熔断器阈值生产调优

### 5.5.1 业务规则

1. **生产配置项**：熔断器阈值（失败次数阈值 failure_threshold、恢复超时 reset_timeout）必须可通过生产配置文件设定。
   a. 验收条件：[配置 `circuit_breaker.failure_threshold=10, reset_timeout=60s`] → [连续 10 次失败后熔断，60 秒后进入 HalfOpen]
2. **阈值合理性验证**：配置加载时必须验证阈值合理性（failure_threshold > 0、reset_timeout > 0），不合理则拒绝启动。
   a. 验收条件：[配置 `failure_threshold=0`] → [配置验证拒绝，返回错误 `circuit breaker failure_threshold must be positive`]
3. **运行时状态查询**：熔断器当前状态（Closed/Open/HalfOpen）与统计（连续失败数、熔断次数）必须可通过运行时查询。
   a. 验收条件：[查询熔断器状态] → [返回当前状态、连续失败数、累计熔断次数]
4. **复用既有熔断器**：必须复用 sz-orm-core 既有 CircuitBreaker（`packages/sz-orm-core/src/circuit_breaker.rs:3`）与 configure_circuit_breaker（`packages/sz-orm-core/src/pool.rs:1173`），不重复实现。
   a. 验收条件：[生产配置的阈值] → [通过 configure_circuit_breaker 设置到既有 CircuitBreaker 实例]

### 5.5.2 交互流程

```plantuml
@startuml
title 熔断器阈值调优 交互流程
actor "运维/SRE" as sre
participant "sz-orm-core" as core
participant "连接池" as pool
participant "熔断器" as cb

sre -> core : 配置熔断阈值(failure_threshold=10, reset_timeout=60s)
core -> pool : configure_circuit_breaker(10, 60s)
pool -> cb : 设置阈值
sre -> pool : acquire()
pool -> cb : 检查状态
cb --> pool : Closed(放行)/Open(拒绝)
sre -> pool : 查询熔断状态
pool --> sre : 返回状态+统计
@enduml
```

### 5.5.3 异常场景

1. **阈值配置非法**
   a. 触发条件：failure_threshold 或 reset_timeout 为 0
   b. 系统行为：配置加载阶段返回错误，拒绝启动
   c. 用户感知：错误提示 `invalid circuit breaker config`
2. **熔断后下游恢复但未自动恢复**
   a. 触发条件：熔断器 Open 状态，reset_timeout 未到，下游已恢复
   b. 系统行为：保持 Open 拒绝请求，reset_timeout 后进入 HalfOpen 试探
   c. 用户感知：可手动调用 reset_circuit_breaker() 立即恢复

## 5.6 日志级别生产配置

### 5.6.1 业务规则

1. **生产级别强制**：生产环境日志级别必须为 warn 及以上（warn/error），禁止 debug/trace 级别在生产环境输出。
   a. 验收条件：[生产环境配置 `log_level=debug`] → [配置验证拒绝，返回错误 `log level debug forbidden in production, minimum warn`]
2. **级别可配置**：日志级别必须可通过配置文件设定，支持 error/warn/info/debug/trace 五级。
   a. 验收条件：[配置 `log_level=info`] → [info 及以上级别日志输出，debug/trace 不输出]
3. **环境区分**：必须区分开发环境（允许 debug/trace）与生产环境（强制 warn+），通过环境标识或配置项区分。
   a. 验收条件：[环境标识=production，配置 `log_level=warn`] → [验证通过，warn/error 输出]
4. **禁止项**：禁止生产环境输出 debug/trace 级别日志（含敏感信息、SQL 明文、连接详情）。
   a. 验收条件：[生产环境运行] → [日志中无 debug/trace 级别记录，无 SQL 明文、无连接串明文]

### 5.6.2 交互流程

```plantuml
@startuml
title 日志级别配置 交互流程
actor "运维/SRE" as sre
participant "sz-orm-logger" as logger

sre -> logger : 配置日志级别(level=warn, env=production)
logger -> logger : 验证生产级别(>=warn)
logger --> sre : 验证通过/拒绝
sre -> logger : 运行时输出日志
logger --> sre : 仅 warn/error 级别输出
@enduml
```

### 5.6.3 异常场景

1. **生产环境配置低级别**
   a. 触发条件：生产环境配置 log_level 为 debug 或 trace
   b. 系统行为：配置验证拒绝，拒绝启动
   c. 用户感知：错误提示 `log level debug forbidden in production`
2. **日志含敏感信息**
   a. 触发条件：warn/error 级别日志中含密码、密钥等敏感字段
   b. 系统行为：日志输出前对敏感字段脱敏（复用 sz-orm-masking）
   c. 用户感知：日志中敏感字段显示为 `***`

## 5.7 Prometheus metrics 端点访问控制

### 5.7.1 业务规则

1. **访问控制方式**：metrics 端点必须支持三种访问控制方式：IP 白名单、Bearer Token 认证、Basic Auth 认证，可组合使用。
   a. 验收条件：[配置 IP 白名单 `10.0.0.0/8`] → [仅 10.0.0.0/8 网段可访问 /metrics，其他 IP 返回 403]
2. **默认行为不变**：未配置访问控制时，行为与 v3.7.0 一致（裸暴露），仅在显式配置时启用，避免破坏既有集成。
   a. 验收条件：[未配置访问控制] → [/metrics 端点裸暴露，行为与 v3.7.0 一致]
3. **生产强制访问控制**：生产环境 metrics 端点必须启用访问控制，禁止裸暴露。
   a. 验收条件：[生产环境未配置访问控制] → [启动时告警 `metrics endpoint exposed without access control in production`]
4. **复用既有 metrics server**：必须复用 sz-orm-observability 既有 start_metrics_server（`packages/sz-orm-observability/src/lib.rs:418`），在其基础上增加访问控制层，不重复实现。
   a. 验收条件：[启用访问控制] → [基于既有 start_metrics_server，请求经鉴权层后再返回 metrics]

### 5.7.2 交互流程

```plantuml
@startuml
title metrics 端点访问控制 交互流程
actor "Prometheus" as prom
participant "sz-orm-observability" as obs
participant "鉴权层" as auth

prom -> obs : GET /metrics (Bearer token)
obs -> auth : 鉴权(token)
auth --> obs : 通过/拒绝
alt 通过
obs --> prom : 200 Prometheus 文本格式
else 拒绝
obs --> prom : 401/403 Forbidden
end
@enduml
```

### 5.7.3 异常场景

1. **鉴权失败**
   a. 触发条件：Bearer Token 无效或 IP 不在白名单
   b. 系统行为：返回 401（认证失败）或 403（禁止访问）
   c. 用户感知：HTTP 401/403，无 metrics 内容
2. **生产环境裸暴露**
   a. 触发条件：生产环境启动时未配置访问控制
   b. 系统行为：输出告警（不阻止启动，由运维决定）
   c. 用户感知：告警日志 `metrics endpoint exposed without access control in production`

## 5.8 健康检查端点配置

### 5.8.1 业务规则

1. **HTTP 端点暴露**：健康检查必须以 HTTP 端点形式暴露，支持配置端点路径（默认 `/health`）、端口、检查资源集合。
   a. 验收条件：[配置 `health.path=/health, health.port=8080`] → [GET http://localhost:8080/health 返回聚合健康状态 JSON]
2. **资源集合可配置**：健康检查的资源集合（连接池名列表）必须可配置，仅检查配置的资源。
   a. 验收条件：[配置检查资源 `["pool_mysql", "pool_pg"]`] → [健康报告仅含这两个池的状态]
3. **缓存 TTL 可配置**：健康检查缓存 TTL 必须可配置，复用 sz-orm-health 既有 HealthCheckCache（`packages/sz-orm-health/src/advanced.rs`），避免高频探活压后端。
   a. 验收条件：[配置 `health.cache_ttl=5s`] → [5 秒内重复请求返回缓存结果，不实际检查后端]
4. **复用既有健康检查**：必须复用 sz-orm-health 既有 DbHealthChecker/HealthReport（`packages/sz-orm-health/src/lib.rs:26`），不重复实现。
   a. 验收条件：[端点返回的健康报告] → [结构与既有 HealthReport 一致]

### 5.8.2 交互流程

```plantuml
@startuml
title 健康检查端点 交互流程
actor "运维/SRE" as sre
participant "sz-orm-health" as health
database "DB" as db

sre -> health : GET /health
health -> health : 检查缓存(TTL内?)
alt 缓存命中
health --> sre : 返回缓存结果
else 缓存未命中
health -> db : 检查各资源(pool_mysql, pool_pg)
db --> health : 返回状态
health -> health : 更新缓存
health --> sre : 返回聚合健康报告(JSON)
end
@enduml
```

### 5.8.3 异常场景

1. **检查资源不存在**
   a. 触发条件：配置的检查资源名不存在
   b. 系统行为：该资源状态标记为 Unknown，附消息 `resource not found`
   c. 用户感知：健康报告中该资源 status=Unknown
2. **后端检查超时**
   a. 触发条件：后端 DB 检查超时
   b. 系统行为：复用 TimeoutHealthChecker，超时后标记 Unhealthy
   c. 用户感知：健康报告中该资源 status=Unhealthy, message=timeout

## 5.9 优雅关闭超时配置

### 5.9.1 业务规则

1. **可配置超时**：连接池优雅关闭必须支持可配置超时，新增 `shutdown_with_timeout(timeout)` 方法，超时后强制关闭。
   a. 验收条件：[调用 shutdown_with_timeout(10s)，有在途连接 15 秒后才归还] → [10 秒后强制关闭，不等待 15 秒]
2. **既有 shutdown 保留**：既有 `shutdown()`（`packages/sz-orm-core/src/pool.rs:1695`，硬编码 30 秒）必须保留，行为不变，`shutdown_with_timeout` 为新增。
   a. 验收条件：[调用既有 shutdown()] → [行为与 v3.7.0 一致，30 秒超时]
3. **超时保证**：shutdown_with_timeout(timeout) 必须在 timeout 内完成或在 timeout 后强制关闭，不无限等待。
   a. 验收条件：[shutdown_with_timeout(5s)] → [在 5 秒内完成或 5 秒后强制关闭，总时间不超过 5 秒 + 容差]
4. **关闭后拒绝新请求**：关闭后新 acquire 必须立即返回 Closed 错误，不等待。
   a. 验收条件：[shutdown_with_timeout 后调用 acquire()] → [立即返回 PoolError::Closed]

### 5.9.2 交互流程

```plantuml
@startuml
title 优雅关闭超时 交互流程
actor "运维/SRE" as sre
participant "sz-orm-core" as core
participant "连接池" as pool

sre -> core : shutdown_with_timeout(10s)
core -> pool : 标记关闭(拒绝新acquire)
core -> pool : 通知等待者
core -> pool : 关闭空闲连接
core -> pool : 等待在途连接归还(超时10s)
alt 在途连接10s内归还
pool --> core : 全部归还
else 10s超时
core -> pool : 强制关闭剩余在途连接
end
core --> sre : 关闭完成
@enduml
```

### 5.9.3 异常场景

1. **在途连接超时未归还**
   a. 触发条件：shutdown_with_timeout 超时，仍有在途连接未归还
   b. 系统行为：强制关闭剩余在途连接，记录告警
   c. 用户感知：告警 `graceful shutdown timeout, N connections force closed`
2. **重复调用关闭**
   a. 触发条件：已调用 shutdown 后再次调用
   b. 系统行为：幂等，直接返回不重复执行
   c. 用户感知：无副作用

## 5.10 K8s readiness/liveness probe 配置

### 5.10.1 业务规则

1. **双探针端点**：必须提供 readiness 与 liveness 两个独立 HTTP 端点，路径可配置（默认 `/ready` 与 `/live`），两者独立管理。
   a. 验收条件：[配置 `probe.ready_path=/ready, probe.live_path=/live`] → [GET /ready 返回就绪状态，GET /live 返回存活状态，两者独立]
2. **就绪与存活语义区分**：readiness 反映"是否就绪可接流量"（连接池已建立、依赖可用），liveness 反映"进程是否存活"（不依赖外部资源，避免误杀）。
   a. 验收条件：[DB 暂时不可用] → [/ready 返回 503（未就绪），/live 返回 200（进程存活，不重启）]
3. **复用既有 ProbeManager**：必须复用 sz-orm-health 既有 ProbeManager（`packages/sz-orm-health/src/advanced.rs:9`），不重复实现。
   a. 验收条件：[探针端点] → [基于既有 ProbeManager 的 readiness/liveness 管理]
4. **K8s 集成配置输出**：须提供 K8s 探针配置片段输出（yaml 片段），含 livenessProbe/readinessProbe 的 path、port、initialDelaySeconds、periodSeconds。
   a. 验收条件：[生成 K8s 配置] → [输出含 livenessProbe.httpGet.path=/live, port=8080 等的 yaml 片段]

### 5.10.2 交互流程

```plantuml
@startuml
title K8s 探针配置 交互流程
cloud "Kubernetes" as k8s
participant "sz-orm-health" as health
participant "ProbeManager" as probe

k8s -> health : GET /live (liveness)
health -> probe : liveness 检查
probe --> health : 存活(进程级)
health --> k8s : 200 OK
k8s -> health : GET /ready (readiness)
health -> probe : readiness 检查(含依赖)
probe --> health : 就绪/未就绪
health --> k8s : 200/503
@enduml
```

### 5.10.3 异常场景

1. **readiness 检查依赖不可用**
   a. 触发条件：readiness 检查的依赖资源（DB 连接池）不可用
   b. 系统行为：返回 503，K8s 将 Pod 标记为未就绪，停止转发流量
   c. 用户感知：HTTP 503，Pod 从 Service endpoints 摘除
2. **liveness 检查误判**
   a. 触发条件：liveness 检查依赖了外部资源，外部资源不可用导致 liveness 失败
   b. 系统行为：liveness 须仅检查进程级存活，不依赖外部资源，避免误杀
   c. 用户感知：liveness 始终 200（进程存活），不触发 K8s 重启

## 5.11 SQL 注入防护生产验证

### 5.11.1 业务规则

1. **全路径参数化验证**：生产部署前必须验证所有查询路径均使用参数化查询（where_eq/or_where_eq 等），无 SQL 字符串拼接。
   a. 验收条件：[运行 SQL 注入扫描 `scripts/check-sql-injection.ps1`] → [扫描通过，无拼接 SQL 命中]
2. **禁止项验证**：必须验证无 `where_cond`/`or_where`（已 deprecated）的使用，无 SQL 字符串拼接。
   a. 验收条件：[扫描代码中 where_cond/or_where 调用] → [无命中，全部使用参数化 where_eq/or_where_eq]
3. **编译期验证**：通过 sz-orm-macros 的 `query!` 宏（db-verify feature）在编译期验证 SQL 参数化。
   a. 验收条件：[启用 db-verify feature 编译] → [编译期校验所有 query! 宏 SQL 参数化，非参数化编译失败]
4. **检查清单化**：SQL 注入防护验证必须纳入生产就绪检查清单，附代码证据（file:line）。
   a. 验收条件：[执行检查清单 SQL 注入项] → [输出通过/失败结论，附扫描脚本路径与命中 file:line]

### 5.11.2 交互流程

```plantuml
@startuml
title SQL 注入防护验证 交互流程
actor "维护者" as dev
participant "扫描脚本" as scan
participant "编译期验证" as compile

dev -> scan : 运行 check-sql-injection.ps1
scan -> scan : 扫描所有 .rs 文件
scan --> dev : 扫描结果(通过/命中file:line)
dev -> compile : cargo build --features db-verify
compile -> compile : 编译期校验 query! 宏
compile --> dev : 编译结果(通过/失败)
@enduml
```

### 5.11.3 异常场景

1. **扫描发现 SQL 拼接**
   a. 触发条件：扫描发现 SQL 字符串拼接或 deprecated where_cond 使用
   b. 系统行为：扫描失败，输出命中 file:line
   c. 用户感知：检查清单该项标记 FAIL，附命中位置
2. **编译期验证失败**
   a. 触发条件：db-verify feature 编译期发现非参数化 SQL
   b. 系统行为：编译失败，指明错误位置
   c. 用户感知：编译错误 `non-parameterized SQL detected at ...`

## 5.12 连接泄漏检测生产配置

### 5.12.1 业务规则

1. **检测配置项**：连接泄漏检测必须支持配置：检测开关、检测周期（默认 60 秒）、泄漏告警阈值（借出超时未归还的连接数）、借出超时（默认 60 秒）。
   a. 验收条件：[配置 `leak_detection.enabled=true, threshold=5, borrow_timeout=60s`] → [借出超 60 秒未归还的连接超 5 个时告警]
2. **运行时报告**：必须提供运行时泄漏报告，含当前借出连接数、最长借出时长、疑似泄漏连接列表。
   a. 验收条件：[查询泄漏报告] → [返回借出数、最长借出时长、疑似泄漏连接（含借出时间、调用栈）]
3. **默认关闭**：泄漏检测默认关闭（避免开销），仅在显式配置时启用。
   a. 验收条件：[未配置 leak_detection] → [检测不启用，行为与 v3.7.0 一致]

### 5.12.2 交互流程

```plantuml
@startuml
title 连接泄漏检测 交互流程
participant "检测器" as detector
participant "连接池" as pool
actor "运维/SRE" as sre

detector -> pool : 周期检查(60s)
pool -> pool : 遍历借出连接, 检查借出时长
pool --> detector : 疑似泄漏连接列表
alt 泄漏数 > 阈值
detector -> sre : 告警(泄漏连接详情)
end
sre -> detector : 查询泄漏报告
detector --> sre : 报告(借出数/最长时长/疑似列表)
@enduml
```

### 5.12.3 异常场景

1. **检测到连接泄漏**
   a. 触发条件：借出超时未归还的连接数超过告警阈值
   b. 系统行为：输出告警，含泄漏连接借出时间与调用栈
   c. 用户感知：告警 `connection leak detected: N connections borrowed > 60s`
2. **检测开销过大**
   a. 触发条件：连接池规模大，周期检查开销超 10ms
   b. 系统行为：记录性能告警，建议调大检测周期
   c. 用户感知：告警 `leak detection overhead high, consider increasing interval`

## 5.13 N+1 查询检测生产调优

### 5.13.1 业务规则

1. **检测阈值可配置**：N+1 查询检测器（N1QueryDetector）的阈值（检测窗口、告警阈值、拦截开关）必须可通过生产配置设定。
   a. 验收条件：[配置 `n1_detection.window=1s, threshold=10, block=true`] → [1 秒内同一查询 10 次触发告警并拦截]
2. **拦截与告警区分**：必须支持仅告警不拦截（观察模式）与告警且拦截（防护模式），通过配置切换。
   a. 验收条件：[配置 `block=false`] → [N+1 触发时仅告警不拦截，查询继续执行]
3. **运行时统计**：必须提供运行时 N+1 检测统计（触发次数、拦截次数、Top N 查询）。
   a. 验收条件：[查询 N+1 统计] → [返回触发次数、拦截次数、Top 10 高频查询]

### 5.13.2 交互流程

```plantuml
@startuml
title N+1 检测调优 交互流程
actor "应用" as app
participant "sz-orm-core" as core
participant "N1QueryDetector" as n1

app -> core : 查询(含关联预加载)
core -> n1 : 记录查询(窗口内计数)
n1 -> n1 : 检测是否超阈值
alt 超阈值且 block=true
n1 --> core : 拦截
core --> app : 错误(N+1 detected, blocked)
else 超阈值且 block=false
n1 --> core : 告警(不拦截)
core --> app : 查询继续执行
end
@enduml
```

### 5.13.3 异常场景

1. **N+1 误报**
   a. 触发条件：合法的批量查询被误判为 N+1
   b. 系统行为：告警但（若 block=true）拦截，需调优阈值
   c. 用户感知：告警/拦截 `N+1 suspected: query X executed N times in window`
2. **检测器开销**
   a. 触发条件：检测器单次检测开销超 0.1ms
   b. 系统行为：记录性能告警
   c. 用户感知：告警 `N+1 detection overhead high`

## 5.14 连接池生产参数调优

### 5.14.1 业务规则

1. **生产参数配置项**：连接池参数（max_size、acquire_timeout、idle_timeout、connection_timeout、query_timeout、min_idle、prewarm）必须可通过生产配置文件设定。
   a. 验收条件：[配置 `pool.max_size=50, acquire_timeout=10s`] → [连接池按配置参数生效]
2. **参数合理性验证**：配置加载时必须验证参数合理性（max_size > 0、各 timeout > 0、min_idle <= max_size），不合理则拒绝启动。
   a. 验收条件：[配置 `max_size=0`] → [配置验证拒绝，返回错误 `pool max_size must be positive`]
3. **运行时动态调整**：max_size 支持运行时动态调整（复用既有 resize，`packages/sz-orm-core/src/pool.rs:1712`），不重启。
   a. 验收条件：[运行时 resize(100)] → [连接池最大容量调整为 100]
4. **参数可观测**：当前连接池参数与运行时统计（active/idle/total、创建/关闭计数）必须可通过运行时查询或 metrics 暴露。
   a. 验收条件：[查询连接池 metrics] → [返回 max_size、active、idle、total、acquire_timeout 等当前值]

### 5.14.2 交互流程

```plantuml
@startuml
title 连接池参数调优 交互流程
actor "运维/SRE" as sre
participant "sz-orm-core" as core
participant "连接池" as pool

sre -> core : 配置池参数(max_size=50, acquire_timeout=10s)
core -> pool : 创建连接池(配置)
sre -> pool : resize(100)
pool -> pool : 动态调整 max_size
sre -> pool : 查询 metrics
pool --> sre : 返回参数+统计
@enduml
```

### 5.14.3 异常场景

1. **参数配置非法**
   a. 触发条件：max_size=0 或 timeout=0 或 min_idle > max_size
   b. 系统行为：配置加载阶段返回错误，拒绝启动
   c. 用户感知：错误提示 `invalid pool config: ...`
2. **resize 缩容冲突**
   a. 触发条件：resize 缩容时有多于新 max_size 的活跃连接
   b. 系统行为：阻止新连接创建，多余连接在 release 时自然回收（复用既有 resize 行为）
   c. 用户感知：连接数逐步收敛到新 max_size

## 5.15 五方言连接安全验证

### 5.15.1 业务规则

1. **五方言覆盖**：连接安全验证必须覆盖 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五种方言，每种方言验证 TLS、认证、连接串脱敏、连接池参数。
   a. 验收条件：[执行五方言连接安全验证] → [五种方言均输出验证报告，全部通过]
2. **连接串脱敏**：所有方言的连接串在日志/错误/审计中必须脱敏（密码字段掩码），禁止明文密码泄露。
   a. 验收条件：[MySQL 连接失败，错误日志含连接串] → [连接串中 password 显示为 `***`，不含 `test123`]
3. **TLS 按方言能力适配**：TLS 验证按方言能力适配（MySQL/PostgreSQL/MSSQL 支持 TLS，SQLite 文件型无需 TLS，Oracle 支持 TLS），不支持 TLS 的方言标记为 N/A。
   a. 验收条件：[SQLite 连接安全验证] → [TLS 项标记 N/A（文件型无需 TLS），其他项正常验证]
4. **方言行为一致性**：连接池参数、优雅关闭、泄漏检测在五方言上行为一致。
   a. 验收条件：[五方言连接池 shutdown_with_timeout(10s)] → [行为一致，10 秒超时保证]

### 5.15.2 交互流程

```plantuml
@startuml
title 五方言连接安全验证 交互流程
actor "维护者" as dev
participant "验证器" as verifier
database "MySQL" as mysql
database "PostgreSQL" as pg
database "SQLite" as sqlite
database "Oracle" as oracle
database "MSSQL" as mssql

dev -> verifier : 执行五方言验证
verifier -> mysql : TLS/认证/脱敏/池参数
verifier -> pg : TLS/认证/脱敏/池参数
verifier -> sqlite : 脱敏/池参数(TLS=N/A)
verifier -> oracle : TLS/认证/脱敏/池参数
verifier -> mssql : TLS/认证/脱敏/池参数
verifier --> dev : 五方言验证报告
@enduml
```

### 5.15.3 异常场景

1. **某方言验证失败**
   a. 触发条件：某方言连接安全验证未通过（如 TLS 未启用、连接串未脱敏）
   b. 系统行为：该方言标记 FAIL，附失败原因与 file:line
   c. 用户感知：验证报告该方言标记 FAIL
2. **方言不可用**
   a. 触发条件：某方言本机不可用（如 MSSQL 未安装）
   b. 系统行为：该方言标记 SKIPPED，附原因
   c. 用户感知：验证报告该方言标记 SKIPPED

---

# 6. 数据约束

## 6.1 生产就绪检查项

1. **检查项 ID**：唯一标识，格式 `REQ-PROD-xxx`，必填。
2. **检查项名称**：人类可读名称，必填。
3. **检查项分类**：安全红线 / 配置可观测 / 阈值调优 / ORM 防护，必填。
4. **验证方法**：可执行的验证命令或测试描述，必填。
5. **代码证据**：相关 file:line 引用，必填，遵循审计合规铁律。
6. **验收条件**：触发场景 → 预期行为，必填。
7. **状态**：PASS / FAIL / SKIPPED / PENDING，必填。

## 6.2 生产配置对象

1. **环境标识**：production / development / staging，必填，决定日志级别、metrics 访问控制等强制策略。
2. **Redis TLS 配置**：enabled（bool）、ca_cert_path（string）、client_cert_path（Option）、client_key_path（Option）、sni（Option）、skip_verify（bool，生产禁止 true）。
3. **JWT 密钥集配置**：keys（Map<kid, secret>）、active_kid（string）、min_secret_length（u32，默认 32）。
4. **限流配置**：capacity（u64）、rate（u64，每秒）、window_size（Duration）、max_keys（usize，默认 10000）。
5. **熔断配置**：failure_threshold（u32）、reset_timeout（Duration）。
6. **日志配置**：level（error/warn/info/debug/trace）、env（环境标识）。
7. **metrics 访问控制配置**：enabled（bool）、ip_whitelist（Vec<Cidr>）、bearer_token（Option）、basic_auth（Option）。
8. **健康检查配置**：path（string，默认 /health）、port（u16）、resources（Vec<string>）、cache_ttl（Duration）。
9. **优雅关闭配置**：timeout（Duration，默认 30s）。
10. **K8s 探针配置**：ready_path（string，默认 /ready）、live_path（string，默认 /live）、port（u16）、initial_delay_seconds（u32）、period_seconds（u32）。
11. **连接泄漏检测配置**：enabled（bool，默认 false）、interval（Duration，默认 60s）、threshold（u32）、borrow_timeout（Duration，默认 60s）。
12. **N+1 检测配置**：window（Duration）、threshold（u32）、block（bool）。
13. **连接池配置**：max_size（u32）、acquire_timeout（Duration）、idle_timeout（Duration）、connection_timeout（Duration）、query_timeout（Option<Duration>）、min_idle（Option<u32>）、prewarm（Option<u32>）。

## 6.3 验证报告

1. **检查项 ID**：对应 REQ-PROD-xxx，必填。
2. **状态**：PASS / FAIL / SKIPPED，必填。
3. **证据**：file:line 引用与验证输出，必填。
4. **时间戳**：验证执行时间，必填。
5. **失败原因**：状态为 FAIL 时的原因描述，可选。

---

# 7. 需求追溯矩阵

| 需求编号 | 检查项 | 分类 | 验收条件（节选） | 现有代码证据 |
|---------|--------|------|----------------|-------------|
| REQ-PROD-001 | 配置敏感字段脱敏验证 | 安全红线 | 配置加载后验证无明文残留 | `packages/sz-orm-config/src/lib.rs:1479` ConfigEncryption、`packages/sz-orm-masking/src/lib.rs:21` DataMasker |
| REQ-PROD-002 | Redis 连接 TLS 加密 | 安全红线 | TLS 配置启用后加密传输 | `packages/sz-orm-queue/src/`（当前无 TLS，需新增） |
| REQ-PROD-003 | JWT 签名密钥轮换 | 安全红线 | 多 kid 密钥并存，无停机轮换 | `packages/sz-orm-auth/src/jwt.rs:1`（当前单密钥，需新增 kid） |
| REQ-PROD-004 | 限流阈值生产调优 | 阈值调优 | 配置阈值生效，可动态调整 | `packages/sz-orm-limit/src/lib.rs:21` DEFAULT_MAX_KEYS |
| REQ-PROD-005 | 熔断器阈值生产调优 | 阈值调优 | 配置阈值通过 configure_circuit_breaker 生效 | `packages/sz-orm-core/src/circuit_breaker.rs:3`、`packages/sz-orm-core/src/pool.rs:1173` |
| REQ-PROD-006 | 日志级别生产配置 | 配置可观测 | 生产强制 warn+，禁止 debug | `packages/sz-orm-logger/src/lib.rs` |
| REQ-PROD-007 | metrics 端点访问控制 | 安全红线 | IP 白名单/认证生效 | `packages/sz-orm-observability/src/lib.rs:418`（当前裸暴露，需新增鉴权层） |
| REQ-PROD-008 | 健康检查端点配置 | 配置可观测 | HTTP 端点返回聚合健康状态 | `packages/sz-orm-health/src/lib.rs:26` HealthReport、`packages/sz-orm-health/src/advanced.rs` HealthCheckCache |
| REQ-PROD-009 | 优雅关闭超时配置 | 配置可观测 | shutdown_with_timeout 超时保证 | `packages/sz-orm-core/src/pool.rs:1695` shutdown（当前硬编码 30s `:1703`，需新增可配置版本） |
| REQ-PROD-010 | K8s readiness/liveness probe | 配置可观测 | 双探针端点独立，语义区分 | `packages/sz-orm-health/src/advanced.rs:9` ProbeManager |
| REQ-PROD-011 | SQL 注入防护生产验证 | ORM 防护 | 全路径参数化，扫描通过 | `scripts/check-sql-injection.ps1`、sz-orm-macros db-verify feature |
| REQ-PROD-012 | 连接泄漏检测配置 | ORM 防护 | 借出超时未归还超阈值告警 | `packages/sz-orm-core/src/pool.rs:310`（已有防护注释，需补检测配置） |
| REQ-PROD-013 | N+1 查询检测调优 | ORM 防护 | 阈值可配置，拦截/告警可切换 | sz-orm-core N1QueryDetector（既有自动拦截） |
| REQ-PROD-014 | 连接池参数调优 | 阈值调优 | 参数可配置，可动态调整 | `packages/sz-orm-core/src/pool.rs:449` PoolConfig、`packages/sz-orm-core/src/pool.rs:1712` resize |
| REQ-PROD-015 | 五方言连接安全验证 | ORM 防护 | 五方言验证报告全部通过 | MySQL/PostgreSQL/SQLite/Oracle/MSSQL 连接路径 |

---

# 8. 验收标准总览

## 8.1 安全红线类（最高优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-PROD-001 | 配置加载后敏感字段全部脱敏/加密，无明文残留 | 调用脱敏验证入口，检查报告无 VIOLATION |
| REQ-PROD-002 | Redis TLS 配置启用后加密传输，生产禁止跳过证书校验 | 配置 TLS 连接 Redis，抓包验证加密；配置 skip_verify=true 验证拒绝 |
| REQ-PROD-003 | JWT 多 kid 密钥并存，无停机轮换，密钥 ≥32 字节 | 配置双密钥，签发+验证新旧令牌；配置短密钥验证拒绝 |
| REQ-PROD-007 | metrics 端点访问控制生效，生产禁止裸暴露 | 配置 IP 白名单/Token，验证非授权 403；生产裸暴露验证告警 |
| REQ-PROD-011 | 全路径参数化，无 SQL 拼接，无 deprecated where_cond | 运行 check-sql-injection.ps1 + db-verify 编译 |

## 8.2 配置可观测类（高优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-PROD-006 | 生产日志 warn+，禁止 debug/trace | 配置 debug 验证拒绝；运行验证无 debug 输出 |
| REQ-PROD-008 | 健康检查 HTTP 端点返回聚合状态，缓存 TTL 生效 | GET /health 验证返回 JSON；连续请求验证缓存命中 |
| REQ-PROD-009 | shutdown_with_timeout 超时保证，既有 shutdown 不变 | 调用 shutdown_with_timeout(5s) 验证 5 秒强制关闭；调用 shutdown() 验证 30 秒 |
| REQ-PROD-010 | readiness/liveness 双端点独立，语义区分 | DB 不可用时 /ready 503、/live 200 |

## 8.3 阈值调优类（高优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-PROD-004 | 限流阈值配置生效，可动态调整，可观测 | 配置阈值验证生效；运行时调整验证立即生效；查询统计 |
| REQ-PROD-005 | 熔断阈值配置生效，可查询状态 | 配置阈值通过 configure_circuit_breaker 生效；查询状态 |
| REQ-PROD-014 | 连接池参数可配置，可动态调整，可观测 | 配置参数验证生效；resize 验证动态调整；查询 metrics |

## 8.4 ORM 防护类（中优先级）

| 编号 | 验收标准 | 验证方法 |
|------|---------|---------|
| REQ-PROD-012 | 连接泄漏检测配置生效，可报告 | 配置检测，模拟泄漏，验证告警与报告 |
| REQ-PROD-013 | N+1 检测阈值可配置，拦截/告警可切换 | 配置阈值与 block，触发 N+1 验证告警/拦截 |
| REQ-PROD-015 | 五方言连接安全验证全部通过 | 执行五方言验证，检查报告全部 PASS（不可用 SKIPPED） |

## 8.5 全局验收条件

1. **API 兼容性**：v3.8.0 既有公开 API 完全向后兼容，sz-pay 既有代码不受影响。
2. **feature gate 隔离**：所有新能力通过 `prod-ready` feature gate 隔离，默认 feature 行为不变。
3. **测试基线不回退**：v3.7.0 已验收测试基线不回退，v3.8.0 仅增不减。
4. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（TLS 按方言能力适配）。
5. **审计证据**：每项检查结论附 file:line 证据，遵循 AGENTS.md 审计合规铁律。
6. **14 道门禁通过**：v3.8.0 须通过 AGENTS.md 定义的 14 道门禁（fmt/check/clippy/test/doc/audit/integration/占位检查/SQL注入/feature全组合/上游未改/文档一致/审计证据/文档同步）。