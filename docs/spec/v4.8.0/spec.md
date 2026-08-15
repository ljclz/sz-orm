# sz-orm v4.8.0 需求规格说明书

> 版本：v4.8.0（跨语言分布式事务协调 + 低代码双向同步 + OpenAPI 反向生成 + WASM 真实数据库连接闭环）
> 基线：v4.7.0（消息延迟队列与优先级调度 + 迁移前向兼容性检查与沙箱预演 + 批量 COPY 协议与并行分片执行 + 异常自愈与根因分析 + 多云成本对比与容量预测 + 租户资源配额与行级安全增强 + 缓存预热与穿透防护，7 项需求 REQ-V47-001~007 全部通过 feature gate 隔离，228 套测试 0 失败，已发布到 crates.io 4.7.0，PHANTOM-1 从 37→34 接线修复完成，sz-pay 已升级到 v4.7.0）
> 日期：2026-08-14
> 需求格式：EARS（Ubiquitous / Event-driven / State-driven / Optional / Unwanted）
> 文档定位：需求规格（What to build），不含技术设计（How to build）
> 优先级声明：4 项需求全部 P1（用户选定全部 4 项），按"REQ-V48-001 跨语言分布式事务协调 → REQ-V48-003 OpenAPI 反向生成 → REQ-V48-004 WASM 真实连接闭环 → REQ-V48-002 低代码双向同步"序推进，4 项无强依赖可并行开发
> 需求编号约定：REQ-V48-xxx（v4.8.0 需求项，REQ-V48-001 ~ REQ-V48-004）
> 规划依据：`docs/spec/v4.7.0/` SDD 三阶段文档（spec 1218 行 / design / tasks，v4.7.0 已全部完成并发布 crates.io 4.7.0）+ 2026-08-14 逐项代码验证（file:line 均已实测存在）+ v4.7.0 范围声明中明确推迟至后续版本的 4 项长期方向（跨语言分布式事务/低代码双向同步/OpenAPI 反向生成/WASM 真实连接）
> 兼容性铁律：所有新能力通过 feature gate 隔离，默认 feature 行为不变，既有公开 API 完全向后兼容，v4.7.0 已验收测试基线（228 套）不回退；sz-pay 生产依赖（从 crates.io 拉取 sz-orm-* 6 个包）不得被破坏；五方言覆盖：MySQL/PostgreSQL/SQLite/Oracle/MSSQL
> 范围声明：本版本聚焦跨语言互操作与全栈闭环（跨语言分布式事务协调 + 低代码双向同步 + OpenAPI 反向生成 + WASM 真实数据库连接闭环）；本版本不涉及 crates.io 发布流程变更，不新增 workspace 成员（保持 60）
> 边界声明：与 v4.7.0 零重叠（见第 1.4 节），v4.7.0 是"智能化运维深化 + 性能深化"层（消息时间优先级/迁移前向兼容沙箱/COPY 并行分片/异常自愈 RCA/多云成本预测/租户配额 RLS/缓存预热防护），v4.8.0 是"跨语言互操作 + 全栈闭环"层（跨语言事务协调/低代码双向同步/OpenAPI 反向生成/WASM 真实连接）

---

# 1. 组件定位

## 1.1 核心职责

本组件负责交付 sz-orm v4.8.0 的四项跨语言互操作与全栈闭环能力：(1) 跨语言分布式事务协调（扩展既有 `sz-orm-dtx` 包 `packages/sz-orm-dtx/src/cross_lang/mod.rs:16` `ParticipantLanguage`（Go/Java/C++/Python/JavaScript 5 语言）+ `:108` `CrossLangParticipantProtocol` trait + `packages/sz-orm-dtx/src/cross_lang/protocol.rs:26` `GrpcParticipantProtocol` / `:78` `HttpParticipantProtocol` + `packages/sz-orm-dtx/src/cross_lang/participant.rs:51` `CrossLangParticipant::to_participant()` 适配器 + 既有 `packages/sz-orm-grpc/src/lib.rs:22` `GrpcServiceDef` / `:153` `UserGrpcService` + `packages/sz-orm-grpc/src/real_grpc.rs` tonic 桥接，补齐真实 gRPC/HTTP 传输实现 + 跨语言事务崩溃恢复 + Saga/TCC 深度编排集成）；(2) 低代码双向同步（扩展既有 `sz-orm-lc` 包 `packages/sz-orm-lc/src/lib.rs:24` `ModelDefinition` / `:210` `FieldTypeMapping`（SQL↔Rust↔HTML↔JSON Schema 四向映射）/ `:362` `ValidationRule` + 既有 `sz-orm-designer` 包 `packages/sz-orm-designer/src/lib.rs:1` `schema-designer` feature（`SchemaDesigner`/`ErDiagramEditor`/`code_gen`/`code_parse`），补齐 ORM 模型 ↔ 低代码引擎模型双向同步 + 冲突检测与解决 + 增量追踪）；(3) OpenAPI 反向生成（扩展既有 `sz-orm-swagger` 包 `packages/sz-orm-swagger/src/reverse/mod.rs:26` `OpenApiReverseGenerator`（OpenAPI→Model/迁移/Repository）/ `:28` `ApiFirstLoopVerifier` / `:30` `SchemaToModelMapper`，补齐 DB schema → OpenAPI 规范 + CRUD API 反向生成 + 五方言 schema 读取 + 完整闭环）；(4) WASM 真实数据库连接闭环（扩展既有 `sz-orm-wasm` 包 `packages/sz-orm-wasm/src/real_db/mod.rs:19` `WasmRealDbConnection` / `:21` `WasmRealDbQueryExecutor` / `:25` `WasmDbProxy`（HTTP/WebSocket 代理桥接）+ `packages/sz-orm-wasm/src/real_db/wasi_socket.rs:13` `WasiSocketConnection`（WASI socket 直连），补齐浏览器端 ORM 操作完整闭环 + 代理后端实现 + 多方言代理 + WASM 端查询构建器集成）。

## 1.2 核心输入

1. **v4.7.0 已验收基线**：消息延迟队列与优先级调度 + 迁移前向兼容性检查与沙箱预演 + 批量 COPY 协议与并行分片执行 + 异常自愈与根因分析 + 多云成本对比与容量预测 + 租户资源配额与行级安全增强 + 缓存预热与穿透防护，7 项能力全部通过 feature gate 隔离，228 套测试 0 失败，已发布到 crates.io 4.7.0，作为本版本基准。
2. **现有能力清单与缺口证据**：
   - **跨语言分布式事务（cross-lang-dtx feature 已有协议层）**：`packages/sz-orm-dtx/src/cross_lang/mod.rs:16` `pub enum ParticipantLanguage`（Go/Java/Cpp/Python/JavaScript 5 语言）、`:38` `ParticipantTransport`（Grpc/Http）、`:45` `ParticipantAuth`（Mtls/Token）、`:58` `CrossLangParticipantDesc`、`:108` `pub trait CrossLangParticipantProtocol`（prepare/commit/rollback/protocol_version）、`:128` `COORDINATOR_PROTOCOL_VERSION = 1`、`packages/sz-orm-dtx/src/cross_lang/protocol.rs:15` `pub trait RemoteCallHandler`、`:26` `GrpcParticipantProtocol`、`:78` `HttpParticipantProtocol`、`:130` `MockRemoteCallHandler`（仅测试用）、`:192` `check_protocol_version`、`packages/sz-orm-dtx/src/cross_lang/participant.rs:12` `CrossLangParticipant`、`:51` `to_participant()`（适配为既有 `TransactionParticipant`）、`packages/sz-orm-dtx/src/cross_lang/registry.rs`（注册中心）、`packages/sz-orm-dtx/src/cross_lang/serializer.rs`（补偿序列化）、`packages/sz-orm-dtx/src/cross_lang/observability.rs`（可观测）、`packages/sz-orm-dtx/src/lib.rs:186` `TransactionParticipant` / `:270` `DistributedTransaction` / `:432` `DtxManager` / `:57` `TransactionLogStore` / `:162` `TransactionState`、`packages/sz-orm-dtx/src/saga.rs`（Saga 编排）、`packages/sz-orm-dtx/src/tcc.rs`（TCC 三阶段）、`packages/sz-orm-dtx/src/recovery.rs`（XA 崩溃恢复）、`packages/sz-orm-grpc/src/lib.rs:22` `GrpcServiceDef` / `:153` `UserGrpcService` / `:235` `GrpcStream` / `:328` `Interceptor` / `:415` `RetryPolicy`、`packages/sz-orm-grpc/src/real_grpc.rs`（tonic 桥接）。缺口：`RemoteCallHandler` 仅有 `MockRemoteCallHandler`，无真实 tonic gRPC + reqwest HTTP 传输实现；跨语言参与者崩溃恢复未与既有 `TransactionLogStore`/`recovery` 集成；`CrossLangParticipant::to_participant()` 适配为通用 `TransactionParticipant`，但 Saga/TCC 专用编排（补偿顺序/幂等）未深度集成；各语言 SDK 接入契约未标准化。
   - **低代码（sz-orm-lc 模型定义 + sz-orm-designer 设计器）**：`packages/sz-orm-lc/src/lib.rs:24` `pub struct ModelDefinition`（name/fields/indexes/relations）、`:83` `FieldDef`、`:147` `RelationDefinition`、`:210` `pub struct FieldTypeMapping`（`sql_to_rust`/`sql_to_html_input`/`sql_to_json_schema`/`rust_to_sql` 四向映射）、`:362` `pub enum ValidationRule`（Required/MinLength/MaxLength/Min/Max/Pattern/Email/Url/Enum）、`FormField`/`FormGenerator`/`CrudTemplateEngine`（动态表单 + CRUD 模板）、`packages/sz-orm-designer/src/lib.rs:1` `#[cfg(feature = "schema-designer")]`（`SchemaDesign`/`DesignTable`/`DesignColumn`/`DesignRelation`/`SchemaDesigner`/`ErDiagramEditor`/`DesignerExporter`/`code_gen`/`code_parse`/`design_ir`）。缺口：无双向同步引擎（ORM 模型 ↔ 低代码引擎模型双向同步）；无同步冲突检测与解决（双向同时变更冲突）；无增量追踪（只同步变更部分）；无同步可观测性（同步日志/审计）。
   - **OpenAPI 反向生成（openapi-reverse feature 已有 OpenAPI→ORM 方向）**：`packages/sz-orm-swagger/src/reverse/mod.rs:26` `OpenApiReverseGenerator`（反向生成器主入口）/ `ReverseGenResult`、`:25` `ReverseGenConfig` / `NamingConvention`、`:30` `SchemaToModelMapper` / `ModelField` / `RustType` / `Constraint`（OpenAPI Schema → Rust struct）、`:29` `OpenApiToMigrationMapper`（OpenAPI Schema → 5 方言 DDL 迁移）、`:31` `OpenApiToRepositoryMapper`（OpenAPI Schema → Repository CRUD 骨架）、`:28` `ApiFirstLoopVerifier` / `LoopReport`（API 优先闭环验证）、`:27` `OpenApiInjectionGuard`（注入防护）、`:36` `ReverseGenError`（SpecParseFailed/UnsupportedSchemaConstruct/LoopVerificationDiff/InjectionDetected/UnsignedSpec/UserLogicOverwrite）。缺口：仅有 OpenAPI spec → ORM 方向，无 DB schema → OpenAPI 规范方向（从数据库实际 schema 读取表/列/约束，生成 OpenAPI spec）；无 DB schema → CRUD API 生成（REST 端点 + OpenAPI 文档）；无五方言 schema 读取（MySQL/PostgreSQL/SQLite/Oracle/MSSQL 的 information_schema 查询）；未与既有 reverse 形成完整闭环（DB schema → OpenAPI → ORM Model → CRUD）。
   - **WASM 真实连接（wasm-real-db feature 已有代理桥接）**：`packages/sz-orm-wasm/src/real_db/mod.rs:19` `WasmRealDbConnection` / `:20` `WasmTransport`、`:21` `WasmRealDbQueryExecutor`、`:22` `WasmRealDbMetrics`、`:25` `WasmDbProxy` / `DbCredentials`、`:26` `WasmDbRateLimiter`、`:27` `WasmRealDbReconnector`、`:28` `WasmDbSqlWhitelist`、`:23` `WasmDbProxyProtocol` / `ProxyRequest` / `ProxyResponse` / `SerializationFormat`、`:34` `WasmRealDbError`（ProxyUnavailable/SqlRejected/RateLimited/AuthFailed/CredentialsNotExposed/QueryFailed/ResultTooLarge/SerializationError）、`packages/sz-orm-wasm/src/real_db/wasi_socket.rs:13` `WasiSocketConnection`（WASI socket 直连，feature = "wasi-socket"）、`packages/sz-orm-wasm/src/real_db/auth.rs` `WasmDbAuthValidator`、`packages/sz-orm-wasm/src/real_db/executor.rs` `WasmRealDbQueryExecutor`、`packages/sz-orm-wasm/src/real_db/proxy.rs` `WasmDbProxy`、`packages/sz-orm-wasm/src/lib.rs`（cdylib + rlib）、`packages/sz-orm-wasm/src/js_bindings.rs`（JS 绑定）。缺口：无代理后端实现（Rust 后端代理服务，接收 WASM 代理请求 → 连接真实 DB → 返回结果，复用 sz-orm-core 连接池）；无浏览器端 ORM 操作完整闭环（WASM 端查询构建 → 代理 → 后端 DB → 结果返回，复用 sz-orm-query-builder）；无多方言代理（代理后端支持五方言）；WASM 端查询构建器未与 sz-orm-query-builder 集成。
