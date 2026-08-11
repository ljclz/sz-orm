# sz-orm v4.2.0 编码任务规划

> 版本：v4.2.0（跨语言分布式事务 + Go/Java/C++ 绑定 + 可视化 Schema 设计器 + OpenAPI → ORM 反向生成 + WASM 真实数据库连接）
> 基线：v4.1.0（数据 seeding/fixture + schema diff 可视化 + 缓存一致性协议 + 消息轨迹追踪 + 存储生命周期管理 + 数据质量自动检测 + 批量流式处理 + 迁移版本分支 + 备份验证自动化，9 项能力全部通过 feature gate 隔离，已验收基线）
> 日期：2026-08-11
> 文档定位：编码任务规划（How to execute），对应需求规格 `spec.md`（What to build）与技术设计 `design.md`（How to build）
> 任务约束：无 Breaking Change（7 个 feature gate 隔离）+ 优先复用既有能力 + 五方言覆盖 + 每项任务附 file:line 代码证据 + unsafe 零容忍 + 禁止占位实现（todo!/unimplemented!/unreachable!）
> 审计合规铁律：每项任务结论须附真实存在的 file:line 证据，修复后必须运行 `cargo test` 并附输出，禁止未验证即标记 ✅
> 实施顺序：按 design.md 第二章依赖关系，M1 P1（跨语言分布式事务，微服务互操作核心）→ M2 P2（Go/Java/C++ 绑定，跨语言生态扩展）→ M3 P2（Schema 设计器 + OpenAPI 反向生成，低代码 + API 优先，可并行）→ M4 P3（WASM 真实 DB，浏览器端 ORM 探索）+ 最终验证

---

# 一、任务总览

## 1.1 里程碑 × 任务数 × 预期工作量

| 里程碑 | 名称 | 对应需求 | 优先级 | 任务数 | 子任务数 | 预期工作量 |
|--------|------|---------|--------|--------|----------|-----------|
| M1 | 跨语言分布式事务 | REQ-V42-001 | P1 | 8 | 56 | 2 周 |
| M2 | Go/Java/C++ 绑定 | REQ-V42-002 | P2 | 9 | 64 | 2.5 周 |
| M3 | 可视化 Schema 设计器 | REQ-V42-003 | P2 | 7 | 48 | 1.5 周 |
| M3 | OpenAPI → ORM 反向生成 | REQ-V42-004 | P2 | 6 | 40 | 1.5 周 |
| M4 | WASM 真实数据库连接 | REQ-V42-005 | P3 | 7 | 46 | 1.5 周 |
| M4 | 最终验证与文档同步 | 全局 | — | 3 | 24 | 0.5 周 |
| **合计** | — | **5 项全覆盖** | — | **40** | **278** | **9.5 周** |

## 1.2 任务编号约定

- 主任务：`M{里程碑号}-T{任务序号}`（如 M1-T1）
- 子任务：`M{里程碑号}-T{任务序号}.{子任务序号}`（如 M1-T2.1）
- 集成验证任务：每个需求末尾固定一个集成测试任务（如 M1-T8）
- 里程碑内需求按 REQ-V42-xxx 序号顺序编排任务

## 1.3 全局约束（适用于所有任务）

1. **feature gate 隔离**：所有新能力通过 feature gate 隔离（`cross-lang-dtx` / `lang-binding-go` / `lang-binding-java` / `lang-binding-cpp` / `schema-designer` / `openapi-reverse` / `wasm-real-db`），默认 feature 行为不变
2. **既有 API 不变**：既有公开 API 签名完全向后兼容，sz-pay 既有代码不受影响（sz-pay 从 crates.io 拉取 sz-orm-* 6 个包）
3. **禁止占位实现**：禁止 `todo!`/`unimplemented!`/`unreachable!`
4. **unsafe 零容忍**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释（FFI/JNI/cgo 边界须显式标注）
5. **五方言覆盖**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL，Schema 设计器/OpenAPI 反向生成/WASM 真实 DB 按方言能力适配
6. **审计证据**：每项任务结论附真实存在的 file:line 证据
7. **测试基线不回退**：v4.1.0 已验收测试基线不回退，v4.2.0 仅增不减
8. **复用优先**：优先复用既有能力，不重复实现（跨语言事务复用 DtxManager/saga/tcc/xa + sz-orm-grpc/sz-orm-queue/sz-orm-tracing；Go/Java/C++ 绑定复用 python/js 模式 + sz-orm-core API；Schema 设计器复用 SchemaDiff/diff/DdlGenerator + sz-orm-masking；OpenAPI 反向生成复用 OpenAPISpec/Schema + DdlGenerator；WASM 真实 DB 复用 WasmQuery/WasmDatabase/js_bindings + Pool/sz-orm-sql-validator/sz-orm-limit/sz-orm-observability）
9. **Windows MSVC 编译环境**：RUST_MIN_STACK=134217728, CARGO_INCREMENTAL=0
10. **测试命令**：`cargo test --workspace -j 2 --no-fail-fast`

## 1.4 里程碑依赖关系

```
M1（P1，跨语言分布式事务，微服务互操作核心）
M2（P2，Go/Java/C++ 绑定，跨语言生态扩展）
  - REQ-V42-002 复用既有 sz-orm-core + python/js cdylib 模式
  - REQ-V42-001 与 REQ-V42-002 存在可选协同（参与者可经语言绑定接入，非强依赖）
M3（P2，Schema 设计器 + OpenAPI 反向生成，可并行）
  - REQ-V42-003 复用既有 sz-orm-core/schema_sync + sz-orm-masking
  - REQ-V42-004 复用既有 sz-orm-swagger + sz-orm-core/schema_sync
M4（P3，WASM 真实 DB + 最终验证）
  - REQ-V42-005 复用既有 sz-orm-wasm + sz-orm-observability
  - 最终验证依赖 M1-M3 全部完成
```

> **依赖关系说明**：M1/M2/M3 内各需求相互独立可并行开发；跨需求依赖仅复用既有包（sz-orm-grpc/sz-orm-queue/sz-orm-tracing/sz-orm-core），无新增需求间强依赖；M4 必须最后执行。

## 1.5 feature gate 定义与测试命令

| feature gate | 所属包 | 依赖 | 测试命令 |
|-------------|--------|------|---------|
| `cross-lang-dtx` | sz-orm-dtx | prost/tonic（optional）+ sz-orm-grpc/real | `cargo test -p sz-orm-dtx --features cross-lang-dtx` |
| `lang-binding-go` | sz-orm-go（新包） | sz-orm-cabi | `cargo test -p sz-orm-go --features lang-binding-go` |
| `lang-binding-java` | sz-orm-java（新包） | sz-orm-cabi | `cargo test -p sz-orm-java --features lang-binding-java` |
| `lang-binding-cpp` | sz-orm-cpp（新包） | sz-orm-cabi + cxx | `cargo test -p sz-orm-cpp --features lang-binding-cpp` |
| `schema-designer` | sz-orm-designer（新包） | axum/syn/quote + sz-orm-core + sz-orm-masking | `cargo test -p sz-orm-designer --features schema-designer` |
| `openapi-reverse` | sz-orm-swagger | quote/syn/proc-macro2 + sz-orm-core | `cargo test -p sz-orm-swagger --features openapi-reverse` |
| `wasm-real-db` | sz-orm-wasm | reqwest/tokio-tungstenite/rmp-serde + js | `cargo test -p sz-orm-wasm --features wasm-real-db` |

---

# 二、M1：跨语言分布式事务（REQ-V42-001，P1）

**目标**：扩展既有 `sz-orm-dtx` 协调器，使 Go/Java/C++/Python/JS 异构语言服务能作为 Saga/TCC/XA 事务参与者，通过 gRPC/HTTP 标准协议接入，不修改既有事务执行逻辑。提供 `CrossLangParticipantProtocol`（跨语言参与者协议）+ `CrossLangParticipant`（适配器）+ `CrossLangCompensationSerializer`（补偿序列化）+ `CrossLangParticipantRegistry`（注册中心）+ `CrossLangTxAlerter`（告警）+ 跨语言事务可观测 + 协调器崩溃恢复 + 故障隔离。
**预期工作量**：2 周
**对应需求**：REQ-V42-001（spec.md 5.1，design.md 2.2.1 REQ-V42-001）
**依赖**：无（M1-001 为 P1 独立需求，复用既有 sz-orm-dtx/sz-orm-grpc/sz-orm-queue/sz-orm-tracing）

## M1-T1：cross-lang-dtx feature gate 体系搭建

**任务描述**：在 sz-orm-dtx 中新增 `cross-lang-dtx` feature gate 及对应可选依赖（prost/tonic），作为跨语言分布式事务的隔离基础。默认关闭，避免无配置环境行为变化。

**涉及文件**：
- `packages/sz-orm-dtx/Cargo.toml`（新增 `cross-lang-dtx` feature + prost/tonic 可选依赖，复用既有 feature gate 模式 `:28-33`）

**复用标注**：复用既有 feature gate 体系（`packages/sz-orm-dtx/Cargo.toml:28-33`，已有 `xa`/`real-db` feature）、既有 `sz-orm-grpc`（`packages/sz-orm-grpc/Cargo.toml:2`，gRPC 传输复用）

**子任务**：
- [ ] M1-T1.1 在 `packages/sz-orm-dtx/Cargo.toml` `[features]` 新增 `cross-lang-dtx = ["dep:prost", "dep:tonic", "sz-orm-grpc/real"]`，位置在既有 feature 之后，默认关闭（design.md `:706-711`）
- [ ] M1-T1.2 在 `packages/sz-orm-dtx/Cargo.toml` `[dependencies]` 新增 `prost = { version = "0.14", optional = true }` 与 `tonic = { version = "0.14", optional = true }`（与既有 sz-orm-grpc 版本一致，避免依赖冲突）
- [ ] M1-T1.3 验证 `cargo check -p sz-orm-dtx`（默认 feature，不启用 cross-lang-dtx）编译通过，行为与 v4.1.0 一致
- [ ] M1-T1.4 验证 `cargo check -p sz-orm-dtx --features cross-lang-dtx` 编译通过
- [ ] M1-T1.5 验证 `cargo check --workspace --all-targets --all-features` 编译通过（feature 全组合门禁）
- [ ] M1-T1.6 验证既有 `xa`/`real-db` feature 行为不变（`cargo check -p sz-orm-dtx --features xa,real-db` 编译通过）

**验收标准**：
1. `cargo check -p sz-orm-dtx` 默认编译通过，无 cross-lang-dtx 相关代码生效
2. `cargo check -p sz-orm-dtx --features cross-lang-dtx` 编译通过
3. 既有 `xa`/`real-db` feature 行为不变，既有 API 签名完全不变
4. `cargo test --workspace` 既有测试全部通过（v4.1.0 基线不回退）
5. 附 `packages/sz-orm-dtx/Cargo.toml` 新增 feature 与依赖定义的 file:line 证据

**依赖**：无（基础设施任务，M1-001 所有任务依赖此任务）

---

## M1-T2：CrossLangParticipantProtocol（跨语言参与者接入协议）

**任务描述**：在 sz-orm-dtx 新增 `cross_lang` 模块，定义跨语言参与者接入协议 trait（gRPC protobuf IDL + HTTP/JSON 备选），标准接口（prepare/commit/rollback/confirm/cancel），协议版本化，复用既有 `sz-orm-grpc` 做 gRPC 传输。

**涉及文件**：
- `packages/sz-orm-dtx/src/cross_lang/mod.rs`（新增模块，定义 `ParticipantLanguage`/`ParticipantTransport`/`ParticipantAuth`/`CrossLangParticipantDesc`/`CrossLangParticipantProtocol` trait/`ParticipantResponse`/`CrossLangTxError`）
- `packages/sz-orm-dtx/src/cross_lang/protocol.rs`（新增，gRPC/HTTP 协议实现）
- `packages/sz-orm-dtx/src/cross_lang/proto/participant.proto`（新增，protobuf IDL）
- `packages/sz-orm-dtx/src/lib.rs`（新增 `#[cfg(feature = "cross-lang-dtx")] pub mod cross_lang;`）

**复用标注**：
- 既有 `sz-orm-grpc`（`packages/sz-orm-grpc/Cargo.toml:2`）：gRPC 传输层复用（tonic + prost）
- 既有 `MessageQueue` trait（`packages/sz-orm-queue/src/queue.rs:18`）：HTTP/JSON 备选传输复用
- 既有 `serde_json`：JSON 序列化复用

**子任务**：
- [ ] M1-T2.1 在 `cross_lang/mod.rs` 定义 `pub enum ParticipantLanguage { Go, Java, Cpp, Python, JavaScript }`（design.md `:575`）
- [ ] M1-T2.2 定义 `pub enum ParticipantTransport { Grpc, Http }`（design.md `:577`）
- [ ] M1-T2.3 定义 `pub enum ParticipantAuth { Mtls { cert: Vec<u8>, key: Vec<u8>, ca: Vec<u8> }, Token(String) }`（mTLS/Token 鉴权凭据，spec 5.1.1 规则 6）
- [ ] M1-T2.4 定义 `pub struct CrossLangParticipantDesc { pub resource_id: String, pub language: ParticipantLanguage, pub transport: ParticipantTransport, pub endpoint: String, pub auth: ParticipantAuth, pub protocol_version: u32 }`（design.md `:581-588`）
- [ ] M1-T2.5 定义 `pub struct ParticipantResponse { pub success: bool, pub payload: Vec<u8>, pub error: Option<String>, pub latency_ms: u64 }`
- [ ] M1-T2.6 定义 `pub enum CrossLangTxError { Timeout, AuthFailed, ProtocolVersionMismatch { coordinator: u32, participant: u32 }, RecoveryConflict, Transport(String), CompensationFailed { participant: String }, RemoteCall(String) }`（design.md `:717-724`）
- [ ] M1-T2.7 定义 `pub trait CrossLangParticipantProtocol: Send + Sync` 含 `fn prepare(&self, tx_id: &str, payload: &[u8]) -> Result<ParticipantResponse, CrossLangTxError>` / `fn commit(...)` / `fn rollback(...)` / `fn protocol_version(&self) -> u32`（design.md `:591-595`）
- [ ] M1-T2.8 在 `protocol.rs` 实现 `GrpcParticipantProtocol`（基于 tonic + prost，复用既有 sz-orm-grpc 模式）+ `HttpParticipantProtocol`（基于 reqwest，HTTP/JSON 备选）
- [ ] M1-T2.9 在 `proto/participant.proto` 定义 gRPC 服务接口 `service CrossLangParticipantService { rpc Prepare(ParticipantRequest) returns (ParticipantResponse); rpc Commit(...) returns (...); rpc Rollback(...) returns (...); }`
- [ ] M1-T2.10 在 `packages/sz-orm-dtx/src/lib.rs` 新增 `#[cfg(feature = "cross-lang-dtx")] pub mod cross_lang;`
- [ ] M1-T2.11 编写单元测试：`GrpcParticipantProtocol` 构造 + protocol_version 返回正确
- [ ] M1-T2.12 编写单元测试：`HttpParticipantProtocol` 构造 + protocol_version 返回正确
- [ ] M1-T2.13 编写单元测试：协议版本不兼容时返回 `CrossLangTxError::ProtocolVersionMismatch`（spec 5.1.3 异常 2）

**验收标准**：
1. `CrossLangParticipantProtocol` trait 定义标准接口（prepare/commit/rollback），gRPC/HTTP 双实现
2. 协议版本化支持，版本不兼容返回明确错误
3. 复用既有 sz-orm-grpc，不引入新 gRPC 依赖
4. `cargo test -p sz-orm-dtx --features cross-lang-dtx` 新增测试全部通过
5. 附 `packages/sz-orm-dtx/src/cross_lang/mod.rs` 新增代码的 file:line 证据

**依赖**：M1-T1（feature gate 体系）

---

## M1-T3：CrossLangCompensationSerializer（补偿回调序列化）

**任务描述**：实现 `CrossLangCompensationSerializer`，将 Rust 闭包的补偿逻辑序列化为跨语言可执行的协议消息（操作描述，非闭包），跨语言参与者收到补偿请求后执行其语言侧补偿逻辑，结果回传协调器。

**涉及文件**：
- `packages/sz-orm-dtx/src/cross_lang/serializer.rs`（新增，CrossLangCompensationSerializer 实现）

**复用标注**：
- 既有 `serde_json`（workspace 依赖）：JSON 序列化复用
- 既有 `ParticipantCallback`（`packages/sz-orm-dtx/src/lib.rs:179`）：补偿回调签名复用

**子任务**：
- [ ] M1-T3.1 定义 `pub struct CompensationPayload { pub action: String, pub target: String, pub params: serde_json::Value, pub idempotency_key: String }`（操作描述，如 `{"action":"deduct","target":"account:A","params":{"amount":100}}`，design.md `:746`）
- [ ] M1-T3.2 定义 `pub struct CrossLangCompensationSerializer`，`fn serialize(payload: &CompensationPayload) -> Result<Vec<u8>, CrossLangTxError>`（序列化为 JSON 字节）
- [ ] M1-T3.3 实现 `fn deserialize(bytes: &[u8]) -> Result<CompensationPayload, CrossLangTxError>`（反序列化，跨语言参与者回传结果解析）
- [ ] M1-T3.4 实现 `fn build_rollback_payload(original_action: &str, target: &str, params: &serde_json::Value) -> CompensationPayload`（自动构造补偿操作描述，如 deduct → refund）
- [ ] M1-T3.5 实现 `fn idempotency_key(tx_id: &str, participant_id: &str, action: &str) -> String`（幂等键生成，确保重复 commit/rollback 不产生副作用，spec 4.3 规则 2）
- [ ] M1-T3.6 编写单元测试：`CompensationPayload` 序列化/反序列化往返无损
- [ ] M1-T3.7 编写单元测试：`build_rollback_payload("deduct", "account:A", {"amount":100})` 返回 `{"action":"refund","target":"account:A","params":{"amount":100}}`
- [ ] M1-T3.8 编写单元测试：幂等键对相同 (tx_id, participant_id, action) 返回相同值
- [ ] M1-T3.9 编写单元测试：序列化开销 ≤1ms（spec 4.1 性能 1，单次参与者协调 ≤50ms 内）

**验收标准**：
1. `CrossLangCompensationSerializer` 序列化/反序列化往返无损
2. 补偿操作描述自动构造（deduct→refund 等逆操作）
3. 幂等键确保重复补偿不产生副作用
4. 性能达标（序列化 ≤1ms）
5. `cargo test -p sz-orm-dtx --features cross-lang-dtx` 新增测试全部通过
6. 附 `packages/sz-orm-dtx/src/cross_lang/serializer.rs` 新增代码的 file:line 证据

**依赖**：M1-T1（feature gate）、M1-T2（CrossLangTxError）

---

## M1-T4：CrossLangParticipant（跨语言参与者适配器）

**任务描述**：实现 `CrossLangParticipant` 适配器，将跨语言参与者（通过 gRPC/HTTP 远程调用）适配为既有 `TransactionParticipant`（`packages/sz-orm-dtx/src/lib.rs:182`），协调器透明编排 Rust 内部参与者与跨语言参与者，不修改既有事务执行逻辑。

**涉及文件**：
- `packages/sz-orm-dtx/src/cross_lang/participant.rs`（新增，CrossLangParticipant 适配器实现）

**复用标注**：
- 既有 `TransactionParticipant`（`packages/sz-orm-dtx/src/lib.rs:182`）：适配为既有参与者
- 既有 `ParticipantCallback`（`:179`）：远程调用包装为 `Arc<dyn Fn() -> Result<(), String> + Send + Sync>` 闭包
- 既有 `with_prepare/with_commit/with_rollback`（`:201/209/217`）：注册跨语言远程调用闭包
- 既有 `ParticipantState`（`:171`）：跨语言参与者状态复用（Failed 标记故障）
- 既有 tokio runtime `block_on`（`:155`）：闭包内阻塞等待异步结果

**子任务**：
- [ ] M1-T4.1 定义 `pub struct CrossLangParticipant { desc: CrossLangParticipantDesc, protocol: Box<dyn CrossLangParticipantProtocol>, serializer: CrossLangCompensationSerializer, timeout_ms: u64 }`（design.md `:598-602`）
- [ ] M1-T4.2 实现 `CrossLangParticipant::new(desc: CrossLangParticipantDesc, protocol: Box<dyn CrossLangParticipantProtocol>) -> Self`
- [ ] M1-T4.3 实现 `CrossLangParticipant::with_timeout(mut self, timeout_ms: u64) -> Self`（配置参与者超时，默认 5000ms，spec 4.1 性能 1）
- [ ] M1-T4.4 实现 `CrossLangParticipant::to_participant(&self) -> TransactionParticipant`：将远程调用包装为 `ParticipantCallback` 闭包，通过 `with_prepare/with_commit/with_rollback` 注册（design.md `:605`）
- [ ] M1-T4.5 在 prepare 闭包内：序列化补偿 payload → 调用 `protocol.prepare(tx_id, payload)` → tokio runtime `block_on` 阻塞等待异步结果 → 超时返回 `CrossLangTxError::Timeout`（design.md `:748`）
- [ ] M1-T4.6 在 commit 闭包内：调用 `protocol.commit(tx_id, payload)` → `block_on` 等待 → 结果映射为 `Result<(), String>`
- [ ] M1-T4.7 在 rollback 闭包内：构造补偿 payload（`build_rollback_payload`）→ 调用 `protocol.rollback(tx_id, compensation_payload)` → `block_on` 等待
- [ ] M1-T4.8 实现超时检测：闭包内用 `tokio::time::timeout` 包裹远程调用，超时标记 `ParticipantState::Failed`（`packages/sz-orm-dtx/src/lib.rs:249` `fail`）
- [ ] M1-T4.9 编写单元测试：`CrossLangParticipant::to_participant()` 返回 `TransactionParticipant`，`resource_id` 一致
- [ ] M1-T4.10 编写单元测试：适配后 `prepare/commit/rollback` 调用触发远程协议调用（mock protocol 验证）
- [ ] M1-T4.11 编写单元测试：远程调用超时返回 `Err("Timeout")`，参与者状态为 `Failed`
- [ ] M1-T4.12 编写单元测试：远程调用成功返回 `Ok(())`，参与者状态正确推进
- [ ] M1-T4.13 编写性能测试：单次参与者协调开销 ≤50ms（含 gRPC/HTTP 往返 + 参与者本地执行，spec 4.1 性能 1）

