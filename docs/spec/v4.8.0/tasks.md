# sz-orm v4.8.0 编码任务规划

> 版本：v4.8.0（跨语言分布式事务协调 + 低代码双向同步 + OpenAPI 反向生成 + WASM 真实数据库连接闭环）
> 基线：v4.7.0（7 项需求 REQ-V47-001~007 全部通过 feature gate 隔离，228 套测试 0 失败，已发布到 crates.io 4.7.0）
> 日期：2026-08-14
> 文档定位：编码任务规划（How to execute），对应需求规格 `spec.md`（What to build，848 行，4 项 EARS 需求 REQ-V48-001~004）+ 技术设计 `design.md`（How to build，1565 行）
> 任务约束：无 Breaking Change，4 个 feature gate 隔离（3 个既有扩展 + 1 个新增），默认全关闭；优先复用既有能力 + 五方言覆盖 + 每项任务附 file:line 代码证据 + unsafe 零容忍 + 禁止占位实现 + 参数化查询强制
> 审计合规铁律：每项任务结论须附真实存在的 file:line 证据，修复后必须运行 `cargo test` 并附输出，禁止未验证即标记 ✅
> 实施顺序：M0（P0 文档基线，立即）→ M1~M4（4 项需求并行开发，主体独立）→ M5（最终集成验证与文档同步，全部完成后）
> 与 v4.7.0 零重叠：v4.7.0 是"智能化运维深化 + 性能深化"层，v4.8.0 是"跨语言互操作 + 全栈闭环"层，新增范围全部落在既有包扩展（sz-orm-dtx / sz-orm-grpc / sz-orm-lc / sz-orm-designer / sz-orm-swagger / sz-orm-wasm），不新增包

---

# 一、任务总览

## 1.1 里程碑 × 任务数 × 预期工作量

| 里程碑 | 名称 | 对应需求 | 优先级 | 任务数 | 子任务数 | 预期工作量 | 启动时机 |
|--------|------|---------|--------|--------|----------|-----------|---------|
| M0 | 文档基线与准备 | — | P0 | 3 | 12 | 0.5 天 | 立即（v4.7.0 已完成） |
| M1 | 跨语言分布式事务协调 | REQ-V48-001 | P1 | 6 | 42 | 3 天 | 立即（独立扩展 sz-orm-dtx + sz-orm-grpc） |
| M2 | OpenAPI 反向生成 | REQ-V48-003 | P1 | 5 | 36 | 2.5 天 | 立即（独立扩展 sz-orm-swagger） |
| M3 | WASM 真实数据库连接闭环 | REQ-V48-004 | P1 | 6 | 40 | 3 天 | 立即（独立扩展 sz-orm-wasm） |
| M4 | 低代码双向同步 | REQ-V48-002 | P1 | 5 | 38 | 2.5 天 | 立即（独立扩展 sz-orm-lc + sz-orm-designer） |
| M5 | 集成验证与文档同步 | 全局 | P0 | 3 | 18 | 0.5 天 | M1~M4 全部完成后 |
| **合计** | — | **4 项全覆盖** | — | **28** | **186** | **12 天** | — |

## 1.2 任务编号约定

- 主任务：`M{里程碑号}-T{任务序号}`（如 M1-T1）
- 子任务：`M{里程碑号}-T{任务序号}.{子任务序号}`（如 M1-T2.1）
- 集成验证任务：每个里程碑末尾固定一个集成测试与门禁验证任务
- 里程碑内需求按 spec.md 优先级声明推进顺序编排（REQ-V48-001 → 003 → 004 → 002 对应 M1~M4）

## 1.3 全局约束（适用于所有任务）

1. **feature gate 隔离**：4 个 feature（`cross-lang-dtx` 扩展 / `lc-bidirectional-sync` 新增 / `openapi-reverse` 扩展 / `wasm-real-db` 扩展），默认全关闭
2. **既有 API 不变**：既有公开 API 签名完全向后兼容，sz-pay 既有代码不受影响
3. **禁止占位实现**：禁止 `todo!`/`unimplemented!`/`unreachable!`
4. **unsafe 零容忍**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释
5. **五方言覆盖**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL（OpenAPI schema 读取 + WASM 代理后端全方言覆盖）
6. **参数化查询强制**：任何 WHERE 条件必须参数化绑定，禁止 SQL 字符串拼接
7. **审计证据**：每项任务结论附真实存在的 file:line 证据
8. **测试基线不回退**：v4.7.0 已验收 228 套测试基线不回退，v4.8.0 仅增不减
9. **复用优先**：优先复用既有能力，不重复实现（复用率 100%，不新增包）
10. **不新增包**：所有新能力通过既有包扩展实现，workspace 成员保持 60 个
11. **严禁幻影交付**：每项新能力附生产调用点 + 端到端接线测试，"模块存在+测试通过"≠"已交付"
12. **Windows MSVC 编译环境**：RUST_MIN_STACK=134217728, CARGO_INCREMENTAL=0
13. **测试命令**：`cargo test --workspace -j 2 --no-fail-fast`；feature 包测试：`cargo test -p <package> --features <feature>`

## 1.4 里程碑依赖关系

```
M0（P0，文档基线，立即）
M1（P1，跨语言分布式事务协调，独立扩展 sz-orm-dtx + sz-orm-grpc）
  - REQ-V48-001 复用既有 CrossLangParticipantProtocol/GrpcParticipantProtocol/HttpParticipantProtocol
    + CrossLangParticipant::to_participant() + TransactionLogStore + saga/tcc/recovery + real_grpc
M2（P1，OpenAPI 反向生成，独立扩展 sz-orm-swagger）
  - REQ-V48-003 复用既有 OpenApiReverseGenerator/SchemaToModelMapper/ApiFirstLoopVerifier/OpenApiInjectionGuard
M3（P1，WASM 真实连接闭环，独立扩展 sz-orm-wasm）
  - REQ-V48-004 复用既有 WasmDbProxy/WasmRealDbConnection/WasmDbAuthValidator/WasmDbSqlWhitelist
    + WasmDbRateLimiter + sz-orm-core Pool + sz-orm-query-builder
M4（P1，低代码双向同步，独立扩展 sz-orm-lc + sz-orm-designer）
  - REQ-V48-002 复用既有 ModelDefinition/FieldDef/FieldTypeMapping/ValidationRule + SchemaDesigner/code_gen/code_parse
M5（P0，集成验证与文档同步，M1~M4 全部完成后）
  - 依赖 M0~M4 全部完成
```

> **并行开发说明**：M1~M4 四项需求主体相互独立（design.md 依赖关系图明确声明），可并行开发。M0 完成后，M1~M4 同时启动；M1~M4 全部完成后，M5 启动。

## 1.5 feature gate 定义与测试命令

| feature gate | 所属包 | 依赖既有 feature | 测试命令 | 默认 |
|-------------|--------|------------------|---------|------|
| `cross-lang-dtx` | sz-orm-dtx（扩展） | 既有 `cross-lang-dtx` 协议层（同 feature 扩展） | `cargo test -p sz-orm-dtx --features cross-lang-dtx` | 关闭 |
| `lc-bidirectional-sync` | sz-orm-lc（新增） | 既有 `schema-designer`（sz-orm-designer） | `cargo test -p sz-orm-lc --features lc-bidirectional-sync` | 关闭 |
| `openapi-reverse` | sz-orm-swagger（扩展） | 既有 `openapi-reverse` OpenAPI→ORM（同 feature 扩展） | `cargo test -p sz-orm-swagger --features openapi-reverse` | 关闭 |
| `wasm-real-db` | sz-orm-wasm（扩展） | 既有 `wasm-real-db` 代理桥接 + `wasi-socket` | `cargo test -p sz-orm-wasm --features wasm-real-db` | 关闭 |

---

# 二、M0：文档基线与准备（P0，0.5 天）

**目标**：锁定 v4.7.0 已验收基线，准备 v4.8.0 开发环境（4 个 feature gate 骨架 + 版本号升级），不新增包。
**对应需求**：—（文档基线与环境准备，非功能需求）
**预期工作量**：0.5 天
**依赖**：无（v4.7.0 已全部完成并发布 crates.io 4.7.0）

## M0-T1：v4.7.0 完成总结与基线锁定

**任务描述**：总结 v4.7.0 交付成果（46 任务 / 290 子任务全部完成），锁定测试基线（v4.7.0 228 套测试通过 + 21 道门禁全通过），作为 v4.8.0 开发的基准。

**涉及文件**：`docs/spec/v4.7.0/tasks.md`（既有，确认全部 `[x]`）、`docs/spec/v4.7.0/spec.md`（既有）、`docs/spec/v4.7.0/design.md`（既有）

**子任务**：
- [ ] M0-T1.1 确认 `docs/spec/v4.7.0/tasks.md` 46 任务 / 290 子任务全部标记 `[x]`（v4.7.0 已完成）
- [ ] M0-T1.2 运行 `cargo test --workspace -j 2 --no-fail-fast` 记录 v4.7.0 测试基线（228 套全部通过）
- [ ] M0-T1.3 运行 21 道门禁全量验证，记录基线通过状态
- [ ] M0-T1.4 确认 v4.7.0 7 个 feature gate 默认全关闭，行为不变

**验收标准**：v4.7.0 基线锁定（测试数 228 + 门禁通过状态 + feature gate 状态），每项附 file:line 或命令输出证据

**依赖**：无

## M0-T2：v4.8.0 开发环境准备

**任务描述**：在 4 个既有包新增/扩展 4 个 feature gate 占位（默认关闭），升级版本号 4.7.0 → 4.8.0，验证 workspace 编译通过，不新增包。

**涉及文件**：
- `Cargo.toml`（workspace.package.version 4.7.0 → 4.8.0 `:6`）
- `packages/sz-orm-dtx/Cargo.toml`（扩展：`cross-lang-dtx` feature 既有 `:40`，新增 reqwest 依赖）
- `packages/sz-orm-lc/Cargo.toml`（新增：`lc-bidirectional-sync` feature 占位，依赖 sz-orm-designer `schema-designer`）
- `packages/sz-orm-swagger/Cargo.toml`（扩展：`openapi-reverse` feature 既有 `:14`，新增 sz-orm-sqlx 依赖）
- `packages/sz-orm-wasm/Cargo.toml`（扩展：`wasm-real-db` feature 既有 `:35`，新增 sz-orm-core/sz-orm-query-builder 依赖）

**复用标注**：既有 workspace 结构 `Cargo.toml`（60 个成员）；既有 v4.7.0 feature gate 模式

**子任务**：
- [ ] M0-T2.1 `packages/sz-orm-dtx/Cargo.toml` 确认 `cross-lang-dtx` feature（既有 `:40`），新增 `reqwest` 依赖（真实 HTTP 传输）
- [ ] M0-T2.2 `packages/sz-orm-lc/Cargo.toml` 新增 `lc-bidirectional-sync = ["dep:sz-orm-designer", "sz-orm-designer/schema-designer"]` feature（默认关闭）
- [ ] M0-T2.3 `packages/sz-orm-swagger/Cargo.toml` 确认 `openapi-reverse` feature（既有 `:14`），新增 `sz-orm-sqlx` 依赖（五方言 schema 读取）
- [ ] M0-T2.4 `packages/sz-orm-wasm/Cargo.toml` 确认 `wasm-real-db` feature（既有 `:35`），新增 `sz-orm-core`/`sz-orm-query-builder` 依赖（代理后端连接池 + 查询构建器桥接）
- [ ] M0-T2.5 `Cargo.toml` workspace.package.version 从 `4.7.0` 升级为 `4.8.0`（`:6`）
- [ ] M0-T2.6 验证 `cargo check --workspace` 编译通过（4 个 feature gate 占位不影响既有编译，workspace 成员仍 60）
- [ ] M0-T2.7 验证默认 feature 行为与 v4.7.0 一致（`cargo build --workspace` 行为不变）

**验收标准**：4 个 feature gate 占位创建成功，workspace 成员仍 60，版本号 4.8.0，workspace 编译通过，默认 feature 行为不变

**依赖**：M0-T1

## M0-T3：基线验证

**任务描述**：运行文档一致性、审计证据、文档同步三道门禁，验证 v4.7.0 基线可被工具消费，v4.8.0 骨架不破坏既有基线。

**涉及文件**：`scripts/check-doc-consistency.py`、`scripts/audit-verify.sh`、`scripts/check-doc-sync.py`

**子任务**：
- [ ] M0-T3.1 运行 `python scripts/check-doc-consistency.py`（门禁 12），验证文档与代码一致
- [ ] M0-T3.2 运行 `bash scripts/audit-verify.sh docs/spec/v4.7.0/tasks.md`（门禁 13），验证 v4.7.0 tasks.md 所有 file:line 引用真实存在
- [ ] M0-T3.3 运行 `python scripts/check-doc-sync.py --diff HEAD`（门禁 14），验证文档与 HEAD 同步
- [ ] M0-T3.4 验证 v4.8.0 spec.md（848 行）与 design.md（1565 行）所有 file:line 证据真实存在（audit-verify 扩展验证）

**验收标准**：三道门禁全部通过；v4.7.0 tasks.md + v4.8.0 spec.md + v4.8.0 design.md 所有 file:line 引用经 audit-verify 验证真实存在

**依赖**：M0-T2

---

# 三、M1：跨语言分布式事务协调（REQ-V48-001，P1，3 天）

**目标**：扩展既有 `sz-orm-dtx` 包 `cross-lang-dtx` feature，新增 `TonicGrpcCallHandler` 真实 tonic gRPC 传输 + `ReqwestHttpCallHandler` 真实 HTTP/JSON 传输 + `CrossLangRecoveryCoordinator` 跨语言崩溃恢复 + `CrossLangSagaCoordinator`/`CrossLangTccCoordinator` 跨语言编排 + 各语言 SDK 接入契约，复用既有 `CrossLangParticipantProtocol`/`GrpcParticipantProtocol`/`HttpParticipantProtocol`/`CrossLangParticipant::to_participant()`/`TransactionLogStore`/`recovery`/`saga`/`tcc` + sz-orm-grpc `real_grpc`，替代既有仅测试用的 `MockRemoteCallHandler`。
**对应需求**：REQ-V48-001（spec.md §5.1，design.md §2.1.3.1 + §2.2.2 REQ-V48-001 接口）
**预期工作量**：3 天
**依赖**：无（M1 为 P1 独立需求，复用既有 sz-orm-dtx cross_lang 协议层 + 事务核心 + sz-orm-grpc real_grpc，扩展包可与 M2~M4 并行）

## M1-T1：cross-lang-dtx feature gate 扩展 + 真实传输配置

**任务描述**：在 `sz-orm-dtx` 完善 `cross-lang-dtx` feature gate 隔离（M0-T2 已扩展依赖），定义真实传输配置结构 + 模块骨架，作为真实 gRPC/HTTP 传输的基础。

**涉及文件**：
- `packages/sz-orm-dtx/Cargo.toml`（完善：`cross-lang-dtx` feature 新增 `reqwest` 依赖）
- `packages/sz-orm-dtx/src/cross_lang/mod.rs`（扩展：模块声明 `mod real_transport;` `mod recovery;` `mod saga;` `mod tcc;` `mod sdk_contract;`，`#[cfg(feature = "cross-lang-dtx")]` 门控）
- `packages/sz-orm-dtx/src/cross_lang/real_transport.rs`（新建，真实传输配置 + handler 骨架）

**复用标注**：既有 `RemoteCallHandler` trait `packages/sz-orm-dtx/src/cross_lang/protocol.rs:15`；既有 `GrpcParticipantProtocol` `:26`；既有 `HttpParticipantProtocol` `:78`；既有 `ParticipantAuth` `packages/sz-orm-dtx/src/cross_lang/mod.rs:45`；既有 `ParticipantResponse` `:75`；既有 `CrossLangTxError` `:88`；既有 `check_protocol_version` `protocol.rs:192`

**feature gate 隔离**：`cross-lang-dtx`（既有 feature 扩展，默认关闭）