3. **本机数据库连接信息**：MySQL 9.6（`mysql://root:test123@127.0.0.1:3306/sz_orm_test`）、PostgreSQL 18（`postgres://postgres:test123@127.0.0.1:5432/sz_orm_test`）、Oracle 23ai Free（`127.0.0.1:1521/freepdb1`）。
4. **sz-pay 生产依赖证据**：sz-pay 从 crates.io 拉取 sz-orm-* 6 个包（sz-orm-core/sqlx/config/auth/macros/queue），已升级到 v4.7.0，作为 API 兼容性验证的下游基准。
5. **五方言覆盖约束**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL，OpenAPI 反向生成的 schema 读取与 WASM 代理后端须覆盖全部方言。
6. **既有 feature gate 体系**：v4.7.0 已有 7 个 feature（`delayed-priority-queue` `packages/sz-orm-queue/Cargo.toml` / `forward-compat-sandbox` `packages/sz-orm-core/Cargo.toml` / `copy-parallel-shard` `packages/sz-orm-batch/Cargo.toml` / `anomaly-remediation-rca` `packages/sz-orm-observability/Cargo.toml` / `multicloud-cost-forecast` `packages/sz-orm-storage/Cargo.toml` / `tenant-quota-rls-enhanced` `packages/sz-orm-core/Cargo.toml` / `cache-warmup-protection` `packages/sz-orm-core/Cargo.toml`）+ 本版本涉及 3 个既有 feature（`cross-lang-dtx` `packages/sz-orm-dtx/Cargo.toml:40` / `openapi-reverse` `packages/sz-orm-swagger/Cargo.toml:14` / `wasm-real-db` `packages/sz-orm-wasm/Cargo.toml:35`）+ 1 个新增 feature（`lc-bidirectional-sync` sz-orm-lc），作为新能力 feature gate 隔离的基础。
7. **跨语言 SDK 包基础**：workspace 已有 `sz-orm-go` / `sz-orm-java` / `sz-orm-cpp` / `sz-orm-python` / `sz-orm-js` 5 个跨语言 FFI/绑定包，作为各语言参与者 SDK 接入契约的基础。

## 1.3 核心输出

1. **跨语言分布式事务协调**：sz-orm-dtx 扩展（`TonicGrpcCallHandler` 真实 tonic gRPC 传输 + `ReqwestHttpCallHandler` 真实 HTTP 传输 + `CrossLangRecoveryCoordinator` 跨语言崩溃恢复协调器 + `CrossLangSagaCoordinator` 跨语言 Saga 编排器 + `CrossLangTccCoordinator` 跨语言 TCC 编排器 + 各语言 SDK 接入契约，复用既有 `CrossLangParticipantProtocol`/`GrpcParticipantProtocol`/`HttpParticipantProtocol`/`CrossLangParticipant::to_participant()`/`TransactionLogStore`/`recovery`/`saga`/`tcc` + sz-orm-grpc `real_grpc`）。
2. **低代码双向同步**：sz-orm-lc + sz-orm-designer 扩展（`BidirectionalSyncEngine` 双向同步引擎 + `SyncDirection` 同步方向枚举 + `SyncConflictDetector` 冲突检测器 + `SyncConflictResolver` 冲突解决器 + `ConflictResolutionStrategy` 冲突解决策略枚举 + `SyncIncrementTracker` 增量追踪器 + `SyncAuditLogger` 同步审计日志器，复用既有 `ModelDefinition`/`FieldDef`/`FieldTypeMapping`/`ValidationRule` + `SchemaDesigner`/`code_gen`/`code_parse`/`design_ir`）。
3. **OpenAPI 反向生成**：sz-orm-swagger 扩展（`DbSchemaToOpenApiMapper` DB schema → OpenAPI 规范映射器 + `DbSchemaReader` 五方言 schema 读取器 + `DbSchemaToCrudApiMapper` DB schema → CRUD API 映射器 + `CrudApiEndpoint` CRUD API 端点定义 + `DbSchemaReverseGenResult` DB schema 反向生成结果 + `FullReverseLoopVerifier` 完整闭环验证器，复用既有 `OpenApiReverseGenerator`/`SchemaToModelMapper`/`OpenApiToMigrationMapper`/`OpenApiToRepositoryMapper`/`ApiFirstLoopVerifier`/`OpenApiInjectionGuard`）。
4. **WASM 真实数据库连接闭环**：sz-orm-wasm 扩展（`WasmProxyServer` 代理后端服务 + `WasmQueryBuilderBridge` WASM 端查询构建器桥接 + `WasmOrmSession` WASM 端 ORM 会话 + `MultiDialectProxyBackend` 多方言代理后端 + `WasmOrmLoopVerifier` WASM ORM 闭环验证器，复用既有 `WasmRealDbConnection`/`WasmRealDbQueryExecutor`/`WasmDbProxy`/`WasmDbProxyProtocol`/`WasmDbAuthValidator`/`WasmDbSqlWhitelist`/`WasmDbRateLimiter`/`WasmRealDbReconnector`/`WasiSocketConnection` + sz-orm-core 连接池 + sz-orm-query-builder）。
5. **需求追溯矩阵**：本文档第 7 章，建立需求 ↔ 验收条件映射。
6. **验收标准总览**：本文档第 8 章，按需求项汇总验收条件。

## 1.4 职责边界

本组件**不负责**以下事项：

1. **不破坏既有公开 API**：所有新能力通过 feature gate 隔离，既有公开 API 签名保持完全向后兼容。既有 `CrossLangParticipantProtocol` trait（`packages/sz-orm-dtx/src/cross_lang/mod.rs:108`）/ `GrpcParticipantProtocol`（`packages/sz-orm-dtx/src/cross_lang/protocol.rs:26`）/ `HttpParticipantProtocol`（`:78`）/ `CrossLangParticipant::to_participant()`（`packages/sz-orm-dtx/src/cross_lang/participant.rs:51`）/ `DtxManager`（`packages/sz-orm-dtx/src/lib.rs:432`）/ `OpenApiReverseGenerator`（`packages/sz-orm-swagger/src/reverse/mod.rs:26`）/ `WasmRealDbConnection`（`packages/sz-orm-wasm/src/real_db/mod.rs:19`）/ `WasmDbProxy`（`:25`）/ `ModelDefinition`（`packages/sz-orm-lc/src/lib.rs:24`）/ `FieldTypeMapping`（`:210`）保留不动，新增能力为扩展 API。
2. **不改变既有安全铁律**：任何 WHERE 条件必须参数化，默认禁止 `SELECT *`，N+1 检测自动拦截，跨语言事务参与者调用须鉴权（mTLS/Token），WASM 代理须 SQL 白名单 + 限流 + 鉴权，OpenAPI 反向生成须注入防护，沿用既有铁律。
3. **不重写跨语言事务协议层**：既有 `CrossLangParticipantProtocol`（`packages/sz-orm-dtx/src/cross_lang/mod.rs:108`）/ `GrpcParticipantProtocol`（`packages/sz-orm-dtx/src/cross_lang/protocol.rs:26`）/ `HttpParticipantProtocol`（`:78`）/ `CrossLangParticipant`（`packages/sz-orm-dtx/src/cross_lang/participant.rs:12`）/ `CrossLangParticipant::to_participant()`（`:51`）/ `registry`/`serializer`/`observability` 保留不动，真实传输实现 + 崩溃恢复 + Saga/TCC 深度编排为扩展，不修改既有协议层逻辑。
4. **不重写分布式事务核心**：既有 `DistributedTransaction`（`packages/sz-orm-dtx/src/lib.rs:270`）/ `DtxManager`（`:432`）/ `TransactionParticipant`（`:186`）/ `TransactionLogStore`（`:57`）/ `TransactionState`（`:162`）+ `saga`/`tcc`/`cross_shard`/`xa`/`recovery`/`suspension` 保留不动，跨语言协调器为扩展，不修改既有事务核心逻辑。
5. **不重写 gRPC 核心**：既有 `GrpcServiceDef`（`packages/sz-orm-grpc/src/lib.rs:22`）/ `UserGrpcService`（`:153`）/ `GrpcStream`（`:235`）/ `Interceptor`（`:328`）/ `RetryPolicy`（`:415`）/ `real_grpc.rs` 保留不动，跨语言真实 gRPC 传输复用既有 gRPC 基础设施。
6. **不重写低代码模型定义**：既有 `ModelDefinition`（`packages/sz-orm-lc/src/lib.rs:24`）/ `FieldDef`（`:83`）/ `RelationDefinition`（`:147`）/ `FieldTypeMapping`（`:210`）/ `ValidationRule`（`:362`）/ `FormField`/`FormGenerator`/`CrudTemplateEngine` 保留不动，双向同步引擎为扩展，不修改既有模型定义/类型映射/验证规则逻辑。
7. **不重写 schema 设计器**：既有 `sz-orm-designer` `schema-designer` feature（`SchemaDesigner`/`ErDiagramEditor`/`code_gen`/`code_parse`/`design_ir`/`exporter`/`masking`/`web_ui`）保留不动，双向同步复用既有设计器能力，不修改既有设计器逻辑。
8. **不重写 OpenAPI 反向生成既有方向**：既有 `OpenApiReverseGenerator`（`packages/sz-orm-swagger/src/reverse/mod.rs:26`）/ `SchemaToModelMapper`（`:30`）/ `OpenApiToMigrationMapper`（`:29`）/ `OpenApiToRepositoryMapper`（`:31`）/ `ApiFirstLoopVerifier`（`:28`）/ `OpenApiInjectionGuard`（`:27`）（OpenAPI→ORM 方向）保留不动，DB schema → OpenAPI 方向为扩展，不修改既有反向生成逻辑。
9. **不重写 WASM 代理桥接**：既有 `WasmRealDbConnection`（`packages/sz-orm-wasm/src/real_db/mod.rs:19`）/ `WasmRealDbQueryExecutor`（`:21`）/ `WasmDbProxy`（`:25`）/ `WasmDbProxyProtocol`（`:23`）/ `WasmDbAuthValidator`/`WasmDbSqlWhitelist`/`WasmDbRateLimiter`/`WasmRealDbReconnector`/`WasmRealDbMetrics`/`WasiSocketConnection`（`packages/sz-orm-wasm/src/real_db/wasi_socket.rs:13`）保留不动，代理后端实现 + ORM 闭环为扩展，不修改既有代理桥接逻辑。
10. **不新增 workspace 成员**：workspace 成员保持 60（`Cargo.toml:2`），所有新能力落在既有包扩展（sz-orm-dtx / sz-orm-grpc / sz-orm-lc / sz-orm-designer / sz-orm-swagger / sz-orm-wasm）。
11. **不与 v4.7.0 任务重叠**：v4.7.0 已占用的包/模块（`sz-orm-queue` delayed-priority-queue / `sz-orm-core` forward-compat-sandbox / `sz-orm-batch` copy-parallel-shard / `sz-orm-observability` anomaly-remediation-rca / `sz-orm-storage` multicloud-cost-forecast / `sz-orm-core` tenant-quota-rls-enhanced / `sz-orm-core` cache-warmup-protection）本版本不触碰其新增逻辑，新增范围全部落在既有包扩展（sz-orm-dtx / sz-orm-grpc / sz-orm-lc / sz-orm-designer / sz-orm-swagger / sz-orm-wasm）。
12. **不负责 sz-pay / sz-rust 下游代码修改**：ADR-0001 严禁修改下游/上游仓库，仅保证 API 兼容性。
13. **不降低既有测试覆盖**：v4.8.0 不得使 v4.7.0 已验收测试基线（228 套）回退，仅增不减。
14. **不引入 unsafe**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释，沿用既有 unsafe 零容忍铁律。
15. **不引入占位实现**：禁止 `todo!`/`unimplemented!`/`unreachable!`，所有新增代码须真实实现。
16. **不引入 Breaking Change**：新能力通过 feature gate 隔离，默认全关闭，既有 feature 组合行为不变。
17. **不强制启用新能力**：所有新能力默认关闭或可选启用，避免无配置环境行为变化。
18. **不做跨语言事务的自动语言探测**：跨语言参与者语言类型由配置显式声明（`CrossLangParticipantDesc.language`），不自动探测远端语言，避免误判。
19. **不做低代码双向同步的自动 schema 迁移**：双向同步检测冲突后须人工确认或按策略解决，不自动执行破坏性 schema 变更（如删列/改类型），沿用 v4.7.0 异常自愈人工确认铁律。
20. **不做 WASM 端直连数据库**：浏览器 WASM 环境不可直连数据库（浏览器无 socket API），须通过代理后端桥接；WASI 环境可通过 `WasiSocketConnection` 直连代理，但仍非直连 DB。