**验收标准**：
1. `CrossLangParticipant` 适配为既有 `TransactionParticipant`，复用 `with_prepare/with_commit/with_rollback` 注册
2. 闭包内 `block_on` 阻塞等待异步结果，保持回调签名兼容
3. 超时标记 `ParticipantState::Failed`，触发补偿/回滚
4. 性能达标（单次参与者协调 ≤50ms）
5. `cargo test -p sz-orm-dtx --features cross-lang-dtx` 新增测试全部通过
6. 附 `packages/sz-orm-dtx/src/cross_lang/participant.rs` 新增代码的 file:line 证据

**依赖**：M1-T1（feature gate）、M1-T2（Protocol）、M1-T3（Serializer）

---

## M1-T5：CrossLangParticipantRegistry（注册中心 + 鉴权 + 版本检查）

**任务描述**：实现 `CrossLangParticipantRegistry`，参与者注册中心，鉴权验证（mTLS/Token），协议版本兼容检查，复用既有 `DtxManager` 编排。

**涉及文件**：
- `packages/sz-orm-dtx/src/cross_lang/registry.rs`（新增，CrossLangParticipantRegistry 实现）

**复用标注**：
- 既有 `DtxManager`（`packages/sz-orm-dtx/src/lib.rs:428`）：协调器编排复用
- 既有 `DistributedTransaction`（`:266`）：跨语言参与者加入 participants 列表
- 既有 `TransactionLogStore`（`:53`）：跨语言事务日志持久化

**子任务**：
- [ ] M1-T5.1 定义 `pub struct CrossLangParticipantRegistry { participants: Arc<RwLock<HashMap<String, CrossLangParticipantDesc>>>, auth_verifier: Box<dyn AuthVerifier> }`
- [ ] M1-T5.2 定义 `pub trait AuthVerifier: Send + Sync` 含 `fn verify(&self, auth: &ParticipantAuth) -> Result<(), CrossLangTxError>`（mTLS/Token 鉴权验证）
- [ ] M1-T5.3 实现 `MtlsAuthVerifier`（mTLS 证书验证）+ `TokenAuthVerifier`（Token 验证）
- [ ] M1-T5.4 实现 `Registry::register(&self, desc: CrossLangParticipantDesc) -> Result<(), CrossLangTxError>`：鉴权验证 → 协议版本兼容检查 → 注册参与者（spec 5.1.1 规则 6）
- [ ] M1-T5.5 实现 `Registry::unregister(&self, resource_id: &str) -> Result<(), CrossLangTxError>`
- [ ] M1-T5.6 实现 `Registry::get(&self, resource_id: &str) -> Option<CrossLangParticipantDesc>`
- [ ] M1-T5.7 实现 `Registry::list(&self) -> Vec<CrossLangParticipantDesc>`（列出所有注册参与者）
- [ ] M1-T5.8 实现 `Registry::check_protocol_version(coordinator_version: u32, participant_version: u32) -> Result<(), CrossLangTxError>`：版本兼容检查，不兼容返回 `ProtocolVersionMismatch`（spec 4.4 规则 2）
- [ ] M1-T5.9 编写单元测试：未授权参与者注册返回 `CrossLangTxError::AuthFailed`（spec 5.1.1 规则 6 验收条件）
- [ ] M1-T5.10 编写单元测试：协议版本不兼容返回 `ProtocolVersionMismatch`
- [ ] M1-T5.11 编写单元测试：授权 + 版本兼容注册成功，`list()` 包含注册参与者
- [ ] M1-T5.12 编写单元测试：`unregister` 后 `get` 返回 `None`

**验收标准**：
1. `CrossLangParticipantRegistry` 支持注册/注销/查询/列表
2. mTLS/Token 鉴权验证，未授权拒绝注册
3. 协议版本兼容检查，不兼容拒绝注册
4. `cargo test -p sz-orm-dtx --features cross-lang-dtx` 新增测试全部通过
5. 附 `packages/sz-orm-dtx/src/cross_lang/registry.rs` 新增代码的 file:line 证据

**依赖**：M1-T1（feature gate）、M1-T2（Protocol/Desc）

---

## M1-T6：CrossLangTxAlerter（故障告警）+ 跨语言事务可观测

**任务描述**：实现 `CrossLangTxAlerter`（跨语言参与者故障/超时/恢复冲突告警）+ 跨语言事务结构化日志（事务 ID/参与者列表/语言/状态/耗时/补偿结果），接入既有 `sz-orm-tracing` + `sz-orm-observability`，跨语言参与者 span 关联。

**涉及文件**：
- `packages/sz-orm-dtx/src/cross_lang/alerter.rs`（新增，CrossLangTxAlerter 实现）
- `packages/sz-orm-dtx/src/cross_lang/observability.rs`（新增，跨语言事务可观测）

**复用标注**：
- 既有 `Tracer` trait（`packages/sz-orm-tracing/src/lib.rs:129`）：跨语言 span 关联复用
- 既有 `SzTracer`（`:136`）：自研追踪器实现复用
- 既有 `Tracer::inject/extract`：trace context 传播复用

**子任务**：
- [ ] M1-T6.1 定义 `pub enum CrossLangTxAlert { ParticipantTimeout { participant: String, tx_id: String }, AuthFailed { participant: String }, RecoveryConflict { participant: String, tx_id: String }, CompensationFailed { participant: String, tx_id: String, reason: String } }`
- [ ] M1-T6.2 定义 `pub trait CrossLangTxAlerter: Send + Sync` 含 `fn alert(&self, alert: CrossLangTxAlert) -> Result<(), String>`
- [ ] M1-T6.3 实现 `LoggingTxAlerter`（结构化日志告警）+ `WebhookTxAlerter`（HTTP webhook 告警）
- [ ] M1-T6.4 在 `observability.rs` 定义 `pub struct CrossLangTxSpan { tx_id: String, participant_id: String, language: ParticipantLanguage, state: String, duration_ms: u64, compensation_result: Option<String> }`（spec 4.4 规则 1）
- [ ] M1-T6.5 实现 `CrossLangTxSpan::to_log(&self) -> serde_json::Value`（结构化日志输出，含事务 ID/参与者列表/语言/状态/耗时/补偿结果）
- [ ] M1-T6.6 实现 `CrossLangTxSpan::attach_to_tracer(&self, tracer: &dyn Tracer) -> Span`（接入既有 sz-orm-tracing，跨语言参与者 span 关联，design.md `:234`）
- [ ] M1-T6.7 实现 trace context 传播：`inject` 协调器 trace context 到 gRPC/HTTP 请求 header，参与者侧 `extract` 关联 span（复用既有 `Tracer::inject/extract`）
- [ ] M1-T6.8 定义 Prometheus 指标：`cross_lang_tx_participant_count`（Gauge）/ `cross_lang_tx_duration_seconds`（Histogram）/ `cross_lang_tx_compensation_total`（Counter）（design.md `:1585-1587`）
- [ ] M1-T6.9 编写单元测试：`LoggingTxAlerter` 输出结构化日志含参与者/事务 ID/告警类型
- [ ] M1-T6.10 编写单元测试：`CrossLangTxSpan::to_log` 含所有字段（tx_id/participant_id/language/state/duration_ms/compensation_result）
- [ ] M1-T6.11 编写单元测试：trace context `inject`/`extract` 往返一致
- [ ] M1-T6.12 编写单元测试：Prometheus 指标正确注册与采集

**验收标准**：
1. `CrossLangTxAlerter` 支持故障/超时/恢复冲突/补偿失败告警，不静默
2. 跨语言事务结构化日志含事务 ID/参与者列表/语言/状态/耗时/补偿结果
3. 跨语言参与者 span 关联，trace context 传播
4. Prometheus 指标正确注册
5. `cargo test -p sz-orm-dtx --features cross-lang-dtx` 新增测试全部通过
6. 附 `packages/sz-orm-dtx/src/cross_lang/alerter.rs` 与 `observability.rs` 新增代码的 file:line 证据

**依赖**：M1-T1（feature gate）、M1-T2（Protocol）

---

## M1-T7：协调器崩溃恢复 + 故障隔离

**任务描述**：扩展既有 `DtxManager` 编排跨语言参与者，复用既有 `TransactionLogStore` + `recovery` 实现崩溃恢复，复用既有 `ParticipantState` 实现故障隔离，不修改既有事务状态机。

**涉及文件**：
- `packages/sz-orm-dtx/src/cross_lang/recovery.rs`（新增，跨语言事务崩溃恢复）
- `packages/sz-orm-dtx/src/cross_lang/manager_ext.rs`（新增，DtxManager 跨语言扩展方法）

**复用标注**：
- 既有 `DtxManager`（`packages/sz-orm-dtx/src/lib.rs:428`）：透明编排 Rust + 跨语言参与者
- 既有 `TransactionLogStore`（`:53`）：跨语言事务日志持久化（崩溃恢复）
- 既有 `recovery`（`:25`）：XA 崩溃恢复机制复用
- 既有 `suspension`（`:27`）：XA 悬挂检测复用
- 既有 `TransactionState`（`:159`）：事务状态机复用
- 既有 `ParticipantState`（`:171`）：参与者状态机复用（Failed 标记故障）

**子任务**：
- [ ] M1-T7.1 在 `manager_ext.rs` 实现 `DtxManagerExt` 扩展方法 `begin_cross_lang_tx(&self, tx_type: TxType, participants: Vec<CrossLangParticipant>) -> Result<String, CrossLangTxError>`：创建 `DistributedTransaction`，跨语言参与者通过 `to_participant()` 加入 participants 列表（design.md `:605`）
- [ ] M1-T7.2 实现 `DtxManagerExt::commit_cross_lang_tx(&self, tx_id: &str) -> Result<(), CrossLangTxError>`：复用既有 `DtxManager` 编排 commit，跨语言参与者远程调用
- [ ] M1-T7.3 实现 `DtxManagerExt::rollback_cross_lang_tx(&self, tx_id: &str) -> Result<(), CrossLangTxError>`：复用既有 `DtxManager` 编排 rollback，跨语言参与者远程补偿
- [ ] M1-T7.4 在 `recovery.rs` 实现 `CrossLangTxRecovery::recover(&self, log_store: &dyn TransactionLogStore) -> Result<Vec<DistributedTransaction>, CrossLangTxError>`：从日志恢复未完成跨语言事务（design.md `:669-675`）
- [ ] M1-T7.5 实现恢复逻辑：按 `TransactionState` 判断 `Preparing/Prepared` 继续 commit、`Committing` 继续未完成 commit、`RollingBack` 继续 rollback
- [ ] M1-T7.6 实现恢复冲突检测：参与者已被其他协调器回滚时返回 `CrossLangTxError::RecoveryConflict`，告警人工处理，不盲目重试（spec 5.1.3 异常 3）
- [ ] M1-T7.7 实现故障隔离：单个跨语言参与者故障（超时/不可达）标记 `ParticipantState::Failed`（`packages/sz-orm-dtx/src/lib.rs:249` `fail`），触发补偿/回滚其他参与者，不阻塞整个事务（spec 5.1.1 规则 8）
- [ ] M1-T7.8 实现补偿失败告警：补偿回调失败时 `CrossLangTxAlerter::alert(CompensationFailed)`，记录失败参与者待人工处理，不静默（spec 4.3 规则 2）
- [ ] M1-T7.9 编写单元测试：`begin_cross_lang_tx` 创建事务含跨语言参与者，复用 `DtxManager` 编排
- [ ] M1-T7.10 编写集成测试：事务含 Rust 参与者 A + Go 参与者 B + Java 参与者 C，统一编排，A 本地调用，B/C 通过 gRPC 远程调用（spec 5.1.1 规则 2 验收条件）
- [ ] M1-T7.11 编写集成测试：Saga 事务 Go 参与者 B 失败，触发补偿，协调器发送 rollback 给 B，B 执行 Go 侧补偿，结果回传（spec 5.1.1 规则 3 验收条件）
- [ ] M1-T7.12 编写集成测试：协调器在跨语言事务提交中崩溃，重启后从 `TransactionLogStore` 恢复事务，继续提交或回滚跨语言参与者（spec 5.1.1 规则 7 验收条件）
- [ ] M1-T7.13 编写边界测试：跨语言参与者超时（gRPC 往返超时），标记故障，回滚其他参与者，告警（spec 5.1.3 异常 1）
- [ ] M1-T7.14 编写边界测试：协调器崩溃恢复冲突（参与者已被其他协调器回滚），检测冲突，告警人工处理
- [ ] M1-T7.15 编写性能测试：Saga 10 个跨语言参与者编排 ≤1 秒（spec 4.1 性能 1）

**验收标准**：
1. `DtxManagerExt` 透明编排 Rust + 跨语言参与者，不修改既有事务状态机
2. 协调器崩溃恢复复用既有 `TransactionLogStore` + `recovery`，跨语言参与者状态从日志恢复
3. 恢复冲突检测告警人工处理，不盲目重试
4. 单个参与者故障隔离，不阻塞整个事务
5. 补偿失败告警不静默
6. 性能达标（Saga 10 个跨语言参与者 ≤1s）
7. `cargo test -p sz-orm-dtx --features cross-lang-dtx` 新增测试全部通过
8. 附 `packages/sz-orm-dtx/src/cross_lang/recovery.rs` 与 `manager_ext.rs` 新增代码的 file:line 证据

**依赖**：M1-T1（feature gate）、M1-T4（CrossLangParticipant）、M1-T5（Registry）、M1-T6（Alerter）

---

## M1-T8：M1 集成测试与门禁验证

**任务描述**：M1-001 跨语言分布式事务集成测试与门禁验证，确保 REQ-V42-001 全部验收条件满足。

**涉及文件**：
- `packages/sz-orm-dtx/tests/cross_lang_integration.rs`（新增，集成测试）
- `packages/sz-orm-dtx/tests/cross_lang_e2e.rs`（新增，端到端测试，含 mock Go/Java 服务）

**子任务**：
- [ ] M1-T8.1 编写集成测试：Go 服务实现 `ParticipantProtocol` gRPC 接口，注册到 `DtxManager`，参与 Saga 事务（spec 5.1.1 规则 1 验收条件）
- [ ] M1-T8.2 编写集成测试：跨语言事务全部成功 commit，验证原子性（全部提交，spec 4.2 规则 1）
- [ ] M1-T8.3 编写集成测试：跨语言事务任一失败 rollback，验证原子性（全部回滚，spec 4.2 规则 1）
- [ ] M1-T8.4 编写集成测试：跨语言事务可观测，日志含参与者语言/状态/耗时，tracing span 关联（spec 5.1.1 规则 9 验收条件）
- [ ] M1-T8.5 编写端到端测试：mock Go 服务 + mock Java 服务 + Rust 参与者，完整 Saga 事务流程
- [ ] M1-T8.6 运行 `cargo test -p sz-orm-dtx --features cross-lang-dtx` 全部通过
- [ ] M1-T8.7 运行 `cargo clippy -p sz-orm-dtx --features cross-lang-dtx -- -D warnings` 通过
- [ ] M1-T8.8 运行 `cargo fmt -p sz-orm-dtx -- --check` 通过
- [ ] M1-T8.9 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-dtx/src/cross_lang/` 无占位实现
- [ ] M1-T8.10 验证 `cargo check -p sz-orm-dtx`（默认 feature）行为与 v4.1.0 一致（spec 5.1.1 规则 10 验收条件）

**验收标准**：
1. REQ-V42-001 全部 10 条业务规则验收条件满足
2. 集成测试与端到端测试全部通过
3. clippy/fmt/占位检查门禁通过
4. 默认 feature 行为与 v4.1.0 一致
5. 附集成测试运行输出证据

**依赖**：M1-T1~M1-T7 全部完成

---

# 三、M2：Go/Java/C++ 绑定（REQ-V42-002，P2）

**目标**：参照既有 `sz-orm-python`（PyO3）/`sz-orm-js`（napi-rs）模式，新增 `sz-orm-cabi`（C ABI 导出层）+ `sz-orm-go`（cgo）+ `sz-orm-java`（JNI）+ `sz-orm-cpp`（extern "C" + cxxbindgen）四包，暴露 Model/QueryBuilder/Pool/Transaction 核心 API，含 FFI 内存管理 + panic 捕获 + 异步运行时桥接 + 错误码映射 + 目标语言文档。
**预期工作量**：2.5 周
**对应需求**：REQ-V42-002（spec.md 5.2，design.md 2.2.2 REQ-V42-002）
**依赖**：无强依赖（复用既有 sz-orm-core + python/js 模式；与 M1 存在可选协同：跨语言事务参与者可经语言绑定接入，但参与者亦可经 gRPC/HTTP 直接接入）

## M2-T1：sz-orm-cabi 基础包搭建 + feature gate 体系

**任务描述**：新增 `sz-orm-cabi` 基础包，作为三套绑定（Go/Java/C++）共用的 C ABI 导出层，参照既有 `sz-orm-python`/`sz-orm-js` 的 `crate-type = ["cdylib", "rlib"]` 模式。

**涉及文件**：
- `packages/sz-orm-cabi/Cargo.toml`（新增包，crate-type = ["cdylib", "rlib"]，依赖 sz-orm-core + tokio）
- `packages/sz-orm-cabi/src/lib.rs`（新增，C ABI 导出层入口）
- `Cargo.toml`（workspace.members 新增 `packages/sz-orm-cabi`）

**复用标注**：
- 既有 `sz-orm-python`（`packages/sz-orm-python/Cargo.toml:15`）：cdylib + rlib 模式参考
- 既有 `sz-orm-js`（`packages/sz-orm-js/Cargo.toml:15`）：cdylib + rlib 模式参考
- 既有 `sz-orm-core`：核心 API 复用
- 既有 tokio（workspace 依赖）：异步运行时桥接

**子任务**：
- [ ] M2-T1.1 创建 `packages/sz-orm-cabi/Cargo.toml`：`[package] name = "sz-orm-cabi"`，`[lib] crate-type = ["cdylib", "rlib"]`，依赖 `sz-orm-core = { workspace = true }` + `tokio = { workspace = true }`（design.md `:758`）
- [ ] M2-T1.2 在 `Cargo.toml` workspace.members 新增 `"packages/sz-orm-cabi"`
- [ ] M2-T1.3 创建 `packages/sz-orm-cabi/src/lib.rs`，定义句柄类型 `pub type SzOrmPoolHandle = *mut std::ffi::c_void` / `SzOrmQueryBuilderHandle` / `SzOrmTransactionHandle`（design.md `:770-772`）
- [ ] M2-T1.4 定义错误码枚举 `pub enum SzOrmErrorCode { Ok = 0, NotFound = 1, ConnectionFailed = 2, QueryFailed = 3, PoolExhausted = 4, TransactionAborted = 5, Panic = 6, InvalidArgument = 7, RuntimeNotInitialized = 8, MemoryLeak = 9 }`（design.md `:775-785`）
- [ ] M2-T1.5 定义 `pub struct PoolConfigC { pub max_connections: u32, pub min_connections: u32, pub connect_timeout_ms: u64, pub idle_timeout_ms: u64 }`（C ABI 兼容的连接池配置）
- [ ] M2-T1.6 验证 `cargo check -p sz-orm-cabi` 编译通过
- [ ] M2-T1.7 验证 `cargo check --workspace --all-targets` 编译通过（workspace 集成）
- [ ] M2-T1.8 验证既有 `sz-orm-python`/`sz-orm-js` 行为不变（不修改既有绑定）

**验收标准**：
1. `sz-orm-cabi` 包创建成功，crate-type = ["cdylib", "rlib"]
2. 句柄类型与错误码定义完整
3. workspace 集成编译通过
4. 既有 python/js 绑定行为不变
5. 附 `packages/sz-orm-cabi/Cargo.toml` 与 `src/lib.rs` 新增代码的 file:line 证据

**依赖**：无（基础设施任务，M2-002 所有任务依赖此任务）

---

## M2-T2：FfiMemoryManager + FfiPanicGuard（FFI 内存管理 + panic 捕获）

**任务描述**：在 sz-orm-cabi 实现 `FfiMemoryManager`（FFI 内存分配/释放，Rust 侧分配/释放，语言侧引用，不泄漏不悬空）+ `FfiPanicGuard`（`std::panic::catch_unwind` 捕获 panic 转错误码，不跨语言边界 UB 防护）。

**涉及文件**：
- `packages/sz-orm-cabi/src/ffi_memory.rs`（新增，FfiMemoryManager 实现）
- `packages/sz-orm-cabi/src/panic_guard.rs`（新增，FfiPanicGuard 实现）

**复用标注**：无（FFI 边界安全基础实现）

**子任务**：
- [ ] M2-T2.1 在 `ffi_memory.rs` 定义 `pub struct FfiMemoryManager { allocations: Mutex<HashMap<usize, usize>> }`（跟踪分配地址与大小）
- [ ] M2-T2.2 实现 `FfiMemoryManager::alloc(&self, size: usize) -> *mut std::ffi::c_void`：Rust 侧分配内存，记录分配（design.md `:945`）
- [ ] M2-T2.3 实现 `FfiMemoryManager::free(&self, ptr: *mut std::ffi::c_void)`：释放 Rust 侧分配的内存，移除记录
- [ ] M2-T2.4 实现 `#[no_mangle] pub extern "C" fn sz_orm_free(ptr: *mut std::ffi::c_void)`：统一释放函数，供目标语言 RAII/defer/finally 调用（design.md `:801`）
- [ ] M2-T2.5 实现 `FfiMemoryManager::check_leak(&self) -> Vec<usize>`：检测未释放的分配（用于测试验证）
- [ ] M2-T2.6 在 `panic_guard.rs` 定义 `pub fn ffi_panic_guard<F, R>(f: F) -> Result<R, SzOrmErrorCode> where F: FnOnce() -> R + std::panic::UnwindSafe`：用 `std::panic::catch_unwind` 捕获 panic（design.md `:944`）
- [ ] M2-T2.7 实现 `ffi_panic_guard_with_msg<F, R>(f: F) -> Result<R, (SzOrmErrorCode, String)>`：捕获 panic 并返回 panic message
- [ ] M2-T2.8 在每个 `extern "C"` 函数内用 `ffi_panic_guard` 包裹，确保 panic 不跨语言边界（SAFETY 注释）
- [ ] M2-T2.9 编写 SAFETY 注释：`// SAFETY: FFI 边界 panic 捕获，catch_unwind 确保 panic 不跨语言边界（UB 防护），错误码 SzOrmErrorCode::Panic 返回`
- [ ] M2-T2.10 编写单元测试：`FfiMemoryManager::alloc` + `free` 往返，`check_leak` 返回空
- [ ] M2-T2.11 编写单元测试：未调用 `free` 时 `check_leak` 返回未释放分配
- [ ] M2-T2.12 编写单元测试：`ffi_panic_guard` 捕获 panic 转 `SzOrmErrorCode::Panic`
- [ ] M2-T2.13 编写单元测试：`ffi_panic_guard` 正常执行返回 `Ok(R)`
- [ ] M2-T2.14 编写边界测试：ASan/valgrind 验证无内存泄漏（spec 4.2 规则 3）