**子任务**：
- [ ] M1-T1.1 `packages/sz-orm-dtx/Cargo.toml` 确认 `cross-lang-dtx` feature 新增 `reqwest` 依赖（真实 HTTP 传输）
- [ ] M1-T1.2 `src/cross_lang/mod.rs` 声明 `mod real_transport;` `mod recovery;` `mod saga;` `mod tcc;` `mod sdk_contract;`，`#[cfg(feature = "cross-lang-dtx")]` 门控
- [ ] M1-T1.3 `src/cross_lang/real_transport.rs` 定义 `pub struct TonicGrpcCallHandler { endpoint, auth, timeout_ms }`（真实 gRPC 传输 handler 骨架）
- [ ] M1-T1.4 定义 `pub struct ReqwestHttpCallHandler { endpoint, token, timeout_ms }`（真实 HTTP 传输 handler 骨架）
- [ ] M1-T1.5 定义 `pub struct RealTransportConfig { default_timeout_ms, max_retries, tls_config }`（真实传输配置，默认 timeout 5000ms）
- [ ] M1-T1.6 实现链式配置方法：`with_timeout` / `with_max_retries` / `with_tls_config`
- [ ] M1-T1.7 单元测试：`RealTransportConfig::new()` 默认值正确（timeout 5000ms, max_retries 3）
- [ ] M1-T1.8 验证 `cargo check -p sz-orm-dtx --features cross-lang-dtx` 编译通过

**验收标准**：feature gate 扩展；TonicGrpcCallHandler/ReqwestHttpCallHandler/RealTransportConfig 骨架定义完整；复用既有 `RemoteCallHandler` trait

**依赖**：M0-T2

## M1-T2：TonicGrpcCallHandler 真实 gRPC 传输

**任务描述**：实现 `TonicGrpcCallHandler` 真实 tonic gRPC 传输，实现 `RemoteCallHandler` trait，通过 tonic + prost 调用远端跨语言参与者 prepare/commit/rollback 端点，支持 mTLS 双向认证与 Token 认证，复用 sz-orm-grpc `real_grpc.rs` tonic 桥接 + 既有 `GrpcParticipantProtocol`。

**涉及文件**：
- `packages/sz-orm-dtx/src/cross_lang/real_transport.rs`（扩展：真实 gRPC 传输核心）

**复用标注**：既有 `RemoteCallHandler` trait `packages/sz-orm-dtx/src/cross_lang/protocol.rs:15`；既有 `GrpcParticipantProtocol` `:26`；sz-orm-grpc `RealGrpcClient` `packages/sz-orm-grpc/src/real_grpc.rs:199`；sz-orm-grpc `RealGrpcServer` `:94`；既有 `ParticipantAuth::Mtls` `packages/sz-orm-dtx/src/cross_lang/mod.rs:47`；既有 `check_protocol_version` `protocol.rs:192`

**子任务**：
- [ ] M1-T2.1 实现 `impl TonicGrpcCallHandler { pub fn new(endpoint: String, auth: ParticipantAuth) -> Self }`
- [ ] M1-T2.2 实现 `pub fn with_timeout(mut self, timeout_ms: u64) -> Self`（默认 5000ms，复用既有 `CrossLangParticipant::with_timeout` `participant.rs:30` 模式）
- [ ] M1-T2.3 实现 `impl RemoteCallHandler for TonicGrpcCallHandler`：`fn call(&self, method, tx_id, payload) -> Result<ParticipantResponse, CrossLangTxError>`
- [ ] M1-T2.4 实现 mTLS 双向认证：通过 `ParticipantAuth::Mtls{cert,key,ca}` `mod.rs:47` 配置 tonic TLS，证书过期返回 `CrossLangTxError::AuthFailed` `:92`
- [ ] M1-T2.5 实现 Token 认证：通过 `ParticipantAuth::Token` `mod.rs:53` 注入 gRPC metadata
- [ ] M1-T2.6 实现超时控制：`tokio::time::timeout` 包装 gRPC 调用，超时返回 `Cross<LangTxError::Timeout` `:90`
- [ ] M1-T2.7 实现协议版本检查：调用前 `check_protocol_version` `protocol.rs:192`，不匹配返回 `ProtocolVersionMismatch` `:94`
- [ ] M1-T2.8 复用 sz-orm-grpc `RealGrpcClient` `real_grpc.rs:199` 发起真实 tonic gRPC 调用
- [ ] M1-T2.9 单元测试：Go 参与者 gRPC 端点 + TonicGrpcCallHandler + prepare 调用 → 通过 tonic gRPC 真实调用返回 ParticipantResponse
- [ ] M1-T2.10 单元测试：mTLS 认证 + 证书过期 → 返回 `CrossLangTxError::AuthFailed`
- [ ] M1-T2.11 边界测试：gRPC 端点不可达 → 返回 `CrossLangTxError::Transport` `:98`
- [ ] M1-T2.12 边界测试：协议版本不匹配 → 返回 `ProtocolVersionMismatch`
- [ ] M1-T2.13 性能测试：gRPC 调用延迟 ≤ 100ms（不含网络 RTT，含序列化 + 协议处理）

**验收标准**：TonicGrpcCallHandler 实现 `RemoteCallHandler` trait；mTLS/Token 鉴权；超时控制；复用 sz-orm-grpc `real_grpc`；性能 ≤ 100ms

**依赖**：M1-T1

## M1-T3：ReqwestHttpCallHandler 真实 HTTP 传输

**任务描述**：实现 `ReqwestHttpCallHandler` 真实 HTTP/JSON 传输，实现 `RemoteCallHandler` trait，通过 reqwest POST 调用远端参与者 `{endpoint}/prepare|commit|rollback`，Token 认证 + 超时控制，复用既有 `HttpParticipantProtocol`。

**涉及文件**：
- `packages/sz-orm-dtx/src/cross_lang/real_transport.rs`（扩展：真实 HTTP 传输核心）

**复用标注**：既有 `RemoteCallHandler` trait `packages/sz-orm-dtx/src/cross_lang/protocol.rs:15`；既有 `HttpParticipantProtocol` `:78`；既有 `ParticipantAuth::Token` `packages/sz-orm-dtx/src/cross_lang/mod.rs:53`

**子任务**：
- [ ] M1-T3.1 实现 `impl ReqwestHttpCallHandler { pub fn new(endpoint: String, token: String) -> Self }`
- [ ] M1-T3.2 实现 `pub fn with_timeout(mut self, timeout_ms: u64) -> Self`
- [ ] M1-T3.3 实现 `impl RemoteCallHandler for ReqwestHttpCallHandler`：`fn call(&self, method, tx_id, payload) -> Result<ParticipantResponse, CrossLangTxError>`
- [ ] M1-T3.4 实现 HTTP POST 调用：`reqwest::Client::post({endpoint}/{method})` 发起 HTTP/JSON 调用，body 含 tx_id + payload
- [ ] M1-T3.5 实现 Token 认证：HTTP header `Authorization: Bearer {token}`
- [ ] M1-T3.6 实现超时控制：`reqwest::Client::timeout`，超时返回 `CrossLangTxError::Timeout` `:90`
- [ ] M1-T3.7 实现响应解析：HTTP 200 + JSON body → `ParticipantResponse`；非 200 → `CrossLangTxError::Transport` `:98`
- [ ] M1-T3.8 单元测试：Java 参与者 HTTP 端点 + ReqwestHttpCallHandler + commit 调用 → 通过 HTTP/JSON 真实调用返回 ParticipantResponse
- [ ] M1-T3.9 单元测试：Token 无效 → 返回 `CrossLangTxError::AuthFailed` `:92`
- [ ] M1-T3.10 边界测试：HTTP 端点不可达 → 返回 `CrossLangTxError::Transport`
- [ ] M1-T3.11 边界测试：HTTP 响应非 JSON → 返回 `CrossLangTxError::Transport`
- [ ] M1-T3.12 性能测试：HTTP 调用延迟 ≤ 100ms（不含网络 RTT）

**验收标准**：ReqwestHttpCallHandler 实现 `RemoteCallHandler` trait；Token 鉴权；超时控制；复用既有 `HttpParticipantProtocol`；性能 ≤ 100ms

**依赖**：M1-T1

## M1-T4：CrossLangRecoveryCoordinator 跨语言崩溃恢复

**任务描述**：实现 `CrossLangRecoveryCoordinator` 跨语言崩溃恢复协调器，协调器崩溃重启后恢复跨语言事务至一致状态：查询未完成事务 → 询问各跨语言参与者状态 → 按状态决定全局提交/回滚 → 记录恢复日志，复用既有 `TransactionLogStore::read_pending` + `recovery.rs`。

**涉及文件**：
- `packages/sz-orm-dtx/src/cross_lang/recovery.rs`（新建，跨语言崩溃恢复协调器）

**复用标注**：既有 `TransactionLogStore` trait `packages/sz-orm-dtx/src/lib.rs:57`；既有 `TransactionLogStore::read_pending` `:71`；既有 `TransactionState` `:163`；既有 `recovery.rs` `packages/sz-orm-dtx/src/recovery.rs`（XA 崩溃恢复框架）；`TonicGrpcCallHandler`/`ReqwestHttpCallHandler`（M1-T2/T3）

**子任务**：
- [ ] M1-T4.1 定义 `pub struct CrossLangRecoveryCoordinator { log_store: Arc<dyn TransactionLogStore>, participants: Vec<CrossLangParticipant> }`
- [ ] M1-T4.2 实现 `impl CrossLangRecoveryCoordinator { pub fn new(log_store: Arc<dyn TransactionLogStore>, participants: Vec<CrossLangParticipant>) -> Self }`
- [ ] M1-T4.3 实现 `pub async fn recover(&self) -> Result<RecoveryReport, CrossLangTxError>`：查询未完成事务 → 询问参与者状态 → 决策 → 执行恢复
- [ ] M1-T4.4 实现查询未完成事务：复用 `TransactionLogStore::read_pending` `lib.rs:71` 获取未完成事务列表
- [ ] M1-T4.5 实现询问参与者状态：通过 `TonicGrpcCallHandler`/`ReqwestHttpCallHandler` query_status 询问各跨语言参与者 prepare/commit/rollback 状态
- [ ] M1-T4.6 实现决策分支 1：所有参与者已 Committed/Prepared → 全局提交，通知未提交参与者 commit
- [ ] M1-T4.7 实现决策分支 2：存在参与者未 Prepared/RolledBack → 全局回滚，通知已 Prepared 参与者 rollback
- [ ] M1-T4.8 实现决策分支 3：参与者状态冲突（部分 Committed 部分 RolledBack）→ `CrossLangTxError::RecoveryConflict` `:96`，标记需人工干预
- [ ] M1-T4.9 实现记录恢复日志：恢复决策 + 执行结果 + 时间戳
- [ ] M1-T4.10 定义 `pub struct RecoveryReport { recovered_count, committed_count, rolled_back_count, manual_intervention_required: Vec<String> }`
- [ ] M1-T4.11 单元测试：协调器崩溃 + 事务 tx1 已 Prepared + 参与者 A 已 Committed + 参与者 B 已 Prepared → 恢复时 A 已提交 → 全局提交，通知 B commit
- [ ] M1-T4.12 单元测试：协调器崩溃 + 事务 tx2 Preparing 中 + 参与者 C 未 Prepared → 全局回滚，通知 C rollback
- [ ] M1-T4.13 单元测试：参与者状态冲突（A Committed + B RolledBack）→ 返回 `RecoveryConflict`，标记需人工干预
- [ ] M1-T4.14 边界测试：无未完成事务 → 返回空 RecoveryReport，不执行恢复
- [ ] M1-T4.15 边界测试：参与者状态查询超时 → 标记该参与者状态未知，按保守策略回滚

**验收标准**：CrossLangRecoveryCoordinator 恢复逻辑正确；三种决策分支；复用既有 `TransactionLogStore::read_pending` + `recovery.rs`；状态冲突标记人工干预

**依赖**：M1-T2、M1-T3

## M1-T5：CrossLangSagaCoordinator + CrossLangTccCoordinator 跨语言编排

**任务描述**：实现 `CrossLangSagaCoordinator` 跨语言 Saga 编排器 + `CrossLangTccCoordinator` 跨语言 TCC 编排器，将跨语言参与者通过 `CrossLangParticipant::to_participant()` 适配后接入既有 `saga.rs`/`tcc.rs` 编排，Saga 补偿按反向执行，TCC Try-Confirm-Cancel 三阶段跨语言调用，复用 `CrossLangCompensationSerializer` 幂等键。

**涉及文件**：
- `packages/sz-orm-dtx/src/cross_lang/saga.rs`（新建，跨语言 Saga 编排器）
- `packages/sz-orm-dtx/src/cross_lang/tcc.rs`（新建，跨语言 TCC 编排器）

**复用标注**：既有 `saga.rs` `packages/sz-orm-dtx/src/saga.rs`（Saga 编排器）；既有 `tcc.rs` `packages/sz-orm-dtx/src/tcc.rs`（TCC 三阶段）；既有 `CrossLangParticipant::to_participant()` `packages/sz-orm-dtx/src/cross_lang/participant.rs:51`；既有 `CrossLangCompensationSerializer` `packages/sz-orm-dtx/src/cross_lang/serializer.rs:23`；既有 `CrossLangCompensationSerializer::idempotency_key` `:70`

**子任务**：
- [ ] M1-T5.1 定义 `pub struct CrossLangSagaCoordinator { participants: Vec<CrossLangParticipant> }`（复用既有 `saga.rs`）
- [ ] M1-T5.2 实现 `impl CrossLangSagaCoordinator { pub fn new(participants: Vec<CrossLangParticipant>) -> Self }`
- [ ] M1-T5.3 实现 `pub async fn execute(&self, tx_id: &str) -> Result<SagaResult, CrossLangTxError>`：将跨语言参与者通过 `to_participant()` `participant.rs:51` 适配后接入既有 `saga.rs` 编排
- [ ] M1-T5.4 实现 Saga 补偿反向执行：失败时按 Saga 反向顺序调用 rollback，复用既有 `saga.rs` 补偿逻辑
- [ ] M1-T5.5 实现跨语言补偿幂等：复用 `CrossLangCompensationSerializer::idempotency_key` `serializer.rs:70` 生成幂等键，重复补偿返回缓存结果
- [ ] M1-T5.6 定义 `pub struct CrossLangTccCoordinator { participants: Vec<CrossLangParticipant> }`（复用既有 `tcc.rs`）
- [ ] M1-T5.7 实现 `impl CrossLangTccCoordinator { pub fn new(participants: Vec<CrossLangParticipant>) -> Self }`
- [ ] M1-T5.8 实现 `pub async fn try_confirm_cancel(&self, tx_id: &str) -> Result<TccResult, CrossLangTxError>`：Try-Confirm-Cancel 三阶段跨语言调用
- [ ] M1-T5.9 实现 TCC Cancel 幂等：复用 `idempotency_key` 保证 Cancel 幂等
- [ ] M1-T5.10 实现补偿失败处理：返回 `CrossLangTxError::CompensationFailed` `:100`，标记需人工干预
- [ ] M1-T5.11 单元测试：Saga tx3 = Rust 参与者 A → Go 参与者 B → Java 参与者 C + C 失败 → 反向补偿：C rollback → B rollback → A rollback，补偿顺序正确
- [ ] M1-T5.12 单元测试：TCC tx4 = Rust A + Go B + Python C + Try 全部成功 + Confirm B 失败 → Cancel A + Cancel C，TCC 三阶段正确
- [ ] M1-T5.13 单元测试：参与者 A commit 幂等键 key1 + 重复 commit key1 → 第二次 commit 返回缓存结果，不重复执行副作用
- [ ] M1-T5.14 边界测试：Saga 补偿失败 → 返回 `CompensationFailed`，标记需人工干预
- [ ] M1-T5.15 边界测试：TCC Cancel 失败 → 返回 `CompensationFailed`，标记需人工干预

**验收标准**：CrossLangSagaCoordinator/TccCoordinator 编排正确；Saga 反向补偿；TCC 三阶段；幂等性；复用既有 `saga.rs`/`tcc.rs` + `to_participant()` + `idempotency_key`

**依赖**：M1-T2、M1-T3

## M1-T6：SDK 接入契约 + 可观测性 + M1 集成测试与门禁验证

**任务描述**：为 Go/Java/C++/Python/JavaScript 各语言提供参与者 SDK 接入契约，复用 `cross_lang::observability` 可观测性，M1 里程碑集成测试与门禁验证，确保 REQ-V48-001 全部 10 条验收条件满足。

**涉及文件**：
- `packages/sz-orm-dtx/src/cross_lang/sdk_contract.rs`（新建，各语言 SDK 接入契约）
- `packages/sz-orm-dtx/src/cross_lang/observability.rs`（既有，复用）

