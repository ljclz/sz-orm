# sz-orm v4.8.0 技术设计文档

> 版本：v4.8.0（跨语言分布式事务协调 + 低代码双向同步 + OpenAPI 反向生成 + WASM 真实数据库连接闭环）
> 基线：v4.7.0（消息延迟队列与优先级调度 + 迁移前向兼容性检查与沙箱预演 + 批量 COPY 协议与并行分片执行 + 异常自愈与根因分析 + 多云成本对比与容量预测 + 租户资源配额与行级安全增强 + 缓存预热与穿透防护，7 项需求 REQ-V47-001~007 全部通过 feature gate 隔离，228 套测试 0 失败，已发布到 crates.io 4.7.0）
> 日期：2026-08-14
> 文档定位：技术设计（How to build），对应需求规格 `spec.md`（What to build，848 行，4 项 EARS 需求 REQ-V48-001~004）
> 设计约束：无 Breaking Change（4 个 feature gate 隔离，默认全关闭）+ 优先复用既有能力 + 五方言覆盖（MySQL/PostgreSQL/SQLite/Oracle/MSSQL）+ 每项设计附 file:line 代码证据 + unsafe 零容忍 + 禁止占位实现 + 与 v4.7.0 零重叠 + 不新增包（全部通过既有包扩展实现）+ 参数化查询铁律 + 严禁幻影交付（每项新能力附生产调用点 + 端到端接线测试）
> 需求依赖：四项需求主体相互独立，可并行开发（详见 §三 依赖关系图）
> 证据验证：本文档所有 file:line 证据均已通过源码读取验证（2026-08-14，40+ 项关键证据逐项实测，行号均为实际存在行），遵循 AGENTS.md 审计合规铁律

---

# 概述

## 设计目标

本设计文档将 sz-orm v4.8.0 四项跨语言互操作与全栈闭环需求（REQ-V48-001 ~ REQ-V48-004）转化为可落地的技术方案，核心目标：

1. **跨语言分布式事务协调**：扩展既有 `sz-orm-dtx` 包 `cross-lang-dtx` feature，`TonicGrpcCallHandler` 真实 tonic gRPC 传输 + `ReqwestHttpCallHandler` 真实 HTTP/JSON 传输 + `CrossLangRecoveryCoordinator` 跨语言崩溃恢复协调器 + `CrossLangSagaCoordinator` 跨语言 Saga 编排器 + `CrossLangTccCoordinator` 跨语言 TCC 编排器 + 各语言 SDK 接入契约，复用既有 `CrossLangParticipantProtocol`（`packages/sz-orm-dtx/src/cross_lang/mod.rs:108`）/ `GrpcParticipantProtocol`（`packages/sz-orm-dtx/src/cross_lang/protocol.rs:26`）/ `HttpParticipantProtocol`（`:78`）/ `CrossLangParticipant::to_participant()`（`packages/sz-orm-dtx/src/cross_lang/participant.rs:51`）/ `TransactionLogStore`（`packages/sz-orm-dtx/src/lib.rs:57`）/ `recovery`/`saga`/`tcc` + sz-orm-grpc `real_grpc.rs`（`RealGrpcServer` `:94` / `RealGrpcClient` `:199`），补齐真实传输实现 + 崩溃恢复 + Saga/TCC 深度编排，替代既有仅测试用的 `MockRemoteCallHandler`（`packages/sz-orm-dtx/src/cross_lang/protocol.rs:130`）。
2. **低代码双向同步**：扩展既有 `sz-orm-lc` + `sz-orm-designer` 包，`BidirectionalSyncEngine` 双向同步引擎 + `SyncDirection` 同步方向枚举 + `SyncConflictDetector` 冲突检测器 + `SyncConflictResolver` 冲突解决器 + `ConflictResolutionStrategy` 冲突解决策略枚举 + `SyncIncrementTracker` 增量追踪器 + `SyncAuditLogger` 同步审计日志器，复用既有 `ModelDefinition`（`packages/sz-orm-lc/src/lib.rs:24`）/ `FieldDef`（`:83`）/ `RelationDefinition`（`:147`）/ `FieldTypeMapping`（`:210`）/ `ValidationRule`（`:362`）+ `sz-orm-designer` `schema-designer` feature（`packages/sz-orm-designer/src/lib.rs:1`），补齐 ORM 模型 ↔ 低代码引擎模型双向同步 + 冲突检测与解决 + 增量追踪。
3. **OpenAPI 反向生成**：扩展既有 `sz-orm-swagger` 包 `openapi-reverse` feature，`DbSchemaReader` 五方言 schema 读取器 + `DbSchemaToOpenApiMapper` DB schema → OpenAPI 3.0 规范映射器 + `DbSchemaToCrudApiMapper` DB schema → CRUD API 映射器 + `CrudApiEndpoint` CRUD API 端点定义 + `FullReverseLoopVerifier` 完整闭环验证器，复用既有 `OpenApiReverseGenerator`（`packages/sz-orm-swagger/src/reverse/mod.rs:25`）/ `SchemaToModelMapper`（`:29`）/ `OpenApiToMigrationMapper`（`:28`）/ `OpenApiToRepositoryMapper`（`:30`）/ `ApiFirstLoopVerifier`（`:27`）/ `OpenApiInjectionGuard`（`:26`）/ `ReverseGenConfig`/`NamingConvention`（`:24`），补齐 DB schema → OpenAPI 规范 + CRUD API 反向生成方向，形成完整闭环（DB schema → OpenAPI → ORM Model → CRUD）。
4. **WASM 真实数据库连接闭环**：扩展既有 `sz-orm-wasm` 包 `wasm-real-db` feature，`WasmProxyServer` 代理后端服务 + `MultiDialectProxyBackend` 多方言代理后端 + `WasmOrmSession` WASM 端 ORM 会话 + `WasmQueryBuilderBridge` WASM 端查询构建器桥接 + `WasmOrmLoopVerifier` WASM ORM 闭环验证器，复用既有 `WasmRealDbConnection`（`packages/sz-orm-wasm/src/real_db/mod.rs:19`）/ `WasmRealDbQueryExecutor`（`:20`）/ `WasmDbProxy`（`:25`）/ `WasmDbProxyProtocol`/`ProxyRequest`/`ProxyResponse`（`:22`）/ `WasmDbAuthValidator`（`:18`）/ `WasmDbSqlWhitelist`（`:28`）/ `WasmDbRateLimiter`（`:26`）/ `WasmRealDbReconnector`（`:27`）/ `WasmRealDbMetrics`（`:21`）/ `WasiSocketConnection`（`packages/sz-orm-wasm/src/real_db/wasi_socket.rs:13`）+ sz-orm-core 连接池 `Pool`（`packages/sz-orm-core/src/pool.rs:749`）+ sz-orm-query-builder，补齐浏览器端 ORM 操作完整闭环 + 代理后端实现 + 多方言代理 + WASM 端查询构建器集成。

## 设计约束

| 约束类别 | 约束内容 | 来源 |
|---------|---------|------|
| 兼容性 | 无 Breaking Change，4 个 feature gate 隔离，默认全关闭，既有公开 API 完全向后兼容 | spec.md §1.4 / §4.5.1 |
| sz-pay 不破坏 | sz-pay 从 crates.io 拉取 sz-orm-* 6 个包既有用法不受影响 | spec.md §4.5.3 |
| 五方言覆盖 | MySQL/PostgreSQL/SQLite/Oracle/MSSQL 行为一致（OpenAPI schema 读取 + WASM 代理后端全方言覆盖） | spec.md §4.5.4 / §1.2.5 |
| 复用优先 | 优先复用既有能力，不重复实现（4 项需求全部通过既有包扩展，不新增包） | spec.md §1.4 / §10.4 |
| unsafe 零容忍 | 无 `unsafe` 块，或必须有 `// SAFETY:` 注释 | spec.md §1.4.14 / §4.3 |
| 禁止占位实现 | 禁止 `todo!`/`unimplemented!`/`unreachable!` | AGENTS.md / spec.md §1.4.15 |
| 参数化查询 | 任何 WHERE 条件必须参数化，禁止 SQL 字符串拼接（复用既有 `Connection::execute_with_params` + sz-orm-query-builder `where_eq`/`or_where_eq`） | AGENTS.md / spec.md §4.3.7 |
| 测试基线不回退 | v4.7.0 已验收 228 套测试基线仅增不减 | spec.md §4.5.2 |
| 审计证据 | 每项结论附 file:line 证据，遵循审计合规铁律 | spec.md §4.4.5 / AGENTS.md |
| 与 v4.7.0 零重叠 | v4.7.0 是"智能化运维深化+性能深化"层，v4.8.0 是"跨语言互操作+全栈闭环"层，新增范围全部落在既有包扩展（sz-orm-dtx/grpc/lc/designer/swagger/wasm） | spec.md §1.4 / §10.1 |
| 不新增包 | 4 项需求全部通过既有包扩展实现，workspace 保持 60 成员 | spec.md §10.4 |
| 严禁幻影交付 | 每项新能力附生产调用点 + 端到端接线测试，"模块存在+测试通过"≠"已交付" | AGENTS.md / session-rules |
| 跨语言事务原子性 | 跨语言分布式事务满足原子性，崩溃后可恢复至一致状态，复用 `TransactionLogStore` 日志持久化 | spec.md §4.2.1 |
| 跨语言事务幂等性 | 跨语言参与者 prepare/commit/rollback 须幂等，`CrossLangCompensationSerializer` 生成幂等键 | spec.md §4.2.2 / §5.1.1.6 |
| 跨语言事务鉴权 | 跨语言参与者调用须鉴权（mTLS/Token），协议版本须匹配 | spec.md §4.3.1 |
| 低代码同步一致性 | 双向同步后 ORM 模型与低代码引擎模型一致，冲突按策略解决后记录审计日志 | spec.md §4.2.3 |
| 禁止自动破坏性 schema 变更 | 双向同步检测到破坏性变更（删列/改类型/改约束）须人工确认，不自动执行 | spec.md §5.2.1.10 |
| OpenAPI 注入防护 | OpenAPI spec 须签名验证，`OpenApiInjectionGuard` 检测注入，拒绝未签名/含注入 spec | spec.md §4.3.3 / §5.3.1.7 |
| OpenAPI 闭环一致 | DB schema → OpenAPI → ORM Model → CRUD 闭环验证须一致 | spec.md §4.2.4 / §5.3.1.4 |
| WASM 代理凭据隔离 | 后端数据库凭据不暴露给 WASM 端（`WasmRealDbError::CredentialsNotExposed`） | spec.md §4.2.6 / §5.4.1.1 |
| WASM 代理 SQL 白名单 | WASM 端查询须经 SQL 白名单检查，拒绝非白名单 SQL | spec.md §4.3.4 |
| WASM 代理限流 | WASM 端查询须限流，超限拒绝 | spec.md §4.3.5 |
| WASM 禁止直连 DB | 浏览器 WASM 环境不可直连数据库，须通过代理后端桥接 | spec.md §1.4.20 / §5.4.1.12 |
| 结果集大小限制 | 返回 WASM 端的结果集大小可配（默认 10MB），超限返回 `ResultTooLarge` | spec.md §5.4.1.9 |

## feature gate 总览

| feature | 所属包 | 控制能力 | 默认 | 对应需求 | 状态 | 依赖既有 feature |
|---------|--------|---------|------|---------|------|----------------|
| `cross-lang-dtx` | sz-orm-dtx（扩展） | 跨语言分布式事务协调（真实 gRPC/HTTP 传输 + 崩溃恢复 + Saga/TCC 深度编排） | 关闭 | REQ-V48-001 | 既有 feature 扩展（`packages/sz-orm-dtx/Cargo.toml:40`） | 既有协议层 `cross_lang` 模块（复用 `CrossLangParticipantProtocol`/`GrpcParticipantProtocol`/`HttpParticipantProtocol`） |
| `lc-bidirectional-sync` | sz-orm-lc（扩展） | 低代码双向同步（双向同步引擎 + 冲突检测解决 + 增量追踪） | 关闭 | REQ-V48-002 | 新增 feature | 既有 `ModelDefinition`/`FieldTypeMapping`/`ValidationRule` + `sz-orm-designer` `schema-designer` feature |
| `openapi-reverse` | sz-orm-swagger（扩展） | OpenAPI 反向生成（DB schema → OpenAPI + CRUD + 闭环验证） | 关闭 | REQ-V48-003 | 既有 feature 扩展（`packages/sz-orm-swagger/Cargo.toml:14`） | 既有 `OpenApiReverseGenerator`/`SchemaToModelMapper`（OpenAPI→ORM 方向） |
| `wasm-real-db` | sz-orm-wasm（扩展） | WASM 真实数据库连接闭环（代理后端 + ORM 闭环 + 多方言代理） | 关闭 | REQ-V48-004 | 既有 feature 扩展（`packages/sz-orm-wasm/Cargo.toml:35`） | 既有 `WasmDbProxy`/`WasmRealDbConnection` 代理桥接 + sz-orm-core 连接池 + sz-orm-query-builder |

**既有 feature gate 位置（v4.7.0 基线，扩展基线）：**
- `cross-lang-dtx`：`packages/sz-orm-dtx/Cargo.toml:40`（v4.7.0 已有协议层，本版本补真实传输 + 恢复 + 深度编排）
- `openapi-reverse`：`packages/sz-orm-swagger/Cargo.toml:14`（v4.7.0 已有 OpenAPI→ORM 方向，本版本补 DB→OpenAPI 方向）
- `wasm-real-db`：`packages/sz-orm-wasm/Cargo.toml:35`（v4.7.0 已有代理桥接，本版本补代理后端 + ORM 闭环）
- `wasi-socket`：`packages/sz-orm-wasm/Cargo.toml:36`（既有，`wasm-real-db` 子 feature，WASI socket 直连代理）
- `schema-designer`：`packages/sz-orm-designer/src/lib.rs:1`（既有，低代码双向同步复用，不新增 feature）

## 架构总览

### 扩展包总览（不新增包，workspace 保持 60 成员）

| 包名 | 对应需求 | 依赖（只读复用） | 扩展内容 |
|------|---------|----------------|---------|------|
| `sz-orm-dtx` | REQ-V48-001 | 既有 `cross_lang` 协议层 + 既有事务核心（`TransactionLogStore`/`DistributedTransaction`/`DtxManager`/`TransactionParticipant`/`TransactionState`）+ 既有 `saga`/`tcc`/`recovery` | 真实 gRPC/HTTP 传输 + 跨语言崩溃恢复 + Saga/TCC 深度编排（`cross-lang-dtx` feature 扩展） |
| `sz-orm-grpc` | REQ-V48-001 | 既有 `GrpcServiceDef`/`UserGrpcService`/`GrpcStream`/`Interceptor`/`RetryPolicy` + `real_grpc.rs`（`RealGrpcServer`/`RealGrpcClient` tonic 桥接） | gRPC 基础设施复用（不新增 feature，跨语言真实 gRPC 传输复用 `real_grpc`） |
| `sz-orm-lc` | REQ-V48-002 | 既有 `ModelDefinition`/`FieldDef`/`RelationDefinition`/`FieldTypeMapping`/`ValidationRule`/`FormField`/`FormGenerator`/`CrudTemplateEngine` | 双向同步引擎 + 冲突检测解决 + 增量追踪 + 审计日志（`lc-bidirectional-sync` feature 新增） |
| `sz-orm-designer` | REQ-V48-002 | 既有 `schema-designer` feature（`SchemaDesigner`/`ErDiagramEditor`/`code_gen`/`code_parse`/`design_ir`/`exporter`/`masking`/`web_ui`） | schema 设计器复用（不新增 feature，双向同步复用既有设计器能力） |
| `sz-orm-swagger` | REQ-V48-003 | 既有 `OpenApiReverseGenerator`/`SchemaToModelMapper`/`OpenApiToMigrationMapper`/`OpenApiToRepositoryMapper`/`ApiFirstLoopVerifier`/`OpenApiInjectionGuard`/`ReverseGenConfig`/`NamingConvention` | DB schema → OpenAPI + CRUD + 闭环验证（`openapi-reverse` feature 扩展） |
| `sz-orm-wasm` | REQ-V48-004 | 既有 `WasmRealDbConnection`/`WasmRealDbQueryExecutor`/`WasmDbProxy`/`WasmDbProxyProtocol`/`WasmDbAuthValidator`/`WasmDbSqlWhitelist`/`WasmDbRateLimiter`/`WasmRealDbReconnector`/`WasmRealDbMetrics`/`WasiSocketConnection` + sz-orm-core `Pool` + sz-orm-query-builder | 代理后端 + ORM 闭环 + 多方言代理 + 查询构建器桥接（`wasm-real-db` feature 扩展） |

### 依赖关系图