**验收标准**：
1. `FfiMemoryManager` 跟踪分配/释放，`sz_orm_free` 统一释放函数
2. `FfiPanicGuard` 捕获 panic 转错误码，不跨语言边界
3. SAFETY 注释完整（每个 unsafe 块/FFI 边界）
4. ASan/valgrind 验证无内存泄漏
5. `cargo test -p sz-orm-cabi` 新增测试全部通过
6. 附 `packages/sz-orm-cabi/src/ffi_memory.rs` 与 `panic_guard.rs` 新增代码的 file:line 证据

**依赖**：M2-T1（sz-orm-cabi 基础包）

---

## M2-T3：AsyncRuntimeBridge + ErrorCodeMapper（异步运行时桥接 + 错误码映射）

**任务描述**：实现 `AsyncRuntimeBridge`（桥接 tokio 异步运行时与目标语言异步机制）+ `ErrorCodeMapper`（完整映射 sz-orm-core 错误码到 SzOrmErrorCode）。

**涉及文件**：
- `packages/sz-orm-cabi/src/async_bridge.rs`（新增，AsyncRuntimeBridge 实现）
- `packages/sz-orm-cabi/src/error_mapper.rs`（新增，ErrorCodeMapper 实现）

**复用标注**：
- 既有 tokio（workspace 依赖）：异步运行时桥接
- 既有 `sz-orm-core` 错误类型：错误码映射源

**子任务**：
- [ ] M2-T3.1 在 `async_bridge.rs` 定义 `pub struct AsyncRuntimeBridge { runtime: OnceLock<tokio::runtime::Runtime> }`（全局 tokio 运行时，OnceLock 保证单例）
- [ ] M2-T3.2 实现 `AsyncRuntimeBridge::init()`：初始化 tokio 运行时（多线程，worker_threads 配置）
- [ ] M2-T3.3 实现 `AsyncRuntimeBridge::block_on<F: Future>(future: F) -> F::Output`：在 tokio runtime 上 `block_on` 执行异步查询（design.md `:818`）
- [ ] M2-T3.4 实现 `#[no_mangle] pub extern "C" fn sz_orm_init() -> SzOrmErrorCode`：初始化 tokio 运行时，供目标语言调用
- [ ] M2-T3.5 实现运行时未初始化检查：未初始化返回 `SzOrmErrorCode::RuntimeNotInitialized`（spec 5.2.3 异常 3）
- [ ] M2-T3.6 在 `error_mapper.rs` 实现 `pub fn map_error(err: &sz_orm_core::DbError) -> SzOrmErrorCode`：完整映射 sz-orm-core 错误码（NotFound/ConnectionFailed/QueryFailed/PoolExhausted/TransactionAborted 等，design.md `:919-921`）
- [ ] M2-T3.7 实现 `pub fn error_code_to_message(code: SzOrmErrorCode) -> &'static str`：错误码转人类可读消息
- [ ] M2-T3.8 编写单元测试：`AsyncRuntimeBridge::init` + `block_on` 正常执行异步 future
- [ ] M2-T3.9 编写单元测试：未初始化调用 `block_on` 返回 `RuntimeNotInitialized`
- [ ] M2-T3.10 编写单元测试：`map_error` 完整映射所有 sz-orm-core 错误码
- [ ] M2-T3.11 编写单元测试：`error_code_to_message` 每个错误码返回非空消息

**验收标准**：
1. `AsyncRuntimeBridge` 初始化 tokio 运行时，`block_on` 执行异步查询
2. 未初始化返回明确错误码
3. `ErrorCodeMapper` 完整映射 sz-orm-core 错误码，不丢失错误信息
4. `cargo test -p sz-orm-cabi` 新增测试全部通过
5. 附 `packages/sz-orm-cabi/src/async_bridge.rs` 与 `error_mapper.rs` 新增代码的 file:line 证据

**依赖**：M2-T1（sz-orm-cabi 基础包）

---

## M2-T4：C ABI 导出 Model/QueryBuilder/Pool/Transaction

**任务描述**：通过 `extern "C"` 暴露 sz-orm-core 核心 API（Model/QueryBuilder/Pool/Transaction）为 C ABI，每个导出函数用 `FfiPanicGuard` 包裹，复用既有 sz-orm-core API。

**涉及文件**：
- `packages/sz-orm-cabi/src/pool_c.rs`（新增，Pool C ABI 导出）
- `packages/sz-orm-cabi/src/query_builder_c.rs`（新增，QueryBuilder C ABI 导出）
- `packages/sz-orm-cabi/src/transaction_c.rs`（新增，Transaction C ABI 导出）
- `packages/sz-orm-cabi/src/model_c.rs`（新增，Model C ABI 导出）

**复用标注**：
- 既有 `Model` trait（`packages/sz-orm-core/src/model.rs:37`）：C ABI 导出 Model 元数据
- 既有 `QueryBuilder`（`packages/sz-orm-core/src/query.rs:36`）：C ABI 导出查询构建器方法
- 既有 `Connection` trait（`packages/sz-orm-core/src/pool.rs:45`）：C ABI 导出连接操作
- 既有 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）：C ABI 导出连接池
- 既有 `Transaction`（`packages/sz-orm-core/src/transaction.rs:159`）：C ABI 导出事务
- 既有 `TransactionManager`（`:527`）：C ABI 导出事务管理

**子任务**：
- [ ] M2-T4.1 在 `pool_c.rs` 实现 `#[no_mangle] pub extern "C" fn sz_orm_pool_new(dsn: *const c_char, config: *const PoolConfigC) -> SzOrmPoolHandle`：创建连接池，FFiPanicGuard 包裹（design.md `:789`）
- [ ] M2-T4.2 实现 `sz_orm_pool_free(pool: SzOrmPoolHandle)`：释放连接池
- [ ] M2-T4.3 在 `query_builder_c.rs` 实现 `#[no_mangle] pub extern "C" fn sz_orm_query_builder_new(pool: SzOrmPoolHandle, table: *const c_char) -> SzOrmQueryBuilderHandle`（design.md `:792`）
- [ ] M2-T4.4 实现 `sz_orm_query_builder_where_eq(qb: SzOrmQueryBuilderHandle, col: *const c_char, val: *const c_char) -> SzOrmErrorCode`（参数化查询，禁止 SQL 拼接，design.md `:795`）
- [ ] M2-T4.5 实现 `sz_orm_query_builder_or_where_eq` / `sz_orm_query_builder_order` / `sz_orm_query_builder_limit` / `sz_orm_query_builder_offset`
- [ ] M2-T4.6 实现 `sz_orm_query_builder_first(qb: SzOrmQueryBuilderHandle, out: *mut *mut c_char) -> SzOrmErrorCode`：查询单条，结果序列化为 JSON 返回（design.md `:798`）
- [ ] M2-T4.7 实现 `sz_orm_query_builder_get(qb: SzOrmQueryBuilderHandle, out: *mut *mut c_char) -> SzOrmErrorCode`：查询多条
- [ ] M2-T4.8 实现 `sz_orm_query_builder_insert` / `update` / `delete`（参数化）
- [ ] M2-T4.9 实现 `sz_orm_query_builder_free(qb: SzOrmQueryBuilderHandle)`
- [ ] M2-T4.10 在 `transaction_c.rs` 实现 `sz_orm_tx_begin(pool: SzOrmPoolHandle) -> SzOrmTransactionHandle` / `sz_orm_tx_commit` / `sz_orm_tx_rollback` / `sz_orm_tx_free`
- [ ] M2-T4.11 在 `model_c.rs` 实现 `sz_orm_model_table_name(handle) -> *mut c_char` / `sz_orm_model_pk_name` 等 Model 元数据导出
- [ ] M2-T4.12 每个导出函数加 SAFETY 注释：`// SAFETY: FFI 边界，参数边界检查 + panic 捕获 + 内存由 Rust 侧管理`
- [ ] M2-T4.13 编写单元测试：`sz_orm_pool_new` + `sz_orm_query_builder_new` + `where_eq` + `first` 完整流程（mock 连接）
- [ ] M2-T4.14 编写单元测试：参数化查询验证（`where_eq` 参数化，禁止 SQL 拼接）
- [ ] M2-T4.15 编写单元测试：panic 触发时返回 `SzOrmErrorCode::Panic`，无 UB
- [ ] M2-T4.16 编写性能测试：单次 FFI 调用开销 ≤10μs（含 cgo/JNI/extern "C" 边界 + 参数序列化，spec 4.1 性能 2）

**验收标准**：
1. C ABI 完整导出 Model/QueryBuilder/Pool/Transaction 核心 API
2. 所有导出函数 FfiPanicGuard 包裹，panic 转错误码
3. 参数化查询，禁止 SQL 拼接
4. SAFETY 注释完整
5. 性能达标（单次 FFI 调用 ≤10μs）
6. `cargo test -p sz-orm-cabi` 新增测试全部通过
7. 附 `packages/sz-orm-cabi/src/pool_c.rs` 等新增代码的 file:line 证据

**依赖**：M2-T1（基础包）、M2-T2（FfiMemoryManager/FfiPanicGuard）、M2-T3（AsyncRuntimeBridge/ErrorCodeMapper）

---

## M2-T5：sz-orm-go（Go 绑定 + Go wrapper + goroutine 桥接）

**任务描述**：新增 `sz-orm-go` 包，Go 绑定（cgo + C ABI + Go wrapper），暴露 Model/QueryBuilder/Pool/Transaction，含 Go wrapper（惯用 Go API 风格）+ goroutine 异步桥接 + 错误码映射 + Go doc 文档。

**涉及文件**：
- `packages/sz-orm-go/Cargo.toml`（新增包，crate-type = ["cdylib", "rlib"]，feature `lang-binding-go`）
- `packages/sz-orm-go/src/lib.rs`（新增，Rust 侧 cgo 桥接）
- `packages/sz-orm-go/go/szorm/pool.go`（新增，Go wrapper Pool）
- `packages/sz-orm-go/go/szorm/query_builder.go`（新增，Go wrapper QueryBuilder）
- `packages/sz-orm-go/go/szorm/transaction.go`（新增，Go wrapper Transaction）
- `packages/sz-orm-go/go/szorm/errors.go`（新增，Go 错误码映射）
- `Cargo.toml`（workspace.members 新增 `packages/sz-orm-go`）

**复用标注**：
- 既有 `sz-orm-python`（`packages/sz-orm-python/Cargo.toml:15`）：cdylib + rlib 模式参考
- M2-T4 `sz-orm-cabi` C ABI 导出层：cgo 调用

**子任务**：
- [ ] M2-T5.1 创建 `packages/sz-orm-go/Cargo.toml`：`[lib] crate-type = ["cdylib", "rlib"]`，`[features] lang-binding-go = ["sz-orm-cabi"]`，依赖 `sz-orm-cabi = { workspace = true, optional = true }`（design.md `:888-893`）
- [ ] M2-T5.2 在 `Cargo.toml` workspace.members 新增 `"packages/sz-orm-go"`
- [ ] M2-T5.3 创建 `src/lib.rs`，Rust 侧 cgo 桥接（重导出 sz-orm-cabi C ABI）
- [ ] M2-T5.4 创建 `go/szorm/pool.go`：`type Pool struct { handle C.SzOrmPoolHandle }`，`func NewPool(dsn string) (*Pool, error)`（cgo 调用 `sz_orm_pool_new`，design.md `:809-812`）
- [ ] M2-T5.5 创建 `go/szorm/query_builder.go`：`type QueryBuilder struct { handle C.SzOrmQueryBuilderHandle }`，`func (p *Pool) QueryBuilder(table string) *QueryBuilder`，`func (qb *QueryBuilder) WhereEq(col string, val interface{}) *QueryBuilder`，`func (qb *QueryBuilder) First() (map[string]interface{}, error)`（design.md `:814-815`）
- [ ] M2-T5.6 实现 Go wrapper 方法：`OrWhereEq` / `Order` / `Limit` / `Get` / `Insert` / `Update` / `Delete`
- [ ] M2-T5.7 创建 `go/szorm/transaction.go`：`type Transaction struct { handle C.SzOrmTransactionHandle }`，`func (p *Pool) Begin() (*Transaction, error)` / `func (tx *Transaction) Commit() error` / `func (tx *Transaction) Rollback() error`
- [ ] M2-T5.8 创建 `go/szorm/errors.go`：完整映射 `SzOrmErrorCode` 到 Go error（`ErrNotFound`/`ErrConnectionFailed`/`ErrQueryFailed`/`ErrPoolExhausted`/`ErrPanic` 等，design.md `:921`）
- [ ] M2-T5.9 实现 goroutine 异步桥接：Go 异步调用 `First()` 不阻塞 goroutine 调度，结果通过 channel 返回（spec 5.2.1 规则 7 验收条件）
- [ ] M2-T5.10 实现 `defer sz_orm_free` RAII：Go 侧用 defer 确保 Rust 分配的内存释放
- [ ] M2-T5.11 创建 `go/szorm/doc.go`：Go doc 格式文档 + 示例代码 + 错误码表（spec 5.2.1 规则 9）
- [ ] M2-T5.12 编写 Go 测试：`pool.QueryBuilder("users").WhereEq("id", 1).First()` 通过 cgo 调用 Rust C ABI，返回 Go 结构体，行为与 Rust 一致（spec 5.2.1 规则 1 验收条件）
- [ ] M2-T5.13 编写 Go 测试：相同 CRUD 序列行为与 Rust sz-orm-core 一致（spec 5.2.1 规则 4 验收条件）
- [ ] M2-T5.14 编写 Go 测试：Rust panic 触发时 Go 返回 `ErrPanic`，无 UB（spec 5.2.1 规则 6 验收条件）
- [ ] M2-T5.15 编写性能测试：批量查询 1,000 行 ≤50ms（含 FFI + 反序列化，spec 4.1 性能 2）

**验收标准**：
1. `sz-orm-go` 包创建成功，cgo + Go wrapper 惯用 API
2. 核心 API（Model/QueryBuilder/Pool/Transaction）完整暴露
3. goroutine 异步桥接，不阻塞主线程
4. 错误码完整映射，panic 转错误码无 UB
5. Go doc 文档 + 示例 + 错误码表
6. 性能达标（批量 1,000 行 ≤50ms）
7. `cargo test -p sz-orm-go --features lang-binding-go` + `go test ./...` 全部通过
8. 附 `packages/sz-orm-go/` 新增代码的 file:line 证据

**依赖**：M2-T1（sz-orm-cabi）、M2-T4（C ABI 导出）

---

## M2-T6：sz-orm-java（Java 绑定 + Java wrapper + CompletableFuture 桥接）

**任务描述**：新增 `sz-orm-java` 包，Java 绑定（JNI + C ABI + Java wrapper），暴露 Model/QueryBuilder/Pool/Transaction，含 Java wrapper（惯用 Java API 风格）+ CompletableFuture/虚拟线程异步桥接 + 错误码映射 + Javadoc 文档。

**涉及文件**：
- `packages/sz-orm-java/Cargo.toml`（新增包，crate-type = ["cdylib", "rlib"]，feature `lang-binding-java`）
- `packages/sz-orm-java/src/lib.rs`（新增，Rust 侧 JNI 桥接）
- `packages/sz-orm-java/java/src/main/java/com/szorm/Pool.java`（新增，Java wrapper Pool）
- `packages/sz-orm-java/java/src/main/java/com/szorm/QueryBuilder.java`（新增，Java wrapper QueryBuilder）
- `packages/sz-orm-java/java/src/main/java/com/szorm/Transaction.java`（新增，Java wrapper Transaction）
- `packages/sz-orm-java/java/src/main/java/com/szorm/exceptions/`（新增，Java 异常类）
- `Cargo.toml`（workspace.members 新增 `packages/sz-orm-java`）

**复用标注**：
- 既有 `sz-orm-python`（`packages/sz-orm-python/Cargo.toml:15`）：cdylib + rlib 模式参考
- M2-T4 `sz-orm-cabi` C ABI 导出层：JNI 调用

**子任务**：
- [ ] M2-T6.1 创建 `packages/sz-orm-java/Cargo.toml`：`[lib] crate-type = ["cdylib", "rlib"]`，`[features] lang-binding-java = ["sz-orm-cabi"]`（design.md `:895-900`）
- [ ] M2-T6.2 在 `Cargo.toml` workspace.members 新增 `"packages/sz-orm-java"`
- [ ] M2-T6.3 创建 `src/lib.rs`，Rust 侧 JNI 桥接（JNI_OnLoad 初始化 tokio 运行时）
- [ ] M2-T6.4 创建 `java/.../Pool.java`：`public class Pool { private long handle; }`，`public Pool(String dsn) throws SzOrmException`（JNI 调用 `sz_orm_pool_new`）
- [ ] M2-T6.5 创建 `java/.../QueryBuilder.java`：`public QueryBuilder whereEq(String col, Object val)`，`public Map<String, Object> first() throws SzOrmException`（spec 5.2.1 规则 2 验收条件）
- [ ] M2-T6.6 实现 Java wrapper 方法：`orWhereEq` / `order` / `limit` / `get` / `insert` / `update` / `delete`
- [ ] M2-T6.7 创建 `java/.../Transaction.java`：`public static Transaction begin(Pool pool)` / `public void commit()` / `public void rollback()`
- [ ] M2-T6.8 创建 `java/.../exceptions/`：`SzOrmException` 基类 + `NotFoundException` / `ConnectionFailedException` / `QueryFailedException` / `PoolExhaustedException` / `SzOrmPanicException`（design.md `:921`）
- [ ] M2-T6.9 实现 CompletableFuture 异步桥接：`public CompletableFuture<Map<String, Object>> firstAsync()` 不阻塞主线程，结果异步返回（spec 5.2.1 觺则 7 验收条件）
- [ ] M2-T6.10 实现 `finally { sz_orm_free }` RAII：Java 侧用 finally 确保 Rust 分配的内存释放
- [ ] M2-T6.11 创建 Javadoc 文档：`java/.../doc-files/` 含示例代码 + 错误码表（spec 5.2.1 规则 9）
- [ ] M2-T6.12 编写 Java 测试：`SzOrmJava.queryBuilder().whereEq("id", 1).first()` 通过 JNI 调用 Rust C ABI，返回 Java 对象，行为与 Rust 一致（spec 5.2.1 规则 2 验收条件）
- [ ] M2-T6.13 编写 Java 测试：相同 CRUD 序列行为与 Rust sz-orm-core 一致
- [ ] M2-T6.14 编写 Java 测试：Rust panic 触发时抛出 `SzOrmPanicException`，无 UB
- [ ] M2-T6.15 编写性能测试：批量查询 1,000 行 ≤50ms

**验收标准**：
1. `sz-orm-java` 包创建成功，JNI + Java wrapper 惯用 API
2. 核心 API 完整暴露，CompletableFuture 异步桥接
3. 错误码完整映射到 Java Exception，panic 转 SzOrmPanicException 无 UB
4. Javadoc 文档 + 示例 + 错误码表
5. 性能达标（批量 1,000 行 ≤50ms）
6. `cargo test -p sz-orm-java --features lang-binding-java` + `mvn test` 全部通过
7. 附 `packages/sz-orm-java/` 新增代码的 file:line 证据

**依赖**：M2-T1（sz-orm-cabi）、M2-T4（C ABI 导出）

---

## M2-T7：sz-orm-cpp（C++ 绑定 + cxxbindgen + std::future 桥接）

**任务描述**：新增 `sz-orm-cpp` 包，C++ 绑定（extern "C" + cxxbindgen 头文件 + C++ wrapper），暴露 Model/QueryBuilder/Pool/Transaction，含 C++ wrapper（惯用 C++ API 风格，RAII/智能指针）+ std::future/协程异步桥接 + 错误码映射 + Doxygen 文档。

**涉及文件**：
- `packages/sz-orm-cpp/Cargo.toml`（新增包，crate-type = ["cdylib", "rlib"]，feature `lang-binding-cpp`，依赖 cxx）
- `packages/sz-orm-cpp/src/lib.rs`（新增，Rust 侧 extern "C" + cxx 桥接）
- `packages/sz-orm-cpp/include/szorm/pool.hpp`（新增，C++ wrapper Pool 头文件）
- `packages/sz-orm-cpp/include/szorm/query_builder.hpp`（新增，C++ wrapper QueryBuilder）
- `packages/sz-orm-cpp/include/szorm/transaction.hpp`（新增，C++ wrapper Transaction）
- `packages/sz-orm-cpp/include/szorm/exceptions.hpp`（新增，C++ 异常类）
- `Cargo.toml`（workspace.members 新增 `packages/sz-orm-cpp`）

**复用标注**：
- 既有 `sz-orm-python`（`packages/sz-orm-python/Cargo.toml:15`）：cdylib + rlib 模式参考
- M2-T4 `sz-orm-cabi` C ABI 导出层：extern "C" 调用
- `cxx` crate：C++ 安全 FFI（cxxbindgen 头文件生成）

**子任务**：
- [ ] M2-T7.1 创建 `packages/sz-orm-cpp/Cargo.toml`：`[lib] crate-type = ["cdylib", "rlib"]`，`[features] lang-binding-cpp = ["sz-orm-cabi", "dep:cxx"]`，依赖 `cxx = { version = "1.0", optional = true }`（design.md `:902-907`）
- [ ] M2-T7.2 在 `Cargo.toml` workspace.members 新增 `"packages/sz-orm-cpp"`
- [ ] M2-T7.3 创建 `src/lib.rs`，Rust 侧 extern "C" + cxx 桥接（cxxbindgen 生成 C++ 头文件）
- [ ] M2-T7.4 创建 `include/szorm/pool.hpp`：`class Pool { std::unique_ptr<PoolImpl> impl_; }`（RAII 智能指针），`Pool(const std::string& dsn)`（extern "C" 调用 `sz_orm_pool_new`）
- [ ] M2-T7.5 创建 `include/szorm/query_builder.hpp`：`QueryBuilder& where_eq(const std::string& col, const std::variant<int, std::string, double>& val)`，`std::optional<nlohmann::json> first()`（spec 5.2.1 规则 3 验收条件）
- [ ] M2-T7.6 实现 C++ wrapper 方法：`or_where_eq` / `order` / `limit` / `get` / `insert` / `update` / `delete`
- [ ] M2-T7.7 创建 `include/szorm/transaction.hpp`：`class Transaction { std::unique_ptr<TxImpl> impl_; }`，`static Transaction begin(Pool& pool)` / `void commit()` / `void rollback()`
- [ ] M2-T7.8 创建 `include/szorm/exceptions.hpp`：`class SzOrmException : public std::exception` + `NotFoundException` / `ConnectionFailedException` / `QueryFailedException` / `PoolExhaustedException` / `SzOrmPanicException`（design.md `:921`）
- [ ] M2-T7.9 实现 std::future 异步桥接：`std::future<nlohmann::json> first_async()` 不阻塞主线程，结果异步返回（spec 5.2.1 规则 7 验收条件）
- [ ] M2-T7.10 实现 RAII 智能指针：C++ 侧 `std::unique_ptr` 析构时调用 `sz_orm_free`，确保 Rust 分配的内存释放
- [ ] M2-T7.11 创建 Doxygen 文档：`include/szorm/doc/` 含示例代码 + 错误码表（spec 5.2.1 规则 9）
- [ ] M2-T7.12 编写 C++ 测试：`sz_orm_cpp::QueryBuilder().where_eq("id", 1).first()` 通过 extern "C" 调用 Rust，返回 C++ 对象，行为与 Rust 一致（spec 5.2.1 规则 3 验收条件）
- [ ] M2-T7.13 编写 C++ 测试：相同 CRUD 序列行为与 Rust sz-orm-core 一致
- [ ] M2-T7.14 编写 C++ 测试：Rust panic 触发时抛出 `SzOrmPanicException`，无 UB
- [ ] M2-T7.15 编写性能测试：批量查询 1,000 行 ≤50ms