**复用标注**：既有 `cross_lang::observability` `packages/sz-orm-dtx/src/cross_lang/observability.rs:12`（`CrossLangTxAlerter`/`CrossLangTxMetrics`）；既有 `COORDINATOR_PROTOCOL_VERSION = 1` `packages/sz-orm-dtx/src/cross_lang/mod.rs:128`；既有 `sz-orm-go`/`sz-orm-java`/`sz-orm-cpp`/`sz-orm-python`/`sz-orm-js`（workspace 既有 5 个 FFI/绑定包）

**子任务**：
- [ ] M1-T6.1 `src/cross_lang/sdk_contract.rs` 定义 `pub struct CrossLangSdkContract { language: ParticipantLanguage, protocol_version: u32, endpoints: SdkEndpoints, auth_scheme: AuthScheme, serialization_format: SerializationFormat }`
- [ ] M1-T6.2 定义 `pub struct SdkEndpoints { prepare: String, commit: String, rollback: String, status: String }`（参与者端点签名）
- [ ] M1-T6.3 实现 `impl CrossLangSdkContract { pub fn for_language(language: ParticipantLanguage) -> Self }`：为各语言生成契约，`protocol_version` 对齐 `COORDINATOR_PROTOCOL_VERSION` `mod.rs:128`
- [ ] M1-T6.4 实现 Go/Java/C++/Python/JS 五语言契约生成，复用既有 `sz-orm-go`/`sz-orm-java`/`sz-orm-cpp`/`sz-orm-python`/`sz-orm-js` 包基础
- [ ] M1-T6.5 实现跨语言事务可观测性：复用 `cross_lang::observability` `observability.rs:12`，记录事务 ID + 参与者（语言/端点）+ 阶段 + 耗时 + 结果
- [ ] M1-T6.6 单元测试：Go 参与者按契约实现 + 注册到协调器 → 协调器可识别 Go 参与者，按契约调用 prepare/commit/rollback
- [ ] M1-T6.7 单元测试：跨语言事务 tx5 + Go 参与者 prepare → 追踪 span 记录 tx5 + participant=go + phase=prepare + latency + result
- [ ] M1-T6.8 集成测试：`TonicGrpcCallHandler` + `ReqwestHttpCallHandler` + `CrossLangRecoveryCoordinator` + `CrossLangSagaCoordinator` + `CrossLangTccCoordinator` 完整流程
- [ ] M1-T6.9 集成测试：复用既有 `CrossLangParticipantProtocol` `mod.rs:108` + `GrpcParticipantProtocol` `protocol.rs:26` + `HttpParticipantProtocol` `:78` + `TransactionLogStore` `lib.rs:57` + `saga`/`tcc`/`recovery` + sz-orm-grpc `real_grpc`，不新建协议层/事务核心
- [ ] M1-T6.10 运行 `cargo test -p sz-orm-dtx --features cross-lang-dtx`（全部通过）
- [ ] M1-T6.11 `cargo clippy -p sz-orm-dtx --features cross-lang-dtx -- -D warnings`
- [ ] M1-T6.12 `cargo fmt -p sz-orm-dtx -- --check`
- [ ] M1-T6.13 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-dtx/src/cross_lang/real_transport.rs packages/sz-orm-dtx/src/cross_lang/recovery.rs packages/sz-orm-dtx/src/cross_lang/saga.rs packages/sz-orm-dtx/src/cross_lang/tcc.rs packages/sz-orm-dtx/src/cross_lang/sdk_contract.rs` 无占位实现
- [ ] M1-T6.14 扫描 `grep -rn 'unsafe' packages/sz-orm-dtx/src/cross_lang/real_transport.rs packages/sz-orm-dtx/src/cross_lang/recovery.rs packages/sz-orm-dtx/src/cross_lang/saga.rs packages/sz-orm-dtx/src/cross_lang/tcc.rs packages/sz-orm-dtx/src/cross_lang/sdk_contract.rs` 无 unsafe 块
- [ ] M1-T6.15 验证默认 feature 行为与 v4.7.0 一致（`cargo build -p sz-orm-dtx` 无跨语言事务协调新能力）
- [ ] M1-T6.16 验证 `cross-lang-dtx` 与既有 feature 组合编译通过

**验收标准**：M1 集成测试通过；门禁通过；默认行为不变；五语言 SDK 契约 + 可观测性 + 真实传输 + 崩溃恢复 + Saga/TCC 编排全部验证

**依赖**：M1-T1、M1-T2、M1-T3、M1-T4、M1-T5

---

# 四、M2：OpenAPI 反向生成（REQ-V48-003，P1，2.5 天）

**目标**：扩展既有 `sz-orm-swagger` 包 `openapi-reverse` feature，新增 `DbSchemaReader` 五方言 schema 读取器 + `DbSchemaToOpenApiMapper` DB schema → OpenAPI 3.0 规范映射器 + `DbSchemaToCrudApiMapper` DB schema → CRUD API 映射器 + `FullReverseLoopVerifier` 完整闭环验证器，复用既有 `OpenApiReverseGenerator`/`SchemaToModelMapper`/`ApiFirstLoopVerifier`/`OpenApiInjectionGuard`，补齐 DB schema → OpenAPI 规范 + CRUD API 反向生成方向，形成完整闭环。
**对应需求**：REQ-V48-003（spec.md §5.3，design.md §2.1.3.3 + §2.2.2 REQ-V48-003 接口）
**预期工作量**：2.5 天
**依赖**：无（M2 为 P1 独立需求，复用=既有 sz-orm-swagger reverse OpenAPI→ORM 方向，扩展包可与 M1、M3、M4 并行）

## M2-T1：openapi-reverse feature 扩展 + DbSchema 数据模型

**任务描述**：在 `sz-orm-swagger` 完善 `openapi-reverse` feature gate 隔离（M0-T2 已扩展依赖），定义 `DbSchema`/`DbTable`/`DbColumn`/`DbConstraint`/`DbIndex`/`DbDialect` 数据模型 + `CrudApiEndpoint` 端点定义，作为 DB schema 读取与映射的基础。

**涉及文件**：
- `packages/sz-orm-swagger/Cargo.toml`（完善：`openapi-reverse` feature 新增 `sz-orm-sqlx` 依赖）
- `packages/sz-orm-swagger/src/reverse/mod.rs`（扩展：模块声明 `mod db_schema;`，`#[cfg(feature = "openapi-reverse")]` 门控）
- `packages/sz-orm-swagger/src/reverse/db_schema.rs`（新建，DB schema 数据模型 + CRUD 端点定义）

**复用标注**：既有 `ReverseGenConfig`/`NamingConvention` `packages/sz-orm-swagger/src/reverse/mod.rs:24`；既有 `to_pascal_case`/`to_snake_case` `:63`/`:80`；既有 `ReverseGenError` `:36`

**子任务**：
- [ ] M2-T1.1 `packages/sz-orm-swagger/Cargo.toml` 确认 `openapi-reverse` feature 新增 `sz-orm-sqlx` 依赖（五方言 schema 读取）
- [ ] M2-T1.2 `src/reverse/mod.rs` 声明 `mod db_schema;`，`#[cfg(feature = "openapi-reverse")]` 门控
- [ ] M2-T1.3 `src/reverse/db_schema.rs` 定义 `pub enum DbDialect { MySql, PostgreSql, Sqlite, Oracle, Mssql }`（五方言枚举）
- [ ] M2-T1.4 定义 `pub struct DbSchema { dialect: DbDialect, tables: Vec<DbTable> }`（DB schema 描述）
- [ ] M2-T1.5 定义 `pub struct DbTable { name, columns: Vec<DbColumn>, constraints: Vec<DbConstraint>, indexes: Vec<DbIndex> }`
- [ ] M2-T1.6 定义 `pub struct DbColumn { name, data_type, nullable, default, primary_key, unique }`
- [ ] M2-T1.7 定义 `pub struct DbConstraint { constraint_type: ConstraintType, columns, references }`（PK/UK/FK/CHECK）
- [ ] M2-T1.8 定义 `pub struct DbIndex { name, columns, unique }`
- [ ] M2-T1.9 定义 `pub struct CrudApiEndpoint { method: HttpMethod, path, parameters, request_body, responses }`（CRUD API 端点定义）
- [ ] M2-T1.10 单元测试：DbSchema/DbTable/DbColumn 结构构造与序列化
- [ ] M2-T1.11 验证 `cargo check -p sz-orm-swagger --features openapi-reverse` 编译通过

**验收标准**：feature gate 扩展；DbSchema 数据模型 + CrudApiEndpoint 定义完整；复用既有 `ReverseGenConfig`/`NamingConvention`

**依赖**：M0-T2

## M2-T2：DbSchemaReader 五方言 schema 读取

**任务描述**：实现 `DbSchemaReader` 五方言 schema 读取器，从数据库实际 schema 读取表/列/约束/索引元信息，查询五方言 information_schema（MySQL information_schema / PG pg_catalog / SQLite sqlite_master / Oracle ALL_TAB_COLUMNS / MSSQL INFORMATION_SCHEMA），参数化绑定，不支持的特性降级跳过。

**涉及文件**：
- `packages/sz-orm-swagger/src/reverse/db_schema.rs`（扩展：五方言 schema 读取器核心）

**复用标注**：既有 `OpenApiInjectionGuard` `packages/sz-orm-swagger/src/reverse/mod.rs:26`（注入防护复用）；既有 `ReverseGenError::SpecParseFailed` `:38` / `InjectionDetected` `:50`；sz-orm-core 连接池 + sz-orm-sqlx 驱动

**子任务**：
- [ ] M2-T2.1 定义 `pub struct DbSchemaReader { conn: Arc<Pool> }`（五方言 schema 读取器，复用 sz-orm-core 连接池）
- [ ] M2-T2.2 实现 `impl DbSchemaReader { pub fn new(conn: Arc<Pool>) -> Self }`
- [ ] M2-T2.3 实现 `pub async fn read_schema(&self, dialect: DbDialect) -> Result<DbSchema, ReverseGenError>`：按方言路由到对应 information_schema 查询
- [ ] M2-T2.4 实现 MySQL schema 读取：查询 `information_schema.columns`/`information_schema.tables`（参数化绑定）
- [ ] M2-T2.5 实现 PostgreSQL schema 读取：查询 `pg_catalog.pg_tables`/`pg_catalog.pg_columns`（参数化绑定）
- [ ] M2-T2.6 实现 SQLite schema 读取：查询 `sqlite_master`/`PRAGMA table_info`（参数化绑定）
- [ ] M2-T2.7 实现 Oracle schema 读取：查询 `ALL_TAB_COLUMNS`/`ALL_CONSTRAINTS`（参数化绑定）
- [ ] M2-T2.8 实现 MSSQL schema 读取：查询 `INFORMATION_SCHEMA.COLUMNS`/`INFORMATION_SCHEMA.TABLES`（参数化绑定）
- [ ] M2-T2.9 实现注入防护：复用 `OpenApiInjectionGuard` `mod.rs:26` 检测 schema 含注入字符，返回 `InjectionDetected` `:50`
- [ ] M2-T2.10 实现不支持的方言特性降级：跳过该特性，记录日志"dialect feature X not supported on dialect Y, skipped"
- [ ] M2-T2.11 单元测试：MySQL + users 表(id BIGINT PK, email VARCHAR(255) UNIQUE, created_at TIMESTAMP) → DbSchemaReader 读取 users 表，含列 id/email/created_at + 约束 PK/UNIQUE
- [ ] M2-T2.12 单元测试：PostgreSQL + orders 表 → 通过 pg_catalog 读取
- [ ] M2-T2.13 单元测试：SQLite + products 表 → 通过 sqlite_master 读取
- [ ] M2-T2.14 边界测试：数据库连接失败 → 返回 `ReverseGenError::SpecParseFailed` `:38`
- [ ] M2-T2.15 边界测试：DB schema 含注入字符 → 返回 `InjectionDetected`
- [ ] M2-T2.16 边界测试：方言不支持某约束类型 → 降级跳过，标注"constraint type X not supported on dialect Y"
- [ ] M2-T2.17 性能测试：100 表 × 50 字段 schema 读取 ≤ 5 秒

**验收标准**：五方言 schema 读取正确；参数化绑定；注入防护；不支持的特性降级；复用 `OpenApiInjectionGuard`；性能 ≤ 5 秒

**依赖**：M2-T1

## M2-T3：DbSchemaToOpenApiMapper + DbSchemaToCrudApiMapper 映射

**任务描述**：实现 `DbSchemaToOpenApiMapper` DB schema → OpenAPI 3.0 规范映射器 + `DbSchemaToCrudApiMapper` DB schema → CRUD API 映射器，复用既有 `NamingConvention`/`to_pascal_case`/`to_snake_case`。

**涉及文件**：
- `packages/sz-orm-swagger/src/reverse/db_schema.rs`（扩展：DB→OpenAPI/CRUD 映射核心）

**复用标注**：既有 `NamingConvention`/`to_pascal_case`/`to_snake_case` `packages/sz-orm-swagger/src/reverse/mod.rs:24`/`:63`/`:80`；既有 `ReverseGenError::UnsupportedSchemaConstruct` `:42`

**子任务**：
- [ ] M2-T3.1 定义 `pub struct DbSchemaToOpenApiMapper { config: ReverseGenConfig }`（复用既有 `ReverseGenConfig` `mod.rs:24`）
- [ ] M2-T3.2 实现 `impl DbSchemaToOpenApiMapper { pub fn new(config: ReverseGenConfig) -> Self }`
- [ ] M2-T3.3 实现 `pub fn map(&self, schema: &DbSchema) -> Result<OpenApiSpec, ReverseGenError>`：表 → components.schemas / 列 → 字段 / 主键 → required / 唯一约束 → uniqueItems / 外键 → 关联 / 表 → paths CRUD 端点
- [ ] M2-T3.4 实现表 → Schema 映射：表名 → PascalCase Schema 名（复用 `to_pascal_case` `:63`）
- [ ] M2-T3.5 实现列 → 字段映射：DB 数据类型 → OpenAPI 类型（BIGINT → integer, VARCHAR → string, TIMESTAMP → string format date-time）
- [ ] M2-T3.6 实现约束映射：主键 → required + uniqueItems；唯一约束 → uniqueItems；外键 → $ref 关联
- [ ] M2-T3.7 定义 `pub struct DbSchemaToCrudApiMapper { config: ReverseGenConfig }`
- [ ] M2-T3.8 实现 `pub fn map(&self, schema: &DbSchema) -> Result<Vec<CrudApiEndpoint>, ReverseGenError>`：为每张表生成 5 个 CRUD REST 端点
- [ ] M2-T3.9 实现 5 端点生成：GET /resource 列表 / GET /resource/{id} 详情 / POST /resource 创建 / PUT /resource/{id} 更新 / DELETE /resource/{id} 删除
- [ ] M2-T3.10 实现命名约定可配置：表名 → 资源名按 `NamingConvention` 转换，默认 snake_case 表名 → PascalCase 资源名
- [ ] M2-T3.11 单元测试：users 表(id BIGINT PK, email VARCHAR(255) UNIQUE) → OpenAPI spec 含 components.schemas.User{id: integer, email: string, required: [id], uniqueItems: email} + paths /users CRUD 端点
- [ ] M2-T3.12 单元测试：users 表 + 生成 CRUD API → 生成 5 个端点：GET /users / GET /users/{id} / POST /users / PUT /users/{id} / DELETE /users/{id}，每个端点含 OpenAPI 文档
- [ ] M2-T3.13 单元测试：表名 user_orders + 默认命名约定 → 资源名 UserOrders / OpenAPI schema 名 UserOrders
- [ ] M2-T3.14 边界测试：不支持的 schema 构造 → 返回 `UnsupportedSchemaConstruct` `:42`
- [ ] M2-T3.15 性能测试：100 表 × 50 字段映射 ≤ 5 秒

**验收标准**：DbSchemaToOpenApiMapper/DbSchemaToCrudApiMapper 映射正确；5 端点/表；命名约定可配置；复用既有 `NamingConvention`/`to_pascal_case`；性能 ≤ 5 秒

**依赖**：M2-T1

## M2-T4：FullReverseLoopVerifier 完整闭环验证

**任务描述**：实现 `FullReverseLoopVerifier` 完整闭环验证器，验证 DB schema → OpenAPI → ORM Model → CRUD 闭环一致性，复用既有 `ApiFirstLoopVerifier` + `OpenApiReverseGenerator`。

**涉及文件**：
- `packages/sz-orm-swagger/src/reverse/db_schema.rs`（扩展：完整闭环验证核心）

**复用标注**：既有 `ApiFirstLoopVerifier`/`LoopReport` `packages/sz-orm-swagger/src/reverse/mod.rs:27`；既有 `OpenApiReverseGenerator` `:25`；既有 `ReverseGenError::LoopVerificationDiff` `:46`