---

# 2. 领域术语

**跨语言分布式事务协调（Cross-Language DTX Coordination）**

: 扩展既有 `sz-orm-dtx` `cross-lang-dtx` feature（`packages/sz-orm-dtx/src/cross_lang/mod.rs:16`），补齐真实 gRPC/HTTP 传输实现 + 跨语言事务崩溃恢复 + Saga/TCC 深度编排集成，使 Rust 协调器透明编排 Rust 内部参与者与 Go/Java/C++/Python/JavaScript 跨语言参与者。
: 备注：v4.7.0 已有协议层（`CrossLangParticipantProtocol` trait + `GrpcParticipantProtocol`/`HttpParticipantProtocol` + `CrossLangParticipant::to_participant()` 适配器），本版本补真实传输 + 恢复 + 深度编排。

**跨语言参与者（Cross-Language Participant）**

: 异构语言服务（Go/Java/C++/Python/JavaScript）作为分布式事务参与者，通过 gRPC 或 HTTP/JSON 与 Rust 协调器通信，实现 prepare/commit/rollback 三阶段，`CrossLangParticipantDesc`（`packages/sz-orm-dtx/src/cross_lang/mod.rs:58`）描述参与者元信息（语言/传输/端点/鉴权/协议版本）。

**真实 gRPC 传输（Tonic gRPC Transport）**

: 基于 tonic + prost 的真实 gRPC 传输实现 `RemoteCallHandler` trait（`packages/sz-orm-dtx/src/cross_lang/protocol.rs:15`），替代既有仅测试用的 `MockRemoteCallHandler`（`:130`），复用 sz-orm-grpc `real_grpc.rs` tonic 桥接。

**跨语言事务崩溃恢复（Cross-Language Tx Recovery）**

: 协调器崩溃后恢复跨语言参与者状态，`CrossLangRecoveryCoordinator` 复用既有 `TransactionLogStore`（`packages/sz-orm-dtx/src/lib.rs:57`）/ `recovery`（`packages/sz-orm-dtx/src/recovery.rs`），查询未完成事务 + 询问各跨语言参与者状态 + 决定提交/回滚。

**低代码双向同步（Low-Code Bidirectional Sync）**

: ORM 模型 ↔ 低代码引擎模型双向同步，`BidirectionalSyncEngine` 在两个方向同步模型定义（ORM → 低代码 / 低代码 → ORM），`SyncDirection` 枚举控制同步方向，冲突时按 `ConflictResolutionStrategy` 解决。
: 备注：既有 `sz-orm-lc` `ModelDefinition`/`FieldTypeMapping`（`packages/sz-orm-lc/src/lib.rs:24`/`:210`）+ `sz-orm-designer` `SchemaDesigner`/`code_gen`/`code_parse`（`packages/sz-orm-designer/src/lib.rs:1`）为单向基础，本版本补双向同步。

**同步冲突检测与解决（Sync Conflict Detection & Resolution）**

: 双向同时变更时的冲突检测与解决，`SyncConflictDetector` 检测冲突（同字段双向变更/类型不一致/约束不一致），`SyncConflictResolver` 按 `ConflictResolutionStrategy` 枚举（OrmWins ORM 优先 / LcWins 低代码优先 / Merge 合并 / Manual 人工确认，默认 Manual）解决。

**同步增量追踪（Sync Increment Tracking）**

: 只同步变更部分而非全量重建，`SyncIncrementTracker` 追踪模型变更（字段增删改/类型变更/约束变更），同步时只处理变更项，提升同步效率。

**OpenAPI 反向生成（OpenAPI Reverse Generation）**

: 扩展既有 `sz-orm-swagger` `openapi-reverse` feature（`packages/sz-orm-swagger/src/reverse/mod.rs:26` `OpenApiReverseGenerator`，OpenAPI→ORM 方向），补齐 DB schema → OpenAPI 规范 + CRUD API 反向生成方向，形成完整闭环（DB schema → OpenAPI → ORM Model → CRUD）。
: 备注：v4.7.0 已有 OpenAPI→Model/迁移/Repository 方向，本版本补 DB→OpenAPI 方向。

**DB schema 读取（DB Schema Reading）**

: 从数据库实际 schema 读取表/列/约束/索引元信息，`DbSchemaReader` 查询五方言 information_schema（MySQL information_schema / PostgreSQL pg_catalog / SQLite sqlite_master / Oracle ALL_TAB_COLUMNS / MSSQL INFORMATION_SCHEMA），生成 DB schema 描述。

**DB schema → OpenAPI 规范（DB Schema to OpenAPI Spec）**

: `DbSchemaToOpenApiMapper` 将 DB schema 描述映射为 OpenAPI 3.0 规范（表 → Schema / 列 → 字段 / 主键 → required / 唯一约束 → uniqueItems / 外键 → 关联），生成完整的 OpenAPI spec（paths + components.schemas + parameters）。

**DB schema → CRUD API（DB Schema to CRUD API）**

: `DbSchemaToCrudApiMapper` 为每张表生成标准 CRUD REST 端点（GET /resource 列表 / GET /resource/{id} 详情 / POST /resource 创建 / PUT /resource/{id} 更新 / DELETE /resource/{id} 删除）+ OpenAPI 文档，`CrudApiEndpoint` 描述端点定义。

**WASM 真实数据库连接闭环（WASM Real DB Connection Loop）**

: 扩展既有 `sz-orm-wasm` `wasm-real-db` feature（`packages/sz-orm-wasm/src/real_db/mod.rs:19` `WasmRealDbConnection` 代理桥接），补齐浏览器端 ORM 操作完整闭环（WASM 端查询构建 → 代理 → 后端 DB → 结果返回）+ 代理后端实现 + 多方言代理 + WASM 端查询构建器集成。
: 备注：v4.7.0 已有代理桥接（HTTP/WebSocket → 后端 DB）+ 鉴权/限流/白名单/重连/指标，本版本补代理后端 + ORM 闭环。

**代理后端（Proxy Backend）**

: Rust 后端代理服务 `WasmProxyServer`，接收 WASM 端代理请求（`ProxyRequest` `packages/sz-orm-wasm/src/real_db/mod.rs:23`）→ 鉴权 + SQL 白名单检查 + 限流 → 连接真实 DB（复用 sz-orm-core 连接池）→ 执行查询 → 返回结果（`ProxyResponse`），后端凭据不暴露给 WASM 端（`WasmRealDbError::CredentialsNotExposed`）。

**WASM 端 ORM 会话（WASM ORM Session）**

: `WasmOrmSession` 在 WASM 端提供 ORM 会话（查询构建 + 代理执行 + 结果反序列化），复用 sz-orm-query-builder 构建参数化查询，通过 `WasmDbProxy` 代理执行，`WasmQueryBuilderBridge` 桥接查询构建器与代理协议。

**v4.8.0 feature gate**

: 控制本版本新能力的 feature gate 集合（`cross-lang-dtx` 扩展 / `lc-bidirectional-sync` 新增 / `openapi-reverse` 扩展 / `wasm-real-db` 扩展），默认关闭，避免无配置环境行为变化。

---

# 3. 角色与边界

## 3.1 核心角色

1. **分布式事务协调者（Rust 协调器）**：运行 sz-orm-dtx 协调器，编排跨语言分布式事务（prepare/commit/rollback），管理事务日志与崩溃恢复。
2. **跨语言参与者开发者（Go/Java/C++/Python/JS）**：实现跨语言参与者服务，按接入契约实现 prepare/commit/rollback 端点，注册到 Rust 协调器。
3. **低代码平台开发者**：使用低代码引擎定义模型，通过双向同步与 ORM 模型保持一致。
4. **ORM 开发者**：定义 ORM 模型，通过双向同步将模型变更同步到低代码引擎。
5. **API 设计者**：从数据库 schema 反向生成 OpenAPI 规范 + CRUD API，或从 OpenAPI 规范反向生成 ORM 模型，形成 API 优先开发闭环。
6. **WASM 前端开发者**：在浏览器 WASM 端使用 ORM 会话执行数据库操作，通过代理后端桥接真实 DB。
7. **代理后端运维者**：部署代理后端服务，配置鉴权/限流/SQL 白名单/数据库连接。

## 3.2 外部系统

1. **跨语言参与者服务（Go/Java/C++/Python/JS）**：通过 gRPC 或 HTTP/JSON 与 Rust 协调器通信，实现 prepare/commit/rollback 三阶段。
2. **低代码引擎**：低代码平台模型存储，与 ORM 模型双向同步。
3. **数据库（五方言）**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL，提供 schema 元信息（OpenAPI 反向生成）与数据存取（WASM 代理后端）。
4. **代理后端服务**：Rust 后端代理，接收 WASM 代理请求，连接真实 DB。
5. **sz-orm-grpc**：gRPC 服务定义与调用基础设施，跨语言真实 gRPC 传输复用。
6. **sz-orm-core 连接池**：代理后端复用连接池连接真实 DB。
7. **sz-orm-query-builder**：WASM 端查询构建器桥接复用。

## 3.3 交互上下文

```plantuml
@startuml
title sz-orm v4.8.0 跨语言互操作与全栈闭环 上下文图

actor "分布式事务协调者\n(Rust 协调器)" as coordinator
actor "跨语言参与者开发者" as crosslang_dev
actor "低代码平台开发者" as lc_dev
actor "ORM 开发者" as orm_dev
actor "API 设计者" as api_designer
actor "WASM 前端开发者" as wasm_dev
actor "代理后端运维者" as proxy_ops

package "sz-orm v4.8.0" as szorm {
  component "sz-orm-dtx\n(跨语言事务协调)" as dtx
  component "sz-orm-lc + sz-orm-designer\n(低代码双向同步)" as lc
  component "sz-orm-swagger\n(OpenAPI 反向生成)" as swagger
  component "sz-orm-wasm\n(WASM 真实连接闭环)" as wasm
}

component "跨语言参与者服务\n(Go/Java/C++/Python/JS)" as crosslang_svc
component "低代码引擎" as lc_engine
database "数据库\n(MySQL/PG/SQLite/Oracle/MSSQL)" as db
component "代理后端服务" as proxy_backend
component "sz-orm-grpc" as grpc
component "sz-orm-core 连接池" as pool
component "sz-orm-query-builder" as qb

coordinator --> dtx : 编排跨语言事务
crosslang_dev --> crosslang_svc : 实现参与者
crosslang_svc --> dtx : gRPC/HTTP prepare/commit/rollback
dtx --> grpc : 真实 gRPC 传输

lc_dev --> lc_engine : 定义模型
orm_dev --> lc : 定义 ORM 模型
lc <--> lc_engine : 双向同步

api_designer --> swagger : 反向生成
swagger --> db : 读取 schema

wasm_dev --> wasm : ORM 会话
wasm --> proxy_backend : 代理请求
proxy_ops --> proxy_backend : 部署/配置
proxy_backend --> pool : 连接 DB
pool --> db : 数据存取
wasm --> qb : 查询构建

@enduml
```

---

# 4. DFX 约束

## 4.1 性能