**验收标准**：
1. `sz-orm-cpp` 包创建成功，extern "C" + cxxbindgen + C++ wrapper 惯用 API
2. 核心 API 完整暴露，RAII 智能指针管理内存
3. std::future 异步桥接，错误码完整映射到 C++ Exception
4. Doxygen 文档 + 示例 + 错误码表
5. 性能达标（批量 1,000 行 ≤50ms）
6. `cargo test -p sz-orm-cpp --features lang-binding-cpp` + C++ 测试全部通过
7. 附 `packages/sz-orm-cpp/` 新增代码的 file:line 证据

**依赖**：M2-T1（sz-orm-cabi）、M2-T4（C ABI 导出）

---

## M2-T8：三套绑定文档 + 示例 + 错误码表

**任务描述**：为三套绑定（Go/Java/C++）提供目标语言惯用文档（Go doc/Java Javadoc/C++ Doxygen）+ 示例代码 + 错误码表，可被目标语言开发者直接消费。

**涉及文件**：
- `packages/sz-orm-go/go/szorm/doc.go`（M2-T5 已创建，补充示例）
- `packages/sz-orm-java/java/src/main/java/com/szorm/doc-files/`（M2-T6 已创建，补充示例）
- `packages/sz-orm-cpp/include/szorm/doc/`（M2-T7 已创建，补充示例）
- `packages/sz-orm-go/examples/`（新增，Go 示例代码）
- `packages/sz-orm-java/examples/`（新增，Java 示例代码）
- `packages/sz-orm-cpp/examples/`（新增，C++ 示例代码）

**子任务**：
- [ ] M2-T8.1 创建 Go 示例：`packages/sz-orm-go/examples/basic_crud.go`（Pool/QueryBuilder/Transaction 基本 CRUD 流程）
- [ ] M2-T8.2 创建 Go 示例：`packages/sz-orm-go/examples/async_query.go`（goroutine 异步查询 + channel 结果）
- [ ] M2-T8.3 创建 Java 示例：`packages/sz-orm-java/examples/BasicCrud.java`（Pool/QueryBuilder/Transaction 基本 CRUD）
- [ ] M2-T8.4 创建 Java 示例：`packages/sz-orm-java/examples/AsyncQuery.java`（CompletableFuture 异步查询）
- [ ] M2-T8.5 创建 C++ 示例：`packages/sz-orm-cpp/examples/basic_crud.cpp`（Pool/QueryBuilder/Transaction 基本 CRUD）
- [ ] M2-T8.6 创建 C++ 示例：`packages/sz-orm-cpp/examples/async_query.cpp`（std::future 异步查询）
- [ ] M2-T8.7 编写错误码表：三套绑定统一错误码表（`SzOrmErrorCode` → Go error / Java Exception / C++ Exception 映射，spec 5.2.1 规则 8）
- [ ] M2-T8.8 验证 Go doc 生成：`go doc ./...` 输出完整 API 文档
- [ ] M2-T8.9 验证 Javadoc 生成：`mvn javadoc:javadoc` 输出完整 API 文档
- [ ] M2-T8.10 验证 Doxygen 生成：`doxygen Doxyfile` 输出完整 API 文档

**验收标准**：
1. 三套绑定均有目标语言惯用文档 + 示例代码 + 错误码表
2. Go doc / Javadoc / Doxygen 文档生成成功
3. 示例代码可编译运行，行为与 Rust 一致
4. 附文档生成输出证据

**依赖**：M2-T5（Go 绑定）、M2-T6（Java 绑定）、M2-T7（C++ 绑定）

---

## M2-T9：M2 集成测试与门禁验证

**任务描述**：M2-002 Go/Java/C++ 绑定集成测试与门禁验证，确保 REQ-V42-002 全部验收条件满足。

**涉及文件**：
- `packages/sz-orm-cabi/tests/ffi_integration.rs`（新增，FFI 集成测试）
- `packages/sz-orm-go/tests/integration_test.go`（新增，Go 集成测试）
- `packages/sz-orm-java/src/test/java/IntegrationTest.java`（新增，Java 集成测试）
- `packages/sz-orm-cpp/tests/integration_test.cpp`（新增，C++ 集成测试）

**子任务**：
- [ ] M2-T9.1 编写 FFI 集成测试：三套绑定调用相同 CRUD 序列，行为与 Rust sz-orm-core 一致，结果相同（spec 5.2.1 规则 4 验收条件）
- [ ] M2-T9.2 编写 FFI 边界测试：Rust panic 触发，三套绑定均捕获转错误码，无内存泄漏，无 UB（spec 5.2.3 异常 2）
- [ ] M2-T9.3 编写 FFI 内存泄漏检测：未调用 `sz_orm_free` 时检测泄漏并告警（spec 5.2.3 异常 1）
- [ ] M2-T9.4 编写异步运行时桥接测试：tokio 未初始化返回明确错误，提示初始化（spec 5.2.3 异常 3）
- [ ] M2-T9.5 运行 `cargo test -p sz-orm-cabi` + `cargo test -p sz-orm-go --features lang-binding-go` + `cargo test -p sz-orm-java --features lang-binding-java` + `cargo test -p sz-orm-cpp --features lang-binding-cpp` 全部通过
- [ ] M2-T9.6 运行 `go test ./...`（Go 测试）+ `mvn test`（Java 测试）+ C++ 测试全部通过
- [ ] M2-T9.7 运行 `cargo clippy --workspace --all-targets -- -D warnings` 通过
- [ ] M2-T9.8 运行 `cargo fmt --all -- --check` 通过
- [ ] M2-T9.9 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-cabi/ packages/sz-orm-go/ packages/sz-orm-java/ packages/sz-orm-cpp/` 无占位实现
- [ ] M2-T9.10 验证默认 feature（不启用 lang-binding-*）行为与 v4.1.0 一致（spec 5.2.1 规则 10 验收条件）
- [ ] M2-T9.11 验证既有 `sz-orm-python`/`sz-orm-js` 行为不变（不修改既有绑定）

**验收标准**：
1. REQ-V42-002 全部 10 条业务规则验收条件满足
2. 三套绑定集成测试全部通过
3. FFI 内存安全 + panic 捕获 + 异步桥接验证通过
4. clippy/fmt/占位检查门禁通过
5. 默认 feature 行为与 v4.1.0 一致，既有 python/js 绑定不变
6. 附集成测试运行输出证据

**依赖**：M2-T1~M2-T8 全部完成

---

# 四、M3：可视化 Schema 设计器（REQ-V42-003，P2）

**目标**：新增 `sz-orm-designer` 包，低代码图形化建表/改表/字段配置/关系设计 Web UI（HTML5/Canvas/SVG），ER 图可视化编辑，设计器 ↔ 迁移文件/实体代码双向生成，复用既有 `SchemaDiff`/`diff`/`DdlGenerator`（5 方言），多格式导出 + 脱敏展示 + CLI 集成。
**预期工作量**：1.5 周
**对应需求**：REQ-V42-003（spec.md 5.3，design.md 2.2.3 REQ-V42-003）
**依赖**：无（M3-003 为 P2 独立需求，复用既有 sz-orm-core/schema_sync + sz-orm-masking；与 M3-004 可并行）

## M3-T1：sz-orm-designer 包搭建 + schema-designer feature gate 体系

**任务描述**：新增 `sz-orm-designer` 包，`schema-designer` feature gate 隔离，依赖 axum/syn/quote + sz-orm-core + sz-orm-masking，默认关闭。

**涉及文件**：
- `packages/sz-orm-designer/Cargo.toml`（新增包，feature `schema-designer`）
- `packages/sz-orm-designer/src/lib.rs`（新增，模块入口）
- `Cargo.toml`（workspace.members 新增 `packages/sz-orm-designer`）

**复用标注**：
- 既有 `sz-orm-core`（schema_sync 模块）：SchemaDiff/diff/DdlGenerator 复用
- 既有 `sz-orm-masking`：脱敏展示复用
- 既有 `sz-orm-core/Cargo.toml:173-174`：syn/quote 依赖模式参考

**子任务**：
- [ ] M3-T1.1 创建 `packages/sz-orm-designer/Cargo.toml`：`[lib] crate-type = ["lib"]`，`[features] schema-designer = ["dep:axum", "dep:syn", "dep:quote", "sz-orm-core", "sz-orm-masking"]`（design.md `:1089-1104`）
- [ ] M3-T1.2 新增依赖：`axum = { version = "0.7", optional = true }` / `syn = { version = "2.0", features = ["full", "parsing"], optional = true }` / `quote = { version = "1.0", optional = true }` / `sz-orm-core = { workspace = true, optional = true }` / `sz-orm-masking = { workspace = true, optional = true }`
- [ ] M3-T1.3 在 `Cargo.toml` workspace.members 新增 `"packages/sz-orm-designer"`
- [ ] M3-T1.4 创建 `src/lib.rs`，模块入口 `#[cfg(feature = "schema-designer")] pub mod designer;`
- [ ] M3-T1.5 验证 `cargo check -p sz-orm-designer`（默认 feature）编译通过
- [ ] M3-T1.6 验证 `cargo check -p sz-orm-designer --features schema-designer` 编译通过
- [ ] M3-T1.7 验证 `cargo check --workspace --all-targets --all-features` 编译通过

**验收标准**：
1. `sz-orm-designer` 包创建成功，`schema-designer` feature gate 默认关闭
2. workspace 集成编译通过
3. 附 `packages/sz-orm-designer/Cargo.toml` 新增代码的 file:line 证据

**依赖**：无（基础设施任务，M3-003 所有任务依赖此任务）

---

## M3-T2：SchemaDesign 中间表示 + 双向转换

**任务描述**：定义 `SchemaDesign` 中间表示（IR），包含表/字段/关系/约束，与 DB schema（`TableDef`）双向转换，作为设计器内部统一表示。

**涉及文件**：
- `packages/sz-orm-designer/src/design_ir.rs`（新增，SchemaDesign IR 定义与双向转换）

**复用标注**：
- 既有 `SchemaDiff`（`packages/sz-orm-core/src/schema_sync.rs:100`）：设计↔代码双向一致验证复用
- 既有 `TableDef`/`ColumnDef`：DB schema 结构复用

**子任务**：
- [ ] M3-T2.1 定义 `pub struct SchemaDesign { pub tables: Vec<DesignTable>, pub relations: Vec<DesignRelation>, pub dialect: Dialect }`（design.md `:971-975`）
- [ ] M3-T2.2 定义 `pub struct DesignTable { pub name: String, pub columns: Vec<DesignColumn>, pub indexes: Vec<DesignIndex>, pub comment: Option<String> }`（design.md `:977-982`）
- [ ] M3-T2.3 定义 `pub struct DesignColumn { pub name: String, pub col_type: ColumnType, pub nullable: bool, pub default: Option<Value>, pub comment: Option<String>, pub is_primary_key: bool, pub is_auto_increment: bool }`（design.md `:984-992`）
- [ ] M3-T2.4 定义 `pub struct DesignRelation { pub from_table: String, pub to_table: String, pub from_column: String, pub to_column: String, pub cardinality: Cardinality }`（design.md `:994-1000`）
- [ ] M3-T2.5 定义 `pub enum Cardinality { OneToOne, OneToMany, ManyToOne, ManyToMany }`
- [ ] M3-T2.6 实现 `SchemaDesign::to_table_defs(&self) -> Vec<TableDef>`：IR → DB schema（design.md `:1010`）
- [ ] M3-T2.7 实现 `SchemaDesign::from_table_defs(table_defs: &[TableDef]) -> Self`：DB schema → IR
- [ ] M3-T2.8 编写单元测试：IR → TableDef → IR 往返无损，字段/关系/约束不丢失
- [ ] M3-T2.9 编写单元测试：含外键关系的 IR 转换为 TableDef 后外键正确

**验收标准**：
1. `SchemaDesign` IR 完整描述表/字段/关系/约束
2. IR ↔ TableDef 双向转换无损
3. `cargo test -p sz-orm-designer --features schema-designer` 新增测试全部通过
4. 附 `packages/sz-orm-designer/src/design_ir.rs` 新增代码的 file:line 证据

**依赖**：M3-T1（feature gate）

---

## M3-T3：SchemaDesigner 核心设计器 + ErDiagramEditor（ER 图编辑）

**任务描述**：实现 `SchemaDesigner` 核心设计器（图形化建表/改表/字段配置/关系设计，实时预览 DDL）+ `ErDiagramEditor`（ER 图可视化编辑，表为节点，外键为边，拖拽布局/关系连线/基数标注）。

**涉及文件**：
- `packages/sz-orm-designer/src/designer.rs`（新增，SchemaDesigner 核心）
- `packages/sz-orm-designer/src/er_editor.rs`（新增，ErDiagramEditor 实现）

**复用标注**：
- 既有 `DdlGenerator` trait（`packages/sz-orm-core/src/schema_sync.rs:361`，5 方言）：DDL 生成复用
- 既有 `SchemaSync`（`:612`）：schema 同步编排复用

**子任务**：
- [ ] M3-T3.1 定义 `pub struct SchemaDesigner { design: SchemaDesign, ddl_generator: Box<dyn DdlGenerator> }`（design.md `:1003-1006`）
- [ ] M3-T3.2 实现 `SchemaDesigner::new(design: SchemaDesign, dialect: Dialect) -> Self`：按方言选择 DdlGenerator（MySql/Pg/Sqlite/Oracle/Mssql）
- [ ] M3-T3.3 实现 `SchemaDesigner::add_table(&mut self, table: DesignTable) -> Result<(), DesignerError>`
- [ ] M3-T3.4 实现 `SchemaDesigner::modify_table(&mut self, name: &str, table: DesignTable) -> Result<(), DesignerError>`
- [ ] M3-T3.5 实现 `SchemaDesigner::add_relation(&mut self, relation: DesignRelation) -> Result<(), DesignerError>`
- [ ] M3-T3.6 实现 `SchemaDesigner::preview_ddl(&self) -> Result<Vec<String>, DesignerError>`：实时预览 DDL（复用 `DdlGenerator::generate`，spec 5.3.1 规则 1）
- [ ] M3-T3.7 定义 `pub enum DesignerError { RoundTripInconsistency { field: String }, DdlGenerationPartial { dialect: String, feature: String }, WebUiUnavailable, ParseFailed { line: usize, reason: String }, MaskingRuleNotFound { field: String } }`（design.md `:1112-1116`）
- [ ] M3-T3.8 在 `er_editor.rs` 定义 `pub struct ErDiagramEditor { design: SchemaDesign }`
- [ ] M3-T3.9 实现 `ErDiagramEditor::to_svg(&self) -> String`：ER 图渲染为 SVG（表为节点，外键为边，基数标注 1:N/N:1 等，spec 5.3.1 规则 2）
- [ ] M3-T3.10 实现 `ErDiagramEditor::to_json(&self) -> serde_json::Value`：ER 图序列化为 JSON（可被前端 Canvas 渲染）
- [ ] M3-T3.11 实现 `ErDiagramEditor::layout(&mut self, algorithm: LayoutAlgorithm)`：拖拽布局（force-directed / grid）
- [ ] M3-T3.12 编写单元测试：`SchemaDesigner::preview_ddl` 生成 5 方言 DDL 正确
- [ ] M3-T3.13 编写单元测试：`ErDiagramEditor::to_svg` 含表节点 + 外键边 + 基数标注
- [ ] M3-T3.14 编写单元测试：users 与 orders 一对多关系，ER 图显示 users→orders 连线，标注 1:N（spec 5.3.1 规则 2 验收条件）

**验收标准**：
1. `SchemaDesigner` 支持图形化建表/改表/关系设计，实时预览 DDL
2. `ErDiagramEditor` 渲染 ER 图（SVG/JSON），表为节点外键为边，基数标注
3. 复用既有 `DdlGenerator` 生成 5 方言 DDL
4. `cargo test -p sz-orm-designer --features schema-designer` 新增测试全部通过
5. 附 `packages/sz-orm-designer/src/designer.rs` 与 `er_editor.rs` 新增代码的 file:line 证据

**依赖**：M3-T1（feature gate）、M3-T2（SchemaDesign IR）

---

## M3-T4：SchemaDesignerWebUI（Web UI 服务器）

**任务描述**：实现 `SchemaDesignerWebUI`，Web UI 服务器（HTTP，axum），HTML5/Canvas/SVG 前端，支持浏览器图形化编辑 + 实时 DDL 预览 + ER 图编辑。

**涉及文件**：
- `packages/sz-orm-designer/src/web_ui.rs`（新增，Web UI 服务器）
- `packages/sz-orm-designer/static/index.html`（新增，前端 HTML5）
- `packages/sz-orm-designer/static/app.js`（新增，前端 Canvas/SVG ER 图编辑）
- `packages/sz-orm-designer/static/style.css`（新增，前端样式）

**复用标注**：axum（M3-T1 新增依赖）：HTTP 服务器

**子任务**：
- [ ] M3-T4.1 在 `web_ui.rs` 定义 `pub struct SchemaDesignerWebUI { designer: Arc<RwLock<SchemaDesigner>>, port: u16 }`
- [ ] M3-T4.2 实现 `SchemaDesignerWebUI::new(designer: SchemaDesigner, port: u16) -> Self`
- [ ] M3-T4.3 实现 `SchemaDesignerWebUI::start(&self) -> Result<(), DesignerError>`：axum HTTP 服务器启动，路由 `/`（前端）+ `/api/design`（GET 设计）+ `/api/preview-ddl`（DDL 预览）+ `/api/export`（导出）
- [ ] M3-T4.4 实现 `GET /api/design`：返回当前 SchemaDesign JSON
- [ ] M3-T4.5 实现 `POST /api/design/table`：新建/修改表
- [ ] M3-T4.6 实现 `POST /api/design/relation`：新建关系
- [ ] M3-T4.7 实现 `GET /api/preview-ddl?dialect=mysql`：实时预览 DDL（响应 < 200ms，spec 4.1 性能 3）
- [ ] M3-T4.8 实现 `GET /api/export?format=svg`：导出 ER 图 SVG
- [ ] M3-T4.9 创建 `static/index.html`：HTML5 前端，含 Canvas/SVG ER 图编辑区域 + 表/字段编辑表单 + DDL 预览面板
- [ ] M3-T4.10 创建 `static/app.js`：前端 JS，Canvas/SVG ER 图渲染 + 拖拽布局 + 关系连线 + API 调用
- [ ] M3-T4.11 创建 `static/style.css`：前端样式
- [ ] M3-T4.12 编写集成测试：浏览器打开设计器，新建 users 表含 id/name/email 字段，图形化编辑，实时预览 DDL，响应 < 200ms（spec 5.3.1 规则 1 验收条件）
- [ ] M3-T4.13 编写边界测试：浏览器不兼容时降级提示，提供 CLI 导出备选（spec 5.3.3 异常 3）

**验收标准**：
1. `SchemaDesignerWebUI` 启动 HTTP 服务器，浏览器可访问设计器
2. 图形化建表/改表/关系设计，实时 DDL 预览，响应 < 200ms
3. ER 图 Canvas/SVG 渲染，拖拽布局
4. `cargo test -p sz-orm-designer --features schema-designer` 新增测试全部通过
5. 附 `packages/sz-orm-designer/src/web_ui.rs` 新增代码的 file:line 证据

**依赖**：M3-T1（feature gate）、M3-T3（SchemaDesigner）

---

## M3-T5：DesignerCodeGenerator（设计→代码）+ CodeReverseParser（代码→设计）

**任务描述**：实现 `DesignerCodeGenerator`（设计 → 迁移文件 + 实体 Model 代码）+ `CodeReverseParser`（迁移文件/实体代码 → 设计），支持双向往返，双向一致验证。

**涉及文件**：
- `packages/sz-orm-designer/src/code_gen.rs`（新增，DesignerCodeGenerator 实现）
- `packages/sz-orm-designer/src/code_parse.rs`（新增，CodeReverseParser 实现）

**复用标注**：
- 既有 `DdlGenerator`（`packages/sz-orm-core/src/schema_sync.rs:361`，5 方言）：迁移文件 DDL 生成
- 既有 `SchemaSync`（`:612`）：schema 同步编排
- 既有 `Migration`（`packages/sz-orm-core/src/migration.rs:10`）：迁移文件结构
- `syn`/`quote`（M3-T1 新增依赖）：Rust AST 解析与生成