```plantuml
@startuml
title sz-orm v4.8.0 四项需求 依赖关系

rectangle "sz-orm v4.8.0 扩展（不新增包，workspace 保持 60）" as v48 {
  rectangle "sz-orm-dtx\n(REQ-V48-001 跨语言事务协调)" as dtx
  rectangle "sz-orm-lc\n(REQ-V48-002 低代码双向同步)" as lc
  rectangle "sz-orm-designer\n(REQ-V48-002 设计器复用)" as designer
  rectangle "sz-orm-swagger\n(REQ-V48-003 OpenAPI 反向生成)" as swagger
  rectangle "sz-orm-wasm\n(REQ-V48-004 WASM 真实连接闭环)" as wasm
}

rectangle "sz-orm-grpc\n(REQ-V48-001 gRPC 基础设施复用)" as grpc

rectangle "v4.7.0 既有 feature 基线" as v47 {
  rectangle "cross-lang-dtx\n(协议层)" as v47dtx
  rectangle "openapi-reverse\n(OpenAPI→ORM)" as v47rev
  rectangle "wasm-real-db\n(代理桥接)" as v47wasm
  rectangle "schema-designer\n(设计器)" as v47designer
}

rectangle "sz-orm-core\n(连接池 Pool)" as core
rectangle "sz-orm-query-builder\n(参数化查询构建)" as qb
database "MySQL/PG/SQLite\n/Oracle/MSSQL" as db
component "跨语言参与者服务\n(Go/Java/C++/Python/JS)" as crosslang_svc
component "低代码引擎" as lc_engine

dtx --> v47dtx : 复用既有 cross_lang 协议层 + 事务核心 + saga/tcc/recovery
dtx --> grpc : 复用 real_grpc tonic 桥接
grpc --> crosslang_svc : 真实 gRPC 传输
dtx --> crosslang_svc : 真实 HTTP 传输

lc --> v47designer : 复用 SchemaDesigner/code_gen/code_parse
lc --> lc_engine : 双向同步

swagger --> v47rev : 复用 OpenApiReverseGenerator/SchemaToModelMapper
swagger --> db : 五方言 schema 读取

wasm --> v47wasm : 复用 WasmDbProxy/WasmRealDbConnection 代理桥接
wasm --> core : 复用 Pool 连接池
wasm --> qb : 复用参数化查询构建
core --> db : 连接 DB
designer --> v47designer : 既有 feature 复用

note right of v48
  v4.8.0 四项需求主体独立
  可并行开发
  与 v4.7.0 零重叠
  不新增包（全部既有包扩展）
  每项复用对应 v4.7.0 既有 feature 基线
end note

@enduml
```

---

# 一、需求与存量功能关系分析

## 1.1 需求功能与存量功能对比

### 1.1.1 已实现功能（可直接复用，匹配度 100%）

本节列出 v4.8.0 四项需求可直接复用的既有功能（v4.7.0 基线 + 既有代码），这些功能无需修改，作为扩展基线。

#### REQ-V48-001 跨语言分布式事务协调 — 既有协议层与事务核心

| 需求功能 | 存量功能 | 代码位置 | 匹配度 |
|---------|---------|---------|--------|
| 跨语言参与者语言枚举 | `ParticipantLanguage`（Go/Java/Cpp/Python/JavaScript 5 语言） | `packages/sz-orm-dtx/src/cross_lang/mod.rs:16` | 100% |
| 参与者传输协议枚举 | `ParticipantTransport`（Grpc/Http） | `packages/sz-orm-dtx/src/cross_lang/mod.rs:38` | 100% |
| 参与者鉴权方式 | `ParticipantAuth`（Mtls{cert,key,ca} / Token） | `packages/sz-orm-dtx/src/cross_lang/mod.rs:45` | 100% |
| 跨语言参与者描述 | `CrossLangParticipantDesc`（resource_id/language/transport/endpoint/auth/protocol_version） | `packages/sz-orm-dtx/src/cross_lang/mod.rs:58` | 100% |
| 参与者响应 | `ParticipantResponse`（success/payload/error/latency_ms） | `packages/sz-orm-dtx/src/cross_lang/mod.rs:75` | 100% |
| 跨语言事务错误 | `CrossLangTxError`（Timeout/AuthFailed/ProtocolVersionMismatch/RecoveryConflict/Transport/CompensationFailed/RemoteCall） | `packages/sz-orm-dtx/src/cross_lang/mod.rs:88` | 100% |
| 跨语言参与者协议 trait | `CrossLangParticipantProtocol`（prepare/commit/rollback/protocol_version） | `packages/sz-orm-dtx/src/cross_lang/mod.rs:108` | 100% |
| 协调器协议版本 | `COORDINATOR_PROTOCOL_VERSION = 1` | `packages/sz-orm-dtx/src/cross_lang/mod.rs:128` | 100% |
| 远程调用 handler trait | `RemoteCallHandler`（call(method, tx_id, payload)） | `packages/sz-orm-dtx/src/cross_lang/protocol.rs:15` | 100% |
| gRPC 参与者协议 | `GrpcParticipantProtocol`（基于 `RemoteCallHandler`） | `packages/sz-orm-dtx/src/cross_lang/protocol.rs:26` | 100% |
| HTTP 参与者协议 | `HttpParticipantProtocol`（基于 `RemoteCallHandler`） | `packages/sz-orm-dtx/src/cross_lang/protocol.rs:78` | 100% |
| 协议版本检查 | `check_protocol_version`（版本不匹配返回 `ProtocolVersionMismatch`） | `packages/sz-orm-dtx/src/cross_lang/protocol.rs:192` | 100% |
| 跨语言参与者适配器 | `CrossLangParticipant`（desc/protocol/timeout_ms） | `packages/sz-orm-dtx/src/cross_lang/participant.rs:12` | 100% |
| 参与者适配为 TransactionParticipant | `CrossLangParticipant::to_participant()`（包装为 `ParticipantCallback` 闭包） | `packages/sz-orm-dtx/src/cross_lang/participant.rs:51` | 100% |
| 补偿序列化器 | `CrossLangCompensationSerializer`（serialize/deserialize/build_compensation） | `packages/sz-orm-dtx/src/cross_lang/serializer.rs:23` | 100% |
| 幂等键生成 | `CrossLangCompensationSerializer::idempotency_key`（tx_id:participant_id:action 确定性生成） | `packages/sz-orm-dtx/src/cross_lang/serializer.rs:70` | 100% |
| 跨语言事务可观测性 | `CrossLangTxAlerter`/`CrossLangTxMetrics`/`AlertHandler`/`IsolationReason` | `packages/sz-orm-dtx/src/cross_lang/observability.rs:12` | 100% |
| 事务日志存储 trait | `TransactionLogStore`（append/read_pending/read_transaction） | `packages/sz-orm-dtx/src/lib.rs:57` | 100% |
| 事务日志读取未完成 | `TransactionLogStore::read_pending` | `packages/sz-orm-dtx/src/lib.rs:71` | 100% |
| 事务状态枚举 | `TransactionState`（Preparing/Prepared/Committing/Committed/RollingBack/RolledBack） | `packages/sz-orm-dtx/src/lib.rs:163` | 100% |
| 事务参与者 | `TransactionParticipant`（with_prepare/with_commit/with_rollback） | `packages/sz-orm-dtx/src/lib.rs:186` | 100% |
| 分布式事务 | `DistributedTransaction` | `packages/sz-orm-dtx/src/lib.rs:270` | 100% |
| 分布式事务管理器 | `DtxManager` | `packages/sz-orm-dtx/src/lib.rs:432` | 100% |
| Saga 编排 | `saga.rs`（既有 Saga 编排器） | `packages/sz-orm-dtx/src/saga.rs` | 100% |
| TCC 三阶段 | `tcc.rs`（既有 TCC 编排器） | `packages/sz-orm-dtx/src/tcc.rs` | 100% |
| XA 崩溃恢复 | `recovery.rs`（既有崩溃恢复器） | `packages/sz-orm-dtx/src/recovery.rs` | 100% |
| gRPC 服务定义 | `GrpcServiceDef` | `packages/sz-orm-grpc/src/lib.rs:22` | 100% |
| 用户 gRPC 服务 trait | `UserGrpcService`（get_user/list_users/create_user/update_user/delete_user） | `packages/sz-orm-grpc/src/lib.rs:153` | 100% |
| gRPC 流 | `GrpcStream<T>` | `packages/sz-orm-grpc/src/lib.rs:235` | 100% |
| 拦截器 trait | `Interceptor`（call(request)） | `packages/sz-orm-grpc/src/lib.rs:328` | 100% |
| 重试策略 | `RetryPolicy` | `packages/sz-orm-grpc/src/lib.rs:415` | 100% |
| 真实 tonic gRPC 服务器 | `RealGrpcServer`（tonic 桥接，承载 `InMemoryUserService`） | `packages/sz-orm-grpc/src/real_grpc.rs:94` | 100% |
| 真实 tonic gRPC 客户端 | `RealGrpcClient`（通过 TCP 调用 `RealGrpcServer`） | `packages/sz-orm-grpc/src/real_grpc.rs:199` | 100% |
| 跨语言 SDK 包基础 | `sz-orm-go`/`sz-orm-java`/`sz-orm-cpp`/`sz-orm-python`/`sz-orm-js`（workspace 既有 5 个 FFI/绑定包） | `Cargo.toml:2`（workspace members） | 100% |

#### REQ-V48-002 低代码双向同步 — 既有低代码模型与设计器

| 需求功能 | 存量功能 | 代码位置 | 匹配度 |
|---------|---------|---------|--------|
| 低代码模型定义 | `ModelDefinition`（name/fields/indexes/relations） | `packages/sz-orm-lc/src/lib.rs:24` | 100% |
| 字段定义 | `FieldDef`（name/field_type/nullable/unique/...） | `packages/sz-orm-lc/src/lib.rs:83` | 100% |
| 关联关系定义 | `RelationDefinition` | `packages/sz-orm-lc/src/lib.rs:147` | 100% |
| 字段类型四向映射 | `FieldTypeMapping`（sql_to_rust/sql_to_html_input/sql_to_json_schema/rust_to_sql） | `packages/sz-orm-lc/src/lib.rs:210` | 100% |
| 验证规则枚举 | `ValidationRule`（Required/MinLength/MaxLength/Min/Max/Pattern/Email/Url/Enum） | `packages/sz-orm-lc/src/lib.rs:362` | 100% |
| schema 设计器 feature | `schema-designer` feature（`SchemaDesigner`/`ErDiagramEditor`/`code_gen`/`code_parse`/`design_ir`/`exporter`/`masking`/`web_ui`） | `packages/sz-orm-designer/src/lib.rs:1` | 100% |
| 代码生成 | `code_gen` 模块（设计器代码生成） | `packages/sz-orm-designer/src/lib.rs:2` | 100% |
| 代码解析 | `code_parse` 模块（设计器代码解析） | `packages/sz-orm-designer/src/lib.rs:4` | 100% |
| 设计 IR | `design_ir` 模块（设计器中间表示） | `packages/sz-orm-designer/src/lib.rs:6` | 100% |

#### REQ-V48-003 OpenAPI 反向生成 — 既有 OpenAPI→ORM 反向生成

| 需求功能 | 存量功能 | 代码位置 | 匹配度 |
|---------|---------|---------|--------|
| 反向生成配置 | `ReverseGenConfig` / `NamingConvention` | `packages/sz-orm-swagger/src/reverse/mod.rs:24` | 100% |
| 反向生成器主入口 | `OpenApiReverseGenerator` / `ReverseGenResult`（OpenAPI→Model/迁移/Repository） | `packages/sz-orm-swagger/src/reverse/mod.rs:25` | 100% |
| 注入防护 | `OpenApiInjectionGuard` | `packages/sz-orm-swagger/src/reverse/mod.rs:26` | 100% |
| API 优先闭环验证 | `ApiFirstLoopVerifier` / `LoopReport` | `packages/sz-orm-swagger/src/reverse/mod.rs:27` | 100% |
| OpenAPI→迁移映射 | `OpenApiToMigrationMapper`（OpenAPI Schema → 5 方言 DDL） | `packages/sz-orm-swagger/src/reverse/mod.rs:28` | 100% |
| OpenAPI→Model 映射 | `SchemaToModelMapper` / `ModelField` / `RustType` / `Constraint` | `packages/sz-orm-swagger/src/reverse/mod.rs:29` | 100% |
| OpenAPI→Repository 映射 | `OpenApiToRepositoryMapper`（OpenAPI Schema → Repository CRUD 骨架） | `packages/sz-orm-swagger/src/reverse/mod.rs:30` | 100% |
| 反向生成错误 | `ReverseGenError`（SpecParseFailed/UnsupportedSchemaConstruct/LoopVerificationDiff/InjectionDetected/UnsignedSpec/UserLogicOverwrite） | `packages/sz-orm-swagger/src/reverse/mod.rs:36` | 100% |
| 命名约定转换 | `to_pascal_case` / `to_snake_case` | `packages/sz-orm-swagger/src/reverse/mod.rs:63` | 100% |

#### REQ-V48-004 WASM 真实数据库连接闭环 — 既有代理桥接与连接池

| 需求功能 | 存量功能 | 代码位置 | 匹配度 |
|---------|---------|---------|--------|
| 代理鉴权 | `WasmDbAuthValidator` | `packages/sz-orm-wasm/src/real_db/mod.rs:18` | 100% |
| WASM 真实 DB 连接 | `WasmRealDbConnection` / `WasmTransport` | `packages/sz-orm-wasm/src/real_db/mod.rs:19` | 100% |
| WASM 查询执行器 | `WasmRealDbQueryExecutor` | `packages/sz-orm-wasm/src/real_db/mod.rs:20` | 100% |
| WASM 代理指标 | `WasmRealDbMetrics` | `packages/sz-orm-wasm/src/real_db/mod.rs:21` | 100% |
| 代理协议 | `WasmDbProxyProtocol` / `ProxyRequest` / `ProxyResponse` / `ProxyStatus` / `SerializationFormat` / `ProxyError` | `packages/sz-orm-wasm/src/real_db/mod.rs:22` | 100% |
| WASM 代理客户端 | `WasmDbProxy` / `DbCredentials` | `packages/sz-orm-wasm/src/real_db/mod.rs:25` | 100% |
| 代理限流 | `WasmDbRateLimiter` | `packages/sz-orm-wasm/src/real_db/mod.rs:26` | 100% |
| 代理重连 | `WasmRealDbReconnector` | `packages/sz-orm-wasm/src/real_db/mod.rs:27` | 100% |
| SQL 白名单 | `WasmDbSqlWhitelist` | `packages/sz-orm-wasm/src/real_db/mod.rs:28` | 100% |
| WASM 真实 DB 错误 | `WasmRealDbError`（ProxyUnavailable/SqlRejected/RateLimited/AuthFailed/CredentialsNotExposed/QueryFailed/ResultTooLarge/SerializationError） | `packages/sz-orm-wasm/src/real_db/mod.rs:34` | 100% |
| WASI socket 直连 | `WasiSocketConnection`（feature = "wasi-socket"） | `packages/sz-orm-wasm/src/real_db/wasi_socket.rs:13` | 100% |
| 连接池 | `Pool`（自研连接池，AtomicU32 + crossbeam-queue） | `packages/sz-orm-core/src/pool.rs:749` | 100% |
| 参数化查询构建 | sz-orm-query-builder（`where_eq`/`or_where_eq` 等参数化 API） | `packages/sz-orm-query-builder/src/` | 100% |

### 1.1.2 需要扩展的功能（部分匹配，需在现有基础上改造）

本节列出 v4.8.0 四项需求与存量代码部分匹配、需要在现有基础上扩展的功能。

#### REQ-V48-001 跨语言分布式事务协调

| 需求功能 | 存量功能 | 差异说明 | 扩展方向 |
|---------|---------|---------|---------|
| 真实 gRPC 传输 | 既有 `RemoteCallHandler` trait（`protocol.rs:15`）仅有 `MockRemoteCallHandler`（`:130`，仅测试用），无真实 tonic gRPC 传输实现 | 输入差异：需真实 tonic + prost gRPC 调用；业务逻辑差异：`MockRemoteCallHandler` 返回预设响应，真实传输须通过网络调用远端参与者；边界差异：须 mTLS 双向认证 + 超时控制 | 新增 `TonicGrpcCallHandler` 实现 `RemoteCallHandler` trait，复用 sz-orm-grpc `real_grpc.rs` tonic 桥接（`RealGrpcClient` `:199`）+ 既有 `GrpcParticipantProtocol`（`:26`），支持 mTLS（`ParticipantAuth::Mtls` `mod.rs:47`）与 Token 认证（`:53`），不修改既有 `RemoteCallHandler`/`GrpcParticipantProtocol` |
| 真实 HTTP 传输 | 既有 `HttpParticipantProtocol`（`protocol.rs:78`）基于 `RemoteCallHandler`，但无真实 reqwest HTTP 传输实现 | 业务逻辑差异：须真实 reqwest HTTP/JSON 调用远端参与者；边界差异：须 Token 认证 + 超时控制 | 新增 `ReqwestHttpCallHandler` 实现 `RemoteCallHandler` trait，复用既有 `HttpParticipantProtocol`（`:78`），通过 reqwest 发起 HTTP POST /prepare /commit /rollback，Token 认证 + 超时，不修改既有 `HttpParticipantProtocol` |
| 跨语言崩溃恢复 | 既有 `recovery.rs` 是 XA 崩溃恢复（Rust 内部参与者），未集成跨语言参与者状态查询 | 业务逻辑差异：XA recovery 仅查询本地日志，跨语言恢复须询问各跨语言参与者状态（prepare/commit/rollback）；输出差异：须跨语言参与者状态决策 | 新增 `CrossLangRecoveryCoordinator` 复用既有 `TransactionLogStore::read_pending`（`lib.rs:71`）查询未完成事务 + 通过 `TonicGrpcCallHandler`/`ReqwestHttpCallHandler` 询问各跨语言参与者状态 + 按状态决定全局提交/回滚 + 记录恢复日志，复用既有 `recovery.rs` 恢复框架，不修改既有 XA recovery |
| Saga 跨语言编排 | 既有 `saga.rs` Saga 编排仅支持 Rust 内部参与者，`CrossLangParticipant::to_participant()`（`participant.rs:51`）可适配为 `TransactionParticipant` 但 Saga 专用编排（补偿顺序/幂等）未深度集成 | 业务逻辑差异：Saga 补偿须跨语言调用 rollback，补偿顺序按 Saga 反向执行；边界差异：跨语言补偿可能失败须标记人工干预 | 新增 `CrossLangSagaCoordinator` 将跨语言参与者通过 `CrossLangParticipant::to_participant()` 适配后接入既有 `saga.rs` 编排，补偿顺序按 Saga 反向执行，复用 `CrossLangCompensationSerializer`（`serializer.rs:23`）幂等键，不修改既有 `saga.rs` |
| TCC 跨语言编排 | 既有 `tcc.rs` TCC 三阶段仅支持 Rust 内部参与者，未集成跨语言参与者 | 业务逻辑差异：TCC Try-Confirm-Cancel 三阶段须跨语言调用；边界差异：Cancel 须幂等 | 新增 `CrossLangTccCoordinator` 将跨语言参与者通过 `CrossLangParticipant::to_participant()` 适配后接入既有 `tcc.rs` 三阶段，复用 `CrossLangCompensationSerializer` 幂等键，不修改既有 `tcc.rs` |
| 各语言 SDK 接入契约 | workspace 既有 `sz-orm-go`/`sz-orm-java`/`sz-orm-cpp`/`sz-orm-python`/`sz-orm-js` 5 个 FFI/绑定包，但无统一的参与者接入契约 | 输出差异：须标准化各语言参与者 SDK 接入契约文档 + 接口定义 | 新增各语言 SDK 接入契约（prepare/commit/rollback 端点签名 + 协议版本 = `COORDINATOR_PROTOCOL_VERSION` + mTLS/Token 鉴权 + JSON 序列化），复用既有 5 个 FFI/绑定包基础，不修改既有 FFI 逻辑 |