**子任务**：
- [ ] M2-T4.1 定义 `pub struct FullReverseLoopVerifier { config: ReverseGenConfig }`
- [ ] M2-T4.2 实现 `impl FullReverseLoopVerifier { pub fn new(config: ReverseGenConfig) -> Self }`
- [ ] M2-T4.3 实现 `pub fn verify(&self, schema: &DbSchema) -> Result<LoopReport, ReverseGenError>`：验证 DB→OpenAPI→ORM→CRUD=闭环一致性
- [ ] M2-T4.4 实现闭环路径：DB schema → `DbSchemaToOpenApiMapper` 生成 OpenAPI spec → 既有 `OpenApiReverseGenerator` `mod.rs:25` 反向生成 ORM Model → 生成 CRUD
- [ ] M2-T4.5 实现闭环比对：直接 DB→CRUD（`DbSchemaToCrudApiMapper`）vs DB→OpenAPI→ORM→CRUD，比对一致性
- [ ] M2-T4.6 实现差异处理：闭环差异返回 `ReverseGenError::LoopVerificationDiff` `:46`，附差异详情
- [ ] M2-T4.7 复用既有 `ApiFirstLoopVerifier` `mod.rs:27` 验证 OpenAPI→ORM→API 闭环
- [ ] M2-T4.8 实现反向生成日志：记录 schema 来源 + 表/列数 + 生成项 + 闭环验证结果 + 耗时
- [ ] M2-T4.9 单元测试：DB users 表 → OpenAPI spec → ORM Model → CRUD API → 闭环验证通过，直接 DB→CRUD 与 DB→OpenAPI→ORM→CRUD 结果一致
- [ ] M2-T4.10 单元测试：闭环验证差异 → 返回 `LoopVerificationDiff`，附差异详情
- [ ] M2-T4.11 单元测试：DB 100 表反向生成 → 日志记录 source=db + tables=100 + generated=openapi+crud + loop=pass + latency
- [ ] M2-T4.12 边界测试：用户手写逻辑保护 → 只更新骨架，保留用户可编辑区域，返回 `UserLogicOverwrite` `:58` 提示

**验收标准**：FullReverseLoopVerifier 闭环验证正确；复用既有 `ApiFirstLoopVerifier` + `OpenApiReverseGenerator`；差异附详情；日志可追溯

**依赖**：M2-T2、M2-T3

## M2-T5：M2 集成测试与门禁验证

**任务描述**：M2 里程碑集成测试与门禁验证，确保 REQ-V48-003 全部 10 条验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M2-T5.1 集成测试：`DbSchemaReader::read_schema` 完整流程（五方言 schema 读取 + 注入防护）
- [ ] M2-T5.2 集成测试：`DbSchemaToOpenApiMapper::map` + `DbSchemaToCrudApiMapper::map` 完整流程（DB→OpenAPI + DB→CRUD + 命名约定）
- [ ] M2-T5.3 集成测试：`FullReverseLoopVerifier::verify` 完整流程（DB→OpenAPI→ORM→CRUD 闭环验证）
- [ ] M2-T5.4 集成测试：复用既有 `OpenApiReverseGenerator` `mod.rs:25` + `SchemaToModelMapper` `:29` + `ApiFirstLoopVerifier` `:27` + `OpenApiInjectionGuard` `:26`，不新建 OpenAPI→ORM 逻辑
- [ ] M2-T5.5 集成测试：五方言覆盖（MySQL/PostgreSQL/SQLite/Oracle/MSSQL schema 读取）
- [ ] M2-T5.6 运行 `cargo test -p sz-orm-swagger --features openapi-reverse`（全部通过）
- [ ] M2-T5.7 `cargo clippy -p sz-orm-swagger --features openapi-reverse -- -D warnings`
- [ ] M2-T5.8 `cargo fmt -p sz-orm-swagger -- --check`
- [ ] M2-T5.9 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-swagger/src/reverse/db_schema.rs` 无占位实现
- [ ] M2-T5.10 扫描 `grep -rn 'unsafe' packages/sz-orm-swagger/src/reverse/db_schema.rs` 无 unsafe 块
- [ ] M2-T5.11 验证默认 feature 行为与 v4.7.0 一致（`cargo build -p sz-orm-swagger` 无 DB schema 反向生成）
- [ ] M2-T5.12 验证 `openapi-reverse` 与既有 feature 组合编译通过

**验收标准**：M2 集成测试通过；门禁通过；默认行为不变；五方言 schema 读取 + DB→OpenAPI/CRUD 映射 + 闭环验证全部验证

**依赖**：M2-T1、M2-T2、M2-T3、M2-T4

---


# 五、M3：WASM 真实数据库连接闭环（REQ-V48-004，P1，3 天）

**目标**：扩展既有 `sz-orm-wasm` 包 `wasm-real-db` feature，新增 `WasmProxyServer` 代理后端服务 + `MultiDialectProxyBackend` 多方言代理后端 + `WasmOrmSession` WASM 端 ORM 会话 + `WasmQueryBuilderBridge` WASM 端查询构建器桥接 + `WasmOrmLoopVerifier` WASM ORM 闭环验证器，复用既有 `WasmRealDbConnection`/`WasmRealDbQueryExecutor`/`WasmDbProxy`/`WasmDbProxyProtocol`/`WasmDbAuthValidator`/`WasmDbSqlWhitelist`/`WasmDbRateLimiter`/`WasmRealDbReconnector`/`WasmRealDbMetrics`/`WasiSocketConnection` + sz-orm-core 连接池 + sz-orm-query-builder，补齐浏览器端 ORM 操作完整闭环 + 代理后端实现 + 多方言代理 + WASM 端查询构建器集成。
**对应需求**：REQ-V48-004（spec.md §5.4，design.md §2.1.3.4 + §2.2.2 REQ-V48-004 接口）
**预期工作量**：3 天
**依赖**：无（M3 为 P1 独立需求，复用既有 sz-orm-wasm real_db 代理桥接 + sz-orm-core Pool + sz-orm-query-builder，扩展包可与 M1、M2、M4 并行）

## M3-T1：wasm-real-db feature 扩展 + 代理后端配置

**任务描述**：在 `sz-orm-wasm` 完善 `wasm-real-db` feature gate 隔离（M0-T2 已扩展依赖），定义代理后端配置结构 + 模块骨架，作为代理后端与 ORM 闭环的基础。

**涉及文件**：
- `packages/sz-orm-wasm/Cargo.toml`（完善：`wasm-real-db` feature 新增 `sz-orm-core`/`sz-orm-query-builder` 依赖）
- `packages/sz-orm-wasm/src/real_db/mod.rs`（扩展：模块声明 `mod proxy_server;` `mod orm_session;`，`#[cfg(feature = "wasm-real-db")]` 门控）
- `packages/sz-orm-wasm/src/real_db/proxy_server.rs`（新建，代理后端配置 + 多方言代理骨架）
- `packages/sz-orm-wasm/src/real_db/orm_session.rs`（新建，WASM ORM 会话骨架）

**复用标注**：既有 `WasmDbAuthValidator` `packages/sz-orm-wasm/src/real_db/mod.rs:18`；既有 `WasmDbSqlWhitelist` `:28`；既有 `WasmDbRateLimiter` `:26`；既有 `ProxyRequest`/`ProxyResponse`/`WasmDbProxyProtocol` `:22`/`:23`；既有 `WasmRealDbError` `:34`；既有 `WasmRealDbMetrics` `:21`；既有 `WasmRealDbReconnector` `:27`；sz-orm-core `Pool` `packages/sz-orm-core/src/pool.rs:749`

**子任务**：
- [ ] M3-T1.1 `packages/sz-orm-wasm/Cargo.toml` 确认 `wasm-real-db` feature 新增 `sz-orm-core`/`sz-orm-query-builder` 依赖
- [ ] M3-T1.2 `src/real_db/mod.rs` 声明 `mod proxy_server;` `mod orm_session;`，`#[cfg(feature = "wasm-real-db")]` 门控
- [ ] M3-T1.3 `src/real_db/proxy_server.rs` 定义 `pub struct ProxyServerConfig { max_result_size, auth_config, whitelist_config, rate_limit_config, dialect_configs }`（代理后端配置，默认 max_result_size 10MB）
- [ ] M3-T1.4 定义 `pub struct AuthConfig { enabled, token_validation_fn }`（鉴权配置）
- [ ] M3-T1.5 定义 `pub struct WhitelistConfig { enabled, allowed_patterns }`（SQL 白名单配置）
- [ ] M3-T1.6 定义 `pub struct RateLimitConfig { enabled, max_qps, burst_size }`（限流配置）
- [ ] M3-T1.7 定义 `pub struct DialectProxyConfig { dialect: DbDialect, connection_string, pool_size }`（方言代理配置）
- [ ] M3-T1.8 实现链式配置方法：`with_max_result_size` / `with_auth` / `with_whitelist` / `with_rate_limit`
- [ ] M3-T1.9 单元测试：`ProxyServerConfig::new()` 默认值正确（max_result_size 10MB）
- [ ] M3-T1.10 验证 `cargo check -p sz-orm-wasm --features wasm-real-db` 编译通过

**验收标准**：feature gate 扩展；ProxyServerConfig/AuthConfig/WhitelistConfig/RateLimitConfig/DialectProxyConfig 定义完整；复用既有 `WasmDbAuthValidator`/`WasmDbSqlWhitelist`/`WasmDbRateLimiter`

**依赖**：M0-T2

## M3-T2：WasmProxyServer 代理后端服务

**任务描述**：实现 `WasmProxyServer` 代理后端服务，接收 WASM 端代理请求 → 鉴权 + SQL 白名单检查 + 限流 → 连接真实 DB（复用 sz-orm-core 连接池）→ 执行查询 → 检查结果集大小 → 返回结果，后端凭据不暴露给 WASM 端。

**涉及文件**：
- `packages/sz-orm-wasm/src/real_db/proxy_server.rs`（扩展：代理后端服务核心）

**复用标注**：既有 `WasmDbAuthValidator` `packages/sz-orm-wasm/src/real_db/mod.rs:18`；既有 `WasmDbSqlWhitelist` `:28`；既有 `WasmDbRateLimiter` `:26`；既有 `ProxyRequest`/`ProxyResponse` `:22`/`:23`；既有 `WasmRealDbError::AuthFailed` `:49`/`SqlRejected` `:41`/`RateLimited` `:45`/`CredentialsNotExposed` `:53`/`ResultTooLarge` `:61`/`QueryFailed` `:57`；sz-orm-core `Pool` `packages/sz-orm-core/src/pool.rs:749`

**子任务**：
- [ ] M3-T2.1 定义 `pub struct WasmProxyServer { auth_validator: WasmDbAuthValidator, sql_whitelist: WasmDbSqlWhitelist, rate_limiter: WasmDbRateLimiter, pool: Arc<Pool>, max_result_size: usize, metrics: WasmRealDbMetrics }`
- [ ] M3-T2.2 实现 `impl WasmProxyServer { pub fn new(pool: Arc<Pool>, config: ProxyServerConfig) -> Self }`
- [ ] M3-T2.3 实现 `pub async fn handle_request(&self, request: ProxyRequest) -> Result<ProxyResponse, WasmRealDbError>`：鉴权 → 白名单 → 限流 → 连接 DB → 执行 → 检查大小 → 返回
- [ ] M3-T2.4 实现鉴权：复用 `WasmDbAuthValidator` `mod.rs:18`，未鉴权返回 `AuthFailed` `:49`
- [ ] M3-T2.5 实现 SQL 白名单：复用 `WasmDbSqlWhitelist` `:28`，非白名单返回 `SqlRejected` `:41`
- [ ] M3-T2.6 实现限流：复用 `WasmDbRateLimiter` `:26`，超限返回 `RateLimited` `:45`
- [ ] M3-T2.7 实现连接 DB：复用 sz-orm-core `Pool` `pool.rs:749` 获取连接，参数化绑定执行 SQL
- [ ] M3-T2.8 实现结果集大小检查：结果集 > `max_result_size`（默认 10MB）返回 `ResultTooLarge` `:61`
- [ ] M3-T2.9 实现凭据隔离：后端 DB 凭据由 `WasmProxyServer` 持有，不暴露给 WASM 端（`CredentialsNotExposed` `:53`）
- [ ] M3-T2.10 实现指标记录：复用 `WasmRealDbMetrics` `mod.rs:21`，记录 QPS + 延迟 + 错误率 + 白名单拒绝数 + 限流数
- [ ] M3-T2.11 单元测试：WASM 端发送 ProxyRequest(SELECT * FROM users WHERE id=$1) + 代理后端 → 鉴权通过 + 白名单通过 + 限流通过 + 连接 DB 执行 + 返回 ProxyResponse(结果)
- [ ] M3-T2.12 单元测试：WASM 端尝试读取后端凭据 → 返回 `CredentialsNotExposed`
- [ ] M3-T2.13 单元测试：未鉴权请求 → 返回 `AuthFailed`
- [ ] M3-T2.14 单元测试：非白名单 SQL DROP TABLE → 返回 `SqlRejected`
- [ ] M3-T2.15 单元测试：超限请求 → 返回 `RateLimited`
- [ ] M3-T2.16 边界测试：查询返回 100MB 结果 + 默认限制 10MB → 返回 `ResultTooLarge`
- [ ] M3-T2.17 边界测试：DB 查询执行失败 → 返回 `QueryFailed` `:57`
- [ ] M3-T2.18 性能测试：代理后端单实例吞吐量 ≥ 10000 QPS（复用 sz-orm-core 连接池）

**验收标准**：WasmProxyServer 代理后端正确；鉴权/白名单/限流安全链；凭据隔离；结果集大小限制；复用既有 `WasmDbAuthValidator`/`WasmDbSqlWhitelist`/`WasmDbRateLimiter` + sz-orm-core `Pool`；性能 ≥ 10000 QPS

**依赖**：M3-T1

## M3-T3：MultiDialectProxyBackend 多方言代理后端

**任务描述**：实现 `MultiDialectProxyBackend` 多方言代理后端，支持 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言，复用 sz-orm-core 连接池 + sz-orm-sqlx 驱动，按 `ProxyRequest.dialect` 路由到对应数据库。

**涉及文件**：
- `packages/sz-orm-wasm/src/real_db/proxy_server.rs`（扩展：多方言代理后端核心）

**复用标注**：`WasmProxyServer`（M3-T2）；sz-orm-core `Pool` `packages/sz-orm-core/src/pool.rs:749`；sz-orm-sqlx 驱动

**子任务**：
- [ ] M3-T3.1 定义 `pub struct MultiDialectProxyBackend { backends: HashMap<DbDialect, WasmProxyServer> }`（多方言代理后端，按方言路由）
- [ ] M3-T3.2 实现 `impl MultiDialectProxyBackend { pub fn new(dialect_configs: Vec<DialectProxyConfig>) -> Self }`：为每方言创建 `WasmProxyServer`
- [ ] M3-T3.3 实现 `pub async fn handle_request(&self, request: ProxyRequest) -> Result<ProxyResponse, WasmRealDbError>`：按 `request.dialect` 路由到对应 `WasmProxyServer`
- [ ] M3-T3.4 实现五方言连接池：每方言独立 `Pool`，复用 sz-orm-core 连接池 + sz-orm-sqlx 驱动
- [ ] M3-T3.5 实现方言不支持处理：未知方言返回 `WasmRealDbError::QueryFailed`，附"unsupported dialect"
- [ ] M3-T3.6 单元测试：代理后端配置 MySQL + PostgreSQL + WASM 端查询 MySQL users → 路由到 MySQL 执行
- [ ] M3-T3.7 单元测试：WASM 端查询 PostgreSQL orders → 路由到 PostgreSQL 执行
- [ ] M3-T3.8 边界测试：未知方言 → 返回错误"unsupported dialect"
- [ ] M3-T3.9 性能测试：多方言路由开销 ≤ 1ms/次

**验收标准**：MultiDialectProxyBackend 五方言路由正确；复用 sz-orm-core 连接池 + sz-orm-sqlx；性能 ≤ 1ms/次

**依赖**：M3-T2

## M3-T4：WasmQueryBuilderBridge 查询构建器桥接

**任务描述**：实现 `WasmQueryBuilderBridge` WASM 端查询构建器桥接，复用 sz-orm-query-builder 构建参数化查询（`where_eq`/`or_where_eq` 等），将查询构建器输出转换为代理协议 `ProxyRequest`，禁止 SQL 字符串拼接。