**子任务**：
- [ ] M3-T5.1 在 `code_gen.rs` 定义 `pub struct DesignerCodeGenerator { dialect: Dialect }`
- [ ] M3-T5.2 实现 `DesignerCodeGenerator::generate_migration(&self, design: &SchemaDesign) -> Result<Migration, DesignerError>`：生成迁移文件（up/down SQL，复用 `DdlGenerator::generate`，design.md `:1010`）
- [ ] M3-T5.3 实现 `DesignerCodeGenerator::generate_model_code(&self, design: &SchemaDesign) -> Result<String, DesignerError>`：生成 Rust struct + derive Model 代码（用 quote 代码生成）
- [ ] M3-T5.4 实现 5 方言 DDL 生成：MySQL AUTO_INCREMENT / PG SERIAL / SQLite AUTOINCREMENT / Oracle SEQUENCE / MSSQL IDENTITY（spec 5.3.1 规则 6 验收条件）
- [ ] M3-T5.5 在 `code_parse.rs` 定义 `pub struct CodeReverseParser`
- [ ] M3-T5.6 实现 `CodeReverseParser::parse_migration(sql: &str) -> Result<SchemaDesign, DesignerError>`：迁移文件 SQL → SchemaDesign（SQL 解析）
- [ ] M3-T5.7 实现 `CodeReverseParser::parse_model_code(rust_code: &str) -> Result<SchemaDesign, DesignerError>`：实体 Model 代码 → SchemaDesign（syn 解析 Rust AST，提取 derive Model 字段，design.md `:1011`）
- [ ] M3-T5.8 实现双向一致验证：设计 → 代码 → 设计'，对比 design 与 design' 语义等价（用既有 `diff` 函数，`packages/sz-orm-core/src/schema_sync.rs:200`，design.md `:1012`）
- [ ] M3-T5.9 编写单元测试：设计 users 表 → 生成迁移文件（5 方言 DDL）+ Rust User struct + derive Model（spec 5.3.1 规则 3 验收条件）
- [ ] M3-T5.10 编写单元测试：设计 users 表 → 生成代码 → 反向解析，解析结果与原设计语义等价（spec 5.3.1 规则 4 验收条件）
- [ ] M3-T5.11 编写单元测试：设计含自增主键表 → 生成 MySQL AUTO_INCREMENT、PG SERIAL、Oracle SEQUENCE、MSSQL IDENTITY
- [ ] M3-T5.12 编写边界测试：设计↔代码双向不一致时返回 `DesignerError::RoundTripInconsistency`（spec 5.3.3 异常 1）
- [ ] M3-T5.13 编写边界测试：设计含 SQLite 不支持的 CHECK 约束，降级生成，标注不支持特性（spec 5.3.3 异常 2）
- [ ] M3-T5.14 编写性能测试：设计 → 代码生成 ≤1 秒（表数量 ≤100，spec 4.1 性能 3）

**验收标准**：
1. `DesignerCodeGenerator` 生成迁移文件（5 方言）+ Rust Model 代码
2. `CodeReverseParser` 反向解析迁移文件/实体代码为 SchemaDesign
3. 双向一致验证，往返无损
4. 5 方言 DDL 差异正确（AUTO_INCREMENT/SERIAL/SEQUENCE/IDENTITY）
5. 性能达标（100 表生成 ≤1s）
6. `cargo test -p sz-orm-designer --features schema-designer` 新增测试全部通过
7. 附 `packages/sz-orm-designer/src/code_gen.rs` 与 `code_parse.rs` 新增代码的 file:line 证据

**依赖**：M3-T1（feature gate）、M3-T2（SchemaDesign IR）、M3-T3（SchemaDesigner）

---

## M3-T6：DesignerExporter + DesignerMasking（多格式导出 + 脱敏）

**任务描述**：实现 `DesignerExporter`（多格式导出：DDL/迁移/实体/ER 图 PNG/SVG/JSON）+ `DesignerMasking`（脱敏展示，尊重既有 `sz-orm-masking` 规则）。

**涉及文件**：
- `packages/sz-orm-designer/src/exporter.rs`（新增，DesignerExporter 实现）
- `packages/sz-orm-designer/src/masking.rs`（新增，DesignerMasking 实现）

**复用标注**：
- 既有 `DdlGenerator`（`:361`）：DDL 导出复用
- 既有 `sz-orm-masking`：脱敏规则复用

**子任务**：
- [ ] M3-T6.1 在 `exporter.rs` 定义 `pub enum ExportFormat { DdlSql, Migration, RustModel, ErPng, ErSvg, JsonDesign }`
- [ ] M3-T6.2 实现 `DesignerExporter::export(&self, design: &SchemaDesign, format: ExportFormat, dialect: Dialect) -> Result<Vec<u8>, DesignerError>`（spec 5.3.1 规则 8）
- [ ] M3-T6.3 实现 DDL SQL 导出（复用 `DdlGenerator`）
- [ ] M3-T6.4 实现迁移文件导出（up/down SQL）
- [ ] M3-T6.5 实现 Rust Model 代码导出（复用 M3-T5 `DesignerCodeGenerator`）
- [ ] M3-T6.6 实现 ER 图 SVG 导出（复用 M3-T3 `ErDiagramEditor::to_svg`）
- [ ] M3-T6.7 实现 ER 图 PNG 导出（SVG → PNG 转换）
- [ ] M3-T6.8 实现 JSON 设计文件导出（`SchemaDesign` 序列化为 JSON）
- [ ] M3-T6.9 在 `masking.rs` 定义 `pub struct DesignerMasking { rules: Vec<MaskingRule> }`
- [ ] M3-T6.10 实现 `DesignerMasking::apply(&self, design: &mut SchemaDesign)`：敏感表/字段名可选脱敏展示（如 password → ******，spec 5.3.1 规则 9）
- [ ] M3-T6.11 编写单元测试：导出 DDL + 迁移 + Rust 代码 + ER 图 SVG + JSON 设计文件，多格式验证（spec 5.3.1 规则 8 验收条件）
- [ ] M3-T6.12 编写单元测试：含敏感字段 password 的表，设计器可选脱敏展示为 ******（spec 5.3.1 规则 9 验收条件）

**验收标准**：
1. `DesignerExporter` 支持多格式导出（DDL/迁移/实体/ER 图 PNG/SVG/JSON）
2. `DesignerMasking` 尊重既有脱敏规则，敏感字段可选脱敏
3. `cargo test -p sz-orm-designer --features schema-designer` 新增测试全部通过
4. 附 `packages/sz-orm-designer/src/exporter.rs` 与 `masking.rs` 新增代码的 file:line 证据

**依赖**：M3-T1（feature gate）、M3-T3（ErDiagramEditor）、M3-T5（DesignerCodeGenerator）

---

## M3-T7：CLI 集成 + M3-003 集成测试与门禁验证

**任务描述**：CLI 集成（`sz-orm designer` 启动 Web UI + `sz-orm designer:export` 导出）+ M3-003 集成测试与门禁验证。

**涉及文件**：
- `cli/src/main.rs`（修改，新增 `cmd_designer` + `cmd_designer_export` 命令，复用既有 `cmd_generate_schema` `:1630` 入口）
- `packages/sz-orm-designer/tests/integration_test.rs`（新增，集成测试）

**复用标注**：既有 `cmd_generate_schema`（`cli/src/main.rs:1630`）：CLI 集成入口复用

**子任务**：
- [ ] M3-T7.1 在 `cli/src/main.rs` 新增 `cmd_designer(port: u16)` 命令：启动 `SchemaDesignerWebUI`（spec 5.3.1 规则 7）
- [ ] M3-T7.2 新增 `cmd_designer_export(design_file: &str, format: ExportFormat, dialect: Dialect)` 命令：从设计文件导出迁移/实体代码
- [ ] M3-T7.3 在 CLI 命令分发新增 `sz-orm designer` 与 `sz-orm designer:export` 子命令
- [ ] M3-T7.4 编写集成测试：执行 `sz-orm designer` 启动 Web UI 服务器，浏览器可访问设计器（spec 5.3.1 规则 7 验收条件）
- [ ] M3-T7.5 编写集成测试：执行 `sz-orm designer:export --design=design.json --format=migration --dialect=postgresql` 输出迁移文件
- [ ] M3-T7.6 运行 `cargo test -p sz-orm-designer --features schema-designer` 全部通过
- [ ] M3-T7.7 运行 `cargo clippy -p sz-orm-designer --features schema-designer -- -D warnings` 通过
- [ ] M3-T7.8 运行 `cargo fmt -p sz-orm-designer -- --check` 通过
- [ ] M3-T7.9 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-designer/` 无占位实现
- [ ] M3-T7.10 验证默认 feature（不启用 schema-designer）行为与 v4.1.0 一致（spec 5.3.1 规则 10 验收条件）

**验收标准**：
1. CLI 命令 `sz-orm designer` + `sz-orm designer:export` 可用
2. REQ-V42-003 全部 10 条业务规则验收条件满足
3. clippy/fmt/占位检查门禁通过
4. 默认 feature 行为与 v4.1.0 一致
5. 附集成测试运行输出证据

**依赖**：M3-T1~M3-T6 全部完成

---

# 五、M3：OpenAPI → ORM 反向生成（REQ-V42-004，P2）

**目标**：扩展既有 `sz-orm-swagger`，新增 `OpenApiReverseGenerator`（OpenAPI → Model + 迁移 + Repository）+ `ApiFirstLoopVerifier`（API 优先闭环验证）+ `OpenApiInjectionGuard`（注入防护）+ `ReverseGenConfig`（可配置）+ CLI 集成，复用既有 `OpenAPISpec`/`Schema`/`DdlGenerator`。
**预期工作量**：1.5 周
**对应需求**：REQ-V42-004（spec.md 5.4，design.md 2.2.4 REQ-V42-004）
**依赖**：无（M3-004 为 P2 独立需求，复用既有 sz-orm-swagger + sz-orm-core/schema_sync；与 M3-003 可并行）

## M3-T8：openapi-reverse feature gate 体系搭建

**任务描述**：在 sz-orm-swagger 新增 `openapi-reverse` feature gate 及对应可选依赖（quote/syn/proc-macro2），默认关闭。

**涉及文件**：
- `packages/sz-orm-swagger/Cargo.toml`（新增 `openapi-reverse` feature + quote/syn/proc-macro2 可选依赖）

**复用标注**：复用既有 `sz-orm-swagger`（`packages/sz-orm-swagger/Cargo.toml`，已有 OpenAPISpec/Schema/OpenAPIGenerator）

**子任务**：
- [ ] M3-T8.1 在 `packages/sz-orm-swagger/Cargo.toml` `[features]` 新增 `openapi-reverse = ["dep:quote", "dep:syn", "dep:proc-macro2", "sz-orm-core"]`（design.md `:1275-1284`）
- [ ] M3-T8.2 新增依赖：`quote = { version = "1.0", optional = true }` / `syn = { version = "2.0", features = ["full"], optional = true }` / `proc-macro2 = { version = "1.0", optional = true }`
- [ ] M3-T8.3 验证 `cargo check -p sz-orm-swagger`（默认 feature）编译通过，行为与 v4.1.0 一致
- [ ] M3-T8.4 验证 `cargo check -p sz-orm-swagger --features openapi-reverse` 编译通过
- [ ] M3-T8.5 验证既有 `OpenAPIGenerator`/`model_to_openapi_schema` 行为不变（不修改既有正向生成）

**验收标准**：
1. `openapi-reverse` feature gate 默认关闭
2. 既有正向生成行为不变
3. 附 `packages/sz-orm-swagger/Cargo.toml` 新增代码的 file:line 证据

**依赖**：无（基础设施任务，M3-004 所有任务依赖此任务）

---

## M3-T9：SchemaToModelMapper（OpenAPI Schema → Model 字段映射）

**任务描述**：实现 `SchemaToModelMapper`，OpenAPI Schema → Rust struct + derive Model 字段映射，类型映射（string→String/integer→i64/number→f64/boolean→bool/array→Vec/object→嵌套 struct）+ 约束映射（required→NOT NULL、maxLength→VARCHAR(n)、format:date-time→TIMESTAMP）。

**涉及文件**：
- `packages/sz-orm-swagger/src/reverse/model_mapper.rs`（新增，SchemaToModelMapper 实现）
- `packages/sz-orm-swagger/src/reverse/mod.rs`（新增，reverse 模块入口）

**复用标注**：
- 既有 `Schema`（`packages/sz-orm-swagger/src/lib.rs:328`）：Schema 定义枚举复用
- 既有 `ObjectType`（`:430`）：对象类型 Schema 映射复用
- 既有 `ArrayType`（`:490`）：数组类型 Schema 映射复用
- 既有 `PrimitiveSchema`（`:540`）：基本类型 Schema 映射复用
- `quote`/`syn`（M3-T8 新增依赖）：Rust 代码生成

**子任务**：
- [ ] M3-T9.1 创建 `src/reverse/mod.rs`，模块入口 `#[cfg(feature = "openapi-reverse")] pub mod model_mapper;`
- [ ] M3-T9.2 在 `model_mapper.rs` 定义 `pub struct SchemaToModelMapper`
- [ ] M3-T9.3 实现 `SchemaToModelMapper::map_type(schema: &Schema) -> RustType`：类型映射（string→String/integer→i64/number→f64/boolean→bool/array→Vec/object→嵌套 struct，design.md `:1163-1173`）
- [ ] M3-T9.4 实现 `SchemaToModelMapper::map_constraint(schema: &Schema) -> Vec<Constraint>`：约束映射（required→NOT NULL、maxLength→VARCHAR(n)、format:date-time→TIMESTAMP、uniqueItems→UNIQUE，design.md `:1177-1184`）
- [ ] M3-T9.5 实现 `SchemaToModelMapper::generate_model_code(schema_name: &str, schema: &Schema) -> Result<String, ReverseGenError>`：生成 Rust struct + derive Model 代码（用 quote 代码生成）
- [ ] M3-T9.6 定义 `pub enum ReverseGenError { SpecParseFailed { path: String, reason: String }, UnsupportedSchemaConstruct { construct: String, schema: String }, LoopVerificationDiff { diff: String }, InjectionDetected, UnsignedSpec, UserLogicOverwrite }`（design.md `:1292-1297`）
- [ ] M3-T9.7 编写单元测试：OpenAPI spec 含 User schema（id:integer, name:string, email:string）→ 生成 Rust User struct + derive Model，字段类型正确映射（spec 5.4.1 规则 1 验收条件）
- [ ] M3-T9.8 编写单元测试：OpenAPI User schema 含 required:id, maxLength:255 name → 约束映射正确（spec 5.4.1 规则 2 验收条件）
- [ ] M3-T9.9 编写单元测试：format:date-time → chrono::DateTime<Utc>，format:uuid → uuid::Uuid
- [ ] M3-T9.10 编写单元测试：array + items: T → Vec<T>，object → 嵌套 struct

**验收标准**：
1. `SchemaToModelMapper` 字段类型映射正确（string/integer/number/boolean/array/object）
2. 约束映射正确（required/maxLength/format/uniqueItems）
3. 生成 Rust struct + derive Model 代码编译通过
4. `cargo test -p sz-orm-swagger --features openapi-reverse` 新增测试全部通过
5. 附 `packages/sz-orm-swagger/src/reverse/model_mapper.rs` 新增代码的 file:line 证据

**依赖**：M3-T8（feature gate）

---

## M3-T10：OpenApiToMigrationMapper（OpenAPI → 迁移文件）

**任务描述**：实现 `OpenApiToMigrationMapper`，OpenAPI Schema → 迁移文件（up/down SQL，5 方言），复用既有 `DdlGenerator`。

**涉及文件**：
- `packages/sz-orm-swagger/src/reverse/migration_mapper.rs`（新增，OpenApiToMigrationMapper 实现）

**复用标注**：
- 既有 `DdlGenerator` trait（`packages/sz-orm-core/src/schema_sync.rs:361`，5 方言）：DDL 生成复用
- 既有 5 方言 DdlGenerator 实现（`:369/439/479/522/565`）

**子任务**：
- [ ] M3-T10.1 定义 `pub struct OpenApiToMigrationMapper { dialect: Dialect }`
- [ ] M3-T10.2 实现 `OpenApiToMigrationMapper::generate_migration(schema_name: &str, schema: &Schema) -> Result<Migration, ReverseGenError>`：Schema → TableDef → 既有 `DdlGenerator::generate` 生成 up/down SQL（design.md `:1227`）
- [ ] M3-T10.3 实现 5 方言 DDL 生成：MySQL/PG/SQLite/Oracle/MSSQL（复用既有 DdlGenerator 实现）
- [ ] M3-T10.4 实现约束映射到 DDL：required→NOT NULL、maxLength→VARCHAR(n)、format:date-time→TIMESTAMP、uniqueItems→UNIQUE、pattern→CHECK（方言支持时）
- [ ] M3-T10.5 编写单元测试：OpenAPI User schema 含 required:id, maxLength:255 name → 生成迁移 CREATE TABLE users (id NOT NULL, name VARCHAR(255))，5 方言（spec 5.4.1 规则 2 验收条件）
- [ ] M3-T10.6 编写单元测试：5 方言 DDL 正确（MySQL AUTO_INCREMENT / PG SERIAL / SQLite AUTOINCREMENT / Oracle SEQUENCE / MSSQL IDENTITY）
- [ ] M3-T10.7 编写边界测试：不支持的 Schema 特性（allOf/oneOf）降级生成，标注不支持特性（spec 5.4.3 异常 2）

**验收标准**：
1. `OpenApiToMigrationMapper` 生成迁移文件（up/down SQL，5 方言）
2. 复用既有 `DdlGenerator`，不重复实现 5 方言 DDL
3. `cargo test -p sz-orm-swagger --features openapi-reverse` 新增测试全部通过
4. 附 `packages/sz-orm-swagger/src/reverse/migration_mapper.rs` 新增代码的 file:line 证据

**依赖**：M3-T8（feature gate）、M3-T9（SchemaToModelMapper）

---

## M3-T11：OpenApiToRepositoryMapper（OpenAPI → Repository 骨架）

**任务描述**：实现 `OpenApiToRepositoryMapper`，OpenAPI Schema → Repository 代码骨架（CRUD 方法：find_by_id/find_all/create/update/delete），标注可编辑区，不覆盖用户手写业务逻辑。

**涉及文件**：
- `packages/sz-orm-swagger/src/reverse/repository_mapper.rs`（新增，OpenApiToRepositoryMapper 实现）

**复用标注**：`quote`/`syn`（M3-T8 新增依赖）：Rust 代码生成

**子任务**：
- [ ] M3-T11.1 定义 `pub struct OpenApiToRepositoryMapper`
- [ ] M3-T11.2 实现 `OpenApiToRepositoryMapper::generate_repository(schema_name: &str, schema: &Schema) -> Result<String, ReverseGenError>`：生成 Repository 代码骨架
- [ ] M3-T11.3 生成 CRUD 方法：`find_by_id(id: &i64) -> Result<Option<User>, DbError>` / `find_all() -> Result<Vec<User>, DbError>` / `create(model: &User) -> Result<User, DbError>` / `update(model: &User) -> Result<(), DbError>` / `delete(id: &i64) -> Result<(), DbError>`
- [ ] M3-T11.4 实现可编辑区标注：`// EDITABLE: business logic here`（用户业务逻辑填充区，design.md `:1326`）
- [ ] M3-T11.5 实现幂等生成：同一 spec 多次生成同一代码（除时间戳/注释外），不覆盖可编辑区（spec 5.4.1 规则 7）
- [ ] M3-T11.6 编写单元测试：OpenAPI User schema → 生成 UserRepository 骨架含 CRUD 方法 + 可编辑区标注（spec 5.4.1 规则 3 验收条件）
- [ ] M3-T11.7 编写单元测试：同一 spec 反向生成两次，生成代码一致（除时间戳），用户手写逻辑保留（spec 5.4.1 规则 7 验收条件）

**验收标准**：
1. `OpenApiToRepositoryMapper` 生成 Repository 骨架含 CRUD 方法
2. 可编辑区标注，不覆盖用户手写逻辑
3. 幂等生成
4. `cargo test -p sz-orm-swagger --features openapi-reverse` 新增测试全部通过
5. 附 `packages/sz-orm-swagger/src/reverse/repository_mapper.rs` 新增代码的 file:line 证据

**依赖**：M3-T8（feature gate）、M3-T9（SchemaToModelMapper）

---

## M3-T12：ApiFirstLoopVerifier（闭环验证）+ OpenApiInjectionGuard（注入防护）

**任务描述**：实现 `ApiFirstLoopVerifier`（API 优先闭环验证：反向生成 ORM → 正向生成 OpenAPI' → 对比 spec）+ `OpenApiInjectionGuard`（注入防护：不执行 spec 内嵌代码，不信任未签名 spec）。

**涉及文件**：
- `packages/sz-orm-swagger/src/reverse/loop_verifier.rs`（新增，ApiFirstLoopVerifier 实现）
- `packages/sz-orm-swagger/src/reverse/injection_guard.rs`（新增，OpenApiInjectionGuard 实现）

**复用标注**：既有 `model_to_openapi_schema`（`packages/sz-orm-swagger/src/lib.rs:1325`）：闭环验证正向生成复用

**子任务**：
- [ ] M3-T12.1 在 `loop_verifier.rs` 定义 `pub struct ApiFirstLoopVerifier`
- [ ] M3-T12.2 实现 `ApiFirstLoopVerifier::verify(spec: &OpenAPISpec, generated_model_code: &str) -> Result<LoopReport, ReverseGenError>`：反向生成 ORM → 正向生成（既有 `model_to_openapi_schema`）→ 对比 spec 与 OpenAPI'（design.md `:1186-1190`）
- [ ] M3-T12.3 定义 `pub struct LoopReport { pub spec_schemas: Vec<String>, pub generated_schemas: Vec<String>, pub diffs: Vec<SchemaDiff>, pub consistent: bool }`
- [ ] M3-T12.4 实现差异标注：spec vs OpenAPI' 字段类型/约束差异标注，不阻断生成（spec 5.4.3 异常 3）
- [ ] M3-T12.5 在 `injection_guard.rs` 定义 `pub struct OpenApiInjectionGuard`
- [ ] M3-T12.6 实现 `OpenApiInjectionGuard::check(spec: &OpenAPISpec) -> Result<(), ReverseGenError>`：不执行 spec 内嵌代码（如 `x-exec: "rm -rf /"`），不信任未签名 spec（design.md `:1327`）
- [ ] M3-T12.7 实现签名验证：spec 含签名时验证，未签名 spec 拒绝（或 `--trust-unsigned` 显式信任）
- [ ] M3-T12.8 实现生成代码强制参数化查询：生成的 Repository 代码用 `where_eq` 等参数化方法（spec 5.4.1 规则 9）
- [ ] M3-T12.9 编写单元测试：OpenAPI spec → 反向生成 ORM → 正向生成 OpenAPI'，spec 与 OpenAPI' 一致（除可编辑区），闭环验证报告标注差异（spec 5.4.1 规则 4 验收条件）
- [ ] M3-T12.10 编写单元测试：spec 含恶意内嵌代码（`x-exec`），不执行，返回 `InjectionDetected`（spec 5.4.1 规则 9 验收条件）
- [ ] M3-T12.11 编写单元测试：未签名 spec 拒绝生成，返回 `UnsignedSpec`
- [ ] M3-T12.12 编写边界测试：闭环验证差异时标注差异，告警，不阻断生成（spec 5.4.3 异常 3）