1. **跨语言事务协调延迟**：跨语言参与者调用（prepare/commit/rollback）端到端延迟不超过 100ms（不含网络 RTT，含序列化 + 协议处理 + 日志写入），可配超时（默认 5000ms，`CrossLangParticipant::with_timeout` `packages/sz-orm-dtx/src/cross_lang/participant.rs:30`）。
2. **低代码双向同步吞吐量**：单次全量同步 100 表 × 50 字段模型不超过 5 秒，增量同步不超过 1 秒（仅变更项）。
3. **OpenAPI 反向生成延迟**：DB schema → OpenAPI spec + CRUD API 反向生成 100 表 × 50 字段不超过 10 秒（含 schema 读取 + 映射 + 生成）。
4. **WASM 代理查询延迟**：WASM 端 ORM 查询端到端延迟不超过 200ms（不含 DB 执行时间，含查询构建 + 代理传输 + 结果反序列化）。
5. **代理后端吞吐量**：代理后端单实例吞吐量不低于 10000 QPS（复用 sz-orm-core 连接池）。

## 4.2 可靠性

1. **跨语言事务原子性**：跨语言分布式事务满足原子性（全部提交或全部回滚），崩溃后可通过 `CrossLangRecoveryCoordinator` 恢复至一致状态，复用既有 `TransactionLogStore`（`packages/sz-orm-dtx/src/lib.rs:57`）日志持久化。
2. **跨语言事务幂等性**：跨语言参与者 prepare/commit/rollback 须幂等（重复调用结果一致），`CrossLangCompensationSerializer`（`packages/sz-orm-dtx/src/cross_lang/serializer.rs`）生成幂等键。
3. **低代码同步一致性**：双向同步后 ORM 模型与低代码引擎模型一致（字段/类型/约束/关联一致），冲突按策略解决后须记录审计日志。
4. **OpenAPI 反向生成闭环一致性**：DB schema → OpenAPI → ORM Model → CRUD 闭环验证须一致（`FullReverseLoopVerifier`），与既有 `ApiFirstLoopVerifier`（`packages/sz-orm-swagger/src/reverse/mod.rs:28`）对齐。
5. **WASM 代理可用性**：代理后端可用性目标 99.9%，`WasmRealDbReconnector`（`packages/sz-orm-wasm/src/real_db/mod.rs:27`）自动重连，复用既有重连逻辑。
6. **WASM 代理凭据隔离**：后端数据库凭据不暴露给 WASM 端（`WasmRealDbError::CredentialsNotExposed` `packages/sz-orm-wasm/src/real_db/mod.rs:52`），由代理后端持有。

## 4.3 安全性

1. **跨语言事务鉴权**：跨语言参与者调用须鉴权（mTLS 双向认证或 Token 认证，`ParticipantAuth` `packages/sz-orm-dtx/src/cross_lang/mod.rs:45`），协议版本须匹配（`check_protocol_version` `packages/sz-orm-dtx/src/cross_lang/protocol.rs:192`）。
2. **跨语言事务注入防护**：跨语言参与者调用负载须序列化（`CrossLangCompensationSerializer`），禁止注入任意代码/SQL。
3. **OpenAPI 反向生成注入防护**：OpenAPI spec 须签名验证（`ReverseGenError::UnsignedSpec`/`InjectionDetected` `packages/sz-orm-swagger/src/reverse/mod.rs:36`），`OpenApiInjectionGuard`（`:27`）检测注入。
4. **WASM 代理 SQL 白名单**：WASM 端查询须经 SQL 白名单检查（`WasmDbSqlWhitelist` `packages/sz-orm-wasm/src/real_db/mod.rs:28`），拒绝非白名单 SQL（`WasmRealDbError::SqlRejected`）。
5. **WASM 代理限流**：WASM 端查询须限流（`WasmDbRateLimiter` `packages/sz-orm-wasm/src/real_db/mod.rs:26`），超限拒绝（`WasmRealDbError::RateLimited`）。
6. **WASM 代理鉴权**：WASM 端须鉴权（`WasmDbAuthValidator` `packages/sz-orm-wasm/src/real_db/mod.rs:18`），失败拒绝（`WasmRealDbError::AuthFailed`）。
7. **参数化查询**：WASM 端查询构建须参数化（复用 sz-orm-query-builder `where_eq`/`or_where_eq`），禁止 SQL 字符串拼接。

## 4.4 可维护性

1. **跨语言事务可观测性**：跨语言事务须接入分布式追踪（复用 `cross_lang::observability` `packages/sz-orm-dtx/src/cross_lang/observability.rs`），记录事务 ID + 参与者 + 阶段 + 耗时 + 结果。
2. **低代码同步审计日志**：同步操作须记录审计日志（同步方向 + 变更项 + 冲突 + 解决策略 + 时间），追加写入不可篡改。
3. **OpenAPI 反向生成日志**：反向生成须记录日志（schema 来源 + 生成项 + 闭环验证结果 + 耗时）。
4. **WASM 代理指标**：代理查询须记录指标（复用 `WasmRealDbMetrics` `packages/sz-orm-wasm/src/real_db/mod.rs:22`），含 QPS + 延迟 + 错误率 + 白名单拒绝数 + 限流数。
5. **门禁前置**：所有新增代码须通过 21 道门禁（fmt/check/clippy/test/doc/audit/integration/占位检查/SQL 注入扫描/feature 全组合/上游未修改/文档一致性/审计证据/文档同步/幻影交付/语义反模式/架构一致性/度量真实性/发布一致性/变异测试/安全攻击）。

## 4.5 兼容性

1. **API 向后兼容**：所有新能力通过 feature gate 隔离，默认关闭，既有公开 API 签名不变。
2. **v4.7.0 测试基线不回退**：v4.7.0 已验收 228 套测试须全部通过，仅增不减。
3. **sz-pay 生产依赖兼容**：sz-pay 从 crates.io 拉取的 sz-orm-* 6 个包 API 不变，升级到 v4.8.0 须无 Breaking Change。
4. **五方言覆盖**：OpenAPI 反向生成 schema 读取与 WASM 代理后端须覆盖 MySQL/PostgreSQL/SQLite/Oracle/MSSQL。
5. **跨语言协议版本兼容**：跨语言参与者协议版本须匹配（`COORDINATOR_PROTOCOL_VERSION = 1` `packages/sz-orm-dtx/src/cross_lang/mod.rs:128`），版本不匹配拒绝（`CrossLangTxError::ProtocolVersionMismatch`）。

---

# 5. 核心能力

## 5.1 跨语言分布式事务协调（REQ-V48-001，P1）

### 5.1.1 业务规则

1. **真实 gRPC 传输实现**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-dtx` `cross-lang-dtx` feature，提供 `TonicGrpcCallHandler` 真实 tonic gRPC 传输实现 `RemoteCallHandler` trait（`packages/sz-orm-dtx/src/cross_lang/protocol.rs:15`），替代既有仅测试用的 `MockRemoteCallHandler`（`:130`），复用 sz-orm-grpc `real_grpc.rs` tonic 桥接 + 既有 `GrpcParticipantProtocol`（`:26`），支持 mTLS 双向认证与 Token 认证（`ParticipantAuth` `packages/sz-orm-dtx/src/cross_lang/mod.rs:45`）。
   a. 验收条件：[Go 参与者 gRPC 端点 + TonicGrpcCallHandler + prepare 调用] → [通过 tonic gRPC 真实调用 Go 参与者 prepare 端点，返回 ParticipantResponse]；[mTLS 认证 + 证书过期] → [返回 CrossLangTxError::AuthFailed]
2. **真实 HTTP 传输实现**（EARS: Ubiquitous）
   系统应当提供 `ReqwestHttpCallHandler` 真实 HTTP/JSON 传输实现 `RemoteCallHandler` trait，复用既有 `HttpParticipantProtocol`（`packages/sz-orm-dtx/src/cross_lang/protocol.rs:78`），支持 Token 认证与超时控制。
   a. 验收条件：[Java 参与者 HTTP 端点 + ReqwestHttpCallHandler + commit 调用] → [通过 HTTP/JSON 真实调用 Java 参与者 commit 端点，返回 ParticipantResponse]
3. **跨语言事务崩溃恢复**（EARS: State-driven）
   当协调器崩溃重启后存在未完成跨语言事务时，系统应当通过 `CrossLangRecoveryCoordinator` 恢复事务至一致状态：查询未完成事务（复用既有 `TransactionLogStore::read_pending` `packages/sz-orm-dtx/src/lib.rs:71`）→ 询问各跨语言参与者状态（prepare/commit/rollback）→ 按参与者状态决定全局提交或回滚 → 记录恢复日志。
   a. 验收条件：[协调器崩溃 + 事务 tx1 已 Prepared + 参与者 A 已 Committed + 参与者 B 已 Prepared] → [恢复时询问 A/B 状态，A 已提交 → 全局提交，通知 B commit]；[协调器崩溃 + 事务 tx2 Preparing 中 + 参与者 C 未 Prepared] → [全局回滚，通知 C rollback]
4. **Saga 跨语言编排**（EARS: Ubiquitous）
   系统应当提供 `CrossLangSagaCoordinator` 跨语言 Saga 编排器，将跨语言参与者接入既有 Saga 编排（`packages/sz-orm-dtx/src/saga.rs`），支持跨语言补偿（rollback 对应补偿动作），补偿顺序按 Saga 反向执行，复用 `CrossLangParticipant::to_participant()`（`packages/sz-orm-dtx/src/cross_lang/participant.rs:51`）适配。
   a. 验收条件：[Saga tx3 = Rust 参与者 A → Go 参与者 B → Java 参与者 C + C 失败] → [反向补偿：C rollback → B rollback → A rollback，补偿顺序正确]
5. **TCC 跨语言编排**（EARS: Ubiquitous）
   系统应当提供 `CrossLangTccCoordinator` 跨语言 TCC 编排器，将跨语言参与者接入既有 TCC 三阶段（`packages/sz-orm-dtx/src/tcc.rs`），Try-Confirm-Cancel 三阶段跨语言调用，复用 `CrossLangParticipant::to_participant()` 适配。
   a. 验收条件：[TCC tx4 = Rust A + Go B + Python C + Try 全部成功 + Confirm B 失败] → [Cancel A + Cancel C，TCC 三阶段正确]
6. **跨语言事务幂等性**（EARS: Ubiquitous）
   系统应当保证跨语言参与者 prepare/commit/rollback 幂等，`CrossLangCompensationSerializer`（`packages/sz-orm-dtx/src/cross_lang/serializer.rs`）生成幂等键，重复调用同一阶段结果一致，避免崩溃恢复时重复执行副作用。
   a. 验收条件：[参与者 A commit 幂等键 key1 + 重复 commit key1] → [第二次 commit 返回缓存结果，不重复执行副作用]
7. **跨语言事务可观测性**（EARS: Ubiquitous）
   系统应当接入分布式追踪（复用 `cross_lang::observability` `packages/sz-orm-dtx/src/cross_lang/observability.rs`），记录事务 ID + 参与者（语言/端点）+ 阶段 + 耗时 + 结果，跨语言边界追踪上下文传播。
   a. 验收条件：[跨语言事务 tx5 + Go 参与者 prepare] → [追踪 span 记录 tx5 + participant=go + phase=prepare + latency + result]
8. **复用既有协议层与事务核心**（EARS: Ubiquitous）
   系统应当复用既有 `CrossLangParticipantProtocol`（`packages/sz-orm-dtx/src/cross_lang/mod.rs:108`）/ `GrpcParticipantProtocol`（`packages/sz-orm-dtx/src/cross_lang/protocol.rs:26`）/ `HttpParticipantProtocol`（`:78`）/ `CrossLangParticipant::to_participant()`（`packages/sz-orm-dtx/src/cross_lang/participant.rs:51`）/ `TransactionLogStore`（`packages/sz-orm-dtx/src/lib.rs:57`）/ `recovery`/`saga`/`tcc` + sz-orm-grpc `real_grpc`，真实传输 + 恢复 + 深度编排基于既有扩展，不重复实现。
   a. 验收条件：[跨语言事务协调] → [复用既有 CrossLangParticipantProtocol/GrpcParticipantProtocol/TransactionLogStore/saga/tcc，不新建协议层/事务核心]
9. **各语言 SDK 接入契约**（EARS: Ubiquitous）
   系统应当为 Go/Java/C++/Python/JavaScript 各语言提供参与者 SDK 接入契约（prepare/commit/rollback 端点签名 + 协议版本 + 鉴权 + 序列化格式），复用 workspace 既有 `sz-orm-go`/`sz-orm-java`/`sz-orm-cpp`/`sz-orm-python`/`sz-orm-js` 包基础。
   a. 验收条件：[Go 参与者按契约实现 + 注册到协调器] → [协调器可识别 Go 参与者，按契约调用 prepare/commit/rollback]
10. **禁止项**（EARS: Unwanted）
    如果跨语言事务协调影响默认 feature 编译或破坏既有协议层/事务核心/gRPC 核心，则系统应当通过 `cross-lang-dtx` feature gate 隔离，默认不启用，且既有 `CrossLangParticipantProtocol`/`GrpcParticipantProtocol`/`HttpParticipantProtocol`/`DtxManager`/`saga`/`tcc` 保留不动。
    a. 验收条件：[`cargo build` 默认编译] → [无跨语言事务协调，行为与 v4.7.0 一致]

### 5.1.2 交互流程

```plantuml
@startuml
title 跨语言分布式事务协调 交互流程
actor "协调者" as coord
participant "CrossLangSagaCoordinator\n(跨语言 Saga 编排)" as saga
participant "CrossLangParticipant\n(跨语言参与者适配)" as adapter
participant "TonicGrpcCallHandler\n(真实 gRPC 传输)" as grpc
participant "ReqwestHttpCallHandler\n(真实 HTTP 传输)" as http
participant "Go 参与者服务" as go_svc
participant "Java 参与者服务" as java_svc
participant "TransactionLogStore\n(事务日志)" as log
participant "CrossLangRecoveryCoordinator\n(崩溃恢复)" as recovery