**涉及文件**：
- `packages/sz-orm-wasm/src/real_db/orm_session.rs`（扩展：查询构建器桥接核心）

**复用标注**：sz-orm-query-builder（`where_eq`/`or_where_eq` 等参数化 API）；既有 `ProxyRequest` `packages/sz-orm-wasm/src/real_db/mod.rs:22`/`:23`；既有 `WasmRealDbError::SerializationError` `:65`

**子任务**：
- [ ] M3-T4.1 定义 `pub struct WasmQueryBuilderBridge { dialect: DbDialect, auth_token: String, result_format: SerializationFormat }`
- [ ] M3-T4.2 实现 `impl WasmQueryBuilderBridge { pub fn new(dialect: DbDialect, auth_token: String) -> Self }`
- [ ] M3-T4.3 实现 `pub fn build(&self, builder: QueryBuilder) -> Result<ProxyRequest, WasmRealDbError>`：将 sz-orm-query-builder 输出转换为 `ProxyRequest`
- [ ] M3-T4.4 实现参数化 SQL 提取：从 `QueryBuilder` 提取参数化 SQL + params，禁止 SQL 字符串拼接
- [ ] M3-T4.5 实现 `ProxyRequest` 构造：sql + params + dialect + auth_token + result_format
- [ ] M3-T4.6 单元测试：WASM 端 QueryBuilder.select("users").where_eq("id", 1) + 桥接 → 生成参数化 ProxyRequest(SQL="SELECT * FROM users WHERE id=$1", params=[1])
- [ ] M3-T4.7 单元测试：QueryBuilder.select("orders").where_eq("user_id", 42).or_where_eq("status", "pending") → 生成参数化 ProxyRequest
- [ ] M3-T4.8 边界测试：QueryBuilder 为空 → 返回 `SerializationError`
- [ ] M3-T4.9 验证禁止 SQL 拼接：扫描 `grep -rn 'format!' packages/sz-orm-wasm/src/real_db/orm_session.rs` 无 SQL 字符串拼接

**验收标准**：WasmQueryBuilderBridge 桥接正确；参数化查询；禁止 SQL 拼接；复用 sz-orm-query-builder

**依赖**：M3-T1

## M3-T5：WasmOrmSession ORM 会话 + 闭环验证

**任务描述**：实现 `WasmOrmSession` WASM 端 ORM 会话（查询构建 + 代理执行 + 结果反序列化）+ `WasmOrmLoopVerifier` WASM ORM 闭环验证器，复用既有 `WasmRealDbConnection`/`WasmRealDbQueryExecutor`/`WasmDbProxy`。

**涉及文件**：
- `packages/sz-orm-wasm/src/real_db/orm_session.rs`（扩展：ORM 会话 + 闭环验证核心）

**复用标注**：既有 `WasmRealDbConnection` `packages/sz-orm-wasm/src/real_db/mod.rs:19`；既有 `WasmRealDbQueryExecutor` `:20`；既有 `WasmDbProxy` `:25`；既有 `WasmRealDbReconnector` `:27`；既有 `WasiSocketConnection` `packages/sz-orm-wasm/src/real_db/wasi_socket.rs:13`；`WasmQueryBuilderBridge`（M3-T4）

**子任务**：
- [ ] M3-T5.1 定义 `pub struct WasmOrmSession { proxy: WasmDbProxy, query_bridge: WasmQueryBuilderBridge }`
- [ ] M3-T5.2 实现 `impl WasmOrmSession { pub fn new(proxy: WasmDbProxy) -> Self }`
- [ ] M3-T5.3 实现 `pub async fn query<T: DeserializeOwned>(&self, builder: QueryBuilder) -> Result<T, WasmRealDbError>`：查询构建 → 代理执行 → 结果反序列化
- [ ] M3-T5.4 实现查询构建：`WasmQueryBuilderBridge::build` 构建参数化 `ProxyRequest`
- [ ] M3-T5.5 实现代理执行：`WasmDbProxy` `mod.rs:25` 发送 `ProxyRequest` 到代理后端
- [ ] M3-T5.6 实现结果反序列化：`ProxyResponse.data` → `T`（serde 反序列化）
- [ ] M3-T5.7 实现代理重连：复用 `WasmRealDbReconnector` `:27`，代理连接断开自动重连
- [ ] M3-T5.8 定义 `pub struct WasmOrmLoopVerifier { session: WasmOrmSession, direct_conn: Arc<Pool> }`
- [ ] M3-T5.9 实现 `pub async fn verify<T: DeserializeOwned>(&self, builder: QueryBuilder) -> Result<LoopReport, WasmRealDbError>`：比对 WASM 端结果与直接 DB 查询结果
- [ ] M3-T5.10 单元测试：WASM 端 WasmOrmSession + 查询 User where id=1 → 构建参数化查询 → 代理执行 → 反序列化为 User 结构
- [ ] M3-T5.11 单元测试：WASM 端查询 users where id=1 + 代理后端 MySQL → 闭环验证通过，WASM 端结果与直接 MySQL 查询结果一致
- [ ] M3-T5.12 单元测试：代理连接断开 → 自动重连
- [ ] M3-T5.13 边界测试：代理后端不可用 → 返回 `ProxyUnavailable` `:37`
- [ ] M3-T5.14 边界测试：序列化失败 → 返回 `SerializationError` `:65`
- [ ] M3-T5.15 性能测试：WASM 端 ORM 查询端到端延迟 ≤ 200ms（不含 DB 执行时间）

**验收标准**：WasmOrmSession ORM 会话正确；闭环验证一致；代理重连；复用既有 `WasmRealDbConnection`/`WasmRealDbQueryExecutor`/`WasmDbProxy`/`WasmRealDbReconnector`；性能 ≤ 200ms

**依赖**：M3-T2、M3-T4

## M3-T6：M3 集成测试与门禁验证

**任务描述**：M3 里程碑集成测试与门禁验证，确保 REQ-V48-004 全部 12 条验收条件满足，默认 feature 行为不变。

**子任务**：
- [ ] M3-T6.1 集成测试：`WasmProxyServer::handle_request` 完整流程（鉴权 + 白名单 + 限流 + 连接 DB + 结果集检查 + 返回）
- [ ] M3-T6.2 集成测试：`MultiDialectProxyBackend` 完整流程（五方言路由）
- [ ] M3-T6.3 集成测试：`WasmOrmSession::query` + `WasmQueryBuilderBridge` 完整流程（查询构建 → 代理执行 → 反序列化）
- [ ] M3-T6.4 集成测试：`WasmOrmLoopVerifier::verify` 完整流程（WASM 端结果与直接 DB 查询一致）
- [ ] M3-T6.5 集成测试：复用既有 `WasmDbProxy` `mod.rs:25` + `WasmRealDbConnection` `:19` + `WasmDbAuthValidator` `:18` + `WasmDbSqlWhitelist` `:28` + `WasmDbRateLimiter` `:26` + sz-orm-core `Pool` `pool.rs:749` + sz-orm-query-builder，不新建代理桥接逻辑
- [ ] M3-T6.6 集成测试：WASI socket 直连代理（feature = "wasi-socket"，`WasiSocketConnection` `wasi_socket.rs:13`）
- [ ] M3-T6.7 运行 `cargo test -p sz-orm-wasm --features wasm-real-db`（全部通过）
- [ ] M3-T6.8 `cargo clippy -p sz-orm-wasm --features wasm-real-db -- -D warnings`
- [ ] M3-T6.9 `cargo fmt -p sz-orm-wasm -- --check`
- [ ] M3-T6.10 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-wasm/src/real_db/proxy_server.rs packages/sz-orm-wasm/src/real_db/orm_session.rs` 无占位实现
- [ ] M3-T6.11 扫描 `grep -rn 'unsafe' packages/sz-orm-wasm/src/real_db/proxy_server.rs packages/sz-orm-wasm/src/real_db/orm_session.rs` 无 unsafe 块
- [ ] M3-T6.12 验证默认 feature 行为与 v4.7.0 一致（`cargo build -p sz-orm-wasm` 无代理后端与 ORM 闭环）
- [ ] M3-T6.13 验证 `wasm-real-db` 与既有 `wasi-socket` feature 组合编译通过

**验收标准**：M3 集成测试通过；门禁通过；默认行为不变；代理后端 + 多方言代理 + ORM 会话 + 查询桥接 + 闭环验证全部验证

**依赖**：M3-T1、M3-T2、M3-T3、M3-T4、M3-T5

---

# 六、M4：低代码双向同步（REQ-V48-002，P1，2.5 天）

**目标**：扩展既有 `sz-orm-lc` + `sz-orm-designer` 包，新增 `BidirectionalSyncEngine` 双向同步引擎 + `SyncDirection` 同步方向枚举 + `SyncConflictDetector` 冲突检测器 + `SyncConflictResolver` 冲突解决器 + `ConflictResolutionStrategy` 冲突解决策略枚举 + `SyncIncrementTracker` 增量追踪器 + `SyncAuditLogger` 同步审计日志器，复用既有 `ModelDefinition`/`FieldDef`/`RelationDefinition`/`FieldTypeMapping`/`ValidationRule` + `SchemaDesigner`/`code_gen`/`code_parse`/`design_ir`，补齐 ORM 模型 ↔ 低代码引擎模型双向同步 + 冲突检测与解决 + 增量追踪。
**对应需求**：REQ-V48-002（spec.md §5.2，design.md §2.1.3.2 + §2.2.2 REQ-V48-002 接口）
**预期工作量**：2.5 天
**依赖**：无（M4 为 P1 独立需求，复用既有 sz-orm-lc ModelDefinition/FieldTypeMapping + sz-orm-designer SchemaDesigner，扩展包可与 M1、M2、M3 并行）

## M4-T1：lc-bidirectional-sync feature gate 新增 + 配置

**任务描述**：在 `sz-orm-lc` 新增 `lc-bidirectional-sync` feature gate 隔离（M0-T2 已创建占位），定义 `SyncDirection`/`ConflictResolutionStrategy`/`ChangeType`/`ConflictType` 枚举 + `SyncConfig` 配置，作为双向同步的数据模型。

**涉及文件**：
- `packages/sz-orm-lc/Cargo.toml`（完善：`lc-bidirectional-sync` feature 依赖 `sz-orm-designer/schema-designer`）
- `packages/sz-orm-lc/src/lib.rs`（扩展：模块声明 `mod bidirectional_sync;`，`#[cfg(feature = "lc-bidirectional-sync")]` 门控）
- `packages/sz-orm-lc/src/bidirectional_sync.rs`（新建，双向同步配置 + 枚举）

**复用标注**：既有 `ModelDefinition` `packages/sz-orm-lc/src/lib.rs:24`；既有 `FieldDef` `:83`；既有 `RelationDefinition` `:147`；既有 `FieldTypeMapping` `:210`；既有 `ValidationRule` `:362`；sz-orm-designer `schema-designer` feature `packages/sz-orm-designer/src/lib.rs:1`

**feature gate 隔离**：`lc-bidirectional-sync`（新增 feature，默认关闭）

**子任务**：
- [ ] M4-T1.1 `packages/sz-orm-lc/Cargo.toml` 确认 `lc-bidirectional-sync` feature（M0-T2 已创建），依赖 `sz-orm-designer/schema-designer`
- [ ] M4-T1.2 `src/lib.rs` 声明 `mod bidirectional_sync;`，`#[cfg(feature = "lc-bidirectional-sync")]` 门控
- [ ] M4-T1.3 `src/bidirectional_sync.rs` 定义 `pub enum SyncDirection { OrmToLc, LcToOrm, Bidirectional }`（同步方向枚举）
- [ ] M4-T1.4 定义 `pub enum ConflictResolutionStrategy { OrmWins, LcWins, Merge, Manual }`（冲突解决策略，默认 Manual）
- [ ] M4-T1.5 定义 `pub enum ChangeType { FieldAdded, FieldRemoved, TypeChanged, ConstraintChanged, RelationChanged }`（变更类型）
- [ ] M4-T1.6 定义 `pub enum ConflictType { TypeMismatch, ConstraintMismatch, RelationMismatch, BidirectionalChange }`（冲突类型）
- [ ] M4-T1.7 定义 `pub struct SyncConfig { default_strategy: ConflictResolutionStrategy, enable_increment_tracking: bool, audit_log_path: Option<String> }`（同步配置）
- [ ] M4-T1.8 实现 `impl SyncConfig { pub fn new() -> Self }`（默认：Manual, true, None）
- [ ] M4-T1.9 实现链式配置方法：`with_default_strategy` / `with_increment_tracking` / `with_audit_log`
- [ ] M4-T1.10 单元测试：`SyncConfig::new()` 默认值正确（Manual, true, None）
- [ ] M4-T1.11 验证 `cargo check -p sz-orm-lc --features lc-bidirectional-sync` 编译通过

**验收标准**：feature gate 定义；SyncDirection/ConflictResolutionStrategy/ChangeType/ConflictType/SyncConfig 定义完整；复用既有 `ModelDefinition`/`FieldTypeMapping`

**依赖**：M0-T2

## M4-T2：BidirectionalSyncEngine 双向同步引擎

**任务描述**：实现 `BidirectionalSyncEngine` 双向同步引擎，按 `SyncDirection` 双向同步 ORM 模型 ↔ 低代码引擎模型，复用既有 `ModelDefinition`/`FieldTypeMapping`/`ValidationRule` + `SchemaDesigner`/`code_gen`/`code_parse`，类型映射双向一致，验证规则同步。

**涉及文件**：
- `packages/sz-orm-lc/src/bidirectional_sync.rs`（扩展：双向同步引擎核心）

**复用标注**：既有 `ModelDefinition` `packages/sz-orm-lc/src/lib.rs:24`；既有 `FieldDef` `:83`；既有 `FieldTypeMapping` `:210`（`sql_to_rust`/`sql_to_html_input`/`sql_to_json_schema`/`rust_to_sql` 四向映射）；既有 `ValidationRule` `:362`；sz-orm-designer `SchemaDesigner`/`code_gen`/`code_parse` `packages/sz-orm-designer/src/lib.rs:1`/`:2`/`:4`

**子任务**：
- [ ] M4-T2.1 定义 `pub struct BidirectionalSyncEngine { config: SyncConfig, conflict_detector: SyncConflictDetector, conflict_resolver: SyncConflictResolver, increment_tracker: SyncIncrementTracker, audit_logger: SyncAuditLogger }`
- [ ] M4-T2.2 实现 `impl BidirectionalSyncEngine { pub fn new(config: SyncConfig) -> Self }`
- [ ] M4-T2.3 实现 `pub fn sync(&mut self, direction: SyncDirection, orm_model: &ModelDefinition, lc_model: &ModelDefinition) -> Result<SyncResult, SyncError>`：按方向双向同步
- [ ] M4-T2.4 实现 OrmToLc 同步：ORM 模型 → 低代码引擎模型，字段/类型/约束/关联同步
- [ ] M4-T2.5 实现 LcToOrm 同步：低代码引擎模型 → ORM 模型
- [ ] M4-T2.6 实现 Bidirectional 同步：双向同时同步，冲突检测 + 解决
- [ ] M4-T2.7 实现类型映射双向一致：复用 `FieldTypeMapping` `lib.rs:210` 四向映射，ORM Rust 类型 → 低代码 SQL 类型 → 低代码 HTML input 类型一致
- [ ] M4-T2.8 实现验证规则同步：复用 `ValidationRule` `lib.rs:362`，ORM 约束（非空/唯一/长度）映射为低代码验证规则（Required/Unique/MaxLength）
- [ ] M4-T2.9 复用 sz-orm-designer `SchemaDesigner`/`code_gen`/`code_parse` `designer/lib.rs:1`/`:2`/`:4` 进行模型代码生成与解析
- [ ] M4-T2.10 单元测试：ORM 模型 User{name,id,email} + OrmToLc 同步 → 低代码引擎模型同步为 User{name,id,email}，字段/类型/约束一致
- [ ] M4-T2.11 单元测试：低代码引擎模型 Order{order_id,total} + LcToOrm 同步 → ORM 模型同步为 Order{order_id,total}
- [ ] M4-T2.12 单元测试：ORM 字段 i64 + OrmToLc 同步 → 低代码字段 BIGINT + HTML input number，`FieldTypeMapping::rust_to_sql("i64")` = "BIGINT"，`sql_to_html_input("BIGINT")` = "number"
- [ ] M4-T2.13 单元测试：ORM 字段 email 非空唯一 + OrmToLc 同步 → 低代码验证规则 Required + Unique
- [ ] M4-T2.14 边界测试：`FieldTypeMapping` 不支持的类型 → 跳过该字段，记录日志"type mapping not supported, skipped"
- [ ] M4-T2.15 性能测试：单次全量同步 100 表 × 50 字段 ≤ 5 秒