**验收标准**：
1. `ApiFirstLoopVerifier` 闭环验证，spec vs OpenAPI' 差异标注
2. `OpenApiInjectionGuard` 注入防护，不执行内嵌代码，不信任未签名 spec
3. 生成代码强制参数化查询
4. `cargo test -p sz-orm-swagger --features openapi-reverse` 新增测试全部通过
5. 附 `packages/sz-orm-swagger/src/reverse/loop_verifier.rs` 与 `injection_guard.rs` 新增代码的 file:line 证据

**依赖**：M3-T8（feature gate）、M3-T9（SchemaToModelMapper）

---

## M3-T13：ReverseGenConfig（可配置）+ CLI 集成 + M3-004 集成测试

**任务描述**：实现 `ReverseGenConfig`（可配置：目标方言/代码风格/命名约定/可编辑区标注/是否覆盖）+ CLI 集成（`sz-orm openapi:reverse`）+ M3-004 集成测试与门禁验证。

**涉及文件**：
- `packages/sz-orm-swagger/src/reverse/config.rs`（新增，ReverseGenConfig 实现）
- `packages/sz-orm-swagger/src/reverse/generator.rs`（新增，OpenApiReverseGenerator 主入口，编排解析→映射→生成→闭环验证）
- `cli/src/main.rs`（修改，新增 `cmd_openapi_reverse` 命令）
- `packages/sz-orm-swagger/tests/reverse_integration.rs`（新增，集成测试）

**复用标注**：
- 既有 `OpenAPISpec`（`packages/sz-orm-swagger/src/lib.rs:28`）：spec 解析复用
- 既有 `Components`（`:55`）：`components.schemas` 提取复用
- M3-T9~M3-T12 各 Mapper

**子任务**：
- [ ] M3-T13.1 在 `config.rs` 定义 `pub struct ReverseGenConfig { pub target_dialect: Dialect, pub naming_convention: NamingConvention, pub overwrite: bool, pub editable_region_marker: String, pub trust_unsigned: bool }`（design.md `:1619-1621`）
- [ ] M3-T13.2 定义 `pub enum NamingConvention { SnakeCase, CamelCase, PascalCase }`
- [ ] M3-T13.3 实现 `ReverseGenConfig::from_yaml(path: &str) -> Result<Self, ReverseGenError>`：配置文件版本化管理
- [ ] M3-T13.4 在 `generator.rs` 定义 `pub struct OpenApiReverseGenerator { config: ReverseGenConfig }`
- [ ] M3-T13.5 实现 `OpenApiReverseGenerator::generate(spec_path: &str) -> Result<ReverseGenResult, ReverseGenError>`：编排 注入防护 → 解析 spec → Schema → Model/迁移/Repository → 闭环验证（design.md `:1186-1190`）
- [ ] M3-T13.6 定义 `pub struct ReverseGenResult { pub model_code: HashMap<String, String>, pub migrations: HashMap<Dialect, Migration>, pub repository_code: HashMap<String, String>, pub loop_report: LoopReport }`
- [ ] M3-T13.7 在 `cli/src/main.rs` 新增 `cmd_openapi_reverse(spec: &str, dialect: &str, config: Option<&str>)` 命令（spec 5.4.1 规则 10）
- [ ] M3-T13.8 在 CLI 命令分发新增 `sz-orm openapi:reverse --spec=openapi.yaml --dialect=postgresql` 子命令
- [ ] M3-T13.9 编写集成测试：执行 `sz-orm openapi:reverse --spec=openapi.yaml --dialect=postgresql` 输出 Model + 迁移 + Repository 代码文件（spec 5.4.1 规则 10 验收条件）
- [ ] M3-T13.10 编写集成测试：配置 target_dialect=postgresql, naming=snake_case → 生成 PostgreSQL 迁移 + snake_case 命名（spec 5.4.1 规则 8 验收条件）
- [ ] M3-T13.11 编写性能测试：反向生成开销 ≤2 秒（Schema 数量 ≤100，spec 4.1 性能 4）
- [ ] M3-T13.12 运行 `cargo test -p sz-orm-swagger --features openapi-reverse` 全部通过
- [ ] M3-T13.13 运行 `cargo clippy -p sz-orm-swagger --features openapi-reverse -- -D warnings` 通过
- [ ] M3-T13.14 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-swagger/src/reverse/` 无占位实现
- [ ] M3-T13.15 验证默认 feature（不启用 openapi-reverse）行为与 v4.1.0 一致（spec 5.4.1 规则 11 验收条件）

**验收标准**：
1. `ReverseGenConfig` 可配置（目标方言/代码风格/命名约定/可编辑区/是否覆盖）
2. CLI 命令 `sz-orm openapi:reverse` 可用
3. REQ-V42-004 全部 11 条业务规则验收条件满足
4. 性能达标（≤2 秒，100 Schema）
5. clippy/fmt/占位检查门禁通过
6. 默认 feature 行为与 v4.1.0 一致
7. 附集成测试运行输出证据

**依赖**：M3-T8~M3-T12 全部完成

---

# 六、M4：WASM 真实数据库连接（REQ-V42-005，P3）

**目标**：扩展既有 `sz-orm-wasm`，新增 `WasmRealDbConnection`（HTTP/WebSocket 代理桥接）+ `WasmDbProxyProtocol`（代理协议）+ `WasmDbProxy`（后端代理：鉴权 + 限流 + SQL 白名单 + 连接池）+ `WasiSocketConnection`（WASI socket 直连，可选）+ `WasmRealDbReconnector`（重连）+ `WasmRealDbMetrics`（指标），复用既有 `WasmQuery`/`WasmDatabase`/`js_bindings` + `Pool`/`sz-orm-sql-validator`/`sz-orm-limit`/`sz-orm-observability`。
**预期工作量**：1.5 周
**对应需求**：REQ-V42-005（spec.md 5.5，design.md 2.2.5 REQ-V42-005）
**依赖**：无（M4-005 为 P3 独立需求，复用既有 sz-orm-wasm + sz-orm-observability；与 M4 最终验证串行）

## M4-T1：wasm-real-db feature gate 体系搭建

**任务描述**：在 sz-orm-wasm 新增 `wasm-real-db` feature gate 及对应可选依赖（reqwest/tokio-tungstenite/rmp-serde），默认关闭。

**涉及文件**：
- `packages/sz-orm-wasm/Cargo.toml`（新增 `wasm-real-db` + `wasi-socket` feature + reqwest/tokio-tungstenite/rmp-serde 可选依赖）

**复用标注**：复用既有 `sz-orm-wasm`（`packages/sz-orm-wasm/Cargo.toml:27-30`，已有 `js`/`persistence` feature）

**子任务**：
- [ ] M4-T1.1 在 `packages/sz-orm-wasm/Cargo.toml` `[features]` 新增 `wasm-real-db = ["dep:reqwest", "dep:tokio-tungstenite", "dep:rmp-serde", "js"]` 与 `wasi-socket = ["wasm-real-db"]`（design.md `:1477-1488`）
- [ ] M4-T1.2 新增依赖：`reqwest = { version = "0.12", optional = true }` / `tokio-tungstenite = { version = "0.24", optional = true }` / `rmp-serde = { version = "1.3", optional = true }`
- [ ] M4-T1.3 验证 `cargo check -p sz-orm-wasm`（默认 feature）编译通过，行为与 v4.1.0 一致
- [ ] M4-T1.4 验证 `cargo check -p sz-orm-wasm --features wasm-real-db` 编译通过
- [ ] M4-T1.5 验证既有 `js`/`persistence` feature 行为不变（不修改既有内存数据库/浏览器本地存储）
- [ ] M4-T1.6 验证 `cargo check --workspace --all-targets --all-features` 编译通过

**验收标准**：
1. `wasm-real-db` + `wasi-socket` feature gate 默认关闭
2. 既有 `js`/`persistence` feature 行为不变
3. 附 `packages/sz-orm-wasm/Cargo.toml` 新增代码的 file:line 证据

**依赖**：无（基础设施任务，M4-005 所有任务依赖此任务）

---

## M4-T2：WasmDbProxyProtocol（WASM DB 代理协议）

**任务描述**：定义 WASM ↔ 后端 DB 代理协议（查询请求/响应/参数/事务/错误码格式，JSON/MessagePack），代理负责鉴权/限流/SQL 白名单/连接池。

**涉及文件**：
- `packages/sz-orm-wasm/src/real_db/protocol.rs`（新增，WasmDbProxyProtocol 实现）
- `packages/sz-orm-wasm/src/real_db/mod.rs`（新增，real_db 模块入口）

**复用标注**：
- 既有 `WasmQuery`（`packages/sz-orm-wasm/src/lib.rs:38`）：查询结构复用
- 既有 `serde_json`：JSON 序列化复用
- `rmp-serde`（M4-T1 新增依赖）：MessagePack 序列化

**子任务**：
- [ ] M4-T2.1 创建 `src/real_db/mod.rs`，模块入口 `#[cfg(feature = "wasm-real-db")] pub mod protocol;`
- [ ] M4-T2.2 在 `protocol.rs` 定义 `pub struct ProxyRequest { pub session_id: String, pub token: String, pub query: WasmQuery, pub transaction_id: Option<String> }`（design.md `:1362-1367`）
- [ ] M4-T2.3 定义 `pub struct ProxyResponse { pub status: ProxyStatus, pub rows: Vec<serde_json::Value>, pub rows_affected: Option<usize>, pub error: Option<ProxyError>, pub latency_ms: u64 }`（design.md `:1369-1375`）
- [ ] M4-T2.4 定义 `pub enum ProxyStatus { Ok, Error }` / `pub enum ProxyError { AuthFailed, RateLimited, SqlRejected { reason: String }, QueryFailed { reason: String }, ProxyUnavailable, CredentialsNotExposed, ResultTooLarge }`
- [ ] M4-T2.5 定义 `pub enum SerializationFormat { Json, MessagePack }`
- [ ] M4-T2.6 实现 `ProxyRequest::serialize(&self, format: SerializationFormat) -> Result<Vec<u8>, WasmRealDbError>`：JSON/MessagePack 序列化
- [ ] M4-T2.7 实现 `ProxyResponse::deserialize(bytes: &[u8], format: SerializationFormat) -> Result<Self, WasmRealDbError>`
- [ ] M4-T2.8 定义 `pub enum WasmRealDbError { ProxyUnavailable, SqlRejected { reason: String }, RateLimited, AuthFailed, CredentialsNotExposed, QueryFailed { reason: String }, ResultTooLarge, SerializationError(String) }`（design.md `:1496-1502`）
- [ ] M4-T2.9 编写单元测试：`ProxyRequest`/`ProxyResponse` JSON 序列化/反序列化往返无损
- [ ] M4-T2.10 编写单元测试：MessagePack 序列化/反序列化往返无损
- [ ] M4-T2.11 编写单元测试：MessagePack 比 JSON 体积更小（二进制优势）

**验收标准**：
1. `WasmDbProxyProtocol` 定义查询请求/响应/参数/事务/错误码格式
2. JSON/MessagePack 双序列化支持
3. `cargo test -p sz-orm-wasm --features wasm-real-db` 新增测试全部通过
4. 附 `packages/sz-orm-wasm/src/real_db/protocol.rs` 新增代码的 file:line 证据

**依赖**：M4-T1（feature gate）

---

## M4-T3：WasmRealDbConnection + WasmRealDbQueryExecutor

**任务描述**：实现 `WasmRealDbConnection`（WASM 真实 DB 连接，通过 HTTP/WebSocket 代理桥接后端 DB）+ `WasmRealDbQueryExecutor`（真实 DB 查询执行器，复用既有 `WasmQuery`）。

**涉及文件**：
- `packages/sz-orm-wasm/src/real_db/connection.rs`（新增，WasmRealDbConnection 实现）
- `packages/sz-orm-wasm/src/real_db/executor.rs`（新增，WasmRealDbQueryExecutor 实现）

**复用标注**：
- 既有 `WasmQuery`（`packages/sz-orm-wasm/src/lib.rs:38`）：查询结构复用（sql + params 不变）
- 既有 `js_bindings`（`:15`）：JS 绑定复用
- `reqwest`/`tokio-tungstenite`（M4-T1 新增依赖）：HTTP/WebSocket 客户端

**子任务**：
- [ ] M4-T3.1 在 `connection.rs` 定义 `pub struct WasmRealDbConnection { proxy_endpoint: String, token: String, session_id: String, transport: WasmTransport, reconnector: WasmRealDbReconnector, metrics: WasmRealDbMetrics }`（design.md `:1352-1359`）
- [ ] M4-T3.2 定义 `pub enum WasmTransport { Http, WebSocket }`
- [ ] M4-T3.3 实现 `WasmRealDbConnection::new(proxy_endpoint: &str, token: &str, transport: WasmTransport) -> Self`
- [ ] M4-T3.4 实现 `WasmRealDbConnection::query(&self, sql: &str, params: Vec<serde_json::Value>) -> Result<Vec<serde_json::Value>, WasmRealDbError>`：构造 `WasmQuery`（复用既有 `:38`）→ 构造 `ProxyRequest` → HTTP/WS 发送到代理 → 解析 `ProxyResponse`（spec 5.5.1 规则 1）
- [ ] M4-T3.5 实现 `WasmRealDbConnection::execute(&self, sql: &str, params: Vec<serde_json::Value>) -> Result<usize, WasmRealDbError>`：INSERT/UPDATE/DELETE
- [ ] M4-T3.6 实现 HTTP 传输：用 `reqwest` 发送 POST 请求到代理端点
- [ ] M4-T3.7 实现 WebSocket 传输：用 `tokio-tungstenite` 建立 WS 长连接，发送/接收消息
- [ ] M4-T3.8 在 `executor.rs` 定义 `pub struct WasmRealDbQueryExecutor { connection: WasmRealDbConnection }`
- [ ] M4-T3.9 实现 `WasmRealDbQueryExecutor::execute_query(&self, query: WasmQuery) -> Result<Vec<serde_json::Value>, WasmRealDbError>`：执行真实 DB 查ELECT（参数化），结果集序列化回 WASM 端（spec 5.5.1 规则 5）
- [ ] M4-T3.10 实现 JS 绑定：`#[wasm_bindgen]` 暴露 `WasmRealDbConnection` 给 JS（复用既有 `js_bindings` 模式）
- [ ] M4-T3.11 编写单元测试：`WasmRealDbConnection::query` 构造 `WasmQuery` 正确（sql + params）
- [ ] M4-T3.12 编写集成测试：WASM 端调用 `query("SELECT * FROM users WHERE id = ?", [1])` 通过 HTTP/WS 发送到代理，代理转发后端 DB，返回结果（spec 5.5.1 规则 1 验收条件）
- [ ] M4-T3.13 编写性能测试：单次查询开销 ≤100ms（含 WASM → 代理 HTTP/WS 往返 + 后端 DB 执行 + 结果返回，spec 4.1 性能 5）
- [ ] M4-T3.14 编写性能测试：结果集 1,000 行 ≤200ms（spec 4.1 性能 5）

**验收标准**：
1. `WasmRealDbConnection` 通过 HTTP/WebSocket 代理桥接后端 DB
2. 复用既有 `WasmQuery`，sql + params 不变
3. JS 绑定暴露给 JS
4. 性能达标（单次 ≤100ms，1,000 行 ≤200ms）
5. `cargo test -p sz-orm-wasm --features wasm-real-db` 新增测试全部通过
6. 附 `packages/sz-orm-wasm/src/real_db/connection.rs` 与 `executor.rs` 新增代码的 file:line 证据

**依赖**：M4-T1（feature gate）、M4-T2（Protocol）

---

## M4-T4：WasmDbProxy（后端代理）+ 鉴权 + 限流 + SQL 白名单

**任务描述**：实现 `WasmDbProxy`（后端代理服务）+ `WasmDbAuthValidator`（鉴权）+ `WasmDbRateLimiter`（限流）+ `WasmDbSqlWhitelist`（SQL 白名单），复用既有 `Pool`/`sz-orm-sql-validator`/`sz-orm-limit`。

**涉及文件**：
- `packages/sz-orm-wasm/src/real_db/proxy.rs`（新增，WasmDbProxy 实现）
- `packages/sz-orm-wasm/src/real_db/auth.rs`（新增，WasmDbAuthValidator 实现）
- `packages/sz-orm-wasm/src/real_db/rate_limiter.rs`（新增，WasmDbRateLimiter 实现）
- `packages/sz-orm-wasm/src/real_db/sql_whitelist.rs`（新增，WasmDbSqlWhitelist 实现）

**复用标注**：
- 既有 `Pool`（`packages/sz-orm-core/src/pool.rs:743`）：代理连接池复用
- 既有 `sz-orm-sql-validator`：SQL 白名单复用 SQL 解析校验
- 既有 `sz-orm-limit`：代理限流复用

**子任务**：
- [ ] M4-T4.1 在 `proxy.rs` 定义 `pub struct WasmDbProxy { pool: Pool, auth_validator: WasmDbAuthValidator, rate_limiter: WasmDbRateLimiter, sql_whitelist: WasmDbSqlWhitelist, db_credentials: DbCredentials }`（design.md `:1378-1383`）
- [ ] M4-T4.2 实现 `WasmDbProxy::handle_request(&self, request: ProxyRequest) -> ProxyResponse`：鉴权 → 限流 → SQL 白名单 → 连接池执行 → 序列化结果（design.md `:1386-1392`）
- [ ] M4-T4.3 在 `auth.rs` 实现 `WasmDbAuthValidator::verify(&self, token: &str, session_id: &str) -> Result<(), WasmRealDbError>`：Token/Session 鉴权，会话隔离（spec 5.5.1 规则 6）
- [ ] M4-T4.4 实现后端 DB 凭据隔离：`db_credentials` 仅代理持有，不下发到 WASM 端，WASM 端仅持代理 Token（spec 4.3 规则 8）
- [ ] M4-T4.5 在 `rate_limiter.rs` 实现 `WasmDbRateLimiter::check(&self, session_id: &str) -> Result<(), WasmRealDbError>`：单会话 QPS 上限限流（默认 100，spec 5.5.1 规则 7）
- [ ] M4-T4.6 在 `sql_whitelist.rs` 实现 `WasmDbSqlWhitelist::check(&self, sql: &str) -> Result<(), WasmRealDbError>`：SQL 白名单（仅允许 SELECT/INSERT/UPDATE/DELETE + 参数化，禁止 DDL/批量危险操作，spec 5.5.1 规则 7）
- [ ] M4-T4.7 实现参数化检查：SQL 必须参数化（`?`/`$1` 占位符），禁止 SQL 字符串拼接
- [ ] M4-T4.8 编写单元测试：未授权 WASM 端连接代理拒绝，提示鉴权失败；WASM 端无后端 DB 凭据（spec 5.5.1 规则 6 验收条件）
- [ ] M4-T4.9 编写单元测试：WASM 端发 DDL（CREATE/DROP TABLE）代理拒绝，提示白名单策略（spec 5.5.1 规则 7 验收条件）
- [ ] M4-T4.10 编写单元测试：WASM 端超 QPS（>100）代理限流，提示限流（spec 5.5.1 规则 7 验收条件）
- [ ] M4-T4.11 编写单元测试：WASM 端尝试获取后端 DB 凭据，代理不下发凭据，拒绝凭据请求（spec 5.5.3 异常 4）
- [ ] M4-T4.12 编写性能测试：代理吞吐 ≥10,000 查询/秒（单实例，含鉴权 + 限流 + SQL 白名单 + 连接池复用，spec 4.1 性能 6）

**验收标准**：
1. `WasmDbProxy` 鉴权 + 限流 + SQL 白名单 + 连接池完整
2. 后端 DB 凭据隔离，不下发 WASM 端
3. SQL 白名单禁止 DDL/危险操作，强制参数化
4. 性能达标（吞吐 ≥10,000 查询/秒）
5. `cargo test -p sz-orm-wasm --features wasm-real-db` 新增测试全部通过
6. 附 `packages/sz-orm-wasm/src/real_db/proxy.rs` 等新增代码的 file:line 证据

**依赖**：M4-T1（feature gate）、M4-T2（Protocol）

---

## M4-T5：WasiSocketConnection（WASI socket 直连，可选）

**任务描述**：实现 `WasiSocketConnection`，WASI preview2 socket 直连后端 DB（不经代理，可选，适用于可信 WASI 环境）。

**涉及文件**：
- `packages/sz-orm-wasm/src/real_db/wasi_socket.rs`（新增，WasiSocketConnection 实现）

**复用标注**：WASI preview2 socket API

**子任务**：
- [ ] M4-T5.1 在 `wasi_socket.rs` 定义 `pub struct WasiSocketConnection { db_endpoint: String, db_credentials: DbCredentials }`（design.md `:1340`）
- [ ] M4-T5.2 实现 `WasiSocketConnection::connect(endpoint: &str, credentials: DbCredentials) -> Result<Self, WasmRealDbError>`：WASI preview2 socket 直连
- [ ] M4-T5.3 实现 `WasiSocketConnection::query(&self, sql: &str, params: Vec<serde_json::Value>) -> Result<Vec<serde_json::Value>, WasmRealDbError>`：直连查询
- [ ] M4-T5.4 用 `#[cfg(feature = "wasi-socket")]` 门控（仅 WASI 环境启用，非浏览器）
- [ ] M4-T5.5 编写集成测试：WASI 运行时支持 socket，配置直连模式，WASM 直连后端 DB，不经代理（spec 5.5.1 规则 3 验收条件）
- [ ] M4-T5.6 编写边界测试：非 WASI 环境（浏览器）禁用 wasi-socket feature

**验收标准**：
1. `WasiSocketConnection` WASI preview2 socket 直连后端 DB
2. 仅 WASI 环境启用，非浏览器
3. `cargo test -p sz-orm-wasm --features wasi-socket` 新增测试全部通过
4. 附 `packages/sz-orm-wasm/src/real_db/wasi_socket.rs` 新增代码的 file:line 证据

**依赖**：M4-T1（feature gate）

---

## M4-T6：WasmRealDbReconnector（重连）+ WasmRealDbMetrics（指标）

**任务描述**：实现 `WasmRealDbReconnector`（代理临时不可用后自动重连）+ `WasmRealDbMetrics`（查询指标输出，接入既有 Prometheus）。

**涉及文件**：
- `packages/sz-orm-wasm/src/real_db/reconnector.rs`（新增，WasmRealDbReconnector 实现）
- `packages/sz-orm-wasm/src/real_db/metrics.rs`（新增，WasmRealDbMetrics 实现）

**复用标注**：既有 `sz-orm-observability`（MetricsRegistry）：查询指标复用