#### REQ-V48-002 低代码双向同步

| 需求功能 | 存量功能 | 差异说明 | 扩展方向 |
|---------|---------|---------|---------|
| 双向同步引擎 | 既有 `ModelDefinition`（`lc/lib.rs:24`）/ `FieldTypeMapping`（`:210`）是单向模型定义 + 类型映射，无双向同步引擎 | 业务逻辑差异：须 ORM 模型 ↔ 低代码引擎模型双向同步（OrmToLc/LcToOrm/Bidirectional）；输入差异：须 `SyncDirection` 枚举控制方向 | 新增 `BidirectionalSyncEngine` + `SyncDirection` 枚举，复用既有 `ModelDefinition`/`FieldDef`/`RelationDefinition`/`FieldTypeMapping`/`ValidationRule`，不修改既有模型定义 |
| 同步冲突检测 | 既有 `ModelDefinition` 无冲突检测逻辑 | 业务逻辑差异：双向同时变更须检测冲突；输出差异：须冲突列表 | 新增 `SyncConflictDetector` 检测双向同步冲突，生成 `SyncConflict` 列表，不修改既有 `ModelDefinition` |
| 同步冲突解决 | 既有 `ModelDefinition` 无冲突解决策略 | 业务逻辑差异：须按策略解决冲突（OrmWins/LcWins/Merge/Manual）；边界差异：默认 Manual 须人工确认 | 新增 `SyncConflictResolver` + `ConflictResolutionStrategy` 枚举（默认 Manual），按策略解决冲突，解决后记录审计日志，不修改既有 `ModelDefinition` |
| 增量同步追踪 | 既有 `ModelDefinition` 无增量追踪，全量同步 | 业务逻辑差异：须追踪模型变更只同步变更项；输出差异：须变更项列表 | 新增 `SyncIncrementTracker` 追踪模型变更，同步时只处理变更项，不修改既有 `ModelDefinition` |
| 同步审计日志 | 既有 `ModelDefinition` 无审计日志 | 业务逻辑差异：须记录同步操作审计日志；边界差异：追加写入不可篡改 | 新增 `SyncAuditLogger` 记录同步审计日志，追加写入不可篡改，不修改既有 `ModelDefinition` |

#### REQ-V48-003 OpenAPI 反向生成

| 需求功能 | 存量功能 | 差异说明 | 扩展方向 |
|---------|---------|---------|---------|
| DB schema 读取 | 既有 `OpenApiReverseGenerator`（`reverse/mod.rs:25`）从 OpenAPI spec 反向生成，无 DB schema 读取能力 | 输入差异：须从数据库实际 schema 读取；业务逻辑差异：须五方言 schema 查询 | 新增 `DbSchemaReader` 五方言 schema 读取器，查询各方言 information_schema（参数化绑定），生成 `DbSchema` 描述，不修改既有 `OpenApiReverseGenerator` |
| DB schema → OpenAPI 规范 | 既有 `OpenApiReverseGenerator` 是 OpenAPI→ORM 方向，无 DB→OpenAPI 方向 | 业务逻辑差异：须 DB schema → OpenAPI 3.0 spec 映射；输出差异：须 OpenAPI spec | 新增 `DbSchemaToOpenApiMapper` 将 `DbSchema` 映射为 OpenAPI 3.0 spec，复用既有 `NamingConvention`/`to_pascal_case`（`:24`/`:63`），不修改既有 `OpenApiReverseGenerator` |
| DB schema → CRUD API | 既有 `OpenApiToRepositoryMapper`（`:30`）是 OpenAPI→Repository 方向，无 DB→CRUD 方向 | 业务逻辑差异：须为每张表生成标准 CRUD REST 端点；输出差异：须 CRUD 端点定义 + OpenAPI 文档 | 新增 `DbSchemaToCrudApiMapper` + `CrudApiEndpoint` 为每张表生成 5 个 CRUD 端点 + OpenAPI 文档，不修改既有 `OpenApiToRepositoryMapper` |
| 完整闭环验证 | 既有 `ApiFirstLoopVerifier`（`:28`）验证 OpenAPI→ORM→API 闭环，无 DB→OpenAPI→ORM→CRUD 完整闭环 | 业务逻辑差异：须验证 DB schema → OpenAPI → ORM Model → CRUD 闭环一致性；输出差异：须闭环验证报告 | 新增 `FullReverseLoopVerifier` 验证 DB→OpenAPI→ORM→CRUD 闭环，复用既有 `ApiFirstLoopVerifier`（`:27`）+ `OpenApiReverseGenerator`（`:25`），不修改既有 `ApiFirstLoopVerifier` |

#### REQ-V48-004 WASM 真实数据库连接闭环

| 需求功能 | 存量功能 | 差异说明 | 扩展方向 |
|---------|---------|---------|---------|
| 代理后端服务 | 既有 `WasmDbProxy`（`real_db/mod.rs:25`）是 WASM 端代理客户端，无 Rust 后端代理服务实现 | 业务逻辑差异：`WasmDbProxy` 是客户端发送代理请求，代理后端须接收请求 + 鉴权 + 白名单 + 限流 + 连接 DB + 返回结果；边界差异：后端凭据不暴露给 WASM 端 | 新增 `WasmProxyServer` 代理后端服务，接收 `ProxyRequest`（`:22`）→ 复用 `WasmDbAuthValidator`（`:18`）+ `WasmDbSqlWhitelist`（`:28`）+ `WasmDbRateLimiter`（`:26`）→ 复用 sz-orm-core `Pool`（`pool.rs:749`）连接 DB → 执行查询 → 返回 `ProxyResponse`，不修改既有 `WasmDbProxy` |
| 多方言代理后端 | 既有 `WasmDbProxy` 单方言代理，无多方言路由 | 业务逻辑差异：须按方言路由到对应数据库；边界差异：各方言连接参数不同 | 新增 `MultiDialectProxyBackend` 多方言代理后端，复用 sz-orm-core 连接池 + sz-orm-sqlx 驱动，按 `ProxyRequest.dialect` 路由，不修改既有 `WasmDbProxy` |
| WASM 端 ORM 会话 | 既有 `WasmRealDbConnection`（`:19`）/ `WasmRealDbQueryExecutor`（`:20`）是底层连接 + 执行器，无 ORM 会话层 | 业务逻辑差异：须 ORM 会话（查询构建 + 代理执行 + 结果反序列化）；输出差异：须反序列化为 ORM 结构 | 新增 `WasmOrmSession` WASM 端 ORM 会话，复用既有 `WasmRealDbConnection`/`WasmRealDbQueryExecutor`/`WasmDbProxy`，不修改既有 `WasmRealDbConnection` |
| WASM 端查询构建器桥接 | 既有 `WasmRealDbQueryExecutor` 执行 SQL 字符串，未与 sz-orm-query-builder 集成 | 业务逻辑差异：须通过 sz-orm-query-builder 构建参数化查询再转换为 `ProxyRequest`；边界差异：禁止 SQL 字符串拼接 | 新增 `WasmQueryBuilderBridge` 桥接 sz-orm-query-builder 与代理协议，将 `QueryBuilder.select().where_eq()` 输出转换为 `ProxyRequest`（参数化 SQL + params），不修改既有 `WasmRealDbQueryExecutor` |
| WASM ORM 闭环验证 | 既有 `WasmRealDbConnection` 无闭环验证 | 业务逻辑差异：须验证 WASM 端 ORM 操作闭环（查询构建→代理→后端 DB→结果与直接 DB 查询一致） | 新增 `WasmOrmLoopVerifier` 验证 WASM ORM 闭环，比对 WASM 端结果与直接 DB 查询结果，不修改既有 `WasmRealDbConnection` |

### 1.1.3 需要新增的功能或接口

本节列出 v4.8.0 四项需求在存量代码中完全没有对应实现、需要新增的功能。

#### REQ-V48-001 跨语言分布式事务协调 — 新增组件（sz-orm-dtx `cross-lang-dtx` feature 扩展）

| 新增功能 | 所属模块 | 输入 | 输出 | 核心逻辑 | 依赖 |
|---------|---------|------|------|---------|------|
| `TonicGrpcCallHandler` | `sz-orm-dtx::cross_lang::real_transport` | method/tx_id/payload + endpoint + mTLS/Token | `ParticipantResponse` | 实现 `RemoteCallHandler` trait，通过 tonic + prost 真实 gRPC 调用远端参与者，mTLS/Token 认证，超时控制 | 既有 `RemoteCallHandler`（`protocol.rs:15`）+ sz-orm-grpc `real_grpc.rs` + tonic/prost |
| `ReqwestHttpCallHandler` | `sz-orm-dtx::cross_lang::real_transport` | method/tx_id/payload + endpoint + Token | `ParticipantResponse` | 实现 `RemoteCallHandler` trait，通过 reqwest HTTP/JSON 真实调用远端参与者，Token 认证 + 超时 | 既有 `RemoteCallHandler`（`protocol.rs:15`）+ reqwest |
| `CrossLangRecoveryCoordinator` | `sz-orm-dtx::cross_lang::recovery` | 协调器重启触发 | 恢复结果（提交/回滚决策） | 查询未完成事务（`TransactionLogStore::read_pending`）→ 询问各跨语言参与者状态 → 按状态决定全局提交/回滚 → 记录恢复日志 | 既有 `TransactionLogStore`（`lib.rs:57`）+ `recovery.rs` + `TonicGrpcCallHandler`/`ReqwestHttpCallHandler` |
| `CrossLangSagaCoordinator` | `sz-orm-dtx::cross_lang::saga` | Saga 事务 + 跨语言参与者列表 | Saga 执行结果 | 将跨语言参与者通过 `to_participant()` 适配后接入既有 `saga.rs`，补偿顺序按 Saga 反向执行，跨语言补偿幂等 | 既有 `saga.rs` + `CrossLangParticipant::to_participant()`（`participant.rs:51`）+ `CrossLangCompensationSerializer`（`serializer.rs:23`） |
| `CrossLangTccCoordinator` | `sz-orm-dtx::cross_lang::tcc` | TCC 事务 + 跨语言参与者列表 | TCC 执行结果 | 将跨语言参与者通过 `to_participant()` 适配后接入既有 `tcc.rs`，Try-Confirm-Cancel 三阶段跨语言调用，Cancel 幂等 | 既有 `tcc.rs` + `CrossLangParticipant::to_participant()` + `CrossLangCompensationSerializer` |
| 各语言 SDK 接入契约 | `sz-orm-dtx::cross_lang::sdk_contract` | 语言类型 | 契约文档（端点签名+协议版本+鉴权+序列化） | 为 Go/Java/C++/Python/JS 各语言定义参与者 SDK 接入契约 | 既有 `sz-orm-go`/`sz-orm-java`/`sz-orm-cpp`/`sz-orm-python`/`sz-orm-js` + `COORDINATOR_PROTOCOL_VERSION`（`mod.rs:128`） |

#### REQ-V48-002 低代码双向同步 — 新增组件（sz-orm-lc `lc-bidirectional-sync` feature 新增）

| 新增功能 | 所属模块 | 输入 | 输出 | 核心逻辑 | 依赖 |
|---------|---------|------|------|---------|------|
| `BidirectionalSyncEngine` | `sz-orm-lc::bidirectional_sync` | ORM 模型 + 低代码模型 + `SyncDirection` | 同步结果（变更项+冲突） | 按 `SyncDirection` 双向同步模型定义，复用 `FieldTypeMapping` 类型映射 + `ValidationRule` 验证规则同步 | 既有 `ModelDefinition`/`FieldTypeMapping`/`ValidationRule`（`lc/lib.rs`）+ `SchemaDesigner`/`code_gen`/`code_parse`（`designer/lib.rs`） |
| `SyncDirection` | `sz-orm-lc::bidirectional_sync` | — | 枚举（OrmToLc/LcToOrm/Bidirectional） | 同步方向枚举 | — |
| `SyncConflictDetector` | `sz-orm-lc::bidirectional_sync` | ORM 模型 + 低代码模型 | 冲突列表 | 检测双向同步冲突（同字段双向变更/类型不一致/约束不一致/关联不一致） | 既有 `ModelDefinition`/`FieldDef` |
| `SyncConflictResolver` | `sz-orm-lc::bidirectional_sync` | 冲突列表 + `ConflictResolutionStrategy` | 解决结果 | 按 `ConflictResolutionStrategy`（默认 Manual）解决冲突，解决后记录审计日志 | `SyncConflictDetector` + `SyncAuditLogger` |
| `ConflictResolutionStrategy` | `sz-orm-lc::bidirectional_sync` | — | 枚举（OrmWins/LcWins/Merge/Manual） | 冲突解决策略枚举，默认 Manual | — |
| `SyncIncrementTracker` | `sz-orm-lc::bidirectional_sync` | 模型变更 | 变更项列表 | 追踪模型变更，同步时只处理变更项 | 既有 `ModelDefinition`/`FieldDef` |
| `SyncAuditLogger` | `sz-orm-lc::bidirectional_sync` | 同步操作 | 审计日志 | 记录同步审计日志，追加写入不可篡改 | — |

#### REQ-V48-003 OpenAPI 反向生成 — 新增组件（sz-orm-swagger `openapi-reverse` feature 扩展）

| 新增功能 | 所属模块 | 输入 | 输出 | 核心逻辑 | 依赖 |
|---------|---------|------|------|---------|------|
| `DbSchemaReader` | `sz-orm-swagger::reverse::db_schema` | 数据库连接 + 方言 | `DbSchema`（表/列/约束/索引） | 查询五方言 information_schema，参数化绑定，生成 DB schema 描述 | sz-orm-core 连接池 + sz-orm-sqlx 驱动 |
| `DbSchema` | `sz-orm-swagger::reverse::db_schema` | — | 结构（dialect/tables/columns/constraints/indexes） | DB schema 描述结构 | — |
| `DbSchemaToOpenApiMapper` | `sz-orm-swagger::reverse::db_schema` | `DbSchema` | OpenAPI 3.0 spec | 将 DB schema 映射为 OpenAPI spec（表→schemas/列→字段/主键→required/唯一约束→uniqueItems/外键→关联） | 既有 `NamingConvention`/`to_pascal_case`/`to_snake_case`（`reverse/mod.rs:24`/`:63`） |
| `DbSchemaToCrudApiMapper` | `sz-orm-swagger::reverse::db_schema` | `DbSchema` | CRUD API（端点+OpenAPI 文档） | 为每张表生成 5 个 CRUD REST 端点 + OpenAPI 文档 | `DbSchemaToOpenApiMapper` |
| `CrudApiEndpoint` | `sz-orm-swagger::reverse::db_schema` | — | 结构（method+path+parameters+request_body+responses） | CRUD API 端点定义结构 | — |
| `FullReverseLoopVerifier` | `sz-orm-swagger::reverse::db_schema` | `DbSchema` | `LoopReport`（一致/差异） | 验证 DB→OpenAPI→ORM→CRUD 闭环一致性 | 既有 `ApiFirstLoopVerifier`（`:27`）+ `OpenApiReverseGenerator`（`:25`）+ `DbSchemaToCrudApiMapper` |

#### REQ-V48-004 WASM 真实数据库连接闭环 — 新增组件（sz-orm-wasm `wasm-real-db` feature 扩展）