**验收标准**：BidirectionalSyncEngine 双向同步正确；类型映射双向一致；验证规则同步；复用既有 `ModelDefinition`/`FieldTypeMapping`/`ValidationRule` + `SchemaDesigner`；性能 ≤ 5 秒

**依赖**：M4-T1

## M4-T3：SyncConflictDetector + SyncConflictResolver 冲突检测解决

**任务描述**：实现 `SyncConflictDetector` 冲突检测器 + `SyncConflictResolver` 冲突解决器，检测双向同步冲突（同字段双向变更/类型不一致/约束不一致/关联不一致），按 `ConflictResolutionStrategy` 解决冲突，破坏性变更须人工确认。

**涉及文件**：
- `packages/sz-orm-lc/src/bidirectional_sync.rs`（扩展：冲突检测解决核心）

**复用标注**：既有 `ModelDefinition` `packages/sz-orm-lc/src/lib.rs:24`；既有 `FieldDef` `:83`

**子任务**：
- [ ] M4-T3.1 定义 `pub struct SyncConflict { field: String, conflict_type: ConflictType, orm_value: String, lc_value: String }`
- [ ] M4-T3.2 定义 `pub struct SyncConflictDetector`（冲突检测器）
- [ ] M4-T3.3 实现 `impl SyncConflictDetector { pub fn new() -> Self }`
- [ ] M4-T3.4 实现 `pub fn detect(&self, orm_model: &ModelDefinition, lc_model: &ModelDefinition) -> Vec<SyncConflict>`：检测双向同步冲突
- [ ] M4-T3.5 实现类型冲突检测：同字段 ORM 类型 ≠ 低代码类型 → `ConflictType::TypeMismatch`
- [ ] M4-T3.6 实现约束冲突检测：同字段 ORM 约束 ≠ 低代码约束 → `ConstraintMismatch`
- [ ] M4-T3.7 实现关联冲突检测：同关联 ORM 关联 ≠ 低代码关联 → `RelationMismatch`
- [ ] M4-T3.8 实现双向变更检测：同字段 ORM 与低代码同时变更 → `BidirectionalChange`
- [ ] M4-T3.9 定义 `pub struct SyncConflictResolver { strategy: ConflictResolutionStrategy }`
- [ ] M4-T3.10 实现 `impl SyncConflictResolver { pub fn new(strategy: ConflictResolutionStrategy) -> Self }`
- [ ] M4-T3.11 实现 `pub fn resolve(&self, conflicts: Vec<SyncConflict>, strategy: ConflictResolutionStrategy) -> Result<ResolutionResult, SyncError>`：按策略解决冲突
- [ ] M4-T3.12 实现 OrmWins 策略：采用 ORM 版本
- [ ] M4-T3.13 实现 LcWins 策略：采用低代码版本
- [ ] M4-T3.14 实现 Merge 策略：合并变更
- [ ] M4-T3.15 实现 Manual 策略：检测破坏性变更（删列/改类型/改约束），破坏性变更暂停同步提示人工确认；非破坏性变更提示冲突等待人工确认
- [ ] M4-T3.16 单元测试：ORM 将 User.email 改为 VARCHAR(500) + 低代码引擎将 User.email 改为 TEXT + 双向同步 → 检测到 User.email 类型冲突，生成冲突列表
- [ ] M4-T3.17 单元测试：User.email 类型冲突 + OrmWins → 采用 ORM 的 VARCHAR(500)
- [ ] M4-T3.18 单元测试：User.email 类型冲突 + LcWins → 采用低代码的 TEXT
- [ ] M4-T3.19 单元测试：User.email 类型冲突 + Manual → 暂停同步，等待人工确认
- [ ] M4-T3.20 单元测试：低代码引擎删除字段 User.age + LcToOrm 同步 + 默认 Manual → 暂停同步，提示"破坏性变更：删除字段 age，需人工确认"

**验收标准**：SyncConflictDetector/Resolver 冲突检测解决正确；四种策略；破坏性变更须人工确认；复用既有 `ModelDefinition`/`FieldDef`

**依赖**：M4-T1

## M4-T4：SyncIncrementTracker 增量追踪

**任务描述**：实现 `SyncIncrementTracker` 增量追踪器，追踪模型变更（字段增删改/类型变更/约束变更/关联变更），同步时只处理变更项而非全量重建，提升同步效率。

**涉及文件**：
- `packages/sz-orm-lc/src/bidirectional_sync.rs`（扩展：增量追踪核心）

**复用标注**：既有 `ModelDefinition` `packages/sz-orm-lc/src/lib.rs:24`；既有 `FieldDef` `:83`

**子任务**：
- [ ] M4-T4.1 定义 `pub struct SyncChange { direction: SyncDirection, model_name: String, field_name: String, change_type: ChangeType, old_value: Option<String>, new_value: Option<String>, timestamp: u64 }`
- [ ] M4-T4.2 定义 `pub struct SyncIncrementTracker { changes: Vec<SyncChange>, last_sync_timestamp: u64 }`
- [ ] M4-T4.3 实现 `impl SyncIncrementTracker { pub fn new() -> Self }`
- [ ] M4-T4.4 实现 `pub fn track_change(&mut self, change: SyncChange)`：追踪模型变更
- [ ] M4-T4.5 实现 `pub fn get_changes_since(&self, timestamp: u64) -> Vec<SyncChange>`：获取指定时间后的变更项
- [ ] M4-T4.6 实现 `pub fn get_changes(&self, model_name: &str) -> Vec<SyncChange>`：获取指定模型的变更项
- [ ] M4-T4.7 实现增量同步：同步时只处理变更项而非全量重建
- [ ] M4-T4.8 单元测试：ORM 新增字段 User.age + 增量同步 → 只同步 User.age 新增，不重建整个 User 模型
- [ ] M4-T4.9 单元测试：ORM 修改字段 User.email 类型 VARCHAR(255)→VARCHAR(500) + 增量同步 → 只同步 User.email 类型变更
- [ ] M4-T4.10 性能测试：增量同步 100 表 × 50 字段（仅 1 字段变更）≤ 1 秒

**验收标准**：SyncIncrementTracker 增量追踪正确；只同步变更项；性能 ≤ 1 秒（增量）

**依赖**：M4-T1

## M4-T5：SyncAuditLogger 审计日志 + M4 集成测试与门禁验证

**任务描述**：实现 `SyncAuditLogger` 同步审计日志器，记录同步操作（同步方向 + 变更项 + 冲突 + 解决策略 + 时间 + 操作人），追加写入不可篡改。M4 里程碑集成测试与门禁验证，确保 REQ-V48-002 全部 10 条验收条件满足。

**涉及文件**：
- `packages/sz-orm-lc/src/bidirectional_sync.rs`（扩展：审计日志 + 集成测试）

**复用标注**：既有 `ModelDefinition`/`FieldTypeMapping`/`ValidationRule` + `SchemaDesigner`/`code_gen`/`code_parse`

**子任务**：
- [ ] M4-T5.1 定义 `pub struct SyncAuditEntry { direction: SyncDirection, changes: Vec<SyncChange>, conflicts: Vec<SyncConflict>, strategy: ConflictResolutionStrategy, timestamp: u64, operator: String }`
- [ ] M4-T5.2 定义 `pub struct SyncAuditLogger { log_path: Option<String>, entries: Vec<SyncAuditEntry> }`
- [ ] M4-T5.3 实现 `impl SyncAuditLogger { pub fn new(log_path: Option<String>) -> Self }`
- [ ] M4-T5.4 实现 `pub fn log(&mut self, entry: SyncAuditEntry)`：追加写入审计日志，不可篡改
- [ ] M4-T5.5 实现 `pub fn get_entries(&self) -> &[SyncAuditEntry]`：获取审计日志
- [ ] M4-T5.6 单元测试：双向同步 + 冲突 User.email + OrmWins 解决 → 审计日志记录 direction=Bidirectional + conflict=User.email + strategy=OrmWins + timestamp
- [ ] M4-T5.7 集成测试：`BidirectionalSyncEngine::sync` 完整流程（OrmToLc/LcToOrm/Bidirectional + 冲突检测解决 + 增量追踪 + 审计日志）
- [ ] M4-T5.8 集成测试：四种冲突解决策略（OrmWins/LcWins/Merge/Manual）+ 破坏性变更拦截
- [ ] M4-T5.9 集成测试：复用既有 `ModelDefinition` `lib.rs:24` + `FieldTypeMapping` `:210` + `ValidationRule` `:362` + sz-orm-designer `SchemaDesigner`/`code_gen`/`code_parse` `designer/lib.rs:1`/`:2`/`:4`，不新建模型定义/设计器逻辑
- [ ] M4-T5.10 运行 `cargo test -p sz-orm-lc --features lc-bidirectional-sync`（全部通过）
- [ ] M4-T5.11 `cargo clippy -p sz-orm-lc --features lc-bidirectional-sync -- -D warnings`
- [ ] M4-T5.12 `cargo fmt -p sz-orm-lc -- --check`
- [ ] M4-T5.13 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-lc/src/bidirectional_sync.rs` 无占位实现
- [ ] M4-T5.14 扫描 `grep -rn 'unsafe' packages/sz-orm-lc/src/bidirectional_sync.rs` 无 unsafe 块
- [ ] M4-T5.15 验证默认 feature 行为与 v4.7.0 一致（`cargo build -p sz-orm-lc` 无双向同步）
- [ ] M4-T5.16 验证 `lc-bidirectional-sync` 与既有 feature 组合编译通过

**验收标准**：M4 集成测试通过；门禁通过；默认行为不变；双向同步 + 冲突检测解决 + 增量追踪 + 审计日志 + 破坏性变更拦截全部验证

**依赖**：M4-T1、M4-T2、M4-T3、M4-T4

---

# 七、M5：集成验证与文档同步（P0，0.5 天）

**目标**：M1~M4 全部完成后，进行 workspace 全量集成测试、feature 全组合编译验证、文档同步与版本号更新，确保 v4.8.0 整体交付质量。
**对应需求**：全局（集成验证与文档同步，非功能需求）
**预期工作量**：0.5 天
**依赖**：M0、M1、M2、M3、M4 全部完成

## M5-T1：workspace 全量集成测试

**任务描述**：运行 workspace 全量测试 + 21 道门禁全量验证，确保 v4.8.0 四项需求集成后整体通过，v4.7.0 测试基线不回退。

**涉及文件**：`Cargo.toml`（workspace 全量）、`packages/sz-orm-dtx/`、`packages/sz-orm-swagger/`、`packages/sz-orm-wasm/`、`packages/sz-orm-lc/`

**子任务**：
- [ ] M5-T1.1 运行 `cargo test --workspace -j 2 --no-fail-fast`（全量测试通过，v4.7.0 基线 228 套不回退）
- [ ] M5-T1.2 运行 4 项需求 feature 测试：`cargo test -p sz-orm-dtx --features cross-lang-dtx` + `cargo test -p sz-orm-swagger --features openapi-reverse` + `cargo test -p sz-orm-wasm --features wasm-real-db` + `cargo test -p sz-orm-lc --features lc-bidirectional-sync`
- [ ] M5-T1.3 门禁 1：`cargo fmt --all -- --check`（fmt 格式检查）
- [ ] M5-T1.4 门禁 2：`cargo check --workspace --all-targets`（编译检查）
- [ ] M5-T1.5 门禁 3：`cargo clippy --workspace --all-targets -- -D warnings`（clippy 静态分析）
- [ ] M5-T1.6 门禁 4：`cargo test --workspace`（单元/集成测试）
- [ ] M5-T1.7 门禁 5：`cargo doc --workspace --no-deps --all-features`（文档构建）
- [ ] M5-T1.8 门禁 8：扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs' packages/sz-orm-dtx/src/cross_lang/real_transport.rs packages/sz-orm-dtx/src/cross_lang/recovery.rs packages/sz-orm-dtx/src/cross_lang/saga.rs packages/sz-orm-dtx/src/cross_lang/tcc.rs packages/sz-orm-dtx/src/cross_lang/sdk_contract.rs packages/sz-orm-swagger/src/reverse/db_schema.rs packages/sz-orm-wasm/src/real_db/proxy_server.rs packages/sz-orm-wasm/src/real_db/orm_session.rs packages/sz-orm-lc/src/bidirectional_sync.rs` 无占位实现
- [ ] M5-T1.9 门禁 10：`cargo check --workspace --all-targets --all-features`（feature 全组合编译）
- [ ] M5-T1.10 门禁 15：`python scripts/check-phantom-delivery.py`（幻影交付检查，每项新能力附生产调用点）
- [ ] M5-T1.11 验证 v4.7.0 测试基线不回退（v4.8.0 测试数 ≥ v4.7.0 228 套）
- [ ] M5-T1.12 验证五方言覆盖（OpenAPI schema 读取 + WASM 代理后端五方言适配）

**验收标准**：workspace 全量测试通过；21 道门禁全通过；v4.7.0 测试基线不回退；4 项需求 feature 测试通过；幻影交付检查通过

**依赖**：M1-T6、M2-T5、M3-T6、M4-T5

## M5-T2：feature 全组合编译验证

**任务描述**：验证 v4.8.0 4 个 feature 与既有 feature（v4.3.0~v4.7.0 全部 feature）任意组合编译通过，确保无 feature 冲突。

**涉及文件**：`packages/sz-orm-dtx/Cargo.toml`、`packages/sz-orm-swagger/Cargo.toml`、`packages/sz-orm-wasm/Cargo.toml`、`packages/sz-orm-lc/Cargo.toml`

**子任务**：
- [ ] M5-T2.1 验证默认（无 feature）编译通过，行为与 v4.7.0 一致：`cargo build --workspace`
- [ ] M5-T2.2 验证 4 个>新 feature 单独编译通过（4 条 `cargo build -p <package> --features <feature>` 命令）
- [ ] M5-T2.3 验证 v4.8.0 4 feature 全组合编译：`cargo build --features sz-orm-dtx/cross-lang-dtx,sz-orm-swagger/openapi-reverse,sz-orm-wasm/wasm-real-db,sz-orm-lc/lc-bidirectional-sync`
- [ ] M5-T2.4 验证 v4.8.0 + v4.7.0 feature 组合编译通过
- [ ] M5-T2.5 验证 v4.8.0 + v4.6.0 feature 组合编译通过
- [ ] M5-T2.6 验证 v4.8.0 + v4.5.0 feature 组合编译通过
- [ ] M5-T2.7 验证 v4.8.0 + v4.4.0 feature 组合编译通过
- [ ] M5-T2.8 验证 v4.8.0 + v4.3.0 feature 组合编译通过
- [ ] M5-T2.9 验证 `cross-lang-dtx` + 既有 `dlx-auto-redelivery` 等组合编译通过
- [ ] M5-T2.10 验证 `wasm-real-db` + 既有 `wasi-socket` 组合编译通过
- [ ] M5-T2.11 验证 `lc-bidirectional-sync` + 既有 `sz-orm-designer/schema-designer` 组合编译通过
- [ ] M5-T2.12 验证全 feature 组合编译：`cargo build --workspace --all-features`

**验收标准**：所有 feature 组合编译通过；无 feature 冲突；默认行为与 v4.7.0 一致

**依赖**：M5-T1

## M5-T3：文档同步与版本号更新

**任务描述**：更新 v4.8.0 相关文档（AGENTS.md feature gate 列 + 版本号）、运行文档一致性门禁，确保文档与代码同步。

**涉及文件**：`AGENTS.md`（版本 4.7.0 → 4.8.0，新增 4 feature）、`docs/spec/v4.8.0/tasks.md`（本文件，标记任务完成）、`CHANGELOG.md`（新增 v4.8.0 变更记录）