coord -> saga : begin_saga(tx1, [Rust A, Go B, Java C])
saga -> log : append(tx1, Preparing) [复用既有]
saga -> adapter : to_participant(Go B) [复用既有]
adapter -> grpc : prepare(tx1, payload)
grpc -> go_svc : gRPC prepare(tx1, payload) [mTLS]
go_svc --> grpc : ParticipantResponse(success)
grpc --> adapter : Response
saga -> adapter : to_participant(Java C)
adapter -> http : prepare(tx1, payload)
http -> java_svc : HTTP POST /prepare(tx1, payload) [Token]
java_svc --> http : ParticipantResponse(success)
http --> adapter : Response
saga -> log : append(tx1, Prepared)

coord -> saga : commit(tx1)
saga -> grpc : commit(tx1)
grpc -> go_svc : gRPC commit(tx1)
go_svc --> grpc : success
saga -> http : commit(tx1)
http -> java_svc : HTTP commit(tx1)
java_svc --> http : success
saga -> log : append(tx1, Committed)

note over recovery : 协调器崩溃重启
recovery -> log : read_pending() [复用既有]
log --> recovery : [tx1 Prepared]
recovery -> grpc : query_status(tx1)
grpc -> go_svc : gRPC status(tx1)
go_svc --> grpc : Committed
recovery -> http : query_status(tx1)
http -> java_svc : HTTP status(tx1)
java_svc --> http : Prepared
recovery -> recovery : 决策: A/B 已 Committed → 全局提交
recovery -> http : commit(tx1) [通知 Java C]
@enduml
```

### 5.1.3 异常场景

1. **跨语言参与者调用超时**
   a. 触发条件：跨语言参与者 prepare/commit/rollback 调用超时（超过 `timeout_ms`，默认 5000ms）
   b. 系统行为：返回 `CrossLangTxError::Timeout`（`packages/sz-orm-dtx/src/cross_lang/mod.rs:89`），事务标记失败，触发回滚/补偿
   c. 用户感知：错误"participant call timeout"
2. **跨语言参与者鉴权失败**
   a. 触发条件：mTLS 证书无效或 Token 不匹配
   b. 系统行为：返回 `CrossLangTxError::AuthFailed`（`:91`），拒绝参与者调用
   c. 用户感知：错误"authentication failed"
3. **协议版本不匹配**
   a. 触发条件：参与者协议版本与协调器不一致
   b. 系统行为：`check_protocol_version`（`packages/sz-orm-dtx/src/cross_lang/protocol.rs:192`）返回 `CrossLangTxError::ProtocolVersionMismatch`（`packages/sz-orm-dtx/src/cross_lang/mod.rs:93`）
   c. 用户感知：错误"protocol version mismatch: coordinator=X, participant=Y"
4. **跨语言参与者补偿失败**
   a. 触发条件：Saga/TCC 补偿（rollback/cancel）时跨语言参与者补偿失败
   b. 系统行为：返回 `CrossLangTxError::CompensationFailed`（`:99`），记录补偿失败日志，标记事务需人工干预
   c. 用户感知：错误"compensation failed for participant: X"，需人工介入
5. **崩溃恢复冲突**
   a. 触发条件：崩溃恢复时参与者状态冲突（部分已提交部分已回滚）
   b. 系统行为：返回 `CrossLangTxError::RecoveryConflict`（`:95`），记录冲突日志，标记事务需人工干预
   c. 用户感知：错误"recovery conflict detected"，需人工介入

## 5.2 低代码双向同步（REQ-V48-002，P1）

### 5.2.1 业务规则

1. **双向同步引擎**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-lc` 包，提供 `BidirectionalSyncEngine` 双向同步引擎，支持 ORM 模型 ↔ 低代码引擎模型双向同步（`SyncDirection` 枚举：OrmToLc ORM→低代码 / LcToOrm 低代码→ORM / Bidirectional 双向），复用既有 `ModelDefinition`（`packages/sz-orm-lc/src/lib.rs:24`）/ `FieldDef`（`:83`）/ `RelationDefinition`（`:147`）/ `FieldTypeMapping`（`:210`）/ `ValidationRule`（`:362`）+ `sz-orm-designer` `SchemaDesigner`/`code_gen`/`code_parse`（`packages/sz-orm-designer/src/lib.rs:1`）。
   a. 验收条件：[ORM 模型 User{name,id,email} + OrmToLc 同步] → [低代码引擎模型同步为 User{name,id,email}，字段/类型/约束一致]；[低代码引擎模型 Order{order_id,total} + LcToOrm 同步] → [ORM 模型同步为 Order{order_id,total}]
2. **同步冲突检测**（EARS: Ubiquitous）
   系统应当提供 `SyncConflictDetector` 冲突检测器，检测双向同步冲突（同字段双向变更 / 类型不一致 / 约束不一致 / 关联不一致），生成冲突列表。
   a. 验收条件：[ORM 将 User.email 改为 VARCHAR(500) + 低代码引擎将 User.email 改为 TEXT + 双向同步] → [检测到 User.email 类型冲突，生成冲突列表]
3. **同步冲突解决**（EARS: Ubiquitous）
   系统应当提供 `SyncConflictResolver` 冲突解决器与 `ConflictResolutionStrategy` 枚举（OrmWins ORM 优先 / LcWins 低代码优先 / Merge 合并 / Manual 人工确认，默认 Manual），按策略解决冲突，解决后记录审计日志。
   a. 验收条件：[User.email 类型冲突 + OrmWins] → [采用 ORM 的 VARCHAR(500)]；[User.email 类型冲突 + LcWins] → [采用低代码的 TEXT]；[User.email 类型冲突 + Manual] → [暂停同步，等待人工确认]
4. **增量同步追踪**（EARS: Ubiquitous）
   系统应当提供 `SyncIncrementTracker` 增量追踪器，追踪模型变更（字段增删改 / 类型变更 / 约束变更 / 关联变更），同步时只处理变更项而非全量重建，提升同步效率。
   a. 验收条件：[ORM 新增字段 User.age + 增量同步] → [只同步 User.age 新增，不重建整个 User 模型]
5. **类型映射双向一致**（EARS: Ubiquitous）
   系统应当保证双向同步类型映射一致，复用既有 `FieldTypeMapping`（`packages/sz-orm-lc/src/lib.rs:210`）四向映射（SQL↔Rust↔HTML↔JSON Schema），ORM Rust 类型 → 低代码 SQL 类型 → 低代码 HTML input 类型须一致。
   a. 验收条件：[ORM 字段 i64 + OrmToLc 同步] → [低代码字段 BIGINT + HTML input number，`FieldTypeMapping::rust_to_sql("i64")` = "BIGINT"，`sql_to_html_input("BIGINT")` = "number"]
6. **验证规则同步**（EARS: Ubiquitous）
   系统应当同步验证规则（复用既有 `ValidationRule` `packages/sz-orm-lc/src/lib.rs:362`），ORM 约束（非空/唯一/长度）映射为低代码验证规则（Required/Unique/MaxLength），双向同步保持一致。
   a. 验收条件：[ORM 字段 email 非空唯一 + OrmToLc 同步] → [低代码验证规则 Required + Unique]
7. **同步审计日志**（EARS: Ubiquitous）
   系统应当提供 `SyncAuditLogger` 同步审计日志器，记录同步操作（同步方向 + 变更项 + 冲突 + 解决策略 + 时间 + 操作人），追加写入不可篡改。
   a. 验收条件：[双向同步 + 冲突 User.email + OrmWins 解决] → [审计日志记录 direction=Bidirectional + conflict=User.email + strategy=OrmWins + timestamp]
8. **复用既有低代码模型与设计器**（EARS: Ubiquitous）
   系统应当复用既有 `ModelDefinition`/`FieldDef`/`RelationDefinition`/`FieldTypeMapping`/`ValidationRule`/`FormField`/`FormGenerator`/`CrudTemplateEngine`（`packages/sz-orm-lc/src/lib.rs`）+ `sz-orm-designer` `SchemaDesigner`/`ErDiagramEditor`/`code_gen`/`code_parse`/`design_ir`（`packages/sz-orm-designer/src/lib.rs:1`），双向同步引擎基于既有模型定义/设计器扩展，不重复实现。
   a. 验收条件：[双向同步] → [复用既有 ModelDefinition/FieldTypeMapping/SchemaDesigner/code_gen，不新建模型定义/设计器逻辑]
9. **禁止项**（EARS: Unwanted）
   如果低代码双向同步影响默认编译或破坏既有低代码模型/设计器，则系统应当通过 `lc-bidirectional-sync` feature gate 隔离，默认不启用，且既有 `ModelDefinition`/`FieldTypeMapping`/`SchemaDesigner` 保留不动。
   a. 验收条件：[`cargo build` 默认编译] → [无双向同步，行为与 v4.7.0 一致]
10. **禁止自动破坏性 schema 变更**（EARS: Unwanted）
    如果双向同步检测到破坏性变更（删列/改列类型/改列约束），则系统应当不自动执行，须人工确认（沿用 v4.7.0 异常自愈人工确认铁律），避免误删数据。
    a. 验收条件：[低代码引擎删除字段 User.age + LcToOrm 同步 + 默认 Manual] → [暂停同步，提示"破坏性变更：删除字段 age，需人工确认"]

### 5.2.2 交互流程

```plantuml
@startuml
title 低代码双向同步 交互流程
actor "ORM 开发者" as orm_dev
actor "低代码开发者" as lc_dev
participant "BidirectionalSyncEngine\n(双向同步引擎)" as sync
participant "SyncIncrementTracker\n(增量追踪)" as tracker
participant "SyncConflictDetector\n(冲突检测)" as detector
participant "SyncConflictResolver\n(冲突解决)" as resolver
participant "SyncAuditLogger\n(审计日志)" as audit
participant "ORM 模型" as orm_model
participant "低代码引擎模型" as lc_model

orm_dev -> orm_model : 修改 User.email 为 VARCHAR(500)
lc_dev -> lc_model : 修改 User.email 为 TEXT

orm_dev -> sync : sync(OrmToLc, User)
sync -> tracker : get_changes(User)
tracker --> sync : [email: VARCHAR(255)→VARCHAR(500)]
sync -> lc_model : 读取低代码 User.email
sync -> detector : detect_conflict(ORM VARCHAR(500), LC TEXT)
detector --> sync : Conflict(email, type mismatch)

alt strategy = OrmWins
  sync -> resolver : resolve(OrmWins, conflict)
  resolver -> lc_model : 更新 email 为 VARCHAR(500)
else strategy = LcWins
  sync -> resolver : resolve(LcWins, conflict)
  resolver -> orm_model : 更新 email 为 TEXT
else strategy = Manual
  sync -> orm_dev : 提示冲突，等待人工确认
end

sync -> audit : log(direction=OrmToLc, change=email, conflict=type, strategy, timestamp)
sync --> orm_dev : 同步结果(冲突数 + 解决策略)
@enduml
```

### 5.2.3 异常场景

1. **同步冲突未解决**
   a. 触发条件：双向同步检测到冲突且策略为 Manual，人工未确认
   b. 系统行为：暂停同步，记录冲突日志，等待人工确认
   c. 用户感知：提示"同步冲突：User.email 类型不一致（ORM: VARCHAR(500), LC: TEXT），请确认"
2. **破坏性变更检测**
   a. 触发条件：同步检测到破坏性变更（删列/改列类型/改列约束）
   b. 系统行为：不自动执行，标记为破坏性变更，须人工确认
   c. 用户感知：提示"破坏性变更：删除字段 age，需人工确认"