**子任务**：
- [ ] M4-T6.1 在 `reconnector.rs` 定义 `pub struct WasmRealDbReconnector { max_retries: u32, base_delay_ms: u64, max_delay_ms: u64 }`
- [ ] M4-T6.2 实现 `WasmRealDbReconnector::reconnect(&self, connection: &mut WasmRealDbConnection) -> Result<(), WasmRealDbError>`：指数退避重连（design.md `:1450`）
- [ ] M4-T6.3 实现重连失败返回明确错误码，不静默丢查询（spec 5.5.1 规则 8）
- [ ] M4-T6.4 在 `metrics.rs` 定义 `pub struct WasmRealDbMetrics { registry: MetricsRegistry }`
- [ ] M4-T6.5 实现指标：`wasm_real_db_query_duration_seconds`（Histogram）/ `wasm_real_db_reconnect_total`（Counter）/ `wasm_db_proxy_qps`（Gauge）/ `wasm_db_proxy_sql_whitelist_rejected_total`（Counter）/ `wasm_db_proxy_rate_limited_total`（Counter）（design.md `:1593-1597`）
- [ ] M4-T6.6 实现 `WasmRealDbMetrics::record_query(&self, duration_ms: u64, success: bool)` + `record_reconnect(&self)` + `record_sql_rejected(&self)` + `record_rate_limited(&self)`
- [ ] M4-T6.7 编写单元测试：`WasmRealDbReconnector` 指数退避重连，重连成功后继续查询
- [ ] M4-T6.8 编写集成测试：代理临时不可用，WASM 端自动重连，查询失败返回明确错误码（spec 5.5.1 规则 8 验收条件）
- [ ] M4-T6.9 编写集成测试：启用 WASM 真实 DB，Prometheus 抓取查询指标 + 代理会话/QPS 指标（spec 5.5.1 规则 9 验收条件）
- [ ] M4-T6.10 编写边界测试：代理宕机，自动重连，查询失败返回明确错误码，不静默丢查询（spec 5.5.3 异常 1）

**验收标准**：
1. `WasmRealDbReconnector` 指数退避重连，不静默丢查询
2. `WasmRealDbMetrics` 输出查询数/延迟/错误率/重连次数，接入 Prometheus
3. `cargo test -p sz-orm-wasm --features wasm-real-db` 新增测试全部通过
4. 附 `packages/sz-orm-wasm/src/real_db/reconnector.rs` 与 `metrics.rs` 新增代码的 file:line 证据

**依赖**：M4-T1（feature gate）、M4-T3（WasmRealDbConnection）

---

## M4-T7：M4-005 集成测试与门禁验证

**任务描述**：M4-005 WASM 真实数据库连接集成测试与门禁验证，确保 REQ-V42-005 全部验收条件满足。

**涉及文件**：
- `packages/sz-orm-wasm/tests/real_db_integration.rs`（新增，集成测试）
- `packages/sz-orm-wasm/tests/proxy_e2e.rs`（新增，代理端到端测试）

**子任务**：
- [ ] M4-T7.1 编写集成测试：WASM 端查询经代理到后端 DB（MySQL/PostgreSQL），结果正确（spec 5.5.1 规则 1 验收条件）
- [ ] M4-T7.2 编写集成测试：未授权 WASM 端连接代理拒绝（spec 5.5.1 规则 6 验收条件）
- [ ] M4-T7.3 编写集成测试：WASM 端发 DDL 代理拒绝白名单（spec 5.5.1 规则 7 验收条件）
- [ ] M4-T7.4 编写集成测试：WASM 端超 QPS 代理限流（spec 5.5.1 规则 7 验收条件）
- [ ] M4-T7.5 编写集成测试：代理临时不可用自动重连（spec 5.5.1 规则 8 验收条件）
- [ ] M4-T7.6 编写集成测试：执行 SELECT 返回 1,000 行，结果集序列化回 WASM，耗时 < 200ms（spec 5.5.1 规则 5 验收条件）
- [ ] M4-T7.7 编写集成测试：启用 WASM 真实 DB，Prometheus 抓取查询指标 + 代理会话/QPS 指标（spec 5.5.1 规则 9 验收条件）
- [ ] M4-T7.8 编写端到端测试：WASM 端 + 代理 + 后端 DB 完整流程
- [ ] M4-T7.9 运行 `cargo test -p sz-orm-wasm --features wasm-real-db` 全部通过
- [ ] M4-T7.10 运行 `cargo clippy -p sz-orm-wasm --features wasm-real-db -- -D warnings` 通过
- [ ] M4-T7.11 运行 `cargo fmt -p sz-orm-wasm -- --check` 通过
- [ ] M4-T7.12 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' packages/sz-orm-wasm/src/real_db/` 无占位实现
- [ ] M4-T7.13 验证默认 feature（不启用 wasm-real-db）行为与 v4.1.0 一致（内存数据库 + 本地存储，spec 5.5.1 规则 10 验收条件）

**验收标准**：
1. REQ-V42-005 全部 10 条业务规则验收条件满足
2. 集成测试与端到端测试全部通过
3. clippy/fmt/占位检查门禁通过
4. 默认 feature 行为与 v4.1.0 一致（内存数据库 + 本地存储）
5. 附集成测试运行输出证据

**依赖**：M4-T1~M4-T6 全部完成

---

# 七、M4：最终验证与文档同步（全局）

**目标**：对 v4.2.0 全部 5 项需求进行最终验证，确保 14 道门禁全部通过、文档同步、版本号更新、sz-pay 兼容性。
**预期工作量**：0.5 周
**对应需求**：全局
**依赖**：M1（REQ-V42-001）+ M2（REQ-V42-002）+ M3（REQ-V42-003/004）+ M4（REQ-V42-005）全部完成

## M4-T8：14 道门禁全量验证

**任务描述**：运行 AGENTS.md 定义的 14 道门禁全量验证，确保 v4.2.0 全部门禁通过。

**子任务**：
- [ ] M4-T8.1 运行 `cargo fmt --all -- --check`（门禁 1：fmt 格式检查）
- [ ] M4-T8.2 运行 `cargo check --workspace --all-targets`（门禁 2：默认 feature 编译检查）
- [ ] M4-T8.3 运行 `cargo clippy --workspace --all-targets -- -D warnings`（门禁 3：clippy 静态分析）
- [ ] M4-T8.4 运行 `cargo test --workspace -j 2 --no-fail-fast`（门禁 4：单元/集成测试，v4.1.0 基线不回退）
- [ ] M4-T8.5 运行 `cargo doc --workspace --no-deps --all-features`（门禁 5：文档构建）
- [ ] M4-T8.6 运行 `cargo audit` + `cargo deny check`（门禁 6：安全审计）
- [ ] M4-T8.7 运行 `cargo test --workspace -- --ignored`（门禁 7：真实服务集成测试）
- [ ] M4-T8.8 扫描 `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'`（门禁 8：禁止占位实现）
- [ ] M4-T8.9 运行 `scripts/check-sql-injection.ps1`（门禁 9：SQL 注入扫描）
- [ ] M4-T8.10 运行 `cargo check --workspace --all-targets --all-features`（门禁 10：feature 全组合编译）
- [ ] M4-T8.11 运行 `git diff --name-only HEAD`（门禁 11：上游仓库未修改检查，ADR-0001）
- [ ] M4-T8.12 运行 `python scripts/check-doc-consistency.py`（门禁 12：文档与代码一致性）
- [ ] M4-T8.13 运行 `bash scripts/audit-verify.sh <审计报告.md>`（门禁 13：审计证据验证）
- [ ] M4-T8.14 运行 `python scripts/check-doc-sync.py --diff HEAD`（门禁 14：文档同步更新检查）

**验收标准**：
1. 14 道门禁全部通过
2. v4.1.0 已验收测试基线不回退
3. 附 14 道门禁运行输出证据

**依赖**：M1-T8、M2-T9、M3-T7、M3-T13、M4-T7（所有里程碑集成测试完成）

---

## M4-T9：文档同步 + 版本号更新 + sz-pay 兼容性验证

**任务描述**：同步更新文档（API-STABILITY.md/README/CHANGELOG），更新版本号 v4.1.0→v4.2.0，验证 sz-pay 兼容性。

**子任务**：
- [ ] M4-T9.1 更新 `Cargo.toml` workspace.package.version 从 v4.1.0 → v4.2.0
- [ ] M4-T9.2 更新 `docs/API-STABILITY.md`：新增 7 个 feature gate 对应接口为 Experimental 等级
- [ ] M4-T9.3 更新 `CHANGELOG.md`：记录 v4.2.0 新增 5 项能力（feature gate/用法/示例）
- [ ] M4-T9.4 更新 `README.md`：新增 v4.2.0 能力概览
- [ ] M4-T9.5 更新 `docs/sz-orm-engineering-practices.md`：补充 v4.2.0 feature gate 列表
- [ ] M4-T9.6 更新 `AGENTS.md`：补充 v4.2.0 7 个 feature gate 与新包（sz-orm-cabi/sz-orm-go/sz-orm-java/sz-orm-cpp/sz-orm-designer）
- [ ] M4-T9.7 验证 sz-pay 兼容性：sz-pay 从 crates.io 拉取 sz-orm-* 6 个包，不启用 v4.2.0 新 feature，行为与 v4.1.0 一致（spec 4.5 规则 2）
- [ ] M4-T9.8 运行 `python scripts/check-doc-consistency.py`（门禁 12：文档与代码一致性）
- [ ] M4-T9.9 运行 `python scripts/check-doc-sync.py --diff HEAD`（门禁 14：文档同步更新检查）

**验收标准**：
1. 版本号更新为 v4.2.0
2. 文档同步更新（API-STABILITY/CHANGELOG/README/engineering-practices/AGENTS）
3. sz-pay 兼容性验证通过（不启用新 feature 行为不变）
4. 文档一致性检查通过
5. 附文档更新与 sz-pay 兼容性验证证据

**依赖**：M4-T8（14 道门禁通过）

---

## M4-T10：7 个 feature gate 逐步启用计划验证

**任务描述**：验证 7 个 feature gate 独立/组合/全启用编译通过，制定逐步启用计划。

**子任务**：
- [ ] M4-T10.1 验证每个 feature gate 独立编译通过：`cargo check -p sz-orm-dtx --features cross-lang-dtx` / `cargo check -p sz-orm-go --features lang-binding-go` / `cargo check -p sz-orm-java --features lang-binding-java` / `cargo check -p sz-orm-cpp --features lang-binding-cpp` / `cargo check -p sz-orm-designer --features schema-designer` / `cargo check -p sz-orm-swagger --features openapi-reverse` / `cargo check -p sz-orm-wasm --features wasm-real-db`
- [ ] M4-T10.2 验证 feature gate 组合编译通过：`cargo check --features sz-orm-dtx/cross-lang-dtx,sz-orm-swagger/openapi-reverse` 等
- [ ] M4-T10.3 验证全 feature 启用编译通过：`cargo check --workspace --all-targets --all-features`
- [ ] M4-T10.4 验证默认 feature（不启用任何新 feature）行为与 v4.1.0 一致
- [ ] M4-T10.5 验证 feature gate 与既有 feature 任意组合编译通过（`cross-lang-dtx` + `xa`、`wasm-real-db` + `js` + `persistence` 等）
- [ ] M4-T10.6 制定 feature gate 逐步启用计划文档：按 P1→P2→P3 优先级，分阶段启用（M1 cross-lang-dtx → M2 lang-binding-* → M3 schema-designer + openapi-reverse → M4 wasm-real-db）

**验收标准**：
1. 7 个 feature gate 独立编译全部通过
2. feature gate 组合编译全部通过（无冲突）
3. 全 feature 启用编译通过
4. 默认 feature 行为与 v4.1.0 一致
5. feature gate 与既有 feature 任意组合编译通过
6. 逐步启用计划文档已制定
7. 附 feature gate 验证证据

**依赖**：M4-T8（14 道门禁通过）

---

# 八、任务依赖关系图

```plantuml
@startuml
title sz-orm v4.2.0 任务依赖关系图

' M1 跨语言分布式事务
package "M1: 跨语言分布式事务 (P1)" {
  m1t1 "M1-T1 feature gate" as m1t1
  m1t2 "M1-T2 Protocol" as m1t2
  m1t3 "M1-T3 Serializer" as m1t3
  m1t4 "M1-T4 Participant" as m1t4
  m1t5 "M1-T5 Registry" as m1t5
  m1t6 "M1-T6 Alerter+可观测" as m1t6
  m1t7 "M1-T7 恢复+隔离" as m1t7
  m1t8 "M1-T8 集成测试" as m1t8
}

' M2 Go/Java/C++ 绑定
package "M2: Go/Java/C++ 绑定 (P2)" {
  m2t1 "M2-T1 sz-orm-cabi" as m2t1
  m2t2 "M2-T2 FFI内存+panic" as m2t2
  m2t3 "M2-T3 异步桥接+错误码" as m2t3
  m2t4 "M2-T4 C ABI 导出" as m2t4
  m2t5 "M2-T5 Go 绑定" as m2t5
  m2t6 "M2-T6 Java 绑定" as m2t6
  m2t7 "M2-T7 C++ 绑定" as m2t7
  m2t8 "M2-T8 文档+示例" as m2t8
  m2t9 "M2-T9 集成测试" as m2t9
}

' M3 Schema 设计器 + OpenAPI 反向生成
package "M3: Schema 设计器 (P2)" {
  m3t1 "M3-T1 designer feature" as m3t1
  m3t2 "M3-T2 SchemaDesign IR" as m3t2
  m3t3 "M3-T3 Designer+ER" as m3t3
  m3t4 "M3-T4 WebUI" as m3t4
  m3t5 "M3-T5 CodeGen+Parse" as m3t5
  m3t6 "M3-T6 Exporter+Masking" as m3t6
  m3t7 "M3-T7 CLI+集成测试" as m3t7
}

package "M3: OpenAPI 反向生成 (P2)" {
  m3t8 "M3-T8 reverse feature" as m3t8
  m3t9 "M3-T9 SchemaToModel" as m3t9
  m3t10 "M3-T10 ToMigration" as m3t10
  m3t11 "M3-T11 ToRepository" as m3t11
  m3t12 "M3-T12 Loop+Injection" as m3t12
  m3t13 "M3-T13 Config+CLI+集成" as m3t13
}

' M4 WASM 真实 DB + 最终验证
package "M4: WASM 真实 DB (P3)" {
  m4t1 "M4-T1 wasm-real-db feature" as m4t1
  m4t2 "M4-T2 ProxyProtocol" as m4t2
  m4t3 "M4-T3 Connection+Executor" as m4t3
  m4t4 "M4-T4 Proxy+Auth+Limit+Whitelist" as m4t4
  m4t5 "M4-T5 WasiSocket" as m4t5
  m4t6 "M4-T6 Reconnector+Metrics" as m4t6
  m4t7 "M4-T7 集成测试" as m4t7
}

package "M4: 最终验证" {
  m4t8 "M4-T8 14 道门禁" as m4t8
  m4t9 "M4-T9 文档+版本+sz-pay" as m4t9
  m4t10 "M4-T10 feature 启用计划" as m4t10
}

' M1 内部依赖
m1t2 --> m1t1
m1t3 --> m1t1
m1t3 --> m1t2
m1t4 --> m1t1
m1t4 --> m1t2
m1t4 --> m1t3
m1t5 --> m1t1
m1t5 --> m1t2
m1t6 --> m1t1
m1t6 --> m1t2
m1t7 --> m1t1
m1t7 --> m1t4
m1t7 --> m1t5
m1t7 --> m1t6
m1t8 --> m1t7

' M2 内部依赖
m2t2 --> m2t1
m2t3 --> m2t1
m2t4 --> m2t1
m2t4 --> m2t2
m2t4 --> m2t3
m2t5 --> m2t1
m2t5 --> m2t4
m2t6 --> m2t1
m2t6 --> m2t4
m2t7 --> m2t1
m2t7 --> m2t4
m2t8 --> m2t5
m2t8 --> m2t6
m2t8 --> m2t7
m2t9 --> m2t8

' M3 Schema 设计器内部依赖
m3t2 --> m3t1
m3t3 --> m3t1
m3t3 --> m3t2
m3t4 --> m3t1
m3t4 --> m3t3
m3t5 --> m3t1
m3t5 --> m3t2
m3t5 --> m3t3
m3t6 --> m3t1
m3t6 --> m3t3
m3t6 --> m3t5
m3t7 --> m3t6

' M3 OpenAPI 反向生成内部依赖
m3t9 --> m3t8
m3t10 --> m3t8
m3t10 --> m3t9
m3t11 --> m3t8
m3t11 --> m3t9
m3t12 --> m3t8
m3t12 --> m3t9
m3t13 --> m3t12

' M4 WASM 真实 DB 内部依赖
m4t2 --> m4t1
m4t3 --> m4t1
m4t3 --> m4t2
m4t4 --> m4t1
m4t4 --> m4t2
m4t5 --> m4t1
m4t6 --> m4t1
m4t6 --> m4t3
m4t7 --> m4t6

' M4 最终验证依赖
m4t8 --> m1t8
m4t8 --> m2t9
m4t8 --> m3t7
m4t8 --> m3t13
m4t8 --> m4t7
m4t9 --> m4t8
m4t10 --> m4t8

@enduml
```

**依赖关系说明**：
1. **M1（P1）独立开发**：跨语言分布式事务（M1-T1~T8）复用既有 sz-orm-dtx/sz-orm-grpc/sz-orm-queue/sz-orm-tracing，无新增需求间强依赖
2. **M2（P2）独立开发**：Go/Java/C++ 绑定（M2-T1~T9）复用既有 sz-orm-core + python/js 模式；与 M1 存在可选协同（参与者可经语言绑定接入，非强依赖）
3. **M3（P2）两需求可并行**：Schema 设计器（M3-T1~T7）与 OpenAPI 反向生成（M3-T8~T13）相互独立，可并行开发；跨需求依赖仅复用既有包（sz-orm-core/schema_sync + sz-orm-swagger）
4. **M4（P3）WASM 真实 DB 独立开发**：复用既有 sz-orm-wasm + sz-orm-observability
5. **M4 最终验证必须最后执行**：14 道门禁最终验证依赖所有里程碑集成测试完成，文档同步与版本号更新依赖门禁通过

---

# 九、验收标准汇总

## 9.1 跨语言分布式事务（M1，P1）

| 任务 | 对应需求 | 验收标准（节选） | 验证方法 |
|------|---------|----------------|---------|
| M1-T1 | — | cross-lang-dtx feature gate 搭建，默认 feature 行为不变 | `cargo check` + `--all-features` 编译通过 |
| M1-T2 | REQ-V42-001 | CrossLangParticipantProtocol gRPC/HTTP 双实现 + 协议版本化 | 单元测试验证协议版本不兼容返回错误 |
| M1-T3 | REQ-V42-001 | 补偿序列化往返无损 + 幂等键 | 单元测试验证序列化/反序列化 + 幂等键 |
| M1-T4 | REQ-V42-001 | CrossLangParticipant 适配为 TransactionParticipant + 超时故障 | 单元测试验证适配 + 超时标记 Failed |
| M1-T5 | REQ-V42-001 | Registry 鉴权 + 版本检查 | 单元测试验证未授权/版本不兼容拒绝 |
| M1-T6 | REQ-V42-001 | 跨语言事务可观测 + span 关联 + Prometheus 指标 | 单元测试验证结构化日志 + trace context |
| M1-T7 | REQ-V42-001 | 协调器崩溃恢复 + 故障隔离 + 性能 ≤1s/10参与者 | 集成测试验证恢复 + 隔离 + 性能 |
| M1-T8 | — | M1 集成测试与门禁验证 | M1 相关门禁全部通过 |

## 9.2 Go/Java/C++ 绑定（M2，P2）

| 任务 | 对应需求 | 验收标准（节选） | 验证方法 |
|------|---------|----------------|---------|
| M2-T1 | — | sz-orm-cabi 基础包搭建 | `cargo check` 编译通过 |
| M2-T2 | REQ-V42-002 | FfiMemoryManager + FfiPanicGuard，无内存泄漏无 UB | ASan/valgrind 验证 |
| M2-T3 | REQ-V42-002 | AsyncRuntimeBridge + ErrorCodeMapper 完整映射 | 单元测试验证错误码映射 |
| M2-T4 | REQ-V42-002 | C ABI 导出 Model/QueryBuilder/Pool/Transaction + 参数化 | 单元测试验证参数化 + 性能 ≤10μs |
| M2-T5 | REQ-V42-002 | Go 绑定 + goroutine 桥接 + Go doc | Go 测试验证行为与 Rust 一致 |
| M2-T6 | REQ-V42-002 | Java 绑定 + CompletableFuture 桥接 + Javadoc | Java 测试验证行为与 Rust 一致 |
| M2-T7 | REQ-V42-002 | C++ 绑定 + std::future 桥接 + Doxygen | C++ 测试验证行为与 Rust 一致 |
| M2-T8 | REQ-V42-002 | 三套绑定文档 + 示例 + 错误码表 | Go doc/Javadoc/Doxygen 生成验证 |
| M2-T9 | — | M2 集成测试与门禁验证 | M2 相关门禁全部通过 |

## 9.3 可视化 Schema 设计器（M3，P2）

| 任务 | 对应需求 | 验收标准（节选） | 验证方法 |
|------|---------|----------------|---------|
| M3-T1 | — | schema-designer feature gate 搭建 | `cargo check` 编译通过 |
| M3-T2 | REQ-V42-003 | SchemaDesign IR + 双向转换无损 | 单元测试验证 IR ↔ TableDef 往返 |
| M3-T3 | REQ-V42-003 | SchemaDesigner + ErDiagramEditor + 5 方言 DDL 预览 | 单元测试验证 ER 图 SVG + DDL 预览 |
| M3-T4 | REQ-V42-003 | Web UI + 响应 < 200ms | 集成测试验证浏览器图形化编辑 |
| M3-T5 | REQ-V42-003 | 设计↔代码双向生成 + 一致验证 + 性能 ≤1s/100表 | 单元测试验证双向往返 + 性能 |
| M3-T6 | REQ-V42-003 | 多格式导出 + 脱敏展示 | 单元测试验证 6 格式导出 + 脱敏 |
| M3-T7 | — | CLI 集成 + M3-003 集成测试 | `sz-orm designer` + 门禁通过 |

## 9.4 OpenAPI → ORM 反向生成（M3，P2）