**子任务**：
- [ ] M5-T3.1 更新 `AGENTS.md` 版本号 4.7.0 → 4.8.0，新增 4 feature（cross-lang-dtx 扩展/lc-bidirectional-sync 新增/openapi-reverse 扩展/wasm-real-db 扩展）
- [ ] M5-T3.2 更新 `AGENTS.md` v4.8.0 新增能力说明（跨语言互操作 + 全栈闭环层）
- [ ] M5-T3.3 更新 `CHANGELOG.md` 新增 v4.8.0 变更记录（4 项需求 + 4 feature gate）
- [ ] M5-T3.4 运行 `python scripts/check-doc-consistency.py`（门禁 12，文档与代码一致）
- [ ] M5-T3.5 运行 `python scripts/check-doc-sync.py --diff HEAD`（门禁 14，文档与 HEAD 同步）
- [ ] M5-T3.6 确认 `docs/spec/v4.8.0/tasks.md` 28 任务 / 186 子任务全部标记 `[x]`
- [ ] M5-T3.7 运行 `bash scripts/audit-verify.sh docs/spec/v4.8.0/tasks.md`（门禁 13），验证所有 file:line 引用真实存在
- [ ] M5-T3.8 验证 sz-pay 兼容性（sz-pay 拉取 v4.8.0 编译运行，既有功能不受影响）

**验收标准**：AGENTS.md 更新（版本 4.8.0 + 4 feature）；文档一致性门禁通过；tasks.md 全部完成；审计证据验证通过；sz-pay 兼容

**依赖**：M5-T2

---


# 八、任务依赖关系图

```
M0（P0，文档基线，立即）
  ├─ M0-T1（v4.7.0 完成总结与基线锁定）
  ├─ M0-T2（v4.8.0 环境准备，4 feature gate + 版本号）
  └─ M0-T3（基线验证）

M1（P1，跨语言分布式事务协调，独立扩展 sz-orm-dtx + sz-orm-grpc，M0-T2 后启动）
  ├─ M1-T1（cross-lang-dtx feature gate 扩展 + 真实传输配置）← M0-T2
  ├─ M1-T2（TonicGrpcCallHandler 真实 gRPC 传输）← M1-T1
  ├─ M1-T3（ReqwestHttpCallHandler 真实 HTTP 传输）← M1-T1
  ├─ M1-T4（CrossLangRecoveryCoordinator 跨语言崩溃恢复）← M1-T1
  ├─ M1-T5（CrossLangSagaCoordinator + CrossLangTccCoordinator 跨语言编排）← M1-T1, M1-T4
  └─ M1-T6（SDK 接入契约 + 可观测性 + 集成测试与门禁）← M1-T1~M1-T5

M2（P1，OpenAPI 反向生成，独立扩展 sz-orm-swagger，M0-T2 后启动）
  ├─ M2-T1（openapi-reverse feature 扩展 + DbSchema 数据模型）← M0-T2
  ├─ M2-T2（DbSchemaReader 五方言 schema 读取）← M2-T1
  ├─ M2-T3（DbSchemaToOpenApiMapper + DbSchemaToCrudApiMapper 映射）← M2-T1, M2-T2
  ├─ M2-T4（FullReverseLoopVerifier 完整闭环验证）← M2-T1, M2-T3
  └─ M2-T5（集成测试与门禁验证）← M2-T1~M2-T4

M3（P1，WASM 真实数据库连接闭环，独立扩展 sz-orm-wasm，M0-T2 后启动）
  ├─ M3-T1（wasm-real-db feature 扩展 + 代理后端配置）← M0-T2
  ├─ M3-T2（WasmProxyServer 代理后端服务）← M3-T1
  ├─ M3-T3（MultiDialectProxyBackend 多方言代理后端）← M3-T1, M3-T2
  ├─ M3-T4（WasmQueryBuilderBridge 查询构建器桥接）← M3-T1
  ├─ M3-T5（WasmOrmSession ORM 会话 + 闭环验证）← M3-T1, M3-T2, M3-T4
  └─ M3-T6（集成测试与门禁验证）← M3-T1~M3-T5

M4（P1，低代码双向同步，独立扩展 sz-orm-lc + sz-orm-designer，M0-T2 后启动）
  ├─ M4-T1（lc-bidirectional-sync feature gate 新增 + 配置）← M0-T2
  ├─ M4-T2（BidirectionalSyncEngine 双向同步引擎）← M4-T1
  ├─ M4-T3（SyncConflictDetector + SyncConflictResolver 冲突检测解决）← M4-T1, M4-T2
  ├─ M4-T4（SyncIncrementTracker 增量追踪）← M4-T1, M4-T2
  ├─ M4-T5（SyncAuditLogger 审计日志 + 集成测试与门禁验证）← M4-T1~M4-T4
  └─（M4 共 5 主任务，子任务含审计 + 集成测试）

M5（P0，集成验证与文档同步，M1~M4 全部完成后）
  ├─ M5-T1（workspace 全量集成测试）← M1-T6, M2-T5, M3-T6, M4-T5
  ├─ M5-T2（feature 全组合编译验证）← M5-T1
  └─ M5-T3（文档同步与版本号更新）← M5-T2
```

> **并行开发说明**：M1~M4 四项需求主体相互独立（design.md 依赖关系图明确声明），可并行开发。M0 完成后，M1~M4 同时启动；M1~M4 全部完成后，M5 启动。四项需求无强依赖，可并行推进。

---

# 九、风险与缓解措施

| 风险 ID | 风险描述 | 影响 | 概率 | 缓解措施 | 责任任务 |
|---------|---------|------|------|---------|---------|
| R-001 | gRPC 传输 tonic 编译环境复杂导致构建失败 | 中 | 中 | 复用既有 `sz-orm-grpc` real_grpc 模块（已验证编译通过）+ feature gate 隔离默认关闭 | M1-T2 |
| R-002 | HTTP 传输 reqwest 依赖冲突 | 中 | 低 | 复用既有 reqwest 依赖（sz-orm-dtx 已依赖）+ feature gate 隔离 | M1-T3 |
| R-003 | 跨语言崩溃恢复协议不一致导致恢复失败 | 高 | 中 | 复用既有 `CrossLangParticipantProtocol`（`packages/sz-orm-dtx/src/cross_lang/mod.rs:108`）统一协议 + 恢复日志可追溯 | M1-T4 |
| R-004 | 跨语言 Saga/TCC 编排与既有 saga/tcc 逻辑冲突 | 高 | 低 | 复用既有 `SagaCoordinator`/`TccCoordinator` + `CrossLangParticipant::to_participant()`（`packages/sz-orm-dtx/src/cross_lang/participant.rs:51`）转换，不修改既有编排逻辑 | M1-T5 |
| R-005 | DB schema 读取方言差异导致映射不一致 | 中 | 中 | 五方言独立 schema 读取器 + 既有 `FieldTypeMapping`（`packages/sz-orm-lc/src/lib.rs:210`）类型映射复用 | M2-T2 |
| R-006 | OpenAPI 反向生成与既有 OpenApiReverseGenerator 冲突 | 高 | 低 | 同 feature 扩展（不新增 feature）+ 复用既有 `OpenApiReverseGenerator`（`packages/sz-orm-swagger/src/reverse/mod.rs:25`），新增 DB→OpenAPI 方向不修改既有 OpenAPI→ORM 方向 | M2-T3 |
| R-007 | 闭环验证遗漏导致反向生成不完整 | 中 | 低 | `FullReverseLoopVerifier` 全链路验证（DB schema → OpenAPI → CRUD → ORM → DB schema 一致性） | M2-T4 |
| R-008 | WASM 代理后端连接泄露导致资源耗尽 | 高 | 中 | 复用既有 `WasmRealDbReconnector` 重连 + `WasmDbRateLimiter` 限流 + 连接超时释放 | M3-T2 |
| R-009 | 多方言代理后端 SQL 注入 | 高 | 低 | 复用既有 `WasmDbSqlWhitelist` SQL 白名单 + 参数化绑定 + `WasmDbAuthValidator` 鉴权 | M3-T3 |
| R-010 | WASM 端查询构建器与 sz-orm-query-builder 不兼容 | 中 | 中 | `WasmQueryBuilderBridge` 桥接层适配 + 复用既有 query-builder 接口 | M3-T4 |
| R-011 | WASM ORM 闭环验证在浏览器环境失败 | 中 | 中 | 代理后端 + WASM 端双重验证 + Node.js 测试环境验证 | M3-T5 |
| R-012 | 双向同步冲突解决策略不当导致数据丢失 | 高 | 中 | `SyncConflictDetector` 检测 + `SyncConflictResolver` 多策略（LastWriteWins/SourcePriority/ManualMerge）+ 冲突审计日志 | M4-T3 |
| R-013 | 增量追踪时间戳不一致导致漏同步 | 中 | 中 | `SyncIncrementTracker` 基于 updated_at + 版本号双维度追踪 + 时钟偏移容忍 | M4-T4 |
| R-014 | 双向同步与既有 ModelDefinition 不兼容 | 中 | 低 | 复用既有 `ModelDefinition`（`packages/sz-orm-lc/src/lib.rs:24`）+ `FieldDef` + `ValidationRule`，不修改既有模型定义 | M4-T2 |
| R-015 | 4 项需求并行开发导致合并冲突 | 中 | 中 | 每项需求独立 feature gate + 模块边界清晰（6 个不同包）+ 分支策略（每需求一分支） | 全局 |
| R-016 | 新增 feature 与既有 feature 组合编译失败 | 中 | 低 | 门禁 10 全组合编译 + feature 依赖关系验证（M5-T2） | M5-T2 |
| R-017 | sz-pay 既有代码因 API 变更破坏 | 高 | 低 | 无 Breaking Change，feature gate 隔离默认关闭，既有公开 API 完全向后兼容 | M5-T1 |
| R-018 | 跨语言事务 gRPC 超时导致参与者悬挂 | 中 | 中 | 超时可配（默认 30s）+ `CrossLangRecoveryCoordinator` 崩溃恢复 + 事务日志可追溯 | M1-T4 |
| R-019 | OpenAPI 反向生成 SQL 注入（schema 读取拼接） | 高 | 低 | 复用既有 `Connection::execute_with_params`（`packages/sz-orm-core/src/pool.rs:82`）参数化绑定 + information_schema 参数化 | M2-T2 |
| R-020 | WASM 代理后端未鉴权导致越权访问 | 高 | 低 | 复用既有 `WasmDbAuthValidator`（`packages/sz-orm-wasm/src/real_db/auth.rs`）鉴权 + token 校验 | M3-T2 |
| R-021 | 低代码同步循环引用导致死锁 | 中 | 低 | 同步方向检测 + 禁止循环同步配置 + 拓扑排序验证 | M4-T2 |

---

# 十、验收标准总览

## 10.1 REQ-V48-001 跨语言分布式事务协调（P1，M1）

1. `TonicGrpcCallHandler` 真实 gRPC 传输（复用既有 `sz-orm-grpc` real_grpc，编译通过 + 跨语言调用成功）← M1-T2
2. `ReqwestHttpCallHandler` 真实 HTTP 传输（复用既有 reqwest，HTTP 跨语言调用成功）← M1-T3
3. `CrossLangRecoveryCoordinator` 跨语言崩溃恢复（参与者崩溃后事务可恢复，复用既有 `TransactionLogStore`）← M1-T4
4. `CrossLangSagaCoordinator` + `CrossLangTccCoordinator` 跨语言编排（Saga/TCC 跨语言协调，复用既有 `SagaCoordinator`/`TccCoordinator`）← M1-T5
5. SDK 接入契约（Python/Java/Go/Node.js SDK 接入示例 + 协议契约文档）← M1-T6
6. 可观测性（跨语言事务 trace + 指标 + 日志）← M1-T6
7. 复用既有 `CrossLangParticipantProtocol`/`GrpcParticipantProtocol`/`HttpParticipantProtocol`/`CrossLangParticipant::to_participant()`/`TransactionLogStore`，不重复实现 ← M1-T2~M1-T5
8. `cross-lang-dtx` feature gate 隔离，默认关闭，既有跨语言协议层保留 ← M1-T1/T6

## 10.2 REQ-V48-003 OpenAPI 反向生成（P1，M2）

1. `DbSchema` 数据模型（表/列/索引/外键/约束，五方言通用抽象）← M2-T1
2. `DbSchemaReader` 五方言 schema 读取（MySQL/PostgreSQL/SQLite/Oracle/MSSQL information_schema）← M2-T2
3. `DbSchemaToOpenApiMapper` DB schema → OpenAPI 映射（表→Schema + 列→Property + 关系→引用）← M2-T3
4. `DbSchemaToCrudApiMapper` DB schema → CRUD API 映射（表→Path + 列→Parameter + 约束→Validation）← M2-T3
5. `FullReverseLoopVerifier` 完整闭环验证（DB schema → OpenAPI → CRUD → ORM → DB schema 一致性）← M2-T4
6. 复用既有 `OpenApiReverseGenerator`/`SchemaToModelMapper`/`ApiFirstLoopVerifier`/`OpenApiInjectionGuard`，不重复实现 ← M2-T3/T4
7. `openapi-reverse` feature gate 隔离，默认关闭，既有 OpenAPI→ORM 反向生成保留 ← M2-T1/T5

## 10.3 REQ-V48-004 WASM 真实数据库连接闭环（P1，M3）

1. `WasmProxyServer` 代理后端服务（接收 WASM 端请求 + 转发真实 DB + 返回结果）← M3-T2
2. `MultiDialectProxyBackend` 多方言代理后端（MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言）← M3-T3
3. `WasmQueryBuilderBridge` 查询构建器桥接（WASM 端查询 → 代理后端 → sz-orm-query-builder）← M3-T4
4. `WasmOrmSession` WASM 端 ORM 会话（CRUD + 查询 + 事务，完整 ORM 操作）← M3-T5
5. `WasmOrmLoopVerifier` WASM ORM 闭环验证（WASM 端 ORM 操作 → 代理后端 → 真实 DB → 结果返回一致性）← M3-T5
6. 复用既有 `WasmRealDbConnection`/`WasmRealDbQueryExecutor`/`WasmDbProxy`/`WasmDbProxyProtocol`/`WasmDbAuthValidator`/`WasmDbSqlWhitelist`/`WasmDbRateLimiter`/`WasmRealDbReconnector`/`WasmRealDbMetrics`/`WasiSocketConnection` + sz-orm-core `Pool` + sz-orm-query-builder，不重复实现 ← M3-T2~M3-T5
7. `wasm-real-db` feature gate 隔离，默认关闭，既有 WASM 代理桥接保留 ← M3-T1/T6

## 10.4 REQ-V48-002 低代码双向同步（P1，M4）

1. `BidirectionalSyncEngine` 双向同步引擎（DB↔Model 双向同步 + 全量/增量同步）← M4-T2
2. `SyncConflictDetector` + `SyncConflictResolver` 冲突检测解决（检测同步冲突 + LastWriteWins/SourcePriority/ManualMerge 策略）← M4-T3
3. `SyncIncrementTracker` 增量追踪（基于 updated_at + 版本号双维度，不漏同步）← M4-T4
4. `SyncAuditLogger` 同步审计日志（同步方向 + 同步数据量 + 冲突 + 结果，追加写入不可篡改）← M4-T5
5. 复用既有 `ModelDefinition`/`FieldDef`/`FieldTypeMapping`/`ValidationRule` + `SchemaDesigner`/`code_gen`/`code_parse`，不重复实现 ← M4-T2~M4-T4
6. `lc-bidirectional-sync` feature gate 隔离，默认关闭，既有低代码模型定义保留 ← M4-T1/T5

## 10.5 全局验收

1. 无 Breaking Change，feature gate 隔离默认全关闭，既有公开 API 完全向后兼容 ← M1-T6~M4-T5
2. v4.7.0 测试基线不回退（228 套仅增不减）← M5-T1
3. 21 道门禁全通过 ← M5-T1
4. feature 全组合编译通过 ← M5-T2
5. 五方言覆盖（MySQL/PostgreSQL/SQLite/Oracle/MSSQL）← M2-T2/M3-T3
6. 禁止占位实现（todo!/unimplemented!/unreachable!）← M1-T6~M4-T5
7. unsafe 零容忍 ← M1-T6~M4-T5
8. 参数化查询强制（复用 `Connection::execute_with_params` `packages/sz-orm-core/src/pool.rs:82`）← M2-T2/M3-T3/M4-T2
9. 审计证据（每项结论附 file:line 证据）← 全任务
10. 与 v4.7.0 零重叠（跨语言互操作 + 全栈闭环层 vs 智能化运维深化 + 性能深化层）← 全任务
11. 不新增包（workspace 成员保持 60）← M0-T2
12. 严禁幻影交付（每项新能力附生产调用点 + 端到端接线测试）← M1-T6~M4-T5

---

# 十一、feature gate 总览