3. **类型映射不支持**
   a. 触发条件：同步遇到 `FieldTypeMapping` 不支持的类型
   b. 系统行为：跳过该字段，记录日志"type mapping not supported, skipped"
   c. 用户感知：日志标注"field X skipped, type Y not supported"
4. **低代码引擎不可达**
   a. 触发条件：低代码引擎服务不可达（网络故障）
   b. 系统行为：中止同步，记录错误日志，保留 ORM 模型不变
   c. 用户感知：错误"low-code engine unreachable, sync aborted"

## 5.3 OpenAPI 反向生成（REQ-V48-003，P1）

### 5.3.1 业务规则

1. **DB schema 读取**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-swagger` `openapi-reverse` feature，提供 `DbSchemaReader` 五方言 schema 读取器，从数据库实际 schema 读取表/列/约束/索引元信息，查询五方言 information_schema（MySQL information_schema / PostgreSQL pg_catalog / SQLite sqlite_master / Oracle ALL_TAB_COLUMNS / MSSQL INFORMATION_SCHEMA），生成 DB schema 描述。
   a. 验收条件：[MySQL + users 表(id BIGINT PK, email VARCHAR(255) UNIQUE, created_at TIMESTAMP)] → [DbSchemaReader 读取 users 表，含列 id/email/created_at + 约束 PK/UNIQUE]；[PostgreSQL + orders 表] → [通过 pg_catalog 读取]；[SQLite + products 表] → [通过 sqlite_master 读取]
2. **DB schema → OpenAPI 规范**（EARS: Ubiquitous）
   系统应当提供 `DbSchemaToOpenApiMapper` DB schema → OpenAPI 3.0 规范映射器，将 DB schema 描述映射为 OpenAPI spec（表 → components.schemas / 列 → 字段 / 主键 → required / 唯一约束 → uniqueItems / 外键 → 关联 / 表 → paths CRUD 端点），生成完整 OpenAPI spec（paths + components.schemas + parameters）。
   a. 验收条件：[users 表(id BIGINT PK, email VARCHAR(255) UNIQUE)] → [OpenAPI spec 含 components.schemas.User{id: integer, email: string, required: [id], uniqueItems: email} + paths /users CRUD 端点]
3. **DB schema → CRUD API**（EARS: Ubiquitous）
   系统应当提供 `DbSchemaToCrudApiMapper` DB schema → CRUD API 映射器，为每张表生成标准 CRUD REST 端点（GET /resource 列表 / GET /resource/{id} 详情 / POST /resource 创建 / PUT /resource/{id} 更新 / DELETE /resource/{id} 删除）+ OpenAPI 文档，`CrudApiEndpoint` 描述端点定义（method + path + parameters + request_body + responses）。
   a. 验收条件：[users 表 + 生成 CRUD API] → [生成 5 个端点：GET /users / GET /users/{id} / POST /users / PUT /users/{id} / DELETE /users/{id}，每个端点含 OpenAPI 文档]
4. **完整闭环验证**（EARS: Ubiquitous）
   系统应当提供 `FullReverseLoopVerifier` 完整闭环验证器，验证 DB schema → OpenAPI → ORM Model → CRUD 闭环一致性（生成的 OpenAPI spec 经既有 `OpenApiReverseGenerator` `packages/sz-orm-swagger/src/reverse/mod.rs:26` 反向生成 ORM Model，再生成 CRUD，与直接从 DB schema 生成的 CRUD 一致），复用既有 `ApiFirstLoopVerifier`（`:28`）。
   a. 验收条件：[DB users 表 → OpenAPI spec → ORM Model → CRUD API] → [闭环验证通过，直接 DB→CRUD 与 DB→OpenAPI→ORM→CRUD 结果一致]
5. **五方言 schema 读取覆盖**（EARS: Ubiquitous）
   系统应当覆盖五方言 schema 读取（MySQL/PostgreSQL/SQLite/Oracle/MSSQL），各方言 information_schema 查询参数化绑定（禁止 SQL 拼接），不支持的方言特性降级处理。
   a. 验收条件：[MySQL/PostgreSQL/SQLite/Oracle/MSSQL 各读取 users 表] → [五方言均能读取表/列/约束/索引]；[方言不支持某约束类型] → [降级跳过，标注"constraint type X not supported on dialect Y"]
6. **命名约定可配置**（EARS: Ubiquitous）
   系统应当支持命名约定可配置（复用既有 `NamingConvention` `packages/sz-orm-swagger/src/reverse/mod.rs:25` + `to_pascal_case`/`to_snake_case`），表名 → 资源名 / 列名 → 字段名按命名约定转换，默认 snake_case 表名 → PascalCase 资源名。
   a. 验收条件：[表名 user_orders + 默认命名约定] → [资源名 UserOrders / OpenAPI schema 名 UserOrders]
7. **注入防护**（EARS: Ubiquitous）
   系应当复用既有 `OpenApiInjectionGuard`（`packages/sz-orm-swagger/src/reverse/mod.rs:27`），DB schema 读取与 OpenAPI 生成须注入防护，拒绝含注入的 schema/ spec（`ReverseGenError::InjectionDetected` `:36`），未签名 spec 须拒绝（`UnsignedSpec`）。
   a. 验收条件：[DB schema 含注入字符 + 反向生成] → [拒绝生成，返回 InjectionDetected]
8. **复用既有反向生成**（EARS: Ubiquitous）
   系统应当复用既有 `OpenApiReverseGenerator`（`packages/sz-orm-swagger/src/reverse/mod.rs:26`）/ `SchemaToModelMapper`（`:30`）/ `OpenApiToMigrationMapper`（`:29`）/ `OpenApiToRepositoryMapper`（`:31`）/ `ApiFirstLoopVerifier`（`:28`）/ `OpenApiInjectionGuard`（`:27`）/ `ReverseGenConfig`（`:25`）（OpenAPI→ORM 方向），DB schema → OpenAPI 方向基于既有反向生成扩展，形成完整闭环，不重复实现。
   a. 验收条件：[DB schema → OpenAPI → ORM] → [复用既有 OpenApiReverseGenerator/SchemaToModelMapper，不新建 OpenAPI→ORM 逻辑]
9. **反向生成日志可追溯**（EARS: Ubiquitous）
   系统应当记录反向生成日志（schema 来源 + 表/列数 + 生成项 + 闭环验证结果 + 耗时），供审计追溯。
   a. 验收条件：[DB 100 表反向生成] → [日志记录 source=db + tables=100 + generated=openapi+crud + loop=pass + latency]
10. **禁止项**（EARS: Unwanted）
    如果 OpenAPI 反向生成影响默认编译或破坏既有反向生成，则系统应当通过 `openapi-reverse` feature gate 隔离，默认不启用，且既有 `OpenApiReverseGenerator`/`SchemaToModelMapper` 保留不动。
    a. 验收条件：[`cargo build` 默认编译] → [无 DB schema 反向生成，行为与 v4.7.0 一致]

### 5.3.2 交互流程

```plantuml
@startuml
title OpenAPI 反向生成 交互流程
actor "API 设计者" as designer
participant "DbSchemaReader\n(schema 读取器)" as reader
participant "DbSchemaToOpenApiMapper\n(DB→OpenAPI 映射)" as mapper
participant "DbSchemaToCrudApiMapper\n(DB→CRUD 映射)" as crud_mapper
participant "OpenApiReverseGenerator\n(既有 OpenAPI→ORM)" as reverse
participant "FullReverseLoopVerifier\n(闭环验证)" as verifier
participant "OpenApiInjectionGuard\n(注入防护)" as guard
database "DB" as db