| 任务 | 对应需求 | 验收标准（节选） | 验证方法 |
|------|---------|----------------|---------|
| M3-T8 | — | openapi-reverse feature gate 搭建 | `cargo check` 编译通过 |
| M3-T9 | REQ-V42-004 | SchemaToModelMapper 字段类型/约束映射 | 单元测试验证 string→String 等映射 |
| M3-T10 | REQ-V42-004 | OpenApiToMigrationMapper 5 方言 DDL | 单元测试验证 5 方言迁移文件 |
| M3-T11 | REQ-V42-004 | OpenApiToRepositoryMapper + 可编辑区 + 幂等 | 单元测试验证 CRUD 骨架 + 幂等 |
| M3-T12 | REQ-V42-004 | ApiFirstLoopVerifier 闭环验证 + 注入防护 | 单元测试验证闭环 + 恶意 spec 拒绝 |
| M3-T13 | — | ReverseGenConfig + CLI + M3-004 集成测试 | `sz-orm openapi:reverse` + 门禁通过 |

## 9.5 WASM 真实数据库连接（M4，P3）

| 任务 | 对应需求 | 验收标准（节选） | 验证方法 |
|------|---------|----------------|---------|
| M4-T1 | — | wasm-real-db feature gate 搭建 | `cargo check` 编译通过 |
| M4-T2 | REQ-V42-005 | WasmDbProxyProtocol JSON/MessagePack | 单元测试验证序列化往返 |
| M4-T3 | REQ-V42-005 | WasmRealDbConnection + 性能 ≤100ms | 集成测试验证查询 + 性能 |
| M4-T4 | REQ-V42-005 | 代理鉴权 + 限流 + SQL 白名单 + 吞吐 ≥10,000 QPS | 集成测试验证拒绝 + 性能 |
| M4-T5 | REQ-V42-005 | WasiSocketConnection WASI 直连 | 集成测试验证 WASI 直连 |
| M4-T6 | REQ-V42-005 | 重连 + Prometheus 指标 | 集成测试验证重连 + 指标 |
| M4-T7 | — | M4-005 集成测试与门禁验证 | M4-005 相关门禁全部通过 |

## 9.6 最终验证（M4）

| 任务 | 对应需求 | 验收标准（节选） | 验证方法 |
|------|---------|----------------|---------|
| M4-T8 | 全局 | 14 道门禁全部通过 | 运行 14 道门禁脚本 |
| M4-T9 | 全局 | 文档同步，版本号更新，sz-pay 兼容 | 文档一致性检查 + sz-pay 兼容性验证 |
| M4-T10 | 全局 | 7 个 feature gate 逐步启用计划 | feature 独立/组合/全启用编译验证 |

## 9.7 全局验收条件

1. **API 兼容性**：v4.1.0 既有公开 API 完全向后兼容，sz-pay 既有代码不受影响（sz-pay 不启用 v4.2.0 新 feature，行为与 v4.1.0 一致）
2. **feature gate 隔离**：所有新能力通过 feature gate 隔离（`cross-lang-dtx` / `lang-binding-go` / `lang-binding-java` / `lang-binding-cpp` / `schema-designer` / `openapi-reverse` / `wasm-real-db`），默认 feature 行为不变
3. **测试基线不回退**：v4.1.0 已验收测试基线不回退，v4.2.0 仅增不减
4. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（Schema 设计器/OpenAPI 反向生成/WASM 真实 DB 按方言能力适配）
5. **审计证据**：每项需求结论附 file:line 证据，遵循 AGENTS.md 审计合规铁律（`bash scripts/audit-verify.sh <审计报告.md>` 验证通过）
6. **14 道门禁通过**：v4.2.0 须通过 AGENTS.md 定义的 14 道门禁
7. **unsafe 零容忍**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释（FFI/JNI/cgo 边界须显式标注）
8. **禁止占位实现**：所有新增代码无 `todo!`/`unimplemented!`/`unreachable!`
9. **复用优先**：优先复用既有能力，不重复实现（跨语言事务复用 DtxManager/saga/tcc/xa + sz-orm-grpc/sz-orm-queue/sz-orm-tracing；Go/Java/C++ 绑定复用 python/js 模式 + sz-orm-core API；Schema 设计器复用 SchemaDiff/diff/DdlGenerator + sz-orm-masking；OpenAPI 反向生成复用 OpenAPISpec/Schema + DdlGenerator；WASM 真实 DB 复用 WasmQuery/WasmDatabase/js_bindings + Pool/sz-orm-sql-validator/sz-orm-limit/sz-orm-observability）

---

# 十、已验证的 file:line 代码证据清单

> 本清单所有 file:line 引用均来自 spec.md/design.md 既有代码证据（非编造），已通过源码读取验证（2026-08-11），遵循 AGENTS.md 审计合规铁律。

## 10.1 REQ-V42-001 跨语言分布式事务

| 代码位置 | 内容 | 来源 |
|---------|------|------|
| `packages/sz-orm-dtx/src/lib.rs:19` | `cross_shard`（跨分片事务协调） | spec.md `:27` / design.md `:62` |
| `packages/sz-orm-dtx/src/lib.rs:20` | `saga`（Saga 长事务编排） | spec.md `:27` / design.md `:63` |
| `packages/sz-orm-dtx/src/lib.rs:21` | `tcc`（TCC 三阶段提交） | spec.md `:27` / design.md `:64` |
| `packages/sz-orm-dtx/src/lib.rs:25` | `recovery`（XA 崩溃恢复，feature 隔离） | spec.md `:27` / design.md `:65` |
| `packages/sz-orm-dtx/src/lib.rs:27` | `suspension`（XA 悬挂检测） | spec.md `:27` / design.md `:66` |
| `packages/sz-orm-dtx/src/lib.rs:29` | `xa`（XA 事务） | spec.md `:27` / design.md `:67` |
| `packages/sz-orm-dtx/src/lib.rs:37` | `TransactionLogEntry`（事务日志条目） | spec.md `:27` / design.md `:68` |
| `packages/sz-orm-dtx/src/lib.rs:53` | `TransactionLogStore`（事务日志存储 trait） | spec.md `:27` / design.md `:69` |
| `packages/sz-orm-dtx/src/lib.rs:159` | `TransactionState`（8 态事务状态机） | spec.md `:27` / design.md `:70` |
| `packages/sz-orm-dtx/src/lib.rs:171` | `ParticipantState`（5 态参与者状态机） | spec.md `:27` / design.md `:71` |
| `packages/sz-orm-dtx/src/lib.rs:179` | `ParticipantCallback`（参与者回调签名） | design.md `:195` |
| `packages/sz-orm-dtx/src/lib.rs:182` | `TransactionParticipant`（事务参与者） | spec.md `:27` / design.md `:72` |
| `packages/sz-orm-dtx/src/lib.rs:201` | `with_prepare`（注册 prepare 回调） | design.md `:196` |
| `packages/sz-orm-dtx/src/lib.rs:209` | `with_commit`（注册 commit 回调） | design.md `:196` |
| `packages/sz-orm-dtx/src/lib.rs:217` | `with_rollback`（注册 rollback 回调） | design.md `:196` |
| `packages/sz-orm-dtx/src/lib.rs:249` | `fail`（标记参与者故障） | design.md `:197` |
| `packages/sz-orm-dtx/src/lib.rs:266` | `DistributedTransaction`（分布式事务结构） | spec.md `:27` / design.md `:73` |
| `packages/sz-orm-dtx/src/lib.rs:428` | `DtxManager`（事务管理器） | spec.md `:27` / design.md `:74` |
| `packages/sz-orm-dtx/Cargo.toml:31` | `xa` feature | spec.md `:27` / design.md `:75` |
| `packages/sz-orm-dtx/Cargo.toml:33` | `real-db` feature | spec.md `:27` / design.md `:76` |
| `packages/sz-orm-grpc/Cargo.toml:2` | `sz-orm-grpc`（gRPC 包，tonic + prost） | spec.md `:27` / design.md `:77` |
| `packages/sz-orm-queue/src/queue.rs:18` | `MessageQueue` trait（消息队列） | spec.md `:27` / design.md `:78` |
| `packages/sz-orm-queue/src/queue.rs:57` | `Message`（消息体结构） | spec.md `:27` / design.md `:79` |
| `packages/sz-orm-tracing/src/lib.rs:129` | `Tracer` trait（追踪器统一接口） | spec.md `:27` / design.md `:80` |
| `packages/sz-orm-tracing/src/lib.rs:136` | `SzTracer`（自研追踪器实现） | spec.md `:27` / design.md `:81` |

## 10.2 REQ-V42-002 Go/Java/C++ 绑定

| 代码位置 | 内容 | 来源 |
|---------|------|------|
| `packages/sz-orm-python/Cargo.toml:15` | `sz-orm-python`（PyO3，cdylib + rlib 模式参考） | spec.md `:28` / design.md `:82` |
| `packages/sz-orm-python/Cargo.toml:19` | `pyo3`（Python FFI） | spec.md `:28` / design.md `:83` |
| `packages/sz-orm-python/Cargo.toml:20` | `pyo3-asyncio`（Python 异步桥接） | spec.md `:28` / design.md `:84` |
| `packages/sz-orm-js/Cargo.toml:15` | `sz-orm-js`（napi-rs，cdylib + rlib 模式参考） | spec.md `:28` / design.md `:85` |
| `packages/sz-orm-js/Cargo.toml:19` | `napi`（Node.js FFI） | spec.md `:28` / design.md `:86` |
| `packages/sz-orm-js/Cargo.toml:20` | `napi-derive`（napi 派生宏） | spec.md `:28` / design.md `:87` |
| `packages/sz-orm-core/src/model.rs:37` | `Model` trait（模型统一接口） | spec.md `:28` / design.md `:88` |
| `packages/sz-orm-core/src/query.rs:36` | `QueryBuilder<M: Model>`（查询构建器） | spec.md `:28` / design.md `:89` |
| `packages/sz-orm-core/src/pool.rs:45` | `Connection` trait（连接统一接口） | spec.md `:28` / design.md `:90` |
| `packages/sz-orm-core/src/pool.rs:743` | `Pool`（连接池） | spec.md `:28` / design.md `:91` |
| `packages/sz-orm-core/src/transaction.rs:159` | `Transaction`（事务） | spec.md `:28` / design.md `:92` |
| `packages/sz-orm-core/src/transaction.rs:527` | `TransactionManager`（事务管理器） | spec.md `:28` / design.md `:93` |

## 10.3 REQ-V42-003 可视化 Schema 设计器

| 代码位置 | 内容 | 来源 |
|---------|------|------|
| `packages/sz-orm-core/src/schema_sync.rs:100` | `SchemaDiff`（schema 差异结构） | spec.md `:29` / design.md `:94` |
| `packages/sz-orm-core/src/schema_sync.rs:200` | `diff` 函数（差分计算） | spec.md `:29` / design.md `:95` |
| `packages/sz-orm-core/src/schema_sync.rs:361` | `DdlGenerator` trait（5 方言 DDL 生成器） | spec.md `:29` / design.md `:96` |
| `packages/sz-orm-core/src/schema_sync.rs:369` | `MySqlDdlGenerator`（MySQL DDL 生成器） | design.md `:97` |
| `packages/sz-orm-core/src/schema_sync.rs:439` | `PgDdlGenerator`（PostgreSQL DDL 生成器） | design.md `:98` |
| `packages/sz-orm-core/src/schema_sync.rs:479` | `SqliteDdlGenerator`（SQLite DDL 生成器） | design.md `:99` |
| `packages/sz-orm-core/src/schema_sync.rs:522` | `OracleDdlGenerator`（Oracle DDL 生成器） | design.md `:100` |
| `packages/sz-orm-core/src/schema_sync.rs:565` | `MssqlDdlGenerator`（MSSQL DDL 生成器） | design.md `:101` |
| `packages/sz-orm-core/src/schema_sync.rs:612` | `SchemaSync`（schema 同步编排） | spec.md `:29` / design.md `:102` |
| `cli/src/main.rs:1625` | `cmd_generate_schema`（CLI schema 生成命令） | spec.md `:29` / design.md `:103` |
| `cli/src/main.rs:1630` | `cmd_generate_schema` 实现 | spec.md `:29` / design.md `:104` |
| `packages/sz-orm-core/src/migration.rs:10` | `Migration`（迁移版本结构） | design.md `:159` |

## 10.4 REQ-V42-004 OpenAPI → ORM 反向生成

| 代码位置 | 内容 | 来源 |
|---------|------|------|
| `packages/sz-orm-swagger/src/lib.rs:28` | `OpenAPISpec`（OpenAPI 3.0 规范根对象） | spec.md `:30` / design.md `:105` |
| `packages/sz-orm-swagger/src/lib.rs:55` | `Components`（组件表，含 schemas） | spec.md `:30` / design.md `:106` |
| `packages/sz-orm-swagger/src/lib.rs:328` | `Schema`（Schema 定义枚"enum"） | spec.md `:30` / design.md `:107` |
| `packages/sz-orm-swagger/src/lib.rs:430` | `ObjectType`（对象类型 Schema） | spec.md `:30` / design.md `:108` |
| `packages/sz-orm-swagger/src/lib.rs:490` | `ArrayType`（数组类型 Schema） | spec.md `:30` / design.md `:109` |
| `packages/sz-orm-swagger/src/lib.rs:540` | `PrimitiveSchema`（基本类型 Schema） | spec.md `:30` / design.md `:110` |
| `packages/sz-orm-swagger/src/lib.rs:1096` | `OpenAPIGenerator`（正向生成器） | spec.md `:30` / design.md `:111` |
| `packages/sz-orm-swagger/src/lib.rs:1229` | `SwaggerUi`（Swagger UI 渲染） | spec.md `:30` |
| `packages/sz-orm-swagger/src/lib.rs:1325` | `model_to_openapi_schema`（正向：ORM Model → OpenAPI Schema） | spec.md `:30` / design.md `:112` |

## 10.5 REQ-V42-005 WASM 真实数据库连接

| 代码位置 | 内容 | 来源 |
|---------|------|------|
| `packages/sz-orm-wasm/src/lib.rs:12` | `advanced`（内存限制/WASI 沙箱/异步调度） | spec.md `:31` / design.md `:113` |
| `packages/sz-orm-wasm/src/lib.rs:15` | `js_bindings`（JS 绑定，feature 隔离） | spec.md `:31` / design.md `:114` |
| `packages/sz-orm-wasm/src/lib.rs:18` | `persistence`（持久化，feature 隔离） | spec.md `:31` / design.md `:115` |
| `packages/sz-orm-wasm/src/lib.rs:38` | `WasmQuery`（WASM 查询请求） | spec.md `:31` / design.md `:116` |
| `packages/sz-orm-wasm/src/lib.rs:44` | `WasmQuery::new`（构造方法） | design.md `:284` |
| `packages/sz-orm-wasm/src/lib.rs:51` | `WasmQuery::with_params`（带参数构造） | design.md `:284` |
| `packages/sz-orm-wasm/src/lib.rs:67` | `WasmDatabase`（内存数据库，SQL 子集） | spec.md `:31` / design.md `:117` |
| `packages/sz-orm-wasm/src/lib.rs:78` | `WasmDatabase::query`（SELECT） | design.md `:285` |
| `packages/sz-orm-wasm/src/lib.rs:103` | `WasmDatabase::execute`（INSERT/UPDATE/DELETE/CREATE） | design.md `:285` |
| `packages/sz-orm-wasm/Cargo.toml:29` | `js` feature（wasm-bindgen + js-sys） | spec.md `:31` / design.md `:118` |
| `packages/sz-orm-wasm/Cargo.toml:30` | `persistence` feature（web-sys + thiserror） | spec.md `:31` / design.md `:119` |

## 10.6 既有 feature gate 体系

| 代码位置 | 内容 | 来源 |
|---------|------|------|
| `packages/sz-orm-core/Cargo.toml:83-128` | 既有 feature gate 体系（prod-ready 14 子 feature + v3.9.0 4 feature + v4.1.0 4 feature） | design.md `:295` |
| `packages/sz-orm-dtx/Cargo.toml:28-33` | sz-orm-dtx 既有 `xa`/`real-db` feature | design.md `:295` |
| `packages/sz-orm-wasm/Cargo.toml:27-30` | sz-orm-wasm 既有 `js`/`persistence` feature | design.md `:295` |

---

# 十一、门禁验证清单

> v4.2.0 须通过 AGENTS.md 定义的 14 道门禁，每道门禁附验证命令与验收条件。

| # | 门禁 | 命令 | 验收条件 | 对应任务 |
|---|------|------|---------|---------|
| 1 | fmt 格式检查 | `cargo fmt --all -- --check` | 全 workspace 代码格式一致 | M4-T8.1 |
| 2 | check 编译检查 | `cargo check --workspace --all-targets` | 默认 feature 编译通过，行为与 v4.1.0 一致 | M4-T8.2 |
| 3 | clippy 静态分析 | `cargo clippy --workspace --all-targets -- -D warnings` | 无 clippy 警告 | M4-T8.3 |
| 4 | test 单元/集成测试 | `cargo test --workspace -j 2 --no-fail-fast` | 全部通过，v4.1.0 基线不回退 | M4-T8.4 |
| 5 | doc 文档构建 | `cargo doc --workspace --no-deps --all-features` | 文档构建通过 | M4-T8.5 |
| 6 | audit 安全审计 | `cargo audit` + `cargo deny check` | 无安全漏洞 | M4-T8.6 |
| 7 | integration 真实服务集成 | `cargo test --workspace -- --ignored` | 真实 DB 集成测试通过 | M4-T8.7 |
| 8 | 禁止占位实现检查 | `grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'` | 无占位实现 | M4-T8.8 |
| 9 | SQL 注入扫描 | `scripts/check-sql-injection.ps1` | 无 SQL 注入 | M4-T8.9 |
| 10 | Feature 全组合编译 | `cargo check --workspace --all-targets --all-features` | 全 feature 组合编译通过 | M4-T8.10 |
| 11 | 上游仓库未修改检查 | `git diff --name-only HEAD` | ADR-0001 未修改上游 | M4-T8.11 |
| 12 | 文档与代码一致性检查 | `python scripts/check-doc-consistency.py` | 文档与代码一致 | M4-T8.12 |
| 13 | 审计证据验证 | `bash scripts/audit-verify.sh <审计报告.md>` | 审计证据真实存在 | M4-T8.13 |
| 14 | 文档同步更新检查 | `python scripts/check-doc-sync.py --diff HEAD` | 文档同步更新 | M4-T8.14 |

---

# 十二、风险与缓解措施

> 来自 design.md 第四章风险矩阵，每项风险附对应任务与缓解措施。

| 风险 ID | 风险描述 | 缓解措施 | 对应任务 |
|---------|---------|---------|---------|
| R-001 | 跨语言参与者 gRPC 调用超时导致事务阻塞 | `participant_timeout_ms` 超时配置（默认 5s），超时标记 `ParticipantState::Failed` 触发补偿 | M1-T4/M1-T7 |
| R-002 | 协调器崩溃恢复后跨语言参与者状态冲突 | 复用既有 `TransactionLogStore` + `recovery`，恢复冲突检测告警人工处理 | M1-T7 |
| R-003 | FFI 边界 Rust panic 跨语言传播（UB） | `FfiPanicGuard` 用 `catch_unwind` 捕获每个 `extern "C"` 函数，转错误码 | M2-T2/M2-T4 |
| R-004 | FFI 内存泄漏（Rust 分配未释放） | `FfiMemoryManager` 跟踪分配，`sz_orm_free` 统一释放，ASan/valgrind 验证 | M2-T2 |
| R-005 | 异步运行时桥接失败（tokio 未初始化） | `sz_orm_init` 初始化 tokio 运行时，未初始化返回明确错误码 | M2-T3 |
| R-006 | Schema 设计器双向生成不一致 | `CodeReverseParser` 反向解析后对比 `SchemaDiff`，不一致告警标注差异 | M3-T5 |
| R-007 | DDL 生成方言不支持特性（如 SQLite CHECK） | `DdlGenerator` 降级生成，标注不支持特性，跳过 | M3-T5 |
| R-008 | OpenAPI spec 含恶意内嵌代码（注入） | `OpenApiInjectionGuard` 不执行 spec 内嵌代码，不信任未签名 spec | M3-T12 |
| R-009 | OpenAPI 反向生成覆盖用户手写业务逻辑 | 可编辑区标注，默认 `overwrite=false`，仅更新骨架 | M3-T11 |
| R-010 | WASM DB 代理不可用导致查询丢失 | `WasmRealDbReconnector` 指数退避重连，查询失败返回明确错误码 | M4-T6 |
| R-011 | WASM 端发 DDL/危险 SQL 拖垮后端 DB | `WasmDbSqlWhitelist` 仅允许 SELECT/INSERT/UPDATE/DELETE + 参数化 | M4-T4 |
| R-012 | 后端 DB 凭据泄露到 WASM 端 | 代理持有凭据，WASM 端仅持代理 Token，凭据不下发 | M4-T4 |
| R-013 | WASM 端超 QPS 滥用代理 | `WasmDbRateLimiter` 单会话 QPS 上限（默认 100） | M4-T4 |
| R-014 | 新增 feature 与既有 feature 组合编译失败 | 14 道门禁第 10 项 feature 全组合编译验证 | M4-T8/M4-T10 |
| R-015 | sz-pay 既有代码因 API 变更破坏 | 无 Breaking Change，7 个 feature gate 隔离默认关闭，sz-pay 回归测试 | M4-T9 |

---

# 十三、实施建议

## 13.1 开发顺序

1. **M1（P1，2 周）**：跨语言分布式事务优先开发，微服务互操作核心能力
2. **M2（P2，2.5 周）**：Go/Java/C++ 绑定，与 M1 可并行（可选协同非强依赖）
3. **M3（P2，3 周）**：Schema 设计器 + OpenAPI 反向生成，两需求可并行
4. **M4（P3，2 周）**：WASM 真实 DB + 最终验证，WASM 真实 DB 与 M1-M3 可并行，最终验证必须最后

## 13.2 并行开发建议

- M1/M2/M3/M4-005 主体相互独立，可并行开发（4 个分支并行）
- M4 最终验证（M4-T8~T10）必须等待所有里程碑集成测试完成
- 跨需求依赖仅复用既有包，无新增需求间强依赖

## 13.3 验证节奏

- 每个任务完成后运行 `cargo test -p <package> --features <feature>` 验证
- 每个里程碑末尾运行集成测试与门禁验证（M1-T8/M2-T9/M3-T7/M3-T13/M4-T7）
- 最终验证运行 14 道门禁全量验证（M4-T8）

## 13.4 文档同步

- 每个里程碑完成后更新 CHANGELOG.md
- M4-T9 统一更新 API-STABILITY.md/README/AGENTS/engineering-practices
- 版本号 v4.1.0 → v4.2.0 在 M4-T9 更新

---

> 文档结束。本任务规划对应 spec.md（需求规格）与 design.md（技术设计），所有 file:line 证据已验证，遵循 AGENTS.md 审计合规铁律。