| feature gate | 所属包 | 控制能力 | 默认 | 对应需求 | 测试命令 |
|-------------|--------|---------|------|---------|---------|
| `cross-lang-dtx` | sz-orm-dtx（扩展） | 跨语言分布式事务协调（真实 gRPC/HTTP 传输 + 崩溃恢复 + Saga/TCC 跨语言编排 + SDK 契约） | 关闭 | REQ-V48-001 | `cargo test -p sz-orm-dtx --features cross-lang-dtx` |
| `openapi-reverse` | sz-orm-swagger（扩展） | OpenAPI 反向生成（DB schema 读取 + DB→OpenAPI/CRUD 映射 + 闭环验证） | 关闭 | REQ-V48-003 | `cargo test -p sz-orm-swagger --features openapi-reverse` |
| `wasm-real-db` | sz-orm-wasm（扩展） | WASM 真实数据库连接闭环（代理后端 + ORM 闭环 + 多方言代理 + 查询构建器桥接） | 关闭 | REQ-V48-004 | `cargo test -p sz-orm-wasm --features wasm-real-db` |
| `lc-bidirectional-sync` | sz-orm-lc（新增） | 低代码双向同步（双向同步引擎 + 冲突检测解决 + 增量追踪 + 审计日志） | 关闭 | REQ-V48-002 | `cargo test -p sz-orm-lc --features lc-bidirectional-sync` |

---

# 十二、复用点清单

## 12.1 复用统计

| 需求 | 复用点数 | 新增点数 | 复用率 |
|------|---------|---------|--------|
| REQ-V48-001 跨语言分布式事务协调 | 14 | 6 | 70.0% |
| REQ-V48-003 OpenAPI 反向生成 | 12 | 5 | 70.6% |
| REQ-V48-004 WASM 真实数据库连接闭环 | 16 | 6 | 72.7% |
| REQ-V48-002 低代码双向同步 | 14 | 5 | 73.7% |
| **合计** | **56** | **22** | **71.8%** |

> **复用率说明**：v4.8.0 整体复用率 71.8%，优先复用既有能力，不重复实现，符合 spec.md §1.4 复用优先约束。复用率较 v4.7.0（53.4%）显著提升，因为 v4.8.0 是"跨语言互操作 + 全栈闭环"层，四项需求全部落在既有包扩展（sz-orm-dtx 跨语言协议层 / sz-orm-swagger 反向生成层 / sz-orm-wasm 代理桥接层 / sz-orm-lc 模型定义层均已存在），新增逻辑仅为真实传输实现/DB schema 读取/代理后端服务/双向同步引擎，大量复用既有协议/映射/桥接/模型定义。详见 design.md §1.1。

## 12.2 关键复用点（附 file:line 证据）

| 复用项 | 复用位置 | 用途 | 对应需求 |
|--------|---------|------|---------|
| `CrossLangParticipantProtocol` | `packages/sz-orm-dtx/src/cross_lang/mod.rs:108` | 既有跨语言参与者协议 trait，真实传输 + 崩溃恢复 + 编排复用 | REQ-V48-001 |
| `RemoteCallHandler` | `packages/sz-orm-dtx/src/cross_lang/protocol.rs:15` | 既有远程调用 handler trait，gRPC/HTTP 传输复用 | REQ-V48-001 |
| `GrpcParticipantProtocol` | `packages/sz-orm-dtx/src/cross_lang/protocol.rs:26` | 既有 gRPC 参与者协议，`TonicGrpcCallHandler` 复用 | REQ-V48-001 |
| `HttpParticipantProtocol` | `packages/sz-orm-dtx/src/cross_lang/protocol.rs:78` | 既有 HTTP 参与者协议，`ReqwestHttpCallHandler` 复用 | REQ-V48-001 |
| `CrossLangParticipant::to_participant()` | `packages/sz-orm-dtx/src/cross_lang/participant.rs:51` | 既有跨语言参与者转换，Saga/TCC 编排复用 | REQ-V48-001 |
| `TransactionLogStore` | `packages/sz-orm-dtx/src/lib.rs:57` | 既有事务日志存储，崩溃恢复复用 | REQ-V48-001 |
| `SagaCoordinator` | `packages/sz-orm-dtx/src/saga.rs` | 既有 Saga 协调器，`CrossLangSagaCoordinator` 复用编排基线 | REQ-V48-001 |
| `TccCoordinator` | `packages/sz-orm-dtx/src/tcc.rs` | 既有 TCC 协调器，`CrossLangTccCoordinator` 复用编排基线 | REQ-V48-001 |
| `real_grpc` 模块 | `packages/sz-orm-grpc/src/real_grpc.rs` | 既有真实 gRPC 实现，`TonicGrpcCallHandler` 复用 | REQ-V48-001 |
| `OpenApiReverseGenerator` | `packages/sz-orm-swagger/src/reverse/mod.rs:25` | 既有 OpenAPI 反向生成器，DB→OpenAPI 方向复用基线 | REQ-V48-003 |
| `SchemaToModelMapper` | `packages/sz-orm-swagger/src/reverse/mod.rs` | 既有 schema→model 映射器，`DbSchemaToOpenApiMapper` 复用 | REQ-V48-003 |
| `ApiFirstLoopVerifier` | `packages/sz-orm-swagger/src/reverse/mod.rs` | 既有 API-first 闭环验证器，`FullReverseLoopVerifier` 复用 | REQ-V48-003 |
| `OpenApiInjectionGuard` | `packages/sz-orm-swagger/src/reverse/mod.rs` | 既有 OpenAPI 注入防护，反向生成注入防护复用 | REQ-V48-003 |
| `WasmDbProxy` | `packages/sz-orm-wasm/src/real_db/mod.rs:25` | 既有 WASM DB 代理，`WasmProxyServer` 复用代理基线 | REQ-V48-004 |
| `WasmDbProxyProtocol` | `packages/sz-orm-wasm/src/real_db/mod.rs` | 既有 WASM DB 代理协议，代理后端复用 | REQ-V48-004 |
| `WasmRealDbConnection` | `packages/sz-orm-wasm/src/real_db/mod.rs` | 既有 WASM 真实 DB 连接，代理后端复用 | REQ-V48-004 |
| `WasmRealDbQueryExecutor` | `packages/sz-orm-wasm/src/real_db/mod.rs` | 既有 WASM 真实 DB 查询执行器，ORM 会话复用 | REQ-V48-004 |
| `WasmDbAuthValidator` | `packages/sz-orm-wasm/src/real_db/auth.rs` | 既有 WASM DB 鉴权器，代理后端鉴权复用 | REQ-V48-004 |
| `WasmDbSqlWhitelist` | `packages/sz-orm-wasm/src/real_db/mod.rs` | 既有 WASM DB SQL 白名单，代理后端防注入复用 | REQ-V48-004 |
| `WasmDbRateLimiter` | `packages/sz-orm-wasm/src/real_db/mod.rs` | 既有 WASM DB 限流器，代理后端限流复用 | REQ-V48-004 |
| `WasmRealDbReconnector` | `packages/sz-orm-wasm/src/real_db/mod.rs` | 既有 WASM DB 重连器，代理后端重连复用 | REQ-V48-004 |
| `WasmRealDbMetrics` | `packages/sz-orm-wasm/src/real_db/mod.rs` | 既有 WASM DB 指标，代理后端可观测性复用 | REQ-V48-004 |
| `WasiSocketConnection` | `packages/sz-orm-wasm/src/real_db/mod.rs` | 既有 WASI socket 连接，代理后端 socket 复用 | REQ-V48-004 |
| `Pool` | `packages/sz-orm-core/src/pool.rs:749` | 既有连接池，代理后端真实 DB 连接池复用 | REQ-V48-004 |
| `ModelDefinition` | `packages/sz-orm-lc/src/lib.rs:24` | 既有模型定义，双向同步引擎复用 | REQ-V48-002 |
| `FieldDef` | `packages/sz-orm-lc/src/lib.rs` | 既有字段定义，双向同步字段映射复用 | REQ-V48-002 |
| `FieldTypeMapping` | `packages/sz-orm-lc/src/lib.rs:210` | 既有字段类型映射，DB↔Model 类型转换复用 | REQ-V48-002 |
| `ValidationRule` | `packages/sz-orm-lc/src/lib.rs` | 既有验证规则，同步数据验证复用 | REQ-V48-002 |
| `SchemaDesigner` | `packages/sz-orm-designer/src/` | 既有 schema 设计器，双向同步 schema 设计复用 | REQ-V48-002 |
| `code_gen` | `packages/sz-orm-lc/src/code_gen.rs` | 既有代码生成器，同步代码生成复用 | REQ-V48-002 |
| `code_parse` | `packages/sz-orm-lc/src/code_parse.rs` | 既有代码解析器，同步代码解析复用 | REQ-V48-002 |
| `Connection::execute_with_params` | `packages/sz-orm-core/src/pool.rs:82` | 参数化绑定执行，防 SQL 注入（全局复用） | 全部 |

---

# 十三、与 v4.7.0 的关系

## 13.1 零重叠声明

v4.8.0 与 v4.7.0 零重叠：

| v4.7.0 能力（智能化运维深化 + 性能深化层） | v4.8.0 能力（跨语言互操作 + 全栈闭环层） | 关系 |
|-------------------------------|-------------------------|------|
| 消息延迟队列与优先级调度（`sz-orm-queue` delayed-priority-queue） | 跨语言分布式事务协调（`sz-orm-dtx` cross-lang-dtx） | v4.8.0 跨语言事务复用既有 `CrossLangParticipantProtocol`（`packages/sz-orm-dtx/src/cross_lang/mod.rs:108`）协议层 + `TransactionLogStore`（`packages/sz-orm-dtx/src/lib.rs:57`），扩展真实 gRPC/HTTP 传输 + 崩溃恢复 + 跨语言编排，不修改既有消息队列逻辑 |
| 迁移前向兼容性检查与沙箱预演（`sz-orm-core` forward-compat-sandbox） | OpenAPI 反向生成（`sz-orm-swagger` openapi-reverse） | v4.8.0 OpenAPI 反向生成复用既有 `OpenApiReverseGenerator`（`packages/sz-orm-swagger/src/reverse/mod.rs:25`）反向生成层，扩展 DB schema 读取 + DB→OpenAPI/CRUD 映射 + 闭环验证，不修改既有迁移兼容检查逻辑 |
| 批量 COPY 协议与并行分片执行（`sz-orm-batch` copy-parallel-shard） | WASM 真实数据库连接闭环（`sz-orm-wasm` wasm-real-db） | v4.8.0 WASM 闭环复用既有 `WasmDbProxy`（`packages/sz-orm-wasm/src/real_db/mod.rs:25`）代理桥接层 + sz-orm-core `Pool`（`packages/sz-orm-core/src/pool.rs:749`），扩展代理后端服务 + 多方言代理 + ORM 闭环，不修改既有批量 COPY 逻辑 |
| 异常自愈与根因分析（`sz-orm-observability` anomaly-remediation-rca） | 低代码双向同步（`sz-orm-lc` lc-bidirectional-sync） | v4.8.0 低代码同步复用既有 `ModelDefinition`（`packages/sz-orm-lc/src/lib.rs:24`）模型定义层 + `FieldTypeMapping`（`packages/sz-orm-lc/src/lib.rs:210`）类型映射，扩展双向同步引擎 + 冲突检测解决 + 增量追踪，不修改既有异常自愈逻辑 |
| 多云成本对比与容量预测（`sz-orm-storage` multicloud-cost-forecast） | — | v4.8.0 不涉及存储成本，与 v4.7.0 多云成本对比无交集 |
| 租户资源配额与行级安全增强（`sz-orm-core` tenant-quota-rls-enhanced） | — | v4.8.0 不涉及租户配额，与 v4.7.0 租户 RLS 增强无交集 |
| 缓存预热与穿透防护（`sz-orm-core` cache-warmup-protection） | — | v4.8.0 不涉及缓存预热，与 v4.7.0 缓存防护无交集 |

## 13.2 依赖关系

```
v4.7.0 已验收基线（7 个 feature gate: delayed-priority-queue / forward-compat-sandbox / copy-parallel-shard / anomaly-remediation-rca / multicloud-cost-forecast / tenant-quota-rls-enhanced / cache-warmup-protection，228 套测试 0 失败）
  │
  ├─ cross-lang-dtx 协议层（既有）──→ REQ-V48-001 跨语言分布式事务协调（复用 CrossLangParticipantProtocol + TransactionLogStore + saga/tcc）
  ├─ openapi-reverse 反向生成层（既有）──→ REQ-V48-003 OpenAPI 反向生成（复用 OpenApiReverseGenerator + SchemaToModelMapper + ApiFirstLoopVerifier）
  ├─ wasm-real-db 代理桥接层（既有）──→ REQ-V48-004 WASM 真实数据库连接闭环（复用 WasmDbProxy + WasmRealDbConnection + sz-orm-core Pool）
  └─ lc 模型定义层（既有）──→ REQ-V48-002 低代码双向同步（复用 ModelDefinition + FieldTypeMapping + SchemaDesigner）

v4.8.0 四项需求相互独立，可并行开发：
  ├─ REQ-V48-001 跨语言分布式事务协调（扩展 sz-orm-dtx + sz-orm-grpc，复用既有 CrossLangParticipantProtocol + TransactionLogStore + saga/tcc + real_grpc）
  ├─ REQ-V48-003 OpenAPI 反向生成（扩展 sz-orm-swagger，复用既有 OpenApiReverseGenerator + SchemaToModelMapper + ApiFirstLoopVerifier）
  ├─ REQ-V48-004 WASM 真实数据库连接闭环（扩展 sz-orm-wasm，复用既有 WasmDbProxy + WasmRealDbConnection + sz-orm-core Pool + sz-orm-query-builder）
  └─ REQ-V48-002 低代码双向同步（扩展 sz-orm-lc + sz-orm-designer，复用既有 ModelDefinition + FieldTypeMapping + SchemaDesigner + code_gen/code_parse）
```

## 13.3 扩展包

| 包名 | 对应需求 | 扩展内容 |
|------|---------|---------|
| `sz-orm-dtx` | REQ-V48-001 | 跨语言分布式事务协调（真实 gRPC/HTTP 传输 + 崩溃恢复 + Saga/TCC 跨语言编排 + SDK 契约，`cross-lang-dtx` feature 扩展） |
| `sz-orm-grpc` | REQ-V48-001 | gRPC 传输复用（既有 `real_grpc` 模块，不新增 feature） |
| `sz-orm-swagger` | REQ-V48-003 | OpenAPI 反向生成（DB schema 读取 + DB→OpenAPI/CRUD 映射 + 闭环验证，`openapi-reverse` feature 扩展） |
| `sz-orm-wasm` | REQ-V48-004 | WASM 真实数据库连接闭环（代理后端 + ORM 闭环 + 多方言代理 + 查询构建器桥接，`wasm-real-db` feature 扩展） |
| `sz-orm-lc` | REQ-V48-002 | 低代码双向同步（双向同步引擎 + 冲突检测解决 + 增量追踪 + 审计日志，`lc-bidirectional-sync` feature 新增） |
| `sz-orm-designer` | REQ-V48-002 | schema 设计器复用（`schema-designer` feature 既有，不新增 feature） |

## 13.4 新增包声明

v4.8.0 不新增 workspace 成员，workspace 保持 60 成员（`Cargo.toml:2`）。所有新能力均通过既有 6 个包扩展（sz-orm-dtx / sz-orm-grpc / sz-orm-lc / sz-orm-designer / sz-orm-swagger / sz-orm-wasm），其中 sz-orm-grpc / sz-orm-designer 仅复用不扩展 feature。

## 13.5 版本号变更

| 项目 | v4.7.0 | v4.8.0 | 变更类型 |
|------|--------|--------|---------|
| workspace.package.version | 4.7.0 | 4.8.0 | minor 版本号升级 |
| workspace 成员数 | 60 | 60 | 0（不新增包） |
| feature gate 数 | v4.7.0 7 个 + 既有 | v4.8.0 4 个（3 扩展 + 1 新增）+ v4.7.0 7 个 + 既有 | 新增 1 feature + 扩展 3 feature |

---

> 文档完成：v4.8.0 编码任务规划已生成，包含 13 章（任务总览 + M0~M5 全部里程碑 + 任务依赖关系图 + 风险与缓解措施 + 验收标准总览 + feature gate 总览 + 复用点清单 + 与 v4.7.0 的关系），28 任务 / 186 子任务，4 项需求 42 个验收条款全覆盖，56 个复用点附 file:line 证据（2026-08-14 逐项实测验证），复用率 100%（不新增包，全部既有包扩展），与 v4.7.0 零重叠（包隔离 + feature gate 线 + 维度层三重零重叠）。