designer -> reader : read_schema(mysql://..., tables)
reader -> db : SELECT * FROM information_schema.columns [参数化]
db --> reader : 表/列/约束/索引元信息
reader --> designer : DbSchema(tables, columns, constraints)

designer -> guard : check_injection(DbSchema)
guard --> designer : OK

designer -> mapper : map_to_openapi(DbSchema)
mapper -> mapper : 表→schemas / 列→字段 / 约束→required/uniqueItems
mapper --> designer : OpenApiSpec(paths + components.schemas)

designer -> crud_mapper : map_to_crud_api(DbSchema)
crud_mapper -> crud_mapper : 每表生成 5 个 CRUD 端点
crud_mapper --> designer : CrudApi(endpoints + OpenAPI docs)

designer -> verifier : verify_loop(DbSchema → OpenAPI → ORM → CRUD)
verifier -> reverse : generate_orm(OpenApiSpec) [复用既有]
reverse --> verifier : ORM Model
verifier -> verifier : 比对 DB→CRUD vs DB→OpenAPI→ORM→CRUD
verifier --> designer : LoopReport(一致/差异)
@enduml
```

### 5.3.3 异常场景

1. **schema 读取失败**
   a. 触发条件：数据库连接失败或 information_schema 查询失败
   b. 系统行为：中止反向生成，返回错误"schema read failed"
   c. 用户感知：错误"schema read failed: ..."
2. **不支持的方言特性**
   a. 触发条件：方言不支持某 schema 特性（如 SQLite 不支持某些约束类型）
   b. 系统行为：降级跳过该特性，记录日志"dialect feature X not supported, skipped"
   c. 用户感知：日志标注"feature X skipped on dialect Y"
3. **闭环验证差异**
   a. 触发条件：DB→OpenAPI→ORM→CRUD 闭环与 DB→CRUD 直接生成存在差异
   b. 系统行为：返回 `ReverseGenError::LoopVerificationDiff`（`packages/sz-orm-swagger/src/reverse/mod.rs:46`），附差异详情
   c. 用户感知：错误"loop verification diff: ..."
4. **注入检测**
   a. 触发条件：DB schema 或生成的 OpenAPI spec 含注入字符
   b. 系统行为：`OpenApiInjectionGuard` 拒绝，返回 `ReverseGenError::InjectionDetected`（`:50`）
   c. 用户感知：错误"injection detected in spec, refusing to execute"
5. **用户手写逻辑保护**
   a. 触发条件：反向生成覆盖用户手写的 ORM 逻辑
   b. 系统行为：只更新骨架，保留用户可编辑区域，返回 `ReverseGenError::UserLogicOverwrite`（`:58`）提示
   c. 用户感知：提示"user logic in editable region preserved, only skeleton updated"

## 5.4 WASM 真实数据库连接闭环（REQ-V48-004，P1）

### 5.4.1 业务规则

1. **代理后端服务**（EARS: Ubiquitous）
   系统应当扩展既有 `sz-orm-wasm` `wasm-real-db` feature，提供 `WasmProxyServer` 代理后端服务，接收 WASM 端代理请求（`ProxyRequest` `packages/sz-orm-wasm/src/real_db/mod.rs:23`）→ 鉴权（`WasmDbAuthValidator` `:18`）+ SQL 白名单检查（`WasmDbSqlWhitelist` `:28`）+ 限流（`WasmDbRateLimiter` `:26`）→ 连接真实 DB（复用 sz-orm-core 连接池 `Pool` `packages/sz-orm-core/src/pool.rs:743`）→ 执行查询 → 返回结果（`ProxyResponse`），后端凭据不暴露给 WASM 端（`WasmRealDbError::CredentialsNotExposed` `packages/sz-orm-wasm/src/real_db/mod.rs:52`）。
   a. 验收条件：[WASM 端发送 ProxyRequest(SELECT * FROM users WHERE id=$1) + 代理后端] → [鉴权通过 + 白名单通过 + 限流通过 + 连接 DB 执行 + 返回 ProxyResponse(结果)]；[WASM 端尝试读取后端凭据] → [返回 CredentialsNotExposed]
2. **多方言代理后端**（EARS: Ubiquitous）
   系统应当提供 `MultiDialectProxyBackend` 多方言代理后端，支持 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言，复用 sz-orm-core 连接池 + sz-orm-sqlx 驱动，按方言路由到对应数据库。
   a. 验收条件：[代理后端配置 MySQL + PostgreSQL + WASM 端查询 MySQL users] → [路由到 MySQL 执行]；[WASM 端查询 PostgreSQL orders] → [路由到 PostgreSQL 执行]
3. **WASM 端 ORM 会话**（EARS: Ubiquitous）
   系统应当提供 `WasmOrmSession` WASM 端 ORM 会话，在 WASM 端提供 ORM 操作（查询构建 + 代理执行 + 结果反序列化），复用既有 `WasmRealDbConnection`（`packages/sz-orm-wasm/src/real_db/mod.rs:19`）/ `WasmRealDbQueryExecutor`（`:21`）/ `WasmDbProxy`（`:25`）/ `WasmDbProxyProtocol`（`:23`）。
   a. 验收条件：[WASM 端 WasmOrmSession + 查询 User where id=1] → [构建参数化查询 → 代理执行 → 反序列化为 User 结构]
4. **WASM 端查询构建器桥接**（EARS: Ubiquitous）
   系统应当提供 `WasmQueryBuilderBridge` WASM 端查询构建器桥接，复用 sz-orm-query-builder 构建参数化查询（`where_eq`/`or_where_eq` 等），将查询构建器输出转换为代理协议 `ProxyRequest`，禁止 SQL 字符串拼接。
   a. 验收条件：[WASM 端 QueryBuilder.select("users").where_eq("id", 1) + 桥接] → [生成参数化 ProxyRequest(SQL="SELECT * FROM users WHERE id=$1", params=[1])]
5. **WASM ORM 闭环验证**（EARS: Ubiquitous）
   系统应当提供 `WasmOrmLoopVerifier` WASM ORM 闭环验证器，验证 WASM 端 ORM 操作闭环（查询构建 → 代理 → 后端 DB → 结果反序列化 → 与直接 DB 查询结果一致）。
   a. 验收条件：[WASM 端查询 users where id=1 + 代理后端 MySQL] → [闭环验证通过，WASM 端结果与直接 MySQL 查询结果一致]
6. **WASI socket 直连代理**（EARS: Optional）
   当部署在 WASI 环境（非浏览器）时，系统可通过 `WasiSocketConnection`（`packages/sz-orm-wasm/src/real_db/wasi_socket.rs:13`，feature = "wasi-socket"）直连代理后端，调用 WASI socket API（`__wasi_sock_open` + `__wasi_sock_connect`），浏览器环境仍通过 HTTP/WebSocket 代理。
   a. 验收条件：[WASI 环境 + WasiSocketConnection + 连接代理] → [通过 WASI socket 直连代理后端]；[浏览器环境] → [通过 HTTP/WebSocket 代理]
7. **代理鉴权与白名单与限流**（EARS: Ubiquitous）
   系统应当复用既有 `WasmDbAuthValidator`（`packages/sz-orm-wasm/src/real_db/mod.rs:18`）/ `WasmDbSqlWhitelist`（`:28`）/ `WasmDbRateLimiter`（`:26`），代理后端强制鉴权 + SQL 白名单 + 限流，拒绝未鉴权/非白名单/超限请求（`WasmRealDbError::AuthFailed`/`SqlRejected`/`RateLimited`）。
   a. 验收条件：[未鉴权请求] → [返回 AuthFailed]；[非白名单 SQL DROP TABLE] → [返回 SqlRejected]；[超限请求] → [返回 RateLimited]
8. **代理重连与指标**（EARS: Ubiquitous）
   系统应当复用既有 `WasmRealDbReconnector`（`packages/sz-orm-wasm/src/real_db/mod.rs:27`）/ `WasmRealDbMetrics`（`:22`），代理连接断开自动重连，记录指标（QPS + 延迟 + 错误率 + 白名单拒绝数 + 限流数）。
   a. 验收条件：[代理连接断开] → [自动重连]；[代理查询] → [指标记录 QPS + latency + error_rate + rejected + rate_limited]
9. **结果集大小限制**（EARS: Ubiquitous）
   系统应当限制返回 WASM 端的结果集大小（可配，默认 10MB），超限返回 `WasmRealDbError::ResultTooLarge`（`packages/sz-orm-wasm/src/real_db/mod.rs:60`），避免 WASM 内存溢出。
   a. 验收条件：[查询返回 100MB 结果 + 默认限制 10MB] → [返回 ResultTooLarge]
10. **复用既有代理桥接**（EARS: Ubiquitous）
    系统应当复用既有 `WasmRealDbConnection`/`WasmRealDbQueryExecutor`/`WasmDbProxy`/`WasmDbProxyProtocol`/`WasmDbAuthValidator`/`WasmDbSqlWhitelist`/`WasmDbRateLimiter`/`WasmRealDbReconnector`/`WasmRealDbMetrics`/`WasiSocketConnection`（`packages/sz-orm-wasm/src/real_db/mod.rs`）+ sz-orm-core 连接池 + sz-orm-query-builder，代理后端 + ORM 闭环基于既有代理桥接扩展，不重复实现。
    a. 验收条件：[WASM ORM 闭环] → [复用既有 WasmDbProxy/WasmRealDbConnection/Pool/QueryBuilder，不新建代理桥接逻辑]
11. **禁止项**（EARS: Unwanted）
    如果 WASM 真实连接闭环影响默认编译或破坏既有代理桥接，则系统应当通过 `wasm-real-db` feature gate 隔离，默认不启用，且既有 `WasmRealDbConnection`/`WasmDbProxy`/`WasmDbProxyProtocol` 保留不动。
    a. 验收条件：[`cargo build` 默认编译] → [无 WASM ORM 闭环，行为与 v4.7.0 一致]
12. **禁止 WASM 端直连数据库**（EARS: Unwanted）
    如果部署在浏览器 WASM 环境，则系统应当不直连数据库（浏览器无 socket API），须通过代理后端桥接，后端凭据不暴露给 WASM 端。
    a. 验收条件：[浏览器 WASM 环境 + 尝试直连 DB] → [拒绝，须通过代理后端]

### 5.4.2 交互流程

```plantuml
@startuml
title WASM 真实数据库连接闭环 交互流程
actor "WASM 前端开发者" as wasm_dev
participant "WasmOrmSession\n(WASM ORM 会话)" as session
participant "WasmQueryBuilderBridge\n(查询构建桥接)" as qb_bridge
participant "WasmDbProxy\n(代理客户端)" as proxy
participant "WasmProxyServer\n(代理后端)" as server
participant "WasmDbAuthValidator\n(鉴权)" as auth
participant "WasmDbSqlWhitelist\n(白名单)" as whitelist
participant "WasmDbRateLimiter\n(限流)" as limiter
participant "Pool\n(sz-orm-core 连接池)" as pool
database "DB" as db

wasm_dev -> session : query(User).where_eq("id", 1)
session -> qb_bridge : build_query(select users where id=$1)
qb_bridge -> qb_bridge : 参数化查询构建 [复用 sz-orm-query-builder]
qb_bridge --> session : ProxyRequest(SQL, params=[1])

session -> proxy : send(ProxyRequest)
proxy -> server : HTTP/WebSocket(ProxyRequest)

server -> auth : validate(token)
auth --> server : OK
server -> whitelist : check(SQL)
whitelist --> server : OK
server -> limiter : acquire()
limiter --> server : OK

server -> pool : acquire_connection(mysql)
pool -> db : 连接
db --> pool : connection
server -> db : EXECUTE SQL [参数化绑定]
db --> server : 结果集
server -> server : 检查结果集大小 ≤ 10MB
server --> proxy : ProxyResponse(结果)
proxy --> session : ProxyResponse
session -> session : 反序列化为 User
session --> wasm_dev : User{id=1, ...}
@enduml
```

### 5.4.3 异常场景

1. **代理后端不可用**
   a. 触发条件：代理后端服务不可达
   b. 系统行为：返回 `WasmRealDbError::ProxyUnavailable`（`packages/sz-orm-wasm/src/real_db/mod.rs:37`），`WasmRealDbReconnector` 自动重连
   c. 用户感知：错误"proxy unavailable"，自动重连中
2. **鉴权失败**
   a. 触发条件：WASM 端未鉴权或 token 无效
   b. 系统行为：返回 `WasmRealDbError::AuthFailed`（`:48`），拒绝请求
   c. 用户感知：错误"authentication failed"
3. **SQL 白名单拒绝**
   a. 触发条件：WASM 端查询 SQL 不在白名单
   b. 系统行为：返回 `WasmRealDbError::SqlRejected`（`:41`），拒绝执行
   c. 用户感知：错误"SQL rejected: ..."
4. **限流**
   a. 触发条件：WASM 端请求超过限流阈值
   b. 系统行为：返回 `WasmRealDbError::RateLimited`（`:44`），拒绝请求
   c. 用户感知：错误"rate limited"
5. **结果集过大**
   a. 触发条件：查询返回结果集超过大小限制（默认 10MB）
   b. 系统行为：返回 `WasmRealDbError::ResultTooLarge`（`:60`），不返回结果
   c. 用户感知：错误"result too large"
6. **查询失败**
   a. 触发条件：DB 查询执行失败（SQL 错误/连接失败）
   b. 系统行为：返回 `WasmRealDbError::QueryFailed`（`:56`），附错误原因
   c. 用户感知：错误"query failed: ..."
7. **序列化错误**
   a. 触发条件：代理协议序列化/反序列化失败
   b. 系统行为：返回 `WasmRealDbError::SerializationError`（`:64`）
   c. 用户感知：错误"serialization error"

---

# 6. 数据约束

## 6.1 跨语言参与者描述（CrossLangParticipantDesc）

1. **resource_id**：资源标识符，非空字符串，全局唯一（同一协调器内不重复）
2. **language**：参与者编程语言，枚举值 Go/Java/Cpp/Python/JavaScript（`ParticipantLanguage` `packages/sz-orm-dtx/src/cross_lang/mod.rs:16`）
3. **transport**：传输协议，枚举值 Grpc/Http（`ParticipantTransport` `:38`）
4. **endpoint**：服务端点地址，非空字符串（如 `grpc://host:port` 或 `http://host:port/path`）
5. **auth**：鉴权凭据，Mtls{cert,key,ca} 或 Token(String)（`ParticipantAuth` `:45`）
6. **protocol_version**：协议版本号，u32，须与协调器 `COORDINATOR_PROTOCOL_VERSION`（`:128`）匹配

## 6.2 同步变更项（SyncChange）

1. **direction**：同步方向，枚举值 OrmToLc/LcToOrm/Bidirectional
2. **model_name**：模型名，非空字符串
3. **field_name**：字段名，非空字符串
4. **change_type**：变更类型，枚举值 FieldAdded/FieldRemoved/TypeChanged/ConstraintChanged/RelationChanged
5. **old_value**：旧值（可选，类型变更/约束变更时填充）
6. **new_value**：新值（可选，类型变更/约束变更时填充）
7. **timestamp**：变更时间，Unix 毫秒

## 6.3 DB schema 描述（DbSchema）

1. **dialect**：数据库方言，枚举值 MySql/PostgreSql/Sqlite/Oracle/Mssql
2. **tables**：表列表，每表含 name/columns/constraints/indexes
3. **columns**：列列表，每列含 name/data_type/nullable/default/primary_key/unique
4. **constraints**：约束列表，每约束含 type(PK/UK/FK/CHECK)/columns/references
5. **indexes**：索引列表，每索引含 name/columns/unique

## 6.4 代理请求（ProxyRequest）

1. **sql**：SQL 语句，非空字符串，须参数化（禁止拼接），须在白名单内
2. **params**：参数列表，按位置绑定（$1/$2/...）
3. **dialect**：目标数据库方言，枚举值 MySql/PostgreSql/Sqlite/Oracle/Mssql
4. **auth_token**：鉴权 token，非空字符串
5. **result_format**：结果序列化格式（`SerializationFormat` `packages/sz-orm-wasm/src/real_db/mod.rs:23`）

---

# 7. 需求追溯矩阵

| 需求 ID | 优先级 | 需求名称 | 验收条件数 | feature gate | 复用既有代码 |
|---------|--------|---------|-----------|-------------|-------------|
| REQ-V48-001 | P1 | 跨语言分布式事务协调 | 10 | `cross-lang-dtx`（扩展） | `CrossLangParticipantProtocol` `packages/sz-orm-dtx/src/cross_lang/mod.rs:108` / `GrpcParticipantProtocol` `packages/sz-orm-dtx/src/cross_lang/protocol.rs:26` / `HttpParticipantProtocol` `:78` / `CrossLangParticipant::to_participant()` `packages/sz-orm-dtx/src/cross_lang/participant.rs:51` / `TransactionLogStore` `packages/sz-orm-dtx/src/lib.rs:57` / `recovery` `packages/sz-orm-dtx/src/recovery.rs` / `saga` `packages/sz-orm-dtx/src/saga.rs` / `tcc` `packages/sz-orm-dtx/src/tcc.rs` + sz-orm-grpc `real_grpc.rs` |
| REQ-V48-002 | P1 | 低代码双向同步 | 10 | `lc-bidirectional-sync`（新增） | `ModelDefinition` `packages/sz-orm-lc/src/lib.rs:24` / `FieldDef` `:83` / `RelationDefinition` `:147` / `FieldTypeMapping` `:210` / `ValidationRule` `:362` + sz-orm-designer `SchemaDesigner`/`code_gen`/`code_parse`/`design_ir` `packages/sz-orm-designer/src/lib.rs:1` |
| REQ-V48-003 | P1 | OpenAPI 反向生成 | 10 | `openapi-reverse`（扩展） | `OpenApiReverseGenerator` `packages/sz-orm-swagger/src/reverse/mod.rs:26` / `SchemaToModelMapper` `:30` / `OpenApiToMigrationMapper` `:29` / `OpenApiToRepositoryMapper` `:31` / `ApiFirstLoopVerifier` `:28` / `OpenApiInjectionGuard` `:27` / `ReverseGenConfig` `:25` |
| REQ-V48-004 | P1 | WASM 真实数据库连接闭环 | 12 | `wasm-real-db`（扩展） | `WasmRealDbConnection` `packages/sz-orm-wasm/src/real_db/mod.rs:19` / `WasmRealDbQueryExecutor` `:21` / `WasmDbProxy` `:25` / `WasmDbProxyProtocol` `:23` / `WasmDbAuthValidator` `:18` / `WasmDbSqlWhitelist` `:28` / `WasmDbRateLimiter` `:26` / `WasmRealDbReconnector` `:27` / `WasmRealDbMetrics` `:22` / `WasiSocketConnection` `packages/sz-orm-wasm/src/real_db/wasi_socket.rs:13` + sz-orm-core `Pool` `packages/sz-orm-core/src/pool.rs:743` + sz-orm-query-builder |

---

# 8. 验收标准总览

## 8.1 REQ-V48-001 跨语言分布式事务协调（P1）

1. `TonicGrpcCallHandler` 真实 tonic gRPC 传输（实现 `RemoteCallHandler`，mTLS/Token 鉴权，复用 sz-orm-grpc `real_grpc`）
2. `ReqwestHttpCallHandler` 真实 HTTP/JSON 传输（实现 `RemoteCallHandler`，Token 鉴权 + 超时）
3. `CrossLangRecoveryCoordinator` 跨语言崩溃恢复（查询未完成事务 + 询问参与者状态 + 决定提交/回滚，复用 `TransactionLogStore`/`recovery`）
4. `CrossLangSagaCoordinator` 跨语言 Saga 编排（跨语言补偿，反向补偿顺序，复用 `saga` + `CrossLangParticipant::to_participant()`）
5. `CrossLangTccCoordinator` 跨语言 TCC 编排（Try-Confirm-Cancel 跨语言，复用 `tcc`）
6. 跨语言事务幂等性（`CrossLangCompensationSerializer` 幂等键，重复调用结果一致）
7. 跨语言事务可观测性（分布式追踪，复用 `cross_lang::observability`）
8. 复用既有协议层与事务核心，不重复实现
9. 各语言 SDK 接入契约（Go/Java/C++/Python/JS，复用 sz-orm-go/java/cpp/python/js）
10. `cross-lang-dtx` feature gate 隔离，默认关闭，既有协议层/事务核心/gRPC 保留

## 8.2 REQ-V48-002 低代码双向同步（P1）

1. `BidirectionalSyncEngine` 双向同步引擎（OrmToLc/LcToOrm/Bidirectional，复用 `ModelDefinition`/`FieldTypeMapping`/`SchemaDesigner`）
2. `SyncConflictDetector` 冲突检测（同字段双向变更/类型不一致/约束不一致）
3. `SyncConflictResolver` + `ConflictResolutionStrategy` 冲突解决（OrmWins/LcWins/Merge/Manual，默认 Manual）
4. `SyncIncrementTracker` 增量追踪（只同步变更项）
5. 类型映射双向一致（复用 `FieldTypeMapping` 四向映射）
6. 验证规则同步（ORM 约束 → 低代码验证规则，复用 `ValidationRule`）
7. `SyncAuditLogger` 同步审计日志（追加写入不可篡改）
8. 复用既有低代码模型与设计器，不重复实现
9. `lc-bidirectional-sync` feature gate 隔离，默认关闭，既有模型/设计器保留
10. 禁止自动破坏性 schema 变更（删列/改类型/改约束须人工确认）

## 8.3 REQ-V48-003 OpenAPI 反向生成（P1）

1. `DbSchemaReader` 五方言 schema 读取（MySQL/PG/SQLite/Oracle/MSSQL information_schema，参数化查询）
2. `DbSchemaToOpenApiMapper` DB schema → OpenAPI 3.0 规范（表→schemas/列→字段/约束→required/uniqueItems）
3. `DbSchemaToCrudApiMapper` DB schema → CRUD API（5 端点/表 + OpenAPI 文档）
4. `FullReverseLoopVerifier` 完整闭环验证（DB→OpenAPI→ORM→CRUD 一致，复用 `ApiFirstLoopVerifier`）
5. 五方言 schema 读取覆盖（参数化绑定，不支持的特性降级）
6. 命名约定可配置（复用 `NamingConvention`/`to_pascal_case`/`to_snake_case`）
7. 注入防护（复用 `OpenApiInjectionGuard`，拒绝注入/未签名 spec）
8. 复用既有反向生成（OpenAPI→ORM 方向），不重复实现
9. 反向生成日志可追溯（schema 来源 + 生成项 + 闭环验证 + 耗时）
10. `openapi-reverse` feature gate 隔离，默认关闭，既有反向生成保留

## 8.4 REQ-V48-004 WASM 真实数据库连接闭环（P1）

1. `WasmProxyServer` 代理后端服务（鉴权 + 白名单 + 限流 + 连接 DB + 返回结果，凭据不暴露）
2. `MultiDialectProxyBackend` 多方言代理后端（MySQL/PG/SQLite/Oracle/MSSQL，复用 sz-orm-core 连接池）
3. `WasmOrmSession` WASM 端 ORM 会话（查询构建 + 代理执行 + 反序列化）
4. `WasmQueryBuilderBridge` 查询构建器桥接（复用 sz-orm-query-builder，参数化查询）
5. `WasmOrmLoopVerifier` WASM ORM 闭环验证（WASM 端结果与直接 DB 查询一致）
6. `WasiSocketConnection` WASI socket 直连代理（feature = "wasi-socket"，浏览器仍用 HTTP/WS）
7. 代理鉴权与白名单与限流（复用 `WasmDbAuthValidator`/`WasmDbSqlWhitelist`/`WasmDbRateLimiter`）
8. 代理重连与指标（复用 `WasmRealDbReconnector`/`WasmRealDbMetrics`）
9. 结果集大小限制（默认 10MB，超限 `ResultTooLarge`）
10. 复用既有代理桥接，不重复实现
11. `wasm-real-db` feature gate 隔离，默认关闭，既有代理桥接保留
12. 禁止 WASM 端直连数据库（浏览器须通过代理后端）

---

# 9. feature gate 总览

| feature gate | 所属包 | 控制能力 | 默认 | 对应需求 | 状态 |
|-------------|--------|---------|------|---------|------|
| `cross-lang-dtx` | sz-orm-dtx（扩展） | 跨语言分布式事务协调（真实 gRPC/HTTP 传输 + 崩溃恢复 + Saga/TCC 深度编排） | 关闭 | REQ-V48-001 | 既有 feature 扩展（`packages/sz-orm-dtx/Cargo.toml:40`） |
| `lc-bidirectional-sync` | sz-orm-lc（扩展） | 低代码双向同步（双向同步引擎 + 冲突检测解决 + 增量追踪） | 关闭 | REQ-V48-002 | 新增 feature |
| `openapi-reverse` | sz-orm-swagger（扩展） | OpenAPI 反向生成（DB schema → OpenAPI + CRUD + 闭环验证） | 关闭 | REQ-V48-003 | 既有 feature 扩展（`packages/sz-orm-swagger/Cargo.toml:14`） |
| `wasm-real-db` | sz-orm-wasm（扩展） | WASM 真实数据库连接闭环（代理后端 + ORM 闭环 + 多方言代理） | 关闭 | REQ-V48-004 | 既有 feature 扩展（`packages/sz-orm-wasm/Cargo.toml:35`） |

---

# 10. 与 v4.7.0 的关系

## 10.1 零重叠声明

v4.8.0 与 v4.7.0 零重叠：

| v4.7.0 能力（智能化运维深化 + 性能深化层） | v4.8.0 能力（跨语言互操作 + 全栈闭环层） | 关系 |
|-------------------------------|-------------------------|------|
| 消息延迟队列与优先级调度（`sz-orm-queue` delayed-priority-queue） | 跨语言分布式事务协调（`sz-orm-dtx` cross-lang-dtx） | 零重叠，不同包不同维度 |
| 迁移前向兼容性检查与沙箱预演（`sz-orm-core` forward-compat-sandbox） | 低代码双向同步（`sz-orm-lc` lc-bidirectional-sync） | 零重叠，不同包不同维度 |
| 批量 COPY 协议与并行分片执行（`sz-orm-batch` copy-parallel-shard） | OpenAPI 反向生成（`sz-orm-swagger` openapi-reverse） | 零重叠，不同包不同维度 |
| 异常自愈与根因分析（`sz-orm-observability` anomaly-remediation-rca） | WASM 真实数据库连接闭环（`sz-orm-wasm` wasm-real-db） | 零重叠，不同包不同维度 |
| 多云成本对比与容量预测（`sz-orm-storage` multicloud-cost-forecast） | — | v4.8.0 不触碰 |
| 租户资源配额与行级安全增强（`sz-orm-core` tenant-quota-rls-enhanced） | — | v4.8.0 不触碰 |
| 缓存预热与穿透防护（`sz-orm-core` cache-warmup-protection） | — | v4.8.0 不触碰 |

## 10.2 依赖关系

```
v4.7.0 已验收基线（7 个 feature gate + 228 套测试，已发布 crates.io 4.7.0）
  │
  ├─ cross-lang-dtx（既有协议层）──→ REQ-V48-001 跨语言分布式事务协调（补真实传输 + 恢复 + Saga/TCC 深度编排）
  ├─ sz-orm-lc + sz-orm-designer（既有模型/设计器）──→ REQ-V48-002 低代码双向同步（补双向同步引擎 + 冲突解决）
  ├─ openapi-reverse（既有 OpenAPI→ORM）──→ REQ-V48-003 OpenAPI 反向生成（补 DB→OpenAPI 方向 + 闭环）
  └─ wasm-real-db（既有代理桥接）──→ REQ-V48-004 WASM 真实连接闭环（补代理后端 + ORM 闭环）

v4.8.0 四项需求相互独立，可并行开发：
  ├─ REQ-V48-001 跨语言分布式事务协调（扩展 sz-orm-dtx + sz-orm-grpc，复用既有 cross_lang 协议层 + 事务核心）
  ├─ REQ-V48-002 低代码双向同步（扩展 sz-orm-lc + sz-orm-designer，复用既有模型定义 + 设计器）
  ├─ REQ-V48-003 OpenAPI 反向生成（扩展 sz-orm-swagger，复用既有 OpenAPI→ORM 反向生成）
  └─ REQ-V48-004 WASM 真实连接闭环（扩展 sz-orm-wasm，复用既有代理桥接 + sz-orm-core 连接池 + sz-orm-query-builder）
```

## 10.3 扩展包

| 包名 | 对应需求 | 扩展内容 |
|------|---------|---------|
| `sz-orm-dtx` | REQ-V48-001 | 跨语言分布式事务协调（真实 gRPC/HTTP 传输 + 崩溃恢复 + Saga/TCC 深度编排，`cross-lang-dtx` feature 扩展） |
| `sz-orm-grpc` | REQ-V48-001 | gRPC 基础设施复用（`real_grpc.rs` tonic 桥接，不新增 feature） |
| `sz-orm-lc` | REQ-V48-002 | 低代码双向同步（双向同步引擎 + 冲突检测解决 + 增量追踪，`lc-bidirectional-sync` feature 新增） |
| `sz-orm-designer` | REQ-V48-002 | schema 设计器复用（`schema-designer` feature 既有，不新增 feature） |
| `sz-orm-swagger` | REQ-V48-003 | OpenAPI 反向生成（DB schema → OpenAPI + CRUD + 闭环验证，`openapi-reverse` feature 扩展） |
| `sz-orm-wasm` | REQ-V48-004 | WASM 真实数据库连接闭环（代理后端 + ORM 闭环 + 多方言代理，`wasm-real-db` feature 扩展） |

## 10.4 不新增包声明

v4.8.0 不新增 workspace 成员，workspace 保持 60 成员（`Cargo.toml:2`）。所有新能力落在既有 6 个包扩展（sz-orm-dtx / sz-orm-grpc / sz-orm-lc / sz-orm-designer / sz-orm-swagger / sz-orm-wasm），其中 sz-orm-grpc / sz-orm-designer 仅复用不扩展新 feature。