| 新增功能 | 所属模块 | 输入 | 输出 | 核心逻辑 | 依赖 |
|---------|---------|------|------|---------|------|
| `WasmProxyServer` | `sz-orm-wasm::real_db::proxy_server` | `ProxyRequest` + DB 配置 | `ProxyResponse` | 接收 WASM 端代理请求 → 鉴权 + SQL 白名单 + 限流 → 连接真实 DB（复用 `Pool`）→ 执行查询 → 检查结果集大小 → 返回结果，后端凭据不暴露 | 既有 `WasmDbAuthValidator`/`WasmDbSqlWhitelist`/`WasmDbRateLimiter`（`real_db/mod.rs:18,28,26`）+ sz-orm-core `Pool`（`pool.rs:749`）+ `ProxyRequest`/`ProxyResponse`（`:22`） |
| `MultiDialectProxyBackend` | `sz-orm-wasm::real_db::proxy_server` | `ProxyRequest`（含 dialect） | `ProxyResponse` | 按 `ProxyRequest.dialect` 路由到对应数据库（MySQL/PG/SQLite/Oracle/MSSQL），复用 sz-orm-core 连接池 + sz-orm-sqlx 驱动 | `WasmProxyServer` + sz-orm-core `Pool` + sz-orm-sqlx |
| `WasmOrmSession` | `sz-orm-wasm::real_db::orm_session` | 查询构建 + 代理配置 | ORM 结果（反序列化结构） | WASM 端 ORM 会话：查询构建（`WasmQueryBuilderBridge`）→ 代理执行（`WasmDbProxy`）→ 结果反序列化 | 既有 `WasmRealDbConnection`/`WasmRealDbQueryExecutor`/`WasmDbProxy`（`real_db/mod.rs:19,20,25`）+ `WasmQueryBuilderBridge` |
| `WasmQueryBuilderBridge` | `sz-orm-wasm::real_db::orm_session` | sz-orm-query-builder 查询 | `ProxyRequest`（参数化 SQL + params） | 桥接 sz-orm-query-builder 与代理协议，将 `QueryBuilder.select().where_eq()` 输出转换为 `ProxyRequest`，禁止 SQL 拼接 | sz-orm-query-builder + `ProxyRequest`（`:22`） |
| `WasmOrmLoopVerifier` | `sz-orm-wasm::real_db::orm_session` | WASM 端查询 + 直接 DB 查询 | 闭环验证结果（一致/差异） | 验证 WASM 端 ORM 操作闭环，比对 WASM 端结果与直接 DB 查询结果 | `WasmOrmSession` + sz-orm-core `Pool` |

## 1.2 存量功能详细分析

本节对 §1.1.1 已实现功能进行深入解读，分析接口契约、业务规则、扩展点与约束，为增量设计提供基础。

### 1.2.1 跨语言参与者协议层（REQ-V48-001 复用基线）

**接口契约**：
- `CrossLangParticipantProtocol` trait（`packages/sz-orm-dtx/src/cross_lang/mod.rs:108`）：定义 `prepare(tx_id, payload) -> Result<ParticipantResponse, CrossLangTxError>` / `commit` / `rollback` / `protocol_version() -> u32` 四方法，是跨语言参与者的标准协议。
- `RemoteCallHandler` trait（`packages/sz-orm-dtx/src/cross_lang/protocol.rs:15`）：定义 `call(method, tx_id, payload) -> Result<ParticipantResponse, CrossLangTxError>`，抽象远程调用传输，`GrpcParticipantProtocol`/`HttpParticipantProtocol` 均基于此 trait。
- `CrossLangParticipant::to_participant()`（`packages/sz-orm-dtx/src/cross_lang/participant.rs:51`）：将跨语言参与者适配为既有 `TransactionParticipant`，包装为 `ParticipantCallback` 闭包，通过 `with_prepare/with_commit/with_rollback` 注册。

**业务规则**：
- 协议版本须匹配：`check_protocol_version`（`protocol.rs:192`）校验参与者协议版本与 `COORDINATOR_PROTOCOL_VERSION = 1`（`mod.rs:128`）一致，不匹配返回 `CrossLangTxError::ProtocolVersionMismatch`。
- 幂等键确定性生成：`CrossLangCompensationSerializer::idempotency_key`（`serializer.rs:70`）按 `tx_id:participant_id:action` 确定性生成幂等键，重复调用同一阶段结果一致。

**扩展点**：
- `RemoteCallHandler` trait 是传输层扩展点：新增 `TonicGrpcCallHandler`/`ReqwestHttpCallHandler` 实现此 trait 即可接入真实传输，无需修改 `GrpcParticipantProtocol`/`HttpParticipantProtocol`。
- `CrossLangParticipant::to_participant()` 是编排层扩展点：跨语言参与者适配为 `TransactionParticipant` 后可接入既有 `saga`/`tcc`/`DtxManager` 编排。

**约束**：
- 既有协议层不可修改（spec.md §1.4.3），新增传输/恢复/编排为扩展。
- 跨语言参与者调用须鉴权（`ParticipantAuth` mTLS/Token，`mod.rs:45`）。
- 超时控制（`CrossLangParticipant::with_timeout` 默认 5000ms，`participant.rs:30`）。

### 1.2.2 事务核心与编排（REQ-V48-001 复用基线）

**接口契约**：
- `TransactionLogStore` trait（`packages/sz-orm-dtx/src/lib.rs:57`）：`append`/`read_pending`/`read_transaction`，事务日志持久化抽象。
- `DtxManager`（`packages/sz-orm-dtx/src/lib.rs:432`）：分布式事务管理器，编排 prepare/commit/rollback。
- `saga.rs`/`tcc.rs`/`recovery.rs`：既有 Saga 编排 / TCC 三阶段 / XA 崩溃恢复。

**业务规则**：
- 事务状态流转：`TransactionState`（`lib.rs:163`）Preparing → Prepared → Committing → Committed / RollingBack → RolledBack。
- 崩溃恢复：`recovery.rs` 查询 `TransactionLogStore::read_pending`（`lib.rs:71`）未完成事务，按日志决定提交/回滚。

**扩展点**：
- `TransactionParticipant`（`lib.rs:186`）的 `with_prepare/with_commit/with_rollback` 是参与者接入扩展点，跨语言参与者通过 `to_participant()` 适配接入。
- `saga.rs`/`tcc.rs` 接受 `TransactionParticipant` 列表，跨语言参与者适配后可直接接入。

**约束**：
- 既有事务核心不可修改（spec.md §1.4.4），跨语言协调器为扩展。
- 事务日志持久化须保证崩溃恢复一致性。

### 1.2.3 低代码模型定义与类型映射（REQ-V48-002 复用基线）

**接口契约**：
- `ModelDefinition`（`packages/sz-orm-lc/src/lib.rs:24`）：`name`/`fields`/`indexes`/`relations`，低代码模型定义。
- `FieldTypeMapping`（`packages/sz-orm-lc/src/lib.rs:210`）：`sql_to_rust`/`sql_to_html_input`/`sql_to_json_schema`/`rust_to_sql` 四向映射。
- `ValidationRule`（`packages/sz-orm-lc/src/lib.rs:362`）：Required/MinLength/MaxLength/Min/Max/Pattern/Email/Url/Enum 验证规则枚举。

**业务规则**：
- 类型映射双向一致：`rust_to_sql("i64")` = "BIGINT"，`sql_to_html_input("BIGINT")` = "number"，四向映射闭环一致。
- 模型名推导：`ModelDefinition::pascal_case_name`（`lc/lib.rs:43`）将表名转 PascalCase 单数模型名。

**扩展点**：
- `ModelDefinition` 是双向同步的模型载体，新增 `BidirectionalSyncEngine` 基于 `ModelDefinition` 扩展，不修改既有定义。
- `FieldTypeMapping` 四向映射是类型同步基础，双向同步复用此映射保证类型一致。

**约束**：
- 既有模型定义/类型映射不可修改（spec.md §1.4.6），双向同步引擎为扩展。
- 破坏性变更（删列/改类型/改约束）须人工确认，不自动执行（spec.md §5.2.1.10）。

### 1.2.4 OpenAPI 反向生成既有方向（REQ-V48-003 复用基线）

**接口契约**：
- `OpenApiReverseGenerator`（`packages/sz-orm-swagger/src/reverse/mod.rs:25`）：反向生成器主入口，OpenAPI spec → Model + 迁移 + Repository。
- `SchemaToModelMapper`（`:29`）：OpenAPI Schema → Rust struct 字段映射。
- `ApiFirstLoopVerifier`（`:27`）：API 优先闭环验证（OpenAPI→ORM→API）。
- `OpenApiInjectionGuard`（`:26`）：注入防护，检测含注入的 spec。

**业务规则**：
- 注入防护：`OpenApiInjectionGuard` 检测注入，返回 `ReverseGenError::InjectionDetected`（`:50`）；未签名 spec 返回 `UnsignedSpec`（`:54`）。
- 闭环验证：`ApiFirstLoopVerifier` 验证 OpenAPI→ORM→API 闭环一致性，差异返回 `LoopVerificationDiff`（`:46`）。

**扩展点**：
- `OpenApiReverseGenerator` 是 OpenAPI→ORM 方向的主入口，新增 DB→OpenAPI 方向（`DbSchemaToOpenApiMapper`）后可形成完整闭环（DB→OpenAPI→ORM→CRUD）。
- `NamingConvention`/`to_pascal_case`/`to_snake_case`（`:24`/`:63`）是命名约定扩展点，DB schema → OpenAPI 映射复用。

**约束**：
- 既有 OpenAPI→ORM 反向生成不可修改（spec.md §1.4.8），DB→OpenAPI 方向为扩展。
- 注入防护须复用既有 `OpenApiInjectionGuard`，未签名/含注入 spec 拒绝。

### 1.2.5 WASM 代理桥接与连接池（REQ-V48-004 复用基线）

**接口契约**：
- `WasmDbProxy`（`packages/sz-orm-wasm/src/real_db/mod.rs:25`）：WASM 端代理客户端，发送 `ProxyRequest`（`:22`）到代理后端，接收 `ProxyResponse`。
- `WasmDbProxyProtocol`（`:22`）：代理协议（`ProxyRequest`/`ProxyResponse`/`SerializationFormat`）。
- `WasmDbAuthValidator`（`:18`）/ `WasmDbSqlWhitelist`（`:28`）/ `WasmDbRateLimiter`（`:26`）：鉴权/SQL 白名单/限流。
- `Pool`（`packages/sz-orm-core/src/pool.rs:749`）：自研连接池（AtomicU32 + crossbeam-queue ArrayQueue + Notify）。

**业务规则**：
- 代理安全链：鉴权 → SQL 白名单 → 限流 → 执行，任一失败拒绝（`WasmRealDbError::AuthFailed`/`SqlRejected`/`RateLimited`，`mod.rs:49,41,45`）。
- 凭据隔离：后端数据库凭据不暴露给 WASM 端（`WasmRealDbError::CredentialsNotExposed` `mod.rs:53`）。
- 结果集大小限制：超限返回 `WasmRealDbError::ResultTooLarge`（`mod.rs:61`）。

**扩展点**：
- `WasmDbProxy` 是代理客户端，新增 `WasmProxyServer` 代理后端实现接收请求 + 连接 DB + 返回结果，不修改既有客户端。
- `WasmRealDbConnection`/`WasmRealDbQueryExecutor` 是底层连接 + 执行器，新增 `WasmOrmSession` ORM 会话层基于此扩展。

**约束**：
- 既有代理桥接不可修改（spec.md §1.4.9），代理后端 + ORM 闭环为扩展。
- 浏览器 WASM 环境不可直连数据库，须通过代理后端桥接（spec.md §1.4.20）。
- 参数化查询铁律：WASM 端查询构建须参数化（复用 sz-orm-query-builder `where_eq`/`or_where_eq`），禁止 SQL 拼接。

---

# 二、增量设计方案

## 2.1 实现模型

### 2.1.1 上下文视图

本节展示 v4.8.0 四项需求模块与外部的交互关系。

```plantuml
@startuml
title sz-orm v4.8.0 上下文视图

actor "分布式事务协调者\n(Rust 协调器)" as coordinator
actor "跨语言参与者服务\n(Go/Java/C++/Python/JS)" as crosslang_svc
actor "低代码引擎" as lc_engine
actor "API 设计者" as api_designer
actor "WASM 前端" as wasm_frontend
actor "代理后端运维者" as proxy_ops

package "sz-orm v4.8.0" as szorm {
  component "sz-orm-dtx\n(cross-lang-dtx)\n真实传输+恢复+Saga/TCC" as dtx
  component "sz-orm-grpc\n(real_grpc 复用)" as grpc
  component "sz-orm-lc\n(lc-bidirectional-sync)\n双向同步+冲突解决" as lc
  component "sz-orm-designer\n(schema-designer 复用)" as designer
  component "sz-orm-swagger\n(openapi-reverse)\nDB→OpenAPI+CRUD+闭环" as swagger
  component "sz-orm-wasm\n(wasm-real-db)\n代理后端+ORM 闭环" as wasm
}

component "sz-orm-core\n(Pool 连接池)" as core
component "sz-orm-query-builder\n(参数化查询)" as qb
component "sz-orm-sqlx\n(五方言驱动)" as sqlx
database "DB\n(MySQL/PG/SQLite\n/Oracle/MSSQL)" as db

coordinator --> dtx : 编排跨语言事务
dtx --> grpc : 真实 gRPC 传输
dtx --> crosslang_svc : gRPC/HTTP\nprepare/commit/rollback
grpc --> crosslang_svc : tonic gRPC

lc <--> lc_engine : 双向同步\n(OrmToLc/LcToOrm)
lc --> designer : 复用 SchemaDesigner\n/code_gen/code_parse

api_designer --> swagger : DB schema 反向生成
swagger --> db : 五方言 schema 读取\n(information_schema)
swagger --> core : schema 查询连接

wasm_frontend --> wasm : ORM 会话
wasm --> core : 代理后端连接 DB
wasm --> qb : 查询构建器桥接
core --> sqlx : 驱动
sqlx --> db : 数据存取
proxy_ops --> wasm : 部署代理后端\n/配置鉴权/白名单

@enduml
```

**通信协议与调用频率**：
- 跨语言事务协调：gRPC（tonic + prost）/ HTTP/JSON，事务生命周期内 prepare/commit/rollback 三次调用，mTLS/Token 鉴权。
- 低代码双向同步：进程内调用（ORM 模型 ↔ 低代码引擎模型），同步触发时调用，增量同步仅变更项。
- OpenAPI 反向生成：SQL（参数化查询 information_schema），一次性 schema 读取 + 映射 + 生成。
- WASM 代理：HTTP/WebSocket（浏览器）/ WASI socket（WASI 环境），每查询一次代理调用，鉴权 + 白名单 + 限流。

### 2.1.2 服务/组件总体架构

本节展示 v4.8.0 四项需求模块内部的组成结构。

```plantuml
@startuml
title sz-orm v4.8.0 服务/组件总体架构

package "REQ-V48-001 跨语言分布式事务协调" as req1 {
  component "TonicGrpcCallHandler\n(真实 gRPC 传输)" as tonic_handler
  component "ReqwestHttpCallHandler\n(真实 HTTP 传输)" as http_handler
  component "CrossLangRecoveryCoordinator\n(跨语言崩溃恢复)" as recovery_coord
  component "CrossLangSagaCoordinator\n(跨语言 Saga 编排)" as saga_coord
  component "CrossLangTccCoordinator\n(跨语言 TCC 编排)" as tcc_coord
  component "SDK 接入契约\n(Go/Java/C++/Python/JS)" as sdk_contract
}

package "REQ-V48-002 低代码双向同步" as req2 {
  component "BidirectionalSyncEngine\n(双向同步引擎)" as sync_engine
  component "SyncConflictDetector\n(冲突检测)" as conflict_det
  component "SyncConflictResolver\n(冲突解决)" as conflict_res
  component "SyncIncrementTracker\n(增量追踪)" as increment
  component "SyncAuditLogger\n(审计日志)" as audit
}

package "REQ-V48-003 OpenAPI 反向生成" as req3 {
  component "DbSchemaReader\n(五方言 schema 读取)" as schema_reader
  component "DbSchemaToOpenApiMapper\n(DB→OpenAPI 映射)" as openapi_mapper
  component "DbSchemaToCrudApiMapper\n(DB→CRUD 映射)" as crud_mapper
  component "FullReverseLoopVerifier\n(闭环验证)" as loop_verifier
}

package "REQ-V48-004 WASM 真实连接闭环" as req4 {
  component "WasmProxyServer\n(代理后端)" as proxy_server
  component "MultiDialectProxyBackend\n(多方言代理)" as multi_dialect
  component "WasmOrmSession\n(ORM 会话)" as orm_session
  component "WasmQueryBuilderBridge\n(查询构建桥接)" as qb_bridge
  component "WasmOrmLoopVerifier\n(闭环验证)" as wasm_loop
}

package "既有复用（不修改）" as reuse {
  component "CrossLangParticipantProtocol\n(mod.rs:108)" as proto
  component "GrpcParticipantProtocol\n(protocol.rs:26)" as grpc_proto
  component "HttpParticipantProtocol\n(protocol.rs:78)" as http_proto
  component "CrossLangParticipant::to_participant()\n(participant.rs:51)" as adapter
  component "TransactionLogStore\n(lib.rs:57)" as logstore
  component "saga.rs / tcc.rs / recovery.rs" as saga_tcc
  component "real_grpc.rs\n(RealGrpcServer/Client)" as real_grpc
  component "ModelDefinition/FieldTypeMapping\n(lc/lib.rs:24,210)" as lc_model
  component "SchemaDesigner/code_gen\n(designer/lib.rs:1)" as designer_reuse
  component "OpenApiReverseGenerator\n(reverse/mod.rs:25)" as rev_gen
  component "OpenApiInjectionGuard\n(reverse/mod.rs:26)" as inj_guard
  component "WasmDbProxy/AuthValidator\n/SqlWhitelist/RateLimiter\n(real_db/mod.rs)" as wasm_proxy
  component "Pool\n(pool.rs:749)" as pool
  component "sz-orm-query-builder" as qb_reuse
}

tonic_handler --> proto : 实现 RemoteCallHandler
tonic_handler --> real_grpc : 复用 tonic 桥接
http_handler --> proto : 实现 RemoteCallHandler
recovery_coord --> logstore : read_pending
recovery_coord --> tonic_handler : 询问参与者状态
recovery_coord --> http_handler
saga_coord --> adapter : to_participant()
saga_coord --> saga_tcc : 接入 saga.rs
tcc_coord --> adapter : to_participant()
tcc_coord --> saga_tcc : 接入 tcc.rs

sync_engine --> lc_model : 复用 ModelDefinition
sync_engine --> designer_reuse : 复用 SchemaDesigner
conflict_res --> audit : 记录审计

schema_reader --> pool : schema 查询连接
openapi_mapper --> rev_gen : 复用 NamingConvention
loop_verifier --> rev_gen : 复用 OpenApiReverseGenerator

proxy_server --> wasm_proxy : 复用鉴权/白名单/限流
proxy_server --> pool : 连接 DB
multi_dialect --> proxy_server
orm_session --> wasm_proxy : 复用 WasmDbProxy
orm_session --> qb_bridge
qb_bridge --> qb_reuse : 复用参数化查询

@enduml
```

**模块划分与职责**：
- `sz-orm-dtx::cross_lang::real_transport`：真实传输实现（`TonicGrpcCallHandler`/`ReqwestHttpCallHandler`），实现 `RemoteCallHandler` trait。
- `sz-orm-dtx::cross_lang::recovery`：跨语言崩溃恢复（`CrossLangRecoveryCoordinator`），复用 `TransactionLogStore`/`recovery.rs`。
- `sz-orm-dtx::cross_lang::saga`：跨语言 Saga 编排（`CrossLangSagaCoordinator`），复用 `saga.rs` + `to_participant()`。
- `sz-orm-dtx::cross_lang::tcc`：跨语言 TCC 编排（`CrossLangTccCoordinator`），复用 `tcc.rs` + `to_participant()`。
- `sz-orm-dtx::cross_lang::sdk_contract`：各语言 SDK 接入契约。
- `sz-orm-lc::bidirectional_sync`：双向同步引擎 + 冲突检测解决 + 增量追踪 + 审计日志。
- `sz-orm-swagger::reverse::db_schema`：DB schema 读取 + DB→OpenAPI/CRUD 映射 + 闭环验证。
- `sz-orm-wasm::real_db::proxy_server`：代理后端 + 多方言代理。
- `sz-orm-wasm::real_db::orm_session`：WASM 端 ORM 会话 + 查询构建器桥接 + 闭环验证。

**配置项及取值策略**：
- 跨语言事务超时：`timeout_ms` 默认 5000ms（`participant.rs:30`），可配。
- 冲突解决策略：`ConflictResolutionStrategy` 默认 Manual（spec.md §5.2.1.3），可配 OrmWins/LcWins/Merge。
- 结果集大小限制：默认 10MB（spec.md §5.4.1.9），可配。
- 命名约定：默认 snake_case 表名 → PascalCase 资源名（spec.md §5.3.1.6），复用 `NamingConvention`。

### 2.1.3 实现设计文档

本节对核心逻辑进行设计说明，包含状态机设计、流程分支设计、扩展点设计、事务设计。

#### 2.1.3.1 跨语言事务崩溃恢复状态机（REQ-V48-001）

```plantuml
@startuml
title 跨语言事务崩溃恢复状态机

state "协调器运行中" as running
state "协调器崩溃" as crashed
state "协调器重启" as restarting
state "查询未完成事务\n(TransactionLogStore::read_pending)" as query_pending
state "询问参与者状态\n(gRPC/HTTP query_status)" as query_status
state "决策: 全局提交" as decide_commit
state "决策: 全局回滚" as decide_rollback
state "决策: 人工干预\n(RecoveryConflict)" as decide_manual
state "执行恢复\n(通知参与者 commit/rollback)" as execute
state "记录恢复日志" as log
state "恢复完成" as done

running --> crashed : 协调器进程崩溃
crashed --> restarting : 协调器重启
restarting --> query_pending : 触发恢复
query_pending --> query_status : 存在未完成事务
query_pending --> done : 无未完成事务

query_status --> decide_commit : 所有参与者已 Committed/Prepared
query_status --> decide_rollback : 存在参与者未 Prepared/RolledBack
query_status --> decide_manual : 参与者状态冲突\n(部分 Committed 部分 RolledBack)

decide_commit --> execute : 通知未提交参与者 commit
decide_rollback --> execute : 通知已 Prepared 参与者 rollback
decide_manual --> log : 标记需人工干预\n(CrossLangTxError::RecoveryConflict)
execute --> log
log --> done

@enduml
```

**触发条件与处理策略**：
- 协调器崩溃重启 → 查询 `TransactionLogStore::read_pending`（`lib.rs:71`）未完成事务。
- 询问各跨语言参与者状态（通过 `TonicGrpcCallHandler`/`ReqwestHttpCallHandler` query_status）。
- 所有参与者已 Committed/Prepared → 全局提交，通知未提交参与者 commit。
- 存在参与者未 Prepared/RolledBack → 全局回滚，通知已 Prepared 参与者 rollback。
- 参与者状态冲突（部分 Committed 部分 RolledBack）→ `CrossLangTxError::RecoveryConflict`（`mod.rs:96`），标记需人工干预。

#### 2.1.3.2 低代码双向同步流程分支（REQ-V48-002）

```plantuml
@startuml
title 低代码双向同步流程分支

start
:接收同步请求(direction, model);
:SyncIncrementTracker.get_changes(model);
:获取变更项;

if (变更项为空?) then (是)
  :返回无变更;
  stop
else (否)
  :读取目标模型;
  :SyncConflictDetector.detect(源模型, 目标模型);
  :获取冲突列表;
  
  if (冲突列表为空?) then (是)
    :直接应用变更;
  else (否)
    if (strategy?) then (OrmWins)
      :采用 ORM 版本;
    elseif (LcWins) then
      :采用低代码版本;
    elseif (Merge) then
      :合并变更;
    else (Manual)
      :检测破坏性变更?;
      if (破坏性变更?) then (是)
        :暂停同步,提示人工确认;
        :SyncAuditLogger.log(暂停);
        stop
      else (否)
        :提示冲突,等待人工确认;
        :SyncAuditLogger.log(等待);
        stop
      endif
    endif
    :应用解决后变更;
  endif
  
  :SyncAuditLogger.log(direction, changes, conflicts, strategy);
  :返回同步结果;
  stop
endif

@enduml
```

**分支触发条件与处理策略**：
- 变更项为空 → 返回无变更（增量同步优化）。
- 无冲突 → 直接应用变更。
- 有冲突 + OrmWins → 采用 ORM 版本。
- 有冲突 + LcWins → 采用低代码版本。
- 有冲突 + Merge → 合并变更。
- 有冲突 + Manual + 破坏性变更（删列/改类型/改约束）→ 暂停同步，提示人工确认（spec.md §5.2.1.10）。
- 有冲突 + Manual + 非破坏性变更 → 提示冲突，等待人工确认。
- 所有同步操作记录审计日志（`SyncAuditLogger`）。

#### 2.1.3.3 OpenAPI 反向生成闭环流程（REQ-V48-003）

```plantuml
@startuml
title OpenAPI 反向生成闭环流程

start
:DbSchemaReader.read_schema(db_conn, dialect);
:获取 DbSchema;
:OpenApiInjectionGuard.check(DbSchema);
if (注入检测?) then (是)
  :返回 ReverseGenError::InjectionDetected;
  stop
else (否)
  :DbSchemaToOpenApiMapper.map(DbSchema);
  :生成 OpenAPI spec;
  :DbSchemaToCrudApiMapper.map(DbSchema);
  :生成 CRUD API;
  :FullReverseLoopVerifier.verify(DbSchema);
  :验证 DB→OpenAPI→ORM→CRUD 闭环;
  if (闭环一致?) then (是)
    :返回 ReverseGenResult(spec, crud, loop=pass);
    stop
  else (否)
    :返回 ReverseGenError::LoopVerificationDiff;
    stop
  endif
endif

@enduml
```

**分支触发条件与处理策略**：
- DB schema 含注入字符 → `OpenApiInjectionGuard` 拒绝，返回 `InjectionDetected`（`reverse/mod.rs:50`）。
- 闭环验证一致 → 返回 `ReverseGenResult`（spec + crud + loop=pass）。
- 闭环验证差异 → 返回 `LoopVerificationDiff`（`:46`），附差异详情。
- 五方言 schema 读取：MySQL information_schema / PG pg_catalog / SQLite sqlite_master / Oracle ALL_TAB_COLUMNS / MSSQL INFORMATION_SCHEMA，参数化绑定，不支持的特性降级跳过。

#### 2.1.3.4 WASM ORM 闭环事务设计（REQ-V48-004）

```plantuml
@startuml
title WASM ORM 闭环事务设计

actor "WASM 前端" as frontend
participant "WasmOrmSession" as session
participant "WasmQueryBuilderBridge" as bridge
participant "WasmDbProxy" as proxy
participant "WasmProxyServer" as server
participant "WasmDbAuthValidator" as auth
participant "WasmDbSqlWhitelist" as whitelist
participant "WasmDbRateLimiter" as limiter
participant "Pool" as pool
database "DB" as db

frontend -> session : query(User).where_eq("id", 1)
session -> bridge : build_query(QueryBuilder)
bridge -> bridge : 参数化查询构建\n(复用 sz-orm-query-builder)
bridge --> session : ProxyRequest(SQL, params=[1])

session -> proxy : send(ProxyRequest)
proxy -> server : HTTP/WebSocket(ProxyRequest)

server -> auth : validate(token)
alt 鉴权失败
  auth --> server : AuthFailed
  server --> proxy : WasmRealDbError::AuthFailed
else 鉴权通过
  server -> whitelist : check(SQL)
  alt SQL 非白名单
    whitelist --> server : SqlRejected
    server --> proxy : WasmRealDbError::SqlRejected
  else 白名单通过
    server -> limiter : acquire()
    alt 限流
      limiter --> server : RateLimited
      server --> proxy : WasmRealDbError::RateLimited
    else 限流通过
      server -> pool : acquire_connection(dialect)
      pool -> db : 连接
      server -> db : EXECUTE SQL [参数化绑定]
      db --> server : 结果集
      server -> server : 检查结果集大小 ≤ 10MB
      alt 结果集过大
        server --> proxy : WasmRealDbError::ResultTooLarge
      else 大小合规
        server --> proxy : ProxyResponse(结果)
      end
    end
  end
end

proxy --> session : ProxyResponse
session -> session : 反序列化为 User
session --> frontend : User{id=1, ...}

@enduml
```

**事务设计与安全链**：
- 代理安全链：鉴权（`WasmDbAuthValidator` `mod.rs:18`）→ SQL 白名单（`WasmDbSqlWhitelist` `:28`）→ 限流（`WasmDbRateLimiter` `:26`）→ 连接 DB（`Pool` `pool.rs:749`）→ 执行查询 → 检查结果集大小 → 返回结果，任一失败拒绝。
- 凭据隔离：后端 DB 凭据由 `WasmProxyServer` 持有，不暴露给 WASM 端（`WasmRealDbError::CredentialsNotExposed` `mod.rs:53`）。
- 参数化查询：`WasmQueryBuilderBridge` 复用 sz-orm-query-builder `where_eq`/`or_where_eq` 构建参数化查询，禁止 SQL 拼接。
- 结果集大小限制：默认 10MB，超限返回 `ResultTooLarge`（`mod.rs:61`）。
- 多方言路由：`MultiDialectProxyBackend` 按 `ProxyRequest.dialect` 路由到对应数据库。

## 2.2 接口设计

### 2.2.1 总体设计

本节列出 v4.8.0 四项需求的接口分类与调用关系。

**接口分类依据**：
- 跨语言事务协调接口：按传输层（gRPC/HTTP）/ 恢复层 / 编排层（Saga/TCC）/ 契约层分类。
- 低代码双向同步接口：按同步引擎 / 冲突检测解决 / 增量追踪 / 审计日志分类。
- OpenAPI 反向生成接口：按 schema 读取 / 映射 / 闭环验证分类。
- WASM 真实连接闭环接口：按代理后端 / ORM 会话 / 查询桥接 / 闭环验证分类。

**接口变更策略**：
- 所有新接口通过 feature gate 隔离，默认关闭，既有公开 API 不变。
- 新接口为扩展 API，不修改既有 trait/struct 签名。

| 接口分类 | 接口名 | 所属 feature | 稳定性 | 对应需求 |
|---------|--------|-------------|--------|---------|
| 传输层 | `TonicGrpcCallHandler` | `cross-lang-dtx` | 稳定 | REQ-V48-001 |
| 传输层 | `ReqwestHttpCallHandler` | `cross-lang-dtx` | 稳定 | REQ-V48-001 |
| 恢复层 | `CrossLangRecoveryCoordinator` | `cross-lang-dtx` | 稳定 | REQ-V48-001 |
| 编排层 | `CrossLangSagaCoordinator` | `cross-lang-dtx` | 稳定 | REQ-V48-001 |
| 编排层 | `CrossLangTccCoordinator` | `cross-lang-dtx` | 稳定 | REQ-V48-001 |
| 契约层 | `CrossLangSdkContract` | `cross-lang-dtx` | 稳定 | REQ-V48-001 |
| 同步引擎 | `BidirectionalSyncEngine` | `lc-bidirectional-sync` | 稳定 | REQ-V48-002 |
| 冲突检测 | `SyncConflictDetector` | `lc-bidirectional-sync` | 稳定 | REQ-V48-002 |
| 冲突解决 | `SyncConflictResolver` | `lc-bidirectional-sync` | 稳定 | REQ-V48-002 |
| 增量追踪 | `SyncIncrementTracker` | `lc-bidirectional-sync` | 稳定 | REQ-V48-002 |
| 审计日志 | `SyncAuditLogger` | `lc-bidirectional-sync` | 稳定 | REQ-V48-002 |
| schema 读取 | `DbSchemaReader` | `openapi-reverse` | 稳定 | REQ-V48-003 |
| DB→OpenAPI 映射 | `DbSchemaToOpenApiMapper` | `openapi-reverse` | 稳定 | REQ-V48-003 |
| DB→CRUD 映射 | `DbSchemaToCrudApiMapper` | `openapi-reverse` | 稳定 | REQ-V48-003 |
| 闭环验证 | `FullReverseLoopVerifier` | `openapi-reverse` | 稳定 | REQ-V48-003 |
| 代理后端 | `WasmProxyServer` | `wasm-real-db` | 稳定 | REQ-V48-004 |
| 多方言代理 | `MultiDialectProxyBackend` | `wasm-real-db` | 稳定 | REQ-V48-004 |
| ORM 会话 | `WasmOrmSession` | `wasm-real-db` | 稳定 | REQ-V48-004 |
| 查询桥接 | `WasmQueryBuilderBridge` | `wasm-real-db` | 稳定 | REQ-V48-004 |
| 闭环验证 | `WasmOrmLoopVerifier` | `wasm-real-db` | 稳定 | REQ-V48-004 |

### 2.2.2 接口清单

本节列出所有接口的详细说明，包含接口签名、业务说明、前置条件、后置条件、异常映射、调用示例。

#### REQ-V48-001 跨语言分布式事务协调接口

##### TonicGrpcCallHandler（真实 gRPC 传输）

```rust
pub struct TonicGrpcCallHandler {
    endpoint: String,
    auth: ParticipantAuth,
    timeout_ms: u64,
}

impl TonicGrpcCallHandler {
    pub fn new(endpoint: String, auth: ParticipantAuth) -> Self;
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self;
}

impl RemoteCallHandler for TonicGrpcCallHandler {
    fn call(&self, method: &str, tx_id: &str, payload: &[u8])
        -> Result<ParticipantResponse, CrossLangTxError>;
}
```

- **业务说明**：真实 tonic gRPC 传输实现 `RemoteCallHandler` trait，通过 tonic + prost 调用远端跨语言参与者 prepare/commit/rollback 端点，支持 mTLS 双向认证与 Token 认证。
- **前置条件**：远端参与者 gRPC 服务已启动，endpoint 可达，鉴权凭据有效。
- **后置条件**：返回 `ParticipantResponse`（success/payload/error/latency_ms），或 `CrossLangTxError`。
- **异常映射**：连接失败/超时 → `CrossLangTxError::Timeout`（`mod.rs:90`）；mTLS/Token 无效 → `AuthFailed`（`:92`）；协议版本不匹配 → `ProtocolVersionMismatch`（`:94`）。
- **调用示例**：`GrpcParticipantProtocol::new(endpoint, Arc::new(TonicGrpcCallHandler::new(endpoint, auth)))` 构造 gRPC 参与者协议，复用既有 `GrpcParticipantProtocol`（`protocol.rs:26`）。

##### ReqwestHttpCallHandler（真实 HTTP 传输）

```rust
pub struct ReqwestHttpCallHandler {
    endpoint: String,
    token: String,
    timeout_ms: u64,
}

impl ReqwestHttpCallHandler {
    pub fn new(endpoint: String, token: String) -> Self;
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self;
}

impl RemoteCallHandler for ReqwestHttpCallHandler {
    fn call(&self, method: &str, tx_id: &str, payload: &[u8])
        -> Result<ParticipantResponse, CrossLangTxError>;
}
```

- **业务说明**：真实 HTTP/JSON 传输实现 `RemoteCallHandler` trait，通过 reqwest POST 调用远端参与者 `{endpoint}/prepare|commit|rollback`，Token 认证 + 超时控制。
- **前置条件**：远端参与者 HTTP 服务已启动，endpoint 可达，Token 有效。
- **后置条件**：返回 `ParticipantResponse` 或 `CrossLangTxError`。
- **异常映射**：连接失败/超时 → `Timeout`；Token 无效 → `AuthFailed`。
- **调用示例**：`HttpParticipantProtocol::new(endpoint, Arc::new(ReqwestHttpCallHandler::new(endpoint, token)))`，复用既有 `HttpParticipantProtocol`（`protocol.rs:78`）。

##### CrossLangRecoveryCoordinator（跨语言崩溃恢复）

```rust
pub struct CrossLangRecoveryCoordinator {
    log_store: Arc<dyn TransactionLogStore>,
}

impl CrossLangRecoveryCoordinator {
    pub fn new(log_store: Arc<dyn TransactionLogStore>) -> Self;
    pub async fn recover(&self) -> Result<RecoveryReport, CrossLangTxError>;
}
```

- **业务说明**：协调器崩溃重启后恢复跨语言事务至一致状态，查询未完成事务（`TransactionLogStore::read_pending` `lib.rs:71`）→ 询问各跨语言参与者状态 → 按状态决定全局提交/回滚 → 记录恢复日志。
- **前置条件**：协调器已重启，`TransactionLogStore` 可读。
- **后置条件**：未完成事务恢复至 Committed/RolledBack 一致状态，或标记需人工干预。
- **异常映射**：参与者状态冲突 → `RecoveryConflict`（`mod.rs:96`）；补偿失败 → `CompensationFailed`（`:100`）。
- **调用示例**：`coordinator.recover().await` 触发恢复，复用既有 `TransactionLogStore`（`lib.rs:57`）+ `recovery.rs`。

##### CrossLangSagaCoordinator / CrossLangTccCoordinator（跨语言编排）

```rust
pub struct CrossLangSagaCoordinator { /* 复用既有 saga.rs */ }
impl CrossLangSagaCoordinator {
    pub fn new(participants: Vec<CrossLangParticipant>) -> Self;
    pub async fn execute(&self, tx_id: &str) -> Result<SagaResult, CrossLangTxError>;
}

pub struct CrossLangTccCoordinator { /* 复用既有 tcc.rs */ }
impl CrossLangTccCoordinator {
    pub fn new(participants: Vec<CrossLangParticipant>) -> Self;
    pub async fn try_confirm_cancel(&self, tx_id: &str) -> Result<TccResult, CrossLangTxError>;
}
```

- **业务说明**：将跨语言参与者通过 `CrossLangParticipant::to_participant()`（`participant.rs:51`）适配后接入既有 `saga.rs`/`tcc.rs` 编排，Saga 补偿按反向执行，TCC Try-Confirm-Cancel 三阶段跨语言调用，复用 `CrossLangCompensationSerializer`（`serializer.rs:23`）幂等键。
- **前置条件**：跨语言参与者已注册，协议版本匹配。
- **后置条件**：事务按 Saga/TCC 语义执行，补偿/Cancel 幂等。
- **异常映射**：补偿失败 → `CompensationFailed`；超时 → `Timeout`。
- **调用示例**：`CrossLangSagaCoordinator::new(vec![rust_p, go_p, java_p]).execute(tx_id).await`。

#### REQ-V48-002 低代码双向同步接口

##### BidirectionalSyncEngine（双向同步引擎）

```rust
pub enum SyncDirection { OrmToLc, LcToOrm, Bidirectional }

pub struct BidirectionalSyncEngine {
    strategy: ConflictResolutionStrategy,
    conflict_detector: SyncConflictDetector,
    conflict_resolver: SyncConflictResolver,
    increment_tracker: SyncIncrementTracker,
    audit_logger: SyncAuditLogger,
}

impl BidirectionalSyncEngine {
    pub fn new(strategy: ConflictResolutionStrategy) -> Self;
    pub fn sync(&mut self, direction: SyncDirection,
        orm_model: &ModelDefinition, lc_model: &ModelDefinition)
        -> Result<SyncResult, SyncError>;
}
```

- **业务说明**：按 `SyncDirection` 双向同步 ORM 模型 ↔ 低代码引擎模型，复用既有 `ModelDefinition`/`FieldTypeMapping`/`ValidationRule`（`lc/lib.rs:24,210,362`），增量追踪只同步变更项，冲突按 `ConflictResolutionStrategy` 解决，记录审计日志。
- **前置条件**：ORM 模型与低代码模型可读。
- **后置条件**：按方向同步模型，冲突按策略解决，审计日志记录。
- **异常映射**：冲突未解决（Manual）→ `SyncError::ConflictUnresolved`；破坏性变更 → `SyncError::DestructiveChangeRequiresManual`；类型映射不支持 → `SyncError::TypeMappingNotSupported`。
- **调用示例**：`engine.sync(SyncDirection::OrmToLc, &orm_user, &lc_user)`。

##### SyncConflictDetector / SyncConflictResolver（冲突检测解决）

```rust
pub enum ConflictResolutionStrategy { OrmWins, LcWins, Merge, Manual }

pub struct SyncConflict {
    field: String, conflict_type: ConflictType,
    orm_value: String, lc_value: String,
}

impl SyncConflictDetector {
    pub fn detect(&self, orm_model: &ModelDefinition,
        lc_model: &ModelDefinition) -> Vec<SyncConflict>;
}

impl SyncConflictResolver {
    pub fn resolve(&self, conflicts: Vec<SyncConflict>,
        strategy: ConflictResolutionStrategy)
        -> Result<ResolutionResult, SyncError>;
}
```

- **业务说明**：`SyncConflictDetector` 检测双向同步冲突（同字段双向变更/类型不一致/约束不一致/关联不一致），`SyncConflictResolver` 按 `ConflictResolutionStrategy`（默认 Manual）解决冲突。
- **前置条件**：ORM 模型与低代码模型可读。
- **后置条件**：返回冲突列表 / 解决结果，破坏性变更须人工确认。
- **异常映射**：Manual + 破坏性变更 → `SyncError::DestructiveChangeRequiresManual`。

#### REQ-V48-003 OpenAPI 反向生成接口

##### DbSchemaReader（五方言 schema 读取）

```rust
pub enum DbDialect { MySql, PostgreSql, Sqlite, Oracle, Mssql }
pub struct DbSchema { dialect: DbDialect, tables: Vec<DbTable> }

impl DbSchemaReader {
    pub async fn read_schema(&self, conn: &Connection,
        dialect: DbDialect) -> Result<DbSchema, ReverseGenError>;
}
```

- **业务说明**：从数据库实际 schema 读取表/列/约束/索引元信息，查询五方言 information_schema（MySQL information_schema / PG pg_catalog / SQLite sqlite_master / Oracle ALL_TAB_COLUMNS / MSSQL INFORMATION_SCHEMA），参数化绑定，不支持的特性降级跳过。
- **前置条件**：数据库连接可用，方言支持。
- **后置条件**：返回 `DbSchema`（表/列/约束/索引）。
- **异常映射**：连接失败/查询失败 → `ReverseGenError::SpecParseFailed`（`reverse/mod.rs:38`）；注入检测 → `InjectionDetected`（`:50`）。
- **调用示例**：`reader.read_schema(&conn, DbDialect::MySql).await`。

##### DbSchemaToOpenApiMapper / DbSchemaToCrudApiMapper（DB→OpenAPI/CRUD 映射）

```rust
impl DbSchemaToOpenApiMapper {
    pub fn map(&self, schema: &DbSchema) -> Result<OpenApiSpec, ReverseGenError>;
}
impl DbSchemaToCrudApiMapper {
    pub fn map(&self, schema: &DbSchema) -> Result<Vec<CrudApiEndpoint>, ReverseGenError>;
}
```

- **业务说明**：`DbSchemaToOpenApiMapper` 将 DB schema 映射为 OpenAPI 3.0 spec（表→schemas/列→字段/主键→required/唯一约束→uniqueItems/外键→关联），复用既有 `NamingConvention`/`to_pascal_case`（`:24`/`:63`）；`DbSchemaToCrudApiMapper` 为每张表生成 5 个 CRUD REST 端点 + OpenAPI 文档。
- **前置条件**：`DbSchema` 已读取，注入防护通过。
- **后置条件**：返回 OpenAPI spec / CRUD API 端点。
- **异常映射**：注入检测 → `InjectionDetected`；不支持的特性 → `UnsupportedSchemaConstruct`（`:42`）。

##### FullReverseLoopVerifier（完整闭环验证）

```rust
impl FullReverseLoopVerifier {
    pub fn verify(&self, schema: &DbSchema) -> Result<LoopReport, ReverseGenError>;
}
```

- **业务说明**：验证 DB schema → OpenAPI → ORM Model → CRUD 闭环一致性，生成的 OpenAPI spec 经既有 `OpenApiReverseGenerator`（`:25`）反向生成 ORM Model 再生成 CRUD，与直接从 DB schema 生成的 CRUD 比对，复用既有 `ApiFirstLoopVerifier`（`:27`）。
- **前置条件**：DB schema 已读取，OpenAPI spec 与 CRUD API 已生成。
- **后置条件**：返回 `LoopReport`（一致/差异）。
- **异常映射**：闭环差异 → `LoopVerificationDiff`（`:46`）。

#### REQ-V48-004 WASM 真实数据库连接闭环接口

##### WasmProxyServer（代理后端服务）

```rust
pub struct WasmProxyServer {
    auth_validator: WasmDbAuthValidator,
    sql_whitelist: WasmDbSqlWhitelist,
    rate_limiter: WasmDbRateLimiter,
    pool: Arc<Pool>,
    max_result_size: usize, // 默认 10MB
}

impl WasmProxyServer {
    pub fn new(pool: Arc<Pool>, config: ProxyServerConfig) -> Self;
    pub async fn handle_request(&self, request: ProxyRequest)
        -> Result<ProxyResponse, WasmRealDbError>;
}
```

- **业务说明**：接收 WASM 端代理请求 → 鉴权（`WasmDbAuthValidator` `mod.rs:18`）+ SQL 白名单（`WasmDbSqlWhitelist` `:28`）+ 限流（`WasmDbRateLimiter` `:26`）→ 连接真实 DB（复用 `Pool` `pool.rs:749`）→ 执行查询 → 检查结果集大小 → 返回结果，后端凭据不暴露给 WASM 端。
- **前置条件**：代理后端已启动，DB 连接池可用，鉴权/白名单/限流配置就绪。
- **后置条件**：返回 `ProxyResponse` 或 `WasmRealDbError`。
- **异常映射**：未鉴权 → `AuthFailed`（`mod.rs:49`）；非白名单 SQL → `SqlRejected`（`:41`）；超限 → `RateLimited`（`:45`）；结果集过大 → `ResultTooLarge`（`:61`）；查询失败 → `QueryFailed`（`:57`）；凭据暴露尝试 → `CredentialsNotExposed`（`:53`）。
- **调用示例**：`server.handle_request(proxy_request).await`。

##### WasmOrmSession（WASM 端 ORM 会话）

```rust
pub struct WasmOrmSession {
    proxy: WasmDbProxy,
    query_bridge: WasmQueryBuilderBridge,
}

impl WasmOrmSession {
    pub fn new(proxy: WasmDbProxy) -> Self;
    pub async fn query<T: DeserializeOwned>(&self, builder: QueryBuilder)
        -> Result<T, WasmRealDbError>;
}
```

- **业务说明**：WASM 端 ORM 会话，查询构建（`WasmQueryBuilderBridge` 复用 sz-orm-query-builder）→ 代理执行（`WasmDbProxy` `mod.rs:25`）→ 结果反序列化，复用既有 `WasmRealDbConnection`/`WasmRealDbQueryExecutor`。
- **前置条件**：代理后端可达，鉴权凭据有效。
- **后置条件**：返回反序列化的 ORM 结构或 `WasmRealDbError`。
- **异常映射**：代理不可用 → `ProxyUnavailable`（`mod.rs:37`）；序列化失败 → `SerializationError`（`:65`）。
- **调用示例**：`session.query::<User>(QueryBuilder::select("users").where_eq("id", 1)).await`。

##### WasmQueryBuilderBridge（查询构建器桥接）

```rust
impl WasmQueryBuilderBridge {
    pub fn build(&self, builder: QueryBuilder) -> Result<ProxyRequest, WasmRealDbError>;
}
```

- **业务说明**：桥接 sz-orm-query-builder 与代理协议，将 `QueryBuilder.select().where_eq()` 输出转换为 `ProxyRequest`（参数化 SQL + params），禁止 SQL 字符串拼接。
- **前置条件**：`QueryBuilder` 已构建。
- **后置条件**：返回参数化 `ProxyRequest`。
- **异常映射**：序列化失败 → `SerializationError`。

## 2.3 数据模型

### 2.3.1 设计目标

本节阐述 v4.8.0 四项需求的数据模型需要解决的问题。

**需要支持的业务场景**：
- 跨语言事务协调：跨语言参与者描述 + 事务恢复状态 + Saga/TCC 编排上下文。
- 低代码双向同步：模型变更项 + 同步冲突 + 审计日志。
- OpenAPI 反向生成：DB schema 描述 + CRUD 端点定义 + 闭环验证报告。
- WASM 真实连接闭环：代理请求/响应 + 多方言路由 + ORM 会话上下文。

**性能、容量、扩展性目标**：
- 跨语言事务协调延迟 ≤ 100ms（不含网络 RTT，spec.md §4.1.1）。
- 低代码双向同步 100 表 × 50 字段 ≤ 5 秒（全量）/ 1 秒（增量，spec.md §4.1.2）。
- OpenAPI 反向生成 100 表 × 50 字段 ≤ 10 秒（spec.md §4.1.3）。
- WASM 代理查询延迟 ≤ 200ms（不含 DB 执行，spec.md §4.1.4），代理后端 ≥ 10000 QPS（spec.md §4.1.5）。

**与存量数据兼容策略**：
- 跨语言事务复用既有 `TransactionLogStore` 日志格式，不修改既有日志结构。
- 低代码双向同步复用既有 `ModelDefinition`/`FieldDef` 结构，不修改既有模型定义。
- OpenAPI 反向生成复用既有 `ReverseGenResult`/`LoopReport` 结构，DB schema 描述为新增结构。
- WASM 代理复用既有 `ProxyRequest`/`ProxyResponse` 协议，不修改既有代理协议。

### 2.3.2 模型实现

本节展示核心领域对象的类图与关系。

```plantuml
@startuml
title sz-orm v4.8.0 数据模型类图

package "REQ-V48-001 跨语言事务" {
  class CrossLangParticipantDesc {
    +resource_id: String
    +language: ParticipantLanguage
    +transport: ParticipantTransport
    +endpoint: String
    +auth: ParticipantAuth
    +protocol_version: u32
  }
  class ParticipantResponse {
    +success: bool
    +payload: Vec<u8>
    +error: Option<String>
    +latency_ms: u64
  }
  class CrossLangParticipant {
    +desc: CrossLangParticipantDesc
    +timeout_ms: u64
    +to_participant(): TransactionParticipant
  }
}

package "REQ-V48-002 低代码同步" {
  class SyncChange {
    +direction: SyncDirection
    +model_name: String
    +field_name: String
    +change_type: ChangeType
    +old_value: Option<String>
    +new_value: Option<String>
    +timestamp: u64
  }
  class SyncConflict {
    +field: String
    +conflict_type: ConflictType
    +orm_value: String
    +lc_value: String
  }
  class SyncAuditEntry {
    +direction: SyncDirection
    +changes: Vec<SyncChange>
    +conflicts: Vec<SyncConflict>
    +strategy: ConflictResolutionStrategy
    +timestamp: u64
    +operator: String
  }
}

package "REQ-V48-003 OpenAPI 反向生成" {
  class DbSchema {
    +dialect: DbDialect
    +tables: Vec<DbTable>
  }
  class DbTable {
    +name: String
    +columns: Vec<DbColumn>
    +constraints: Vec<DbConstraint>
    +indexes: Vec<DbIndex>
  }
  class DbColumn {
    +name: String
    +data_type: String
    +nullable: bool
    +primary_key: bool
    +unique: bool
  }
  class CrudApiEndpoint {
    +method: HttpMethod
    +path: String
    +parameters: Vec<ApiParameter>
    +request_body: Option<Schema>
    +responses: Vec<ApiResponse>
  }
}

package "REQ-V48-004 WASM 闭环" {
  class ProxyRequest {
    +sql: String
    +params: Vec<ParamValue>
    +dialect: DbDialect
    +auth_token: String
    +result_format: SerializationFormat
  }
  class ProxyResponse {
    +status: ProxyStatus
    +data: Vec<u8>
    +error: Option<String>
  }
  class ProxyServerConfig {
    +max_result_size: usize
    +auth_config: AuthConfig
    +whitelist_config: WhitelistConfig
    +rate_limit_config: RateLimitConfig
  }
}

CrossLangParticipant --> CrossLangParticipantDesc
SyncAuditEntry --> SyncChange : changes
SyncAuditEntry --> SyncConflict : conflicts
DbSchema --> DbTable : tables
DbTable --> DbColumn : columns
ProxyRequest --> DbDialect

note right of DbSchema
  DB schema 描述
  五方言 information_schema 读取
  参数化查询，禁止 SQL 拼接
end note

note right of ProxyRequest
  代理请求
  SQL 须参数化 + 白名单内
  凭据不暴露给 WASM 端
end note

@enduml
```

**对象之间的关系**：
- `CrossLangParticipant` 组合 `CrossLangParticipantDesc`（参与者描述），适配为既有 `TransactionParticipant`。
- `SyncAuditEntry` 聚合 `SyncChange`（变更项）与 `SyncConflict`（冲突），记录同步审计。
- `DbSchema` 聚合 `DbTable`，`DbTable` 聚合 `DbColumn`/`DbConstraint`/`DbIndex`，描述 DB schema 元信息。
- `ProxyRequest` 关联 `DbDialect`（多方言路由），`ProxyServerConfig` 配置代理后端。

**对象创建和销毁策略**：
- 跨语言参与者：按需创建，事务结束后销毁，`CrossLangParticipant` 通过 `to_participant()` 适配为 `TransactionParticipant` 后由既有 `DtxManager` 管理生命周期。
- 同步变更项/冲突/审计：同步触发时创建，审计日志追加写入不可篡改，不销毁。
- DB schema：schema 读取时创建，反向生成完成后可销毁（一次性使用）。
- 代理请求/响应：每查询创建，查询完成销毁，连接复用 `Pool` 连接池。

**持久化策略**：
- 跨语言事务日志：复用既有 `TransactionLogStore`（`lib.rs:57`）持久化，不新增存储。
- 同步审计日志：`SyncAuditLogger` 追加写入（文件/DB），不可篡改。
- OpenAPI 反向生成结果：生成 OpenAPI spec 文件 + CRUD API 代码文件，按需持久化。
- WASM 代理：无状态持久化，连接复用 `Pool` 连接池，指标复用 `WasmRealDbMetrics`（`mod.rs:21`）。

---

# 三、feature gate 依赖关系图

本节展示 v4.8.0 四个 feature gate 的依赖关系，以及与 v4.7.0 既有 feature 的复用关系。

## 3.1 feature gate 依赖关系

```plantuml
@startuml
title sz-orm v4.8.0 feature gate 依赖关系图

rectangle "v4.8.0 feature gate（默认关闭）" as v48 {
  rectangle "cross-lang-dtx\n(sz-orm-dtx 扩展)" as f_dtx
  rectangle "lc-bidirectional-sync\n(sz-orm-lc 新增)" as f_lc
  rectangle "openapi-reverse\n(sz-orm-swagger 扩展)" as f_rev
  rectangle "wasm-real-db\n(sz-orm-wasm 扩展)" as f_wasm
}

rectangle "v4.7.0 既有 feature（复用基线）" as v47 {
  rectangle "cross-lang-dtx 既有协议层\n(sz-orm-dtx Cargo.toml:40)" as e_dtx
  rectangle "openapi-reverse 既有 OpenAPI→ORM\n(sz-orm-swagger Cargo.toml:14)" as e_rev
  rectangle "wasm-real-db 既有代理桥接\n(sz-orm-wasm Cargo.toml:35)" as e_wasm
  rectangle "schema-designer\n(sz-orm-designer 既有)" as e_designer
  rectangle "wasi-socket\n(sz-orm-wasm Cargo.toml:36)" as e_wasi
}

rectangle "v4.7.0 其他 feature（不触碰）" as v47_other {
  rectangle "delayed-priority-queue" as o1
  rectangle "forward-compat-sandbox" as o2
  rectangle "copy-parallel-shard" as o3
  rectangle "anomaly-remediation-rca" as o4
  rectangle "multicloud-cost-forecast" as o5
  rectangle "tenant-quota-rls-enhanced" as o6
  rectangle "cache-warmup-protection" as o7
}

f_dtx --> e_dtx : 扩展既有协议层\n补真实传输+恢复+编排
f_lc --> e_designer : 复用 schema-designer\n(code_gen/code_parse)
f_rev --> e_rev : 扩展既有 OpenAPI→ORM\n补 DB→OpenAPI 方向
f_wasm --> e_wasm : 扩展既有代理桥接\n补代理后端+ORM 闭环
f_wasm --> e_wasi : 复用 wasi-socket\n(WasiSocketConnection)

note right of v48
  v4.8.0 四个 feature 默认关闭
  主体相互独立，可并行开发
  全部通过既有包扩展，不新增包
  workspace 保持 60 成员
end note

note right of v47_other
  v4.7.0 七个 feature
  v4.8.0 不触碰
  零重叠
end note

@enduml
```

## 3.2 feature gate 与包的映射

| feature gate | 所属包 | Cargo.toml 位置 | 依赖既有 feature | 对应需求 | 新增/扩展 |
|-------------|--------|----------------|----------------|---------|----------|
| `cross-lang-dtx` | sz-orm-dtx | `packages/sz-orm-dtx/Cargo.toml:40` | 既有 `cross-lang-dtx` 协议层（同 feature 扩展） | REQ-V48-001 | 扩展 |
| `lc-bidirectional-sync` | sz-orm-lc | `packages/sz-orm-lc/Cargo.toml`（新增） | 既有 `schema-designer`（sz-orm-designer） | REQ-V48-002 | 新增 |
| `openapi-reverse` | sz-orm-swagger | `packages/sz-orm-swagger/Cargo.toml:14` | 既有 `openapi-reverse` OpenAPI→ORM（同 feature 扩展） | REQ-V48-003 | 扩展 |
| `wasm-real-db` | sz-orm-wasm | `packages/sz-orm-wasm/Cargo.toml:35` | 既有 `wasm-real-db` 代理桥接（同 feature 扩展）+ `wasi-socket`（`:36`） | REQ-V48-004 | 扩展 |

## 3.3 feature 全组合编译策略

v4.8.0 四个 feature 主体相互独立，可任意组合启用。门禁 10（Feature 全组合编译）须验证：
- 默认（全关闭）：行为与 v4.7.0 一致，既有 API 不变。
- 单 feature 启用：`cross-lang-dtx` / `lc-bidirectional-sync` / `openapi-reverse` / `wasm-real-db` 各自独立编译通过。
- 全 feature 启用：`--all-features` 编译通过，四项能力共存无冲突。
- 与 v4.7.0 feature 组合：v4.8.0 四个 feature 与 v4.7.0 七个 feature 任意组合编译通过（零重叠保证）。

---

# 四、复用点清单

本节汇总 v4.8.0 四项需求的所有复用点，每项附 file:line 代码证据（2026-08-14 逐项实测验证）。

## 4.1 REQ-V48-001 跨语言分布式事务协调 复用点

| 复用组件 | 代码位置 | 复用方式 | 验证状态 |
|---------|---------|---------|---------|
| `CrossLangParticipantProtocol` trait | `packages/sz-orm-dtx/src/cross_lang/mod.rs:108` | 协议层复用，新传输实现的依赖 trait `RemoteCallHandler` | ✅ 实测存在 |
| `GrpcParticipantProtocol` | `packages/sz-orm-dtx/src/cross_lang/protocol.rs:26` | gRPC 协议复用，`TonicGrpcCallHandler` 注入为 `RemoteCallHandler` | ✅ 实测存在 |
| `HttpParticipantProtocol` | `packages/sz-orm-dtx/src/cross_lang/protocol.rs:78` | HTTP 协议复用，`ReqwestHttpCallHandler` 注入为 `RemoteCallHandler` | ✅ 实测存在 |
| `RemoteCallHandler` trait | `packages/sz-orm-dtx/src/cross_lang/protocol.rs:15` | 传输层扩展点，新增 `TonicGrpcCallHandler`/`ReqwestHttpCallHandler` 实现此 trait | ✅ 实测存在 |
| `CrossLangParticipant::to_participant()` | `packages/sz-orm-dtx/src/cross_lang/participant.rs:51` | 适配器复用，跨语言参与者适配为 `TransactionParticipant` 接入 saga/tcc | ✅ 实测存在 |
| `CrossLangCompensationSerializer` | `packages/sz-orm-dtx/src/cross_lang/serializer.rs:23` | 幂等键复用，`idempotency_key`（`:70`）保证补偿幂等 | ✅ 实测存在 |
| `CrossLangTxAlerter`/`CrossLangTxMetrics` | `packages/sz-orm-dtx/src/cross_lang/observability.rs:12,62` | 可观测性复用，跨语言事务追踪 | ✅ 实测存在 |
| `TransactionLogStore` trait | `packages/sz-orm-dtx/src/lib.rs:57` | 事务日志复用，`read_pending`（`:71`）查询未完成事务 | ✅ 实测存在 |
| `TransactionState` enum | `packages/sz-orm-dtx/src/lib.rs:163` | 事务状态复用，崩溃恢复状态决策 | ✅ 实测存在 |
| `TransactionParticipant` | `packages/sz-orm-dtx/src/lib.rs:186` | 参与者复用，跨语言参与者适配目标 | ✅ 实测存在 |
| `DistributedTransaction` | `packages/sz-orm-dtx/src/lib.rs:270` | 分布式事务复用 | ✅ 实测存在 |
| `DtxManager` | `packages/sz-orm-dtx/src/lib.rs:432` | 事务管理器复用 | ✅ 实测存在 |
| `saga.rs` | `packages/sz-orm-dtx/src/saga.rs` | Saga 编排复用，`CrossLangSagaCoordinator` 接入 | ✅ 实测存在 |
| `tcc.rs` | `packages/sz-orm-dtx/src/tcc.rs` | TCC 三阶段复用，`CrossLangTccCoordinator` 接入 | ✅ 实测存在 |
| `recovery.rs` | `packages/sz-orm-dtx/src/recovery.rs` | XA 崩溃恢复复用，`CrossLangRecoveryCoordinator` 接入 | ✅ 实测存在 |
| `RealGrpcServer` | `packages/sz-orm-grpc/src/real_grpc.rs:94` | tonic gRPC 服务器复用，真实 gRPC 传输桥接 | ✅ 实测存在 |
| `RealGrpcClient` | `packages/sz-orm-grpc/src/real_grpc.rs:199` | tonic gRPC 客户端复用，`TonicGrpcCallHandler` 调用 | ✅ 实测存在 |
| `GrpcServiceDef` | `packages/sz-orm-grpc/src/lib.rs:22` | gRPC 服务定义复用 | ✅ 实测存在 |
| `UserGrpcService` trait | `packages/sz-orm-grpc/src/lib.rs:153` | 用户 gRPC 服务 trait 复用 | ✅ 实测存在 |
| `Interceptor` trait | `packages/sz-orm-grpc/src/lib.rs:328` | 拦截器复用（鉴权拦截） | ✅ 实测存在 |
| `RetryPolicy` | `packages/sz-orm-grpc/src/lib.rs:415` | 重试策略复用 | ✅ 实测存在 |
| `COORDINATOR_PROTOCOL_VERSION = 1` | `packages/sz-orm-dtx/src/cross_lang/mod.rs:128` | 协议版本复用，SDK 契约对齐 | ✅ 实测存在 |
| `ParticipantAuth`（mTLS/Token） | `packages/sz-orm-dtx/src/cross_lang/mod.rs:45` | 鉴权复用，真实传输鉴权 | ✅ 实测存在 |
| `check_protocol_version` | `packages/sz-orm-dtx/src/cross_lang/protocol.rs:192` | 协议版本检查复用 | ✅ 实测存在 |
| `sz-orm-go/java/cpp/python/js` | `Cargo.toml:2`（workspace members） | 跨语言 SDK 包基础复用 | ✅ 实测存在 |

## 4.2 REQ-V48-002 低代码双向同步 复用点

| 复用组件 | 代码位置 | 复用方式 | 验证状态 |
|---------|---------|---------|---------|
| `ModelDefinition` | `packages/sz-orm-lc/src/lib.rs:24` | 模型定义复用，双向同步模型载体 | ✅ 实测存在 |
| `FieldDef` | `packages/sz-orm-lc/src/lib.rs:83` | 字段定义复用 | ✅ 实测存在 |
| `RelationDefinition` | `packages/sz-orm-lc/src/lib.rs:147` | 关联关系复用 | ✅ 实测存在 |
| `FieldTypeMapping` | `packages/sz-orm-lc/src/lib.rs:210` | 四向类型映射复用，双向同步类型一致 | ✅ 实测存在 |
| `ValidationRule` | `packages/sz-orm-lc/src/lib.rs:362` | 验证规则复用，ORM 约束 → 低代码验证规则 | ✅ 实测存在 |
| `schema-designer` feature | `packages/sz-orm-designer/src/lib.rs:1` | 设计器 feature 复用 | ✅ 实测存在 |
| `code_gen` 模块 | `packages/sz-orm-designer/src/lib.rs:2` | 代码生成复用 | ✅ 实测存在 |
| `code_parse` 模块 | `packages/sz-orm-designer/src/lib.rs:4` | 代码解析复用 | ✅ 实测存在 |
| `design_ir` 模块 | `packages/sz-orm-designer/src/lib.rs:6` | 设计 IR 复用 | ✅ 实测存在 |

## 4.3 REQ-V48-003 OpenAPI 反向生成 复用点

| 复用组件 | 代码位置 | 复用方式 | 验证状态 |
|---------|---------|---------|---------|
| `OpenApiReverseGenerator` | `packages/sz-orm-swagger/src/reverse/mod.rs:25` | 反向生成器复用，闭环验证调用 | ✅ 实测存在 |
| `SchemaToModelMapper` | `packages/sz-orm-swagger/src/reverse/mod.rs:29` | OpenAPI→Model 映射复用 | ✅ 实测存在 |
| `OpenApiToMigrationMapper` | `packages/sz-orm-swagger/src/reverse/mod.rs:28` | OpenAPI→迁移映射复用 | ✅ 实测存在 |
| `OpenApiToRepositoryMapper` | `packages/sz-orm-swagger/src/reverse/mod.rs:30` | OpenAPI→Repository 映射复用 | ✅ 实测存在 |
| `ApiFirstLoopVerifier` | `packages/sz-orm-swagger/src/reverse/mod.rs:27` | API 优先闭环验证复用，`FullReverseLoopVerifier` 接入 | ✅ 实测存在 |
| `OpenApiInjectionGuard` | `packages/sz-orm-swagger/src/reverse/mod.rs:26` | 注入防护复用，DB schema 读取注入检测 | ✅ 实测存在 |
| `ReverseGenConfig`/`NamingConvention` | `packages/sz-orm-swagger/src/reverse/mod.rs:24` | 配置/命名约定复用 | ✅ 实测存在 |
| `to_pascal_case`/`to_snake_case` | `packages/sz-orm-swagger/src/reverse/mod.rs:63` | 命名转换复用 | ✅ 实测存在 |
| `ReverseGenError` | `packages/sz-orm-swagger/src/reverse/mod.rs:36` | 错误枚举复用（`InjectionDetected`:50, `LoopVerificationDiff`:46, `UnsignedSpec`:54） | ✅ 实测存在 |

## 4.4 REQ-V48-004 WASM 真实数据库连接闭环 复用点

| 复用组件 | 代码位置 | 复用方式 | 验证状态 |
|---------|---------|---------|---------|
| `WasmDbAuthValidator` | `packages/sz-orm-wasm/src/real_db/mod.rs:18` | 代理鉴权复用，`WasmProxyServer` 调用 | ✅ 实测存在 |
| `WasmRealDbConnection`/`WasmTransport` | `packages/sz-orm-wasm/src/real_db/mod.rs:19` | WASM 连接复用，`WasmOrmSession` 基础 | ✅ 实测存在 |
| `WasmRealDbQueryExecutor` | `packages/sz-orm-wasm/src/real_db/mod.rs:20` | 查询执行器复用 | ✅ 实测存在 |
| `WasmRealDbMetrics` | `packages/sz-orm-wasm/src/real_db/mod.rs:21` | 代理指标复用 | ✅ 实测存在 |
| `ProxyRequest`/`ProxyResponse`/`WasmDbProxyProtocol` | `packages/sz-orm-wasm/src/real_db/mod.rs:22` | 代理协议复用，`WasmProxyServer` 接收/返回 | ✅ 实测存在 |
| `WasmDbProxy`/`DbCredentials` | `packages/sz-orm-wasm/src/real_db/mod.rs:25` | 代理客户端复用，`WasmOrmSession` 调用 | ✅ 实测存在 |
| `WasmDbRateLimiter` | `packages/sz-orm-wasm/src/real_db/mod.rs:26` | 代理限流复用，`WasmProxyServer` 调用 | ✅ 实测存在 |
| `WasmRealDbReconnector` | `packages/sz-orm-wasm/src/real_db/mod.rs:27` | 代理重连复用 | ✅ 实测存在 |
| `WasmDbSqlWhitelist` | `packages/sz-orm-wasm/src/real_db/mod.rs:28` | SQL 白名单复用，`WasmProxyServer` 调用 | ✅ 实测存在 |
| `WasmRealDbError` | `packages/sz-orm-wasm/src/real_db/mod.rs:34` | 错误枚举复用（`AuthFailed`:49, `SqlRejected`:41, `RateLimited`:45, `CredentialsNotExposed`:53, `ResultTooLarge`:61, `QueryFailed`:57, `ProxyUnavailable`:37, `SerializationError`:65） | ✅ 实测存在 |
| `WasiSocketConnection` | `packages/sz-orm-wasm/src/real_db/wasi_socket.rs:13` | WASI socket 直连复用（feature = "wasi-socket"） | ✅ 实测存在 |
| `Pool`（连接池） | `packages/sz-orm-core/src/pool.rs:749` | 连接池复用，`WasmProxyServer` 连接 DB | ✅ 实测存在 |
| sz-orm-query-builder | `packages/sz-orm-query-builder/src/` | 参数化查询构建复用，`WasmQueryBuilderBridge` 桥接 | ✅ 实测存在 |

## 4.5 复用统计

| 需求 | 复用点数 | 既有包数 | 新增包数 | 复用率 |
|------|---------|---------|---------|--------|
| REQ-V48-001 | 25 | 2（sz-orm-dtx + sz-orm-grpc） | 0 | 100%（全部复用既有协议层+事务核心+gRPC） |
| REQ-V48-002 | 9 | 2（sz-orm-lc + sz-orm-designer） | 0 | 100%（全部复用既有模型+设计器） |
| REQ-V48-003 | 9 | 1（sz-orm-swagger） | 0 | 100%（全部复用既有反向生成） |
| REQ-V48-004 | 13 | 3（sz-orm-wasm + sz-orm-core + sz-orm-query-builder） | 0 | 100%（全部复用既有代理桥接+连接池+查询构建器） |
| **合计** | **56** | **6**（去重） | **0** | **100%** |

**复用率 100%**：v4.8.0 四项需求全部通过既有包扩展实现，不新增包，不重复实现既有逻辑，符合 spec.md §10.4 不新增包声明。

---

# 五、与 v4.7.0 的零重叠声明

## 5.1 零重叠声明

v4.8.0 与 v4.7.0 零重叠，新增范围全部落在既有包扩展（sz-orm-dtx / sz-orm-grpc / sz-orm-lc / sz-orm-designer / sz-orm-swagger / sz-orm-wasm），不触碰 v4.7.0 新增逻辑所在包（sz-orm-queue / sz-orm-core / sz-orm-batch / sz-orm-observability / sz-orm-storage）。

| v4.7.0 能力（智能化运维深化 + 性能深化层） | v4.7.0 包/feature | v4.8.0 能力（跨语言互操作 + 全栈闭环层） | v4.8.0 包/feature | 关系 |
|-------------------------------|------|-------------------------|------|------|
| 消息延迟队列与优先级调度 | sz-orm-queue delayed-priority-queue | 跨语言分布式事务协调 | sz-orm-dtx cross-lang-dtx | 零重叠，不同包不同维度 |
| 迁移前向兼容性检查与沙箱预演 | sz-orm-core forward-compat-sandbox | 低代码双向同步 | sz-orm-lc lc-bidirectional-sync | 零重叠，不同包不同维度 |
| 批量 COPY 协议与并行分片执行 | sz-orm-batch copy-parallel-shard | OpenAPI 反向生成 | sz-orm-swagger openapi-reverse | 零重叠，不同包不同维度 |
| 异常自愈与根因分析 | sz-orm-observability anomaly-remediation-rca | WASM 真实数据库连接闭环 | sz-orm-wasm wasm-real-db | 零重叠，不同包不同维度 |
| 多云成本对比与容量预测 | sz-orm-storage multicloud-cost-forecast | — | — | v4.8.0 不触碰 |
| 租户资源配额与行级安全增强 | sz-orm-core tenant-quota-rls-enhanced | — | — | v4.8.0 不触碰 |
| 缓存预热与穿透防护 | sz-orm-core cache-warmup-protection | — | — | v4.8.0 不触碰 |

## 5.2 零重叠证据

**包级零重叠**：
- v4.7.0 新增逻辑所在包：`sz-orm-queue` / `sz-orm-core`（3 个 feature）/ `sz-orm-batch` / `sz-orm-observability` / `sz-orm-storage`（5 个包）。
- v4.8.0 新增逻辑所在包：`sz-orm-dtx` / `sz-orm-grpc`（仅复用）/ `sz-orm-lc` / `sz-orm-designer`（仅复用）/ `sz-orm-swagger` / `sz-orm-wasm`（6 个包）。
- 包集合交集 = ∅（`{queue, core, batch, observability, storage}` ∩ `{dtx, grpc, lc, designer, swagger, wasm}` = ∅），包级零重叠。

**feature gate 级零重叠**：
- v4.7.0 新增 feature：`delayed-priority-queue` / `forward-compat-sandbox` / `copy-parallel-shard` / `anomaly-remediation-rca` / `multicloud-cost-forecast` / `tenant-quota-rls-enhanced` / `cache-warmup-protection`（7 个）。
- v4.8.0 涉及 feature：`cross-lang-dtx`（扩展）/ `lc-bidirectional-sync`（新增）/ `openapi-reverse`（扩展）/ `wasm-real-db`（扩展）（4 个）。
- feature 集合交集 = ∅，feature gate 级零重叠。

**维度级零重叠**：
- v4.7.0 维度：智能化运维深化（消息时间优先级/迁移兼容沙箱/COPY 并行/异常自愈/成本预测/配额 RLS/缓存预热）。
- v4.8.0 维度：跨语言互操作 + 全栈闭环（跨语言事务协调/低代码双向同步/OpenAPI 反向生成/WASM 真实连接）。
- 维度不同，零重叠。

## 5.3 sz-pay 生产依赖兼容性

sz-pay 从 crates.io 拉取 sz-orm-* 6 个包（sz-orm-core/sqlx/config/auth/macros/queue），v4.8.0 不触碰这 6 个包的新增逻辑（sz-orm-core 的 v4.7.0 feature 不扩展，仅 `Pool` 连接池复用为只读依赖），既有公开 API 完全向后兼容，sz-pay 升级到 v4.8.0 须无 Breaking Change（spec.md §4.5.3）。

---

# 六、风险与缓解措施

本节识别 v4.8.0 四项需求的主要风险并制定缓解措施，每项附验证方法。

## 6.1 风险登记

| 风险 ID | 风险描述 | 影响 | 概率 | 风险等级 | 缓解措施 | 验证方法 |
|---------|---------|------|------|---------|---------|---------|
| R-001 | 跨语言参与者 gRPC/HTTP 调用超时导致事务悬挂 | 事务无法提交/回滚，需人工干预 | 中 | 高 | `CrossLangParticipant::with_timeout`（`participant.rs:30`）默认 5000ms 超时，超时返回 `CrossLangTxError::Timeout`（`mod.rs:90`）触发回滚/补偿；`CrossLangRecoveryCoordinator` 崩溃恢复 | `cargo test -p sz-orm-dtx --features cross-lang-dtx` 验证超时场景 |
| R-002 | 跨语言崩溃恢复时参与者状态冲突（部分 Committed 部分 RolledBack） | 事务不一致，需人工干预 | 低 | 高 | `CrossLangRecoveryCoordinator` 检测状态冲突返回 `CrossLangTxError::RecoveryConflict`（`mod.rs:96`），标记需人工干预，记录冲突日志 | `cargo test -p sz-orm-dtx --features cross-lang-dtx` 验证恢复冲突场景 |
| R-003 | 跨语言补偿失败（Saga/TCC rollback/cancel 失败） | 补偿不完整，数据不一致 | 中 | 高 | `CrossLangCompensationSerializer`（`serializer.rs:23`）幂等键保证补偿幂等，补偿失败返回 `CompensationFailed`（`mod.rs:100`），标记需人工干预 | `cargo test -p sz-orm-dtx --features cross-lang-dtx` 验证补偿失败场景 |
| R-004 | 低代码双向同步冲突未解决（Manual 策略人工未确认） | 同步暂停，模型不一致 | 中 | 中 | `SyncConflictResolver` 默认 Manual，暂停同步记录冲突日志，提示人工确认（spec.md §5.2.1.3）；破坏性变更须人工确认（spec.md §5.2.1.10） | `cargo test -p sz-orm-lc --features lc-bidirectional-sync` 验证冲突场景 |
| R-005 | 低代码双向同步破坏性变更误执行（删列/改类型/改约束） | 数据丢失，生产数据丢失 | 低 | 高 | 破坏性变更检测 + 默认 Manual 策略，不自动执行，须人工确认（spec.md §5.2.1.10）；`SyncAuditLogger` 记录审计日志 | `cargo test -p sz-orm-lc --features lc-bidirectional-sync` 验证破坏性变更拦截 |
| R-006 | OpenAPI 反向生成闭环验证差异（DB→OpenAPI→ORM→CRUD 不一致） | 生成的 API 与 DB schema 不一致 | 中 | 中 | `FullReverseLoopVerifier` 验证闭环一致性，差异返回 `ReverseGenError::LoopVerificationDiff`（`reverse/mod.rs:46`），附差异详情 | `cargo test -p sz-orm-swagger --features openapi-reverse` 验证闭环 |
| R-007 | OpenAPI 反向生成注入攻击（DB schema 含注入字符） | 注入攻击，代码执行 | 低 | 高 | `OpenApiInjectionGuard`（`reverse/mod.rs:26`）检测注入，返回 `InjectionDetected`（`:50`）；未签名 spec 拒绝 `UnsignedSpec`（`:54`）；参数化查询 | `cargo test -p sz-orm-swagger --features openapi-reverse` 验证注入拦截 |
| R-008 | WASM 代理后端不可用导致查询失败 | WASM 端无法查询 | 中 | 中 | `WasmRealDbReconnector`（`mod.rs:27`）自动重连，返回 `ProxyUnavailable`（`:37`），99.9% 可用性目标（spec.md §4.2.5） | `cargo test -p sz-orm-wasm --features wasm-real-db` 验证重连 |
| R-009 | WASM 代理 SQL 注入（WASM 端构造恶意 SQL） | SQL 注入，数据泄露/篡改 | 低 | 高 | `WasmDbSqlWhitelist`（`mod.rs:28`）SQL 白名单拒绝非白名单 SQL（`SqlRejected` `:41`）；`WasmQueryBuilderBridge` 参数化查询禁止拼接 | `cargo test -p sz-orm-wasm --features wasm-real-db` 验证白名单 + 参数化 |
| R-010 | WASM 代理凭据泄露（后端 DB 凭据暴露给 WASM 端） | 凭据泄露，数据库被攻击 | 低 | 高 | `WasmRealDbError::CredentialsNotExposed`（`mod.rs:53`），后端凭据由 `WasmProxyServer` 持有不暴露给 WASM 端（spec.md §4.2.6） | `cargo test -p sz-orm-wasm --features wasm-real-db` 验证凭据隔离 |
| R-011 | WASM 代理结果集过大导致 WASM 内存溢出 | WASM 端 OOM | 中 | 中 | 结果集大小限制默认 10MB，超限返回 `WasmRealDbError::ResultTooLarge`（`mod.rs:61`）（spec.md §5.4.1.9） | `cargo test -p sz-orm-wasm --features wasm-real-db` 验证结果集限制 |
| R-012 | 幻影交付（新能力无生产调用点 + 端到端接线测试） | 审计无效，能力未真实交付 | 中 | 高 | 每项新能力附生产调用点 + 端到端接线测试，门禁 15 幻影交付检查（`python scripts/check-phantom-delivery.py`） | 门禁 15 + `cargo test --workspace` 验证接线 |
| R-013 | v4.7.0 测试基线回退 | 既有功能 regression | 低 | 高 | 门禁 4（`cargo test --workspace`）验证 228 套测试不回退，仅增不减（spec.md §4.5.2） | 门禁 4 验证 |
| R-014 | sz-pay 生产依赖破坏 | sz-pay 编译失败/行为变化 | 低 | 高 | API 兼容性铁律，既有公开 API 不变，门禁 11（上游未修改）+ sz-pay 升级验证（spec.md §4.5.3） | 门禁 11 + sz-pay 编译验证 |
| R-015 | 五方言覆盖不全（某方言 schema 读取/代理不支持） | 部分方言不可用 | 中 | 中 | 五方言 schema 读取（MySQL/PG/SQLite/Oracle/MSSQL information_schema），不支持的特性降级跳过标注（spec.md §5.3.1.5）；多方言代理后端按 dialect 路由 | `cargo test --features openapi-reverse,wasm-real-db` 五方言验证 |

## 6.2 风险监控

**门禁前置**：所有新增代码须通过 21 道门禁（AGENTS.md），核心门禁：
- 门禁 4（`cargo test --workspace`）：v4.7.0 测试基线不回退 + v4.8.0 新增测试通过。
- 门禁 8（禁止占位实现）：`grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` 零命中。
- 门禁 9（SQL 注入扫描）：`scripts/check-sql-injection.ps1` 通过。
- 门禁 10（Feature 全组合编译）：`cargo check --workspace --all-targets --all-features` 通过。
- 门禁 15（幻影交付检查）：`python scripts/check-phantom-delivery.py` 通过，每项新能力附生产调用点。
- 门禁 21（安全攻击测试）：`cargo test -p sz-orm-core --features multi-tenant-enhanced --test security_attacks` 通过。

**审计证据**：每项设计结论附 file:line 证据（本文档 §四 复用点清单 56 项均实测验证），遵循 AGENTS.md 审计合规铁律，修复后须运行 `cargo test` 并附输出。

**接线验证**：每项新能力须有入口、有接线、能运行、能观测（session-rules），端到端接线测试附 `cargo test` 验证方法。

---

# 七、需求追溯矩阵

本节建立 v4.8.0 需求 ↔ 验收条件 ↔ 设计方案 ↔ 复用点映射。

| 需求 ID | 优先级 | 需求名称 | 验收条件数 | 设计方案章节 | feature gate | 复用点数 | 复用率 |
|---------|--------|---------|-----------|-------------|-------------|---------|--------|
| REQ-V48-001 | P1 | 跨语言分布式事务协调 | 10 | §2.1.3.1 + §2.2.2（REQ-V48-001 接口） | `cross-lang-dtx`（扩展） | 25 | 100% |
| REQ-V48-002 | P1 | 低代码双向同步 | 10 | §2.1.3.2 + §2.2.2（REQ-V48-002 接口） | `lc-bidirectional-sync`（新增） | 9 | 100% |
| REQ-V48-003 | P1 | OpenAPI 反向生成 | 10 | §2.1.3.3 + §2.2.2（REQ-V48-003 接口） | `openapi-reverse`（扩展） | 9 | 100% |
| REQ-V48-004 | P1 | WASM 真实数据库连接闭环 | 12 | §2.1.3.4 + §2.2.2（REQ-V48-004 接口） | `wasm-real-db`（扩展） | 13 | 100% |

**设计覆盖完整性**：4 项需求的 42 个验收条件（10+10+10+12）全部映射到设计方案章节 + 接口清单 + 复用点，设计覆盖率 100%。

---

# 八、验证方法汇总

本节汇总 v4.8.0 四项需求的验证方法，每项附 `cargo test` / `grep` 命令。

| 需求 | 验证方法 | 验证内容 |
|------|---------|---------|
| REQ-V48-001 | `cargo test -p sz-orm-dtx --features cross-lang-dtx` | TonicGrpcCallHandler/ReqwestHttpCallHandler 真实传输 + CrossLangRecoveryCoordinator 崩溃恢复 + CrossLangSagaCoordinator/TccCoordinator 编排 + 幂等性 + 可观测性 + SDK 契约 |
| REQ-V48-002 | `cargo test -p sz-orm-lc --features lc-bidirectional-sync` | BidirectionalSyncEngine 双向同步 + SyncConflictDetector/Resolver 冲突检测解决 + SyncIncrementTracker 增量追踪 + SyncAuditLogger 审计日志 + 类型映射一致 + 破坏性变更拦截 |
| REQ-V48-003 | `cargo test -p sz-orm-swagger --features openapi-reverse` | DbSchemaReader 五方言读取 + DbSchemaToOpenApiMapper DB→OpenAPI + DbSchemaToCrudApiMapper DB→CRUD + FullReverseLoopVerifier 闭环 + 注入防护 + 命名约定 |
| REQ-V48-004 | `cargo test -p sz-orm-wasm --features wasm-real-db` | WasmProxyServer 代理后端 + MultiDialectProxyBackend 多方言 + WasmOrmSession ORM 会话 + WasmQueryBuilderBridge 查询桥接 + WasmOrmLoopVerifier 闭环 + 鉴权/白名单/限流 + 凭据隔离 + 结果集限制 |
| 全量门禁 | `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo check --workspace --all-targets --all-features` | fmt + clippy + 全量测试 + feature 全组合编译 |
| 占位检查 | `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs' packages/sz-orm-dtx packages/sz-orm-lc packages/sz-orm-swagger packages/sz-orm-wasm` | 零占位实现 |
| 幻影交付 | `python scripts/check-phantom-delivery.py` | 每项新能力附生产调用点 + 端到端接线 |
| sz-pay 兼容 | sz-pay 项目 `cargo build` | sz-pay 从 crates.io 拉取 sz-orm-* 6 个包编译通过 |

---

> 文档完成：v4.8.0 技术设计文档已生成，包含 8 章（概述 + 需求与存量功能关系分析 + 增量设计方案 + feature gate 依赖关系图 + 复用点清单 + 与 v4.7.0 零重叠声明 + 风险与缓解措施 + 需求追溯矩阵 + 验证方法汇总），56 项复用点均附 file:line 证据（2026-08-14 逐项实测验证），4 项需求 42 个验收条件设计覆盖率 100%，复用率 100%（不新增包，全部既有包扩展），与 v4.7.0 零重叠（包级 + feature gate 级 + 维度级三重零重叠）。
