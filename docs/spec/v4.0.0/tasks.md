# sz-orm v4.0.0 编码任务规划

> 版本：v4.0.0（AI 自动调优闭环 + 多 LLM 模型 + 混合搜索 + 数据 lineage + 分片 rebalance + failover 自动化 + 服务网格 + GraphQL 深度集成 + CDC）
> 基线：v3.9.0（criterion benchmark 套件 + semver/API 稳定性 + 数据验证框架 + 迁移 dry-run/影响分析 + CI/CD 模板 + 流式导出，6760+ tests passed 0 failed）
> 日期：2026-08-11
> 文档定位：编码任务规划（How to execute），对应需求规格 `spec.md`（What to build）与技术设计 `design.md`（How to build）
> 任务约束：无 Breaking Change（9 个 feature gate 隔离）+ 优先复用既有能力 + 五方言覆盖 + 每项任务附 file:line 代码证据 + unsafe 零容忍 + 禁止占位实现（todo!/unimplemented!/unreachable!）
> 审计合规铁律：每项任务结论须附真实存在的 file:line 证据，修复后必须运行 `cargo test` 并附输出，禁止未验证即标记 ✅
> 实施顺序：按 design.md 第二章依赖关系，M1 多 LLM（P0 基座）→ M2 AI 调优（P0 依赖 M1）→ M3-M6（P1 可并行）→ M7 CDC（P2 先于 GraphQL）→ M8 GraphQL（P2 依赖 M7）→ M9 服务网格（P2 独立）→ M10 最终验证

---

# 一、任务总览

## 1.1 里程碑 × 任务数 × 预期工作量

| 里程碑 | 名称 | 对应需求 | 优先级 | 任务数 | 子任务数 | 预期工作量 |
|--------|------|---------|--------|--------|----------|-----------|
| M1 | 多 LLM 模型支持 | REQ-V40-002 | P0 | 6 | 48 | 1.5 周 |
| M2 | AI 自动调优闭环 | REQ-V40-001 | P0 | 5 | 42 | 1.5 周 |
| M3 | 混合搜索 | REQ-V40-003 | P1 | 5 | 38 | 1.5 周 |
| M4 | 数据 lineage | REQ-V40-004 | P1 | 6 | 44 | 1.5 周 |
| M5 | 分片自动 rebalance | REQ-V40-005 | P1 | 5 | 36 | 1.5 周 |
| M6 | 数据库 failover 自动化 | REQ-V40-006 | P1 | 5 | 35 | 1 周 |
| M7 | CDC 变更数据捕获 | REQ-V40-009 | P2 | 7 | 52 | 2 周 |
| M8 | GraphQL 深度集成 | REQ-V40-008 | P2 | 7 | 48 | 2 周 |
| M9 | 服务网格集成 | REQ-V40-007 | P2 | 5 | 32 | 1 周 |
| M10 | 最终验证与文档同步 | 全局 | — | 3 | 24 | 0.5 周 |
| **合计** | — | **9 项全覆盖** | — | **54** | **399** | **14 周** |

## 1.2 任务编号约定

- 主任务：`M{里程碑号}-T{任务序号}`（如 M1-T1）
- 子任务：`M{里程碑号}-T{任务序号}.{子任务序号}`（如 M1-T2.1）
- 集成验证任务：每个里程碑末尾固定一个集成测试任务（如 M1-T6）

## 1.3 全局约束（适用于所有任务）

1. **feature gate 隔离**：所有新能力通过 feature gate 隔离（`ai-auto-tuning` / `multi-llm` / `hybrid-search` / `data-lineage` / `shard-rebalance` / `auto-failover` / `service-mesh` / `async-graphql-integration` / `cdc`），默认 feature 行为不变
2. **既有 API 不变**：既有公开 API 签名完全向后兼容，sz-pay 既有代码不受影响（sz-pay 从 crates.io 拉取 sz-orm-* 6 个包）
3. **禁止占位实现**：禁止 `todo!`/`unimplemented!`/`unreachable!`
4. **unsafe 零容忍**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释
5. **五方言覆盖**：MySQL/PostgreSQL/SQLite/Oracle/MSSQL，AI 调优/failover/CDC 按方言能力适配
6. **审计证据**：每项任务结论附真实存在的 file:line 证据
7. **测试基线不回退**：v3.9.0 已验收测试基线（6760+ passed）不回退，v4.0.0 仅增不减
8. **复用优先**：优先复用既有能力，不重复实现（如 AI 调优复用 UnifiedQueryOptimizer/IndexAdvisor/RewriteAdvisor，混合搜索复用 PgVectorStore/ES provider，CDC 复用 sz-orm-queue，GraphQL 复用 DataLoader，failover 复用 HealthChecker/ReadWriteRouter，rebalance 复用 ShardingRouter，服务网格复用 MetricsRegistry/OTLP）

## 1.4 里程碑依赖关系

```
M1（多 LLM，P0 基座）──→ M2（AI 调优，P0 依赖 M1 提供 LlmProvider）
M3（混合搜索，P1 独立）
M4（数据 lineage，P1 独立）
M5（分片 rebalance，P1 独立）
M6（failover 自动化，P1 独立）
M7（CDC，P2 独立）──→ M8（GraphQL 深度集成，P2 依赖 M7 提供 Subscription 数据源）
M9（服务网格，P2 独立）
M10（最终验证）依赖 M1-M9 全部完成
```

> **依赖关系说明**：M3/M4/M5/M6/M9 相互独立可并行开发；M1 必须先于 M2（AI 调优需要 LlmProvider）；M7 必须先于 M8（GraphQL Subscription 需要 CDC ChangeEvent 作为数据源）。

---

# 二、M1：多 LLM 模型支持（REQ-V40-002，P0）

**目标**：提供 `LlmProvider` trait 抽象 LLM 调用，支持 Claude/Gemini/Ollama/OpenAI 四类 provider，统一配置切换，运行时热切换，按能力路由，既有 `OptimizerConfig::with_llm` 包装为 `OpenAIProvider`。
**预期工作量**：1.5 周
**对应需求**：REQ-V40-002（spec.md 5.2，design.md 2.2.2 模块 B）
**依赖**：无（M1 为 P0 基座，含 feature gate 体系搭建）

## M1-T1：multi-llm feature gate 体系搭建

**任务描述**：在 sz-orm-ai 中新增 `multi-llm` feature gate 及对应可选依赖（arc-swap），作为多 LLM provider 的隔离基础。默认关闭，避免无配置环境行为变化。

**涉及文件**：
- `packages/sz-orm-ai/Cargo.toml`（新增 `multi-llm` feature + arc-swap 可选依赖，复用既有 feature gate 模式 `:13-24`）

**复用标注**：复用既有 feature gate 体系（`packages/sz-orm-ai/Cargo.toml:13-24`，已有 real/llm-optimizer/plan-cache 等 feature）、既有 reqwest 依赖（`:33`，real feature 已引入）

**子任务**：
- [ ] M1-T1.1 在 `packages/sz-orm-ai/Cargo.toml` `[features]` 新增 `multi-llm = ["dep:reqwest", "dep:arc-swap", "dep:base64"]`，位置在既有 feature 之后，默认关闭
- [ ] M1-T1.2 在 `packages/sz-orm-ai/Cargo.toml` `[dependencies]` 新增 `arc-swap = { version = "1", optional = true }`（用于 LlmRouter 热切换）
- [ ] M1-T1.3 验证 `cargo check -p sz-orm-ai`（默认 feature，不启用 multi-llm）编译通过，行为与 v3.9.0 一致
- [ ] M1-T1.4 验证 `cargo check -p sz-orm-ai --features multi-llm` 编译通过
- [ ] M1-T1.5 验证 `cargo check --workspace --all-targets --all-features` 编译通过（feature 全组合门禁）

**验收标准**：
1. `cargo check -p sz-orm-ai` 默认编译通过，无 multi-llm 相关代码生效
2. `cargo check -p sz-orm-ai --features multi-llm` 编译通过
3. 既有 API 签名完全不变，`cargo test --workspace` 既有测试全部通过（6760+ passed 不回退）
4. 附 `packages/sz-orm-ai/Cargo.toml` 新增 feature 与依赖定义的 file:line 证据

**依赖**：无（基础设施任务，M1 所有任务依赖此任务）

---

## M1-T2：LlmProvider trait + LlmConfig + LlmError

**任务描述**：在 sz-orm-ai 新增 `llm_provider` 模块，定义 `LlmProvider` trait（complete + embed + provider_name + model）、`LlmConfig`（provider/model/api_key/api_base/timeout/max_tokens/fallback）、`LlmProviderKind` 枚举（Claude/Gemini/Ollama/OpenAI）、`LlmError` 错误类型。

**涉及文件**：
- `packages/sz-orm-ai/src/llm_provider/mod.rs`（新增模块，定义 `LlmProvider` trait、`LlmConfig`、`LlmProviderKind`、`LlmError`）
- `packages/sz-orm-ai/src/llm_provider/types.rs`（新增，`LlmRequestConfig`/`LlmResponse`/`LlmUsage`/`LlmCapability` 类型定义）
- `packages/sz-orm-ai/src/lib.rs`（新增 `#[cfg(feature = "multi-llm")] pub mod llm_provider;`）

**复用标注**：
- 既有 `OptimizerConfig`（`packages/sz-orm-ai/src/query_plan_optimizer.rs:177`）：公开字段 api_key/api_base/model/timeout_secs/max_tokens/enable_llm，作为 LlmConfig 设计参考
- 既有 thiserror 依赖：`packages/sz-orm-ai/Cargo.toml:29`（复用既有 thiserror 派生 Error）
- 既有 async-trait 依赖：`packages/sz-orm-ai/Cargo.toml:27`（复用既有 async-trait）

**子任务**：
- [ ] M1-T2.1 在 `packages/sz-orm-ai/src/llm_provider/mod.rs` 定义 `#[async_trait] pub trait LlmProvider: Send + Sync`：`async fn complete(&self, prompt: &str, config: &LlmRequestConfig) -> Result<LlmResponse, LlmError>` + `async fn embed(&self, text: &str) -> Result<Vec<f32>, LlmError>` + `fn provider_name(&self) -> &'static str` + `fn model(&self) -> &str`（design.md `:656-668`）
- [ ] M1-T2.2 定义 `LlmConfig` 结构：`provider: LlmProviderKind` + `model: String` + `api_key: Option<String>` + `api_base: String` + `timeout: Duration` + `max_tokens: u32` + `fallback: Option<Box<LlmConfig>>`，`#[derive(Debug, Clone)]`（design.md `:672-680`）
- [ ] M1-T2.3 定义 `LlmProviderKind` 枚举：`Claude` / `Gemini` / `Ollama` / `OpenAI`，`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`
- [ ] M1-T2.4 在 `types.rs` 定义 `LlmRequestConfig { temperature: f32, max_tokens: u32 }` + `LlmResponse { text: String, usage: LlmUsage }` + `LlmUsage { prompt_tokens: u32, completion_tokens: u32 }` + `LlmCapability` 枚举（Nl2Sql/QueryOptimization/IndexAdvice/RewriteAdvice/Embedding，design.md `:686-694`、`:726`）
- [ ] M1-T2.5 定义 `LlmError` 枚举（使用 thiserror）：`Timeout` / `Auth { reason: String }` / `ConnectionRefused { endpoint: String }` / `ApiError { status: u16, message: String }` / `Config(String)` / `FallbackExhausted`，各变体附 `#[error("...")]`
- [ ] M1-T2.6 实现 `LlmConfig::default()`：provider=OpenAI、model="gpt-4o"、api_base="https://api.openai.com/v1"、timeout=30s、max_tokens=2000
- [ ] M1-T2.7 实现 `LlmConfig::api_base_for(provider) -> String`：按 provider 推断默认 api_base（Claude→https://api.anthropic.com/v1、Gemini→https://generativelanguage.googleapis.com/v1、Ollama→http://localhost:11434、OpenAI→https://api.openai.com/v1）
- [ ] M1-T2.8 在 `packages/sz-orm-ai/src/lib.rs` 新增 `#[cfg(feature = "multi-llm")] pub mod llm_provider;`
- [ ] M1-T2.9 编写单元测试：`LlmConfig::default()` 返回 OpenAI provider + 默认配置；`api_base_for(Claude)` 返回 Anthropic API base
- [ ] M1-T2.10 编写单元测试：`LlmConfig` clone 后 fallback 链正确（provider A fallback to B fallback to C）

**验收标准**：
1. `LlmProvider` trait + `LlmConfig` + `LlmProviderKind` + `LlmError` + `LlmCapability` 完整可用
2. `LlmConfig::default()` 返回合理默认值（OpenAI provider，30s 超时）
3. `api_base_for` 按 provider 推断正确 API base
4. `cargo test -p sz-orm-ai --features multi-llm` 新增测试全部通过
5. 附 `packages/sz-orm-ai/src/llm_provider/mod.rs` 新增代码的 file:line 证据

**依赖**：M1-T1（feature gate 体系）

---

## M1-T3：ClaudeProvider + GeminiProvider 实现

**任务描述**：实现 `LlmProvider` trait 的 ClaudeProvider（Anthropic Claude API）与 GeminiProvider（Google Gemini API）两个实现，通过 reqwest HTTP 客户端调用各自 API。

**涉及文件**：
- `packages/sz-orm-ai/src/llm_provider/claude.rs`（新增，ClaudeProvider 实现）
- `packages/sz-orm-ai/src/llm_provider/gemini.rs`（新增，GeminiProvider 实现）

**复用标注**：
- 既有 reqwest 依赖：`packages/sz-orm-ai/Cargo.toml:33`（real feature 已引入 reqwest 0.12 with json + rustls-tls）
- 既有 `OptimizerConfig` HTTP 调用范式：`packages/sz-orm-ai/src/query_plan_optimizer.rs`（LLM 调用通过 reqwest 发送 POST）

**子任务**：
- [ ] M1-T3.1 在 `claude.rs` 实现 `pub struct ClaudeProvider { config: LlmConfig, client: reqwest::Client }`，`new(config: LlmConfig) -> Result<Self, LlmError>` 构造 reqwest Client（timeout 从 config）
- [ ] M1-T3.2 实现 `impl LlmProvider for ClaudeProvider`：`complete` 调用 Anthropic Messages API（POST `{api_base}/messages`，header `x-api-key` + `anthropic-version: 2023-06-01`，body `{model, max_tokens, messages:[{role:user, content:prompt}]}`），解析响应 `content[0].text`（design.md `:102`）
- [ ] M1-T3.3 ClaudeProvider `embed` 方法：Anthropic 无原生 embedding API，返回 `LlmError::ApiError { status: 501, message: "Claude does not support embed, use OpenAI/Gemini for embedding" }`
- [ ] M1-T3.4 ClaudeProvider `provider_name` 返回 `"claude"`，`model` 返回 `config.model`
- [ ] M1-T3.5 在 `gemini.rs` 实现 `pub struct GeminiProvider { config: LlmConfig, client: reqwest::Client }`，`new(config) -> Result<Self, LlmError>`
- [ ] M1-T3.6 实现 `impl LlmProvider for GeminiProvider`：`complete` 调用 Google Gemini API（POST `{api_base}/models/{model}:generateContent?key={api_key}`，body `{contents:[{parts:[{text:prompt}]}]}`），解析响应 `candidates[0].content.parts[0].text`（design.md `:103`）
- [ ] M1-T3.7 GeminiProvider `embed` 方法：调用 Gemini embedding API（POST `{api_base}/models/text-embedding-004:embedContent?key={api_key}`），解析响应 `embedding.values`
- [ ] M1-T3.8 GeminiProvider `provider_name` 返回 `"gemini"`，`model` 返回 `config.model`
- [ ] M1-T3.9 API Key 通过 header/query param 注入，禁止硬编码，禁止日志泄露（spec 4.3 规则 1）
- [ ] M1-T3.10 超时处理：reqwest 请求附 `config.timeout`，超时返回 `LlmError::Timeout`
- [ ] M1-T3.11 认证错误处理：API 返回 401/403 时返回 `LlmError::Auth`，不重试（spec 5.2.3 异常 3）
- [ ] M1-T3.12 编写单元测试：ClaudeProvider `provider_name` 返回 "claude"；GeminiProvider `provider_name` 返回 "gemini"（不实际调用 API，验证结构构造）
- [ ] M1-T3.13 编写单元测试：API Key 为 None 时 ClaudeProvider/GeminiProvider 返回 `LlmError::Config`

**验收标准**：
1. ClaudeProvider 调用 Anthropic Messages API，GeminiProvider 调用 Google Gemini API
2. API Key 通过配置注入，禁止硬编码与日志泄露
3. 超时返回 `LlmError::Timeout`，认证错误返回 `LlmError::Auth` 不重试
4. GeminiProvider 支持 embed，ClaudeProvider embed 返回 501 错误
5. `cargo test -p sz-orm-ai --features multi-llm` 新增测试全部通过
6. 附 `packages/sz-orm-ai/src/llm_provider/claude.rs` 与 `gemini.rs` 新增代码的 file:line 证据

**依赖**：M1-T1（feature gate + reqwest 依赖）、M1-T2（LlmProvider trait + LlmConfig + LlmError）

---

## M1-T4：LocalLlamaProvider + OpenAIProvider 实现

**任务描述**：实现 `LlmProvider` trait 的 LocalLlamaProvider（本地 Ollama HTTP API，无外部网络）与 OpenAIProvider（包装既有 `OptimizerConfig::with_llm`，签名不变）。

**涉及文件**：
- `packages/sz-orm-ai/src/llm_provider/ollama.rs`（新增，LocalLlamaProvider 实现）
- `packages/sz-orm-ai/src/llm_provider/openai.rs`（新增，OpenAIProvider 包装既有调用）

**复用标注**：
- 既有 `OptimizerConfig::with_llm`（`packages/sz-orm-ai/src/query_plan_optimizer.rs:207`）：OpenAI 兼容 API 调用，包装为 OpenAIProvider，签名不变
- 既有 `real_embedding.rs`（`packages/sz-orm-ai/src/real_embedding.rs`）：OpenAI 兼容 embedding，OpenAIProvider embed 复用
- 既有 reqwest 依赖（同 M1-T3）

**子任务**：
- [ ] M1-T4.1 在 `ollama.rs` 实现 `pub struct LocalLlamaProvider { config: LlmConfig, client: reqwest::Client }`，`new(config) -> Result<Self, LlmError>`，默认 api_base = `http://localhost:11434`（design.md `:104`）
- [ ] M1-T4.2 实现 `impl LlmProvider for LocalLlamaProvider`：`complete` 调用 Ollama API（POST `{api_base}/api/chat`，body `{model, messages:[{role:user, content:prompt}], stream:false}`），解析响应 `message.content`（design.md `:104`）
- [ ] M1-T4.3 LocalLlamaProvider `embed` 方法：调用 Ollama embedding API（POST `{api_base}/api/embeddings`，body `{model, prompt:text}`），解析响应 `embedding`
- [ ] M1-T4.4 LocalLlamaProvider `provider_name` 返回 `"ollama"`，`model` 返回 `config.model`
- [ ] M1-T4.5 LocalLlamaProvider 连接失败时返回 `LlmError::ConnectionRefused { endpoint: "localhost:11434" }`，提示启动 Ollama（spec 5.2.3 异常 2）
- [ ] M1-T4.6 LocalLlamaProvider 无需 API Key（本地模型），`api_key` 为 None 时不报错
- [ ] M1-T4.7 在 `openai.rs` 实现 `pub struct OpenAIProvider { config: LlmConfig, client: reqwest::Client }`，包装既有 `OptimizerConfig::with_llm`（`:207`）的 OpenAI 兼容调用（design.md `:736-740`）
- [ ] M1-T4.8 实现 `impl LlmProvider for OpenAIProvider`：`complete` 调用 OpenAI Chat Completions API（POST `{api_base}/chat/completions`，header `Authorization: Bearer {api_key}`，body `{model, messages:[{role:user, content:prompt}], max_tokens}`），解析响应 `choices[0].message.content`
- [ ] M1-T4.9 OpenAIProvider `embed` 方法：复用既有 `real_embedding.rs` 的 OpenAI 兼容 embedding 调用
- [ ] M1-T4.10 OpenAIProvider `provider_name` 返回 `"openai"`，`model` 返回 `config.model`
- [ ] M1-T4.11 验证既有 `OptimizerConfig::with_llm`（`:207`）签名与行为完全不变，OpenAIProvider 为新增包装层
- [ ] M1-T4.12 编写单元测试：LocalLlamaProvider `provider_name` 返回 "ollama"；OpenAIProvider `provider_name` 返回 "openai"
- [ ] M1-T4.13 编写单元测试：LocalLlamaProvider api_key 为 None 时不报错（本地模型无需认证）
- [ ] M1-T4.14 编写单元测试：既有 `OptimizerConfig::with_llm("key", "gpt-4o")` 行为不变（spec 4.5 规则 5）

**验收标准**：
1. LocalLlamaProvider 调用 Ollama 本地 API（localhost:11434），无外部网络依赖
2. OpenAIProvider 包装既有 `OptimizerConfig::with_llm`，既有签名与行为不变
3. LocalLlamaProvider 无需 API Key，连接失败提示启动 Ollama
4. OpenAIProvider embed 复用既有 `real_embedding.rs`
5. `cargo test -p sz-orm-ai --features multi-llm` 新增测试全部通过
6. 附 `packages/sz-orm-ai/src/llm_provider/ollama.rs` 与 `openai.rs` 新增代码的 file:line 证据

**依赖**：M1-T1（feature gate）、M1-T2（LlmProvider trait + LlmConfig）

---

## M1-T5：LlmRouter（运行时热切换 + 按能力路由 + fallback）

**任务描述**：实现 `LlmRouter`，使用 `ArcSwap` 持有当前 provider 支持运行时热切换，`capability_routes` 支持按能力路由（NL2SQL→Claude，Embedding→OpenAI），provider 不可用时 fallback 到备用 provider。

**涉及文件**：
- `packages/sz-orm-ai/src/llm_provider/router.rs`（新增，LlmRouter 实现）

**复用标注**：
- arc-swap crate：M1-T1 新增的可选依赖（`multi-llm` feature gate 隔离）
- M1-T2 `LlmProvider` trait + `LlmConfig` + `LlmCapability`
- M1-T3/M1-T4 四个 provider 实现

**子任务**：
- [ ] M1-T5.1 实现 `pub struct LlmRouter { current: ArcSwap<dyn LlmProvider>, capability_routes: RwLock<HashMap<LlmCapability, Arc<dyn LlmProvider>>> }`（design.md `:704-709`）
- [ ] M1-T5.2 实现 `LlmRouter::new(config: &LlmConfig) -> Result<Self, LlmError>`：根据 `config.provider` 构造对应 provider 实例，存入 ArcSwap（design.md `:713`）
- [ ] M1-T5.3 实现 `LlmRouter::switch(&self, config: &LlmConfig) -> Result<(), LlmError>`：运行时热切换 provider，`ArcSwap::store` 原子替换当前 provider（spec 5.2.1 规则 3，design.md `:716`）
- [ ] M1-T5.4 实现 `LlmRouter::complete(&self, prompt: &str, config: &LlmRequestConfig) -> Result<LlmResponse, LlmError>`：调用当前 provider，失败时 fallback 到 `config.fallback`（spec 4.2 规则 2，design.md `:717-721`）
- [ ] M1-T5.5 实现 `LlmRouter::complete_by_capability(&self, cap: LlmCapability, prompt: &str, config: &LlmRequestConfig) -> Result<LlmResponse, LlmError>`：按能力路由到对应 provider（design.md `:719-721`）
- [ ] M1-T5.6 实现 `LlmRouter::set_capability_route(&self, cap: LlmCapability, provider: Arc<dyn LlmProvider>)`：配置能力路由表（如 NL2SQL→Claude，Embedding→OpenAI）
- [ ] M1-T5.7 fallback 链实现：provider 调用失败 → 尝试 `config.fallback` → fallback 的 fallback → 全部失败返回 `LlmError::FallbackExhausted`（spec 4.2 规则 2）
- [ ] M1-T5.8 fallback 时记录日志 `"fallback from {provider_name} to {fallback_provider_name}"`（spec 5.2.3 异常 1 用户感知）
- [ ] M1-T5.9 编写单元测试：`LlmRouter::new` 构造 OpenAI provider，`switch` 切换到 Claude provider，后续 `complete` 调用 Claude
- [ ] M1-T5.10 编写单元测试：provider 调用失败（模拟 Timeout）时自动 fallback 到备用 provider，返回成功结果
- [ ] M1-T5.11 编写单元测试：所有 provider（含 fallback 链）均失败时返回 `LlmError::FallbackExhausted`
- [ ] M1-T5.12 编写单元测试：`set_capability_route(Nl2Sql, ClaudeProvider)` + `complete_by_capability(Nl2Sql, ...)` 路由到 Claude

**验收标准**：
1. `LlmRouter` 使用 ArcSwap 支持运行时热切换 provider，无需重启
2. 按能力路由（NL2SQL→Claude，Embedding→OpenAI）正确
3. provider 不可用时自动 fallback 到备用 provider，全部失败返回 `FallbackExhausted`
4. fallback 时记录日志标记降级
5. `cargo test -p sz-orm-ai --features multi-llm` 新增测试全部通过
6. 附 `packages/sz-orm-ai/src/llm_provider/router.rs` 新增代码的 file:line 证据

**依赖**：M1-T1（arc-swap 依赖）、M1-T2（LlmProvider trait）、M1-T3（Claude/Gemini provider）、M1-T4（Ollama/OpenAI provider）

---

## M1-T6：M1 集成测试与门禁验证

**任务描述**：对 M1 所有任务进行集成验证，确保 feature gate 隔离正确、既有 API 不变、测试基线不回退。

**涉及文件**：
- `packages/sz-orm-ai/tests/llm_provider_test.rs`（新增 M1 集成测试，`required-features = ["multi-llm"]`）
- `packages/sz-orm-ai/Cargo.toml`（新增 `[[test]]` 条目，`required-features = ["multi-llm"]`）

**子任务**：
- [ ] M1-T6.1 运行 `cargo fmt --all -- --check`（门禁 1：fmt 格式检查）
- [ ] M1-T6.2 运行 `cargo check --workspace --all-targets`（门禁 2：默认 feature 编译检查，验证 multi-llm 未启用时行为不变）
- [ ] M1-T6.3 运行 `cargo clippy --workspace --all-targets -- -D warnings`（门禁 3：clippy 静态分析）
- [ ] M1-T6.4 运行 `cargo test --workspace`（门禁 4：既有测试基线不回退，6760+ passed）
- [ ] M1-T6.5 运行 `cargo test -p sz-orm-ai --features multi-llm`（M1 新增测试全部通过）
- [ ] M1-T6.6 运行 `cargo check --workspace --all-targets --all-features`（门禁 10：feature 全组合编译，含 multi-llm）
- [ ] M1-T6.7 扫描新增代码无 `todo!`/`unimplemented!`/`unreachable!`（门禁 8：禁止占位实现）
- [ ] M1-T6.8 扫描新增代码无 `unsafe` 块或有 `// SAFETY:` 注释（unsafe 零容忍）
- [ ] M1-T6.9 验证既有 `OptimizerConfig::with_llm`（`query_plan_optimizer.rs:207`）签名与行为不变

**验收标准**：
1. 14 道门禁中 M1 相关门禁全部通过（1/2/3/4/8/10）
2. 既有测试基线不回退（6760+ passed）
3. `multi-llm` feature 全组合编译通过
4. 新增代码无占位实现、无 unsafe（或有 SAFETY 注释）
5. 既有 `OptimizerConfig::with_llm` 签名与行为不变
6. 附门禁运行输出证据

**依赖**：M1-T1、M1-T2、M1-T3、M1-T4、M1-T5

---

# 三、M2：AI 自动调优闭环（REQ-V40-001，P0）

**目标**：提供 `AutoTuningPipeline` 编排四阶段闭环（Detect→Advise→Apply→Verify），复用既有 `UnifiedQueryOptimizer`/`IndexAdvisor`/`RewriteAdvisor`/`ExplainPlanParser`，低风险自动执行，回归自动回滚，输出调优报告。
**预期工作量**：1.5 周
**对应需求**：REQ-V40-001（spec.md 5.1，design.md 2.2.2 模块 A）
**依赖**：M1（多 LLM 模型支持，提供 LlmProvider 用于 LLM 增强建议）

## M2-T1：ai-auto-tuning feature gate 搭建

**任务描述**：在 sz-orm-ai 中新增 `ai-auto-tuning` feature gate，作为 AI 自动调优闭环的隔离基础。

**涉及文件**：
- `packages/sz-orm-ai/Cargo.toml`（新增 `ai-auto-tuning` feature）

**复用标注**：复用既有 feature gate 体系（`packages/sz-orm-ai/Cargo.toml:13-24`）

**子任务**：
- [ ] M2-T1.1 在 `packages/sz-orm-ai/Cargo.toml` `[features]` 新增 `ai-auto-tuning = ["dep:tokio"]`，默认关闭
- [ ] M2-T1.2 验证 `cargo check -p sz-orm-ai` 默认编译通过，无 ai-auto-tuning 相关代码生效
- [ ] M2-T1.3 验证 `cargo check -p sz-orm-ai --features ai-auto-tuning` 编译通过
- [ ] M2-T1.4 验证 `cargo check --workspace --all-targets --all-features` 编译通过

**验收标准**：
1. `ai-auto-tuning` feature 默认关闭，默认编译行为与 v3.9.0 一致
2. feature 全组合编译通过
3. 附 `packages/sz-orm-ai/Cargo.toml` 新增 feature 的 file:line 证据

**依赖**：M1-T1（feature gate 体系基础）

---

## M2-T2：AutoTuningConfig + TuningSuggestion + AutoTuningReport 数据结构

**任务描述**：在 sz-orm-ai 新增 `auto_tuning` 模块，定义 `AutoTuningConfig`/`TuningSuggestion`/`AutoTuningReport`/`RiskLevel`/`SuggestionType` 等数据结构。

**涉及文件**：
- `packages/sz-orm-ai/src/auto_tuning/mod.rs`（新增模块，定义数据结构）
- `packages/sz-orm-ai/src/auto_tuning/types.rs`（新增，报告类型定义）
- `packages/sz-orm-ai/src/lib.rs`（新增 `#[cfg(feature = "ai-auto-tuning")] pub mod auto_tuning;`）

**复用标注**：
- 既有 `3`IndexAdvisor` 建议格式（`packages/sz-orm-ai/src/index_advisor.rs:100`）：建议含 DDL 文本，作为 TuningSuggestion 设计参考
- 既有 `RewriteAdvisor` 建议格式（`packages/sz-orm-ai/src/rewrite_advisor.rs:89`）：重写建议含 sql_before/sql_after

**子任务**：
- [ ] M2-T2.1 定义 `RiskLevel` 枚举：`Low` / `Medium` / `High`，`#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]`
- [ ] M2-T2.2 定义 `SuggestionType` 枚举：`Index` / `Rewrite` / `Schema`，`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`
- [ ] M2-T2.3 定义 `AutoTuningConfig` 结构：`slow_query_threshold: Duration`（默认 1s）+ `risk_threshold: RiskLevel`（默认 Low）+ `max_suggestions: usize`（默认 10）+ `regression_threshold: f64`（默认 0.1）+ `verify_samples: u32`（默认 3），`#[derive(Debug, Clone)]`（design.md `:760-766`）
- [ ] M2-T2.4 定义 `TuningSuggestion` 结构：`suggestion_type: SuggestionType` + `sql_before: String` + `sql_after: String` + `expected_gain: Option<f32>` + `risk: RiskLevel` + `reason: String`，`#[derive(Debug, Clone)]`（design.md `:770-777`）
- [ ] M2-T2.5 在 `types.rs` 定义 `DetectReport { slow_queries: Vec<SlowQueryInfo>, threshold: Duration }` + `SlowQueryInfo { sql, elapsed, signals }` + `AdviseReport { suggestions: Vec<TuningSuggestion> }` + `ApplyReport { applied: Vec<AppliedSuggestion>, skipped: Vec<SkippedSuggestion> }` + `VerifyReport { results: Vec<VerifyResult>, regressions: Vec<RegressionRecord> }` + `VerifyResult { suggestion_id, before_ms, after_ms, gain_pct, is_regression }` + `AppliedSuggestion { suggestion, apply_time }` + `RegressionRecord { suggestion, before_ms, after_ms, rollback_succeeded }`（design.md `:781-788`）
- [ ] M2-T2.6 定义 `AutoTuningReport` 结构：`detect: DetectReport` + `advise: AdviseReport` + `apply: ApplyReport` + `verify: VerifyReport` + `adoption_rate: f64` + `regressions: Vec<RegressionRecord>`，`#[derive(Debug, Clone)]`（design.md `:781-788`）
- [ ] M2-T2.7 实现 `AutoTuningConfig::default()`：slow_query_threshold=1s、risk_threshold=Low、max_suggestions=10、regression_threshold=0.1、verify_samples=3
- [ ] M2-T2.8 在 `packages/sz-orm-ai/src/lib.rs` 新增 `#[cfg(feature = "ai-auto-tuning")] pub mod auto_tuning;`
- [ ] M2-T2.9 编写单元测试：`4 阶段报告结构正确构造，adoption_rate 计算 = applied / total_suggestions

**验收标准**：
1. `AutoTuningConfig`/`TuningSuggestion`/`AutoTuningReport` 等数据结构完整可用
2. `AutoTuningConfig::default()` 返回合理默认值
3. `cargo test -p sz-orm-ai --features ai-auto-tuning` 新增测试全部通过
4. 附 `packages/sz-orm-ai/src/auto_tuning/mod.rs` 新增代码的 file:line 证据

**依赖**：M2-T1（feature gate）

---

## M2-T3：SlowQueryDetector（Detect 阶段）

**任务描述**：实现 `SlowQueryDetector`，采集慢查询日志 + 复用既有 `ExplainPlanParser`（5 方言）解析 EXPLAIN 识别全表扫描/索引缺失/JOIN 顺序不当等问题。

**涉及文件**：
- `packages/sz-orm-ai/src/auto_tuning/detector.rs`（新增，SlowQueryDetector 实现）

**复用标注**：
- 既有 `ExplainPlanParser` trait（`packages/sz-orm-ai/src/explain_parser.rs:50`）：5 方言解析器（MySQL/PG/SQLite/Oracle/MSSQL），`parse(&self, explain_output: &str) -> Result<Vec<ExplainSignal>, AiError>` + `dialect(&self) -> &'static str`，100% 复用
- 既有 `ExplainSignal`（`packages/sz-orm-ai/src/explain_parser.rs`）：EXPLAIN 信号（全表扫描/索引缺失等）

**子任务**：
- [ ] M2-T3.1 实现 `pub struct SlowQueryDetector { threshold: Duration, parser: Box<dyn ExplainPlanParser> }`，`new(threshold, parser) -> Self`
- [ ] M2-T3.2 实现 `SlowQueryDetector::detect(&self, conn: &dyn Connection) -> Result<DetectReport, TuningError>`：采集慢查询日志（SQL `SELECT * FROM pg_stat_statements WHERE mean_exec_time > threshold` 或方言适配），对每条慢查询执行 EXPLAIN + `parser.parse()` 识别问题（design.md `:795`）
- [ ] M2-T3.3 实现慢查询日志采集方言适配：PostgreSQL `pg_stat_statements`、MySQL `performance_schema.events_statements_summary_by_digest`、SQLite `sqlite_stat1`（无慢查询日志时返回空，不报错）、Oracle `v$sql`、MSSQL `sys.dm_exec_query_stats`
- [ ] M2-T3.4 实现 `SlowQueryDetector::detect_from_sql(&self, sql: &str, conn: &dyn Connection) -> Result<SlowQueryInfo, TuningError>`：对单条 SQL 执行 EXPLAIN + 解析，返回慢查询信息（含 signals）
- [ ] M2-T3.5 无慢查询时返回 `DetectReport { slow_queries: [], threshold }`，不报错（design.md 状态机 Detect→Detect "无慢查询"）
- [ ] M2-T3.6 编写单元测试：查询 `SELECT * FROM users WHERE name LIKE '%foo%'`，EXPLAIN 解析识别为全表扫描 + 索引缺失，标记待调优（spec 5.1.1 规则 2 验收条件）
- [ ] M2-T3.7 编写单元测试：无慢查询（所有查询耗时 < 阈值）时返回空 DetectReport
- [ ] M2-T3.8 编写单元测试：五方言 ExplainPlanParser 均可复用（MySQL/PG/SQLite/Oracle/MSSQL）

**验收标准**：
1. `SlowQueryDetector` 复用既有 `ExplainPlanParser:50` 解析 EXPLAIN，不重复实现 5 方言解析
2. 慢查询日志采集方言适配（PG/MySQL/SQLite/Oracle/MSSQL）
3. 无慢查询时返回空报告，不报错
4. `cargo test -p sz-orm-ai --features ai-auto-tuning` 新增测试全部通过
5. 附 `packages/sz-orm-ai/src/auto_tuning/detector.rs` 新增代码的 file:line 证据

**依赖**：M2-T1（feature gate）、M2-T2（数据结构）

---

## M2-T4：AutoTuningPipeline（Advise + Apply + Verify + Rollback 四阶段编排）

**任务描述**：实现 `AutoTuningPipeline`，编排四阶段闭环：Advise 复用既有 `IndexAdvisor`/`RewriteAdvisor`/`UnifiedQueryOptimizer` 生成建议，Apply 按风险阈值自动执行低风险建议，Verify 对比前后耗时，回归 ≥10% 自动回滚。

**涉及文件**：
- `packages/sz-orm-ai/src/auto_tuning/pipeline.rs`（新增，AutoTuningPipeline 实现）

**复用标注**：
- 既有 `UnifiedQueryOptimizer`（`packages/sz-orm-ai/src/query_plan_optimizer.rs:515`）：rule + llm hint 聚合，`new(config) -> Self`，LLM 建议零次执行（`:512` 注释），Advise 阶段复用生成建议
- 既有 `IndexAdvisor`（`packages/sz-orm-ai/src/index_advisor.rs:100`）：规则型 + 可选 LLM 索引建议，不自动执行（`:99` 注释），Advise 阶段复用
- 既有 `RewriteAdvisor`（`packages/sz-orm-ai/src/rewrite_advisor.rs:89`）：AST 重写建议，不自动重写（`:/`:88` 注释），Advise 阶段复用
- M1 `LlmRouter`（可选 LLM 增强）：`packages/sz-orm-ai/src/llm_provider/router.rs`

**子任务**：
- [ ] M2-T4.1 实现 `pub struct AutoTuningPipeline { detector: SlowQueryDetector, optimizer: Arc<UnifiedQueryOptimizer>, index_advisor: Arc<IndexAdvisor>, rewrite_advisor: Arc<RewriteAdvisor>, llm_router: Option<Arc<LlmRouter>>, config: AutoTuningConfig }`（design.md `:749-756`）
- [ ] M2-T4.2 实现 `AutoTuningPipeline::new(config, llm_router) -> Self`：构造既有 `UnifiedQueryOptimizer`/`IndexAdvisor`/`RewriteAdvisor`，不修改既有构造
- [ ] M2-T4.3 实现 `AutoTuningPipeline::run(&self, conn: &dyn Connection) -> Result<AutoTuningReport, TuningError>`：编排 Detect→Advise→Apply→Verify 四阶段，返回完整报告（design.md `:792`）
- [ ] M2-T4.4 实现 Advise 阶段 `advise(&self, slow_queries) -> Result<AdviseReport, TuningError>`：复用 `IndexAdvisor`/`RewriteAdvisor`/`UnifiedQueryOptimizer` 生成建议，每建议含 type/sql_before/sql_after/risk/expected_gain（design.md `:798`）
- [ ] M2-T4.5 Advise 阶段 LLM 增强（可选）：`llm_router` 存在时调用 LLM 补充建议，LLM 不可用时降级纯规则建议（spec 5.1.3 异常 1）
- [ ] M2-T4.6 实现 Apply 阶段 `apply(&self, conn, suggestions) -> Result<ApplyReport, TuningError>`：按 `config.risk_threshold` 过滤，低风险建议自动执行（创建索引/重写 SQL），高风险标记待人工确认（spec 5.1.1 规则 4，design.md `:801`）
- [ ] M2-T4.7 Apply 执行建议：`SuggestionType::Index` 执行 `CREATE INDEX`、`SuggestionType::Rewrite` 替换 SQL、`SuggestionType::Schema` 高风险不自动执行
- [ ] M2-T4.8 Apply 执行失败处理：创建索引失败（权限/磁盘满/锁冲突）记录失败原因，跳过该建议，继续后续（spec 5.1.3 异常 2）
- [ ] M2-T4.9 实现 Verify 阶段 `verify(&self, conn, applied) -> Result<VerifyReport, TuningError>`：对比调优前后耗时（EXPLAIN 估算 + 实际执行 ≤3 次采样），`gain_pct = (before - after) / before * 100`（spec 5.1.1 规则 5，design.md `:804`）
- [ ] M2-T4.10 Verify 回归检测：`gain_pct < -regression_threshold * 100`（即回退 ≥10%）标记 `is_regression = true`
- [ ] M2-T4.11 实现 `rollback(&self, conn, suggestions) -> Result<(), TuningError>`：回滚已执行建议（DROP INDEX 撤销添加索引、恢复原 SQL 撤销重写），回归时自动调用（spec 4.2 规则 1，design.md `:807`）
- [ ] M2-T4.12 Verify 回归自动回滚：检测到回归的已执行建议调用 `rollback`，恢复调优前状态（design.md 状态机 Verify→Rollback→Detect）
- [ ] M2-T4.13 计算 `adoption_rate = applied.len() / suggestions.len()`，写入 AutoTuningReport
- [ ] M2-T4.14 编写单元测试：配置慢查询阈值 1s，存在耗时 2s 查询，运行 `run` 输出四阶段报告（Detect 识别→Advise 生成建议→Apply 执行→Verify 对比）（spec 5.1.1 规则 1 验收条件）
- [ ] M2-T4.15 编写单元测试：建议为"添加索引"（低风险）自动执行；建议为"DROP COLUMN"（高风险）不自动执行，标记待人工确认（spec 5.1.1 规则 4 验收条件）
- [ ] M2-T4.16 编写单元测试：调优前耗时 2s，执行建议后耗时 0.5s，验证报告 gain=75%；调优后耗时 2.5s（25% 回退）自动回滚（spec 5.1.1 规则 5 验收条件）
- [ ] M2-T4.17 编写单元测试：LLM provider 不可用时降级纯规则建议，报告标记"LLM 降级"（spec 5.1.3 异常 1）
- [ ] M2-T4.18 编写单元测试：执行建议失败（模拟权限不足）记录失败原因，跳过该建议，继续后续（spec 5.1.3 异常 2）

**验收标准**：
1. `AutoTuningPipeline` 编排四阶段闭环，复用既有 `UnifiedQueryOptimizer:515`/`IndexAdvisor:100`/`RewriteAdvisor:89`/`ExplainPlanParser:50`，不重复实现
2. 低风险建议自动执行，高风险标记待人工确认
3. Verify 对比前后耗时，回归 ≥10% 自动回滚
4. LLM 不可用时降级纯规则建议
5. 执行失败跳过该建议，继续后续
6. `cargo test -p sz-orm-ai --features ai-auto-tuning` 新增测试全部通过
7. 附 `packages/sz-orm-ai/src/auto_tuning/pipeline.rs` 新增代码的 file:line 证据

**依赖**：M2-T1（feature gate）、M2-T2（数据结构）、M2-T3（SlowQueryDetector）、M1-T5（LlmRouter，可选 LLM 增强）

---

## M2-T5：M2 集成测试与门禁验证

**任务描述**：对 M2 所有任务进行集成验证。

**涉及文件**：
- `packages/sz-orm-ai/tests/auto_tuning_test.rs`（新增 M2 集成测试，`required-features = ["ai-auto-tuning"]`）

**子任务**：
- [ ] M2-T5.1 运行 `cargo fmt --all -- --check` + `cargo check --workspace --all-targets` + `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] M2-T5.2 运行 `cargo test --workspace`（既有测试基线不回退）
- [ ] M2-T5.3 运行 `cargo test -p sz-orm-ai --features ai-auto-tuning`（M2 新增测试全部通过）
- [ ] M2-T5.4 运行 `cargo test -p sz-orm-ai --features ai-auto-tuning,multi-llm`（AI 调优 + 多 LLM 联合 feature 组合测试）
- [ ] M2-T5.5 运行 `cargo check --workspace --all-targets --all-features`（feature 全组合编译）
- [ ] M2-T5.6 扫描新增代码无占位实现、无 unsafe（或有 SAFETY 注释）
- [ ] M2-T5.7 验证既有 `UnifiedQueryOptimizer`（`:515`）/`IndexAdvisor`（`:100`）/`RewriteAdvisor`（`:89`）/`ExplainPlanParser`（`:50`）签名与行为不变

**验收标准**：
1. M2 相关门禁全部通过
2. 既有测试基线不回退
3. `ai-auto-tuning` + `multi-llm` 联合 feature 组合编译与测试通过
4. 既有优化器/建议器/解析器签名与行为不变
5. 附门禁运行输出证据

**依赖**：M2-T1、M2-T2、M2-T3、M2-T4

---

# 四、M3：混合搜索（REQ-V40-003，P1）

**目标**：提供 `HybridSearcher` 融合向量搜索（pgvector）+ 全文搜索（ES/OpenSearch/Meilisearch）+ 结构化查询（SQL）三源结果，支持 RRF/加权/级联融合排序，并行查询 ≤200ms，部分源降级。
**预期工作量**：1.5 周
**对应需求**：REQ-V40-003（spec.md 5.3，design.md 2.2.2 模块 C）
**依赖**：无（M3 独立，仅需 feature gate 体系）

## M3-T1：hybrid-search feature gate 搭建

**任务描述**：在 sz-orm-vector 中新增 `hybrid-search` feature gate，依赖 sz-orm-search 与 sz-orm-core。

**涉及文件**：
- `packages/sz-orm-vector/Cargo.toml`（新增 `hybrid-search` feature）

**复用标注**：复用既有 feature gate 体系

**子任务**：
- [ ] M3-T1.1 在 `packages/sz-orm-vector/Cargo.toml` `[features]` 新增 `hybrid-search = ["dep:sz-orm-search", "dep:sz-orm-core"]`，默认关闭（design.md `:1484`）
- [ ] M3-T1.2 在 `packages/sz-orm-vector/Cargo.toml` `[dependencies]` 新增 `sz-orm-search = { workspace = true, optional = true }` + `sz-orm-core = { workspace = true, optional = true }`（若未有）
- [ ] M3-T1.3 验证 `cargo check -p sz-orm-vector` 默认编译通过，无 hybrid-search 相关代码生效
- [ ] M3-T1.4 验证 `cargo check -p sz-orm-vector --features hybrid-search` 编译通过

**验收标准**：
1. `hybrid-search` feature 默认关闭，默认编译行为与 v3.9.0 一致
2. feature 全组合编译通过
3. 附 `packages/sz-orm-vector/Cargo.toml` 新增 feature 的 file:line 证据

**依赖**：无

---

## M3-T2：HybridSearcher + HybridQuery + FusionStrategy 数据结构

**任务描述**：在 sz-orm-vector 新增 `hybrid_search` 模块，定义 `HybridSearcher`/`HybridQuery`/`FusionStrategy`/`HybridSearchResult`/`DegradationStatus` 等数据结构。

**涉及文件**：
- `packages/sz-orm-vector/src/hybrid_search/mod.rs`（新增模块，定义数据结构）
- `packages/sz-orm-vector/src/lib.rs`（新增 `#[cfg(feature = "hybrid-search")] pub mod hybrid_search;`）

**复用标注**：
- 既有 `PgVectorStore` trait（`packages/sz-orm-vector/src/lib.rs:189`）：向量搜索，100% 复用
- 既有 `SearchResult`（`packages/sz-orm-vector/src/lib.rs:113`）：id/score/vector/text/metadata，作为 HybridSearchResult 基础
- 既有 `VectorMetric`（`packages/sz-orm-vector/src/lib.rs:145`）：Cosine/Euclidean/DotProduct

**子任务**：
- [ ] M3-T2.1 定义 `FusionStrategy` 枚举：`Rrf { k: u32 }`（默认 k=60）+ `Weighted { vector_w: f32, fulltext_w: f32, structured_w: f32 }` + `Cascade`，`#[derive(Debug, Clone, Copy)]`（design.md `:841-845`）
- [ ] M3-T2.2 定义 `HybridQuery` 结构：`vector: Option<VectorQuery>` + `fulltext: Option<FulltextQuery>` + `structured: Option<StructuredQuery>` + `strategy: FusionStrategy` + `top_k: usize`，`#[derive(Debug, Clone)]`（design.md `:831-837`）
- [ ] M3-T2.3 定义 `VectorQuery`/`FulltextQuery`/`StructuredQuery` 结构：VectorQuery { collection, query_vector, metric, filter }、FulltextQuery { index, query_text, fields }、StructuredQuery { table, where_clauses, order_by }
- [ ] M3-T2.4 定义 `HybridSearchResult` 结构：`id: String` + `score: f32` + `source: SearchResultSource` + `metadata: HashMap<String, serde_json::Value>`，`#[derive(Debug, Clone)]`（design.md `:849-854`）
- [ ] M3-T2.5 定义 `SearchResultSource` 枚举：`Vector` / `Fulltext` / `Structured` / `Hybrid`
- [ ] M3-T2.6 定义 `DegradationStatus` 结构：`vector_degraded: bool` + `fulltext_degraded: bool` + `structured_degraded: bool`，`#[derive(Debug, Clone, Default)]`（design.md `:858-862`）
- [ ] M3-T2.7 定义 `HybridSearchResponse` 结构：`results: Vec<HybridSearchResult>` + `degradation: DegradationStatus` + `elapsed_ms: u64`（design.md `:875-879`）
- [ ] M3-T2.8 定义 `HybridError` 枚举（thiserror）：`SourceTimeout { source: String }` / `AllSourcesFailed` / `VectorError` / `FulltextError` / `StructuredError`
- [ ] M3-T2.9 在 `packages/sz-orm-vector/src/lib.rs` 新增 `#[cfg(feature = "hybrid-search")] pub mod hybrid_search;`
- [ ] M3-T2.10 编写单元测试：`HybridQuery` 构造含三源查询 + RRF 策略 + top_k=10；`DegradationStatus::default()` 全 false

**验收标准**：
1. `HybridSearcher`/`HybridQuery`/`FusionStrategy`/`HybridSearchResult` 等数据结构完整可用
2. `FusionStrategy::Rrf { k: 60 }` 为默认策略
3. `cargo test -p sz-orm-vector --features hybrid-search` 新增测试全部通过
4. 附 `packages/sz-orm-vector/src/hybrid_search/mod.rs` 新增代码的 file:line 证据

**依赖**：M3-T1（feature gate）

---

## M3-T3：三源并行查询 + 部分降级

**任务描述**：实现 `HybridSearcher::search`，通过 `tokio::join!` 并行查询向量/全文/结构化三源，部分源失败时降级为可用源结果。

**涉及文件**：
- `packages/sz-orm-vector/src/hybrid_search/searcher.rs`（新增，HybridSearcher 实现）

**复用标注**：
- 既有 `PgVectorStore`（`packages/sz-orm-vector/src/lib.rs:189`）：向量搜索源，`search` 方法
- 既有 ES/OpenSearch/Meilisearch provider（`packages/sz-orm-search/src/elasticsearch_provider.rs`/`opensearch_provider.rs`/`meilisearch_provider.rs`）：全文搜索源
- 既有 `SearchResult`（`packages/sz-orm-vector/src/lib.rs:113`）：统一结果基础

**子任务**：
- [ ] M3-T3.1 实现 `pub struct HybridSearcher { vector_store: Arc<dyn PgVectorStore>, fulltext_store: Arc<dyn FulltextSearch>, structured_conn: Arc<dyn Connection> }`（design.md `:823-827`）
- [ ] M3-T3.2 定义 `FulltextSearch` trait：`async fn search(&self, query: &FulltextQuery) -> Result<Vec<SearchResult>, HybridError>`，适配既有 ES/OpenSearch/Meilisearch provider
- [ ] M3-T3.3 实现 `HybridSearcher::search(&self, query: &HybridQuery) -> Result<HybridSearchResponse, HybridError>`：`tokio::join!` 并行查询三源（spec 5.3.1 规则 4，design.md `:866-870`）
- [ ] M3-T3.4 向量源查询：调用 `vector_store.search(query.vector)`，复用既有 `PgVectorStore:189`
- [ ] M3-T3.5 全文源查询：调用 `fulltext_store.search(query.fulltext)`，复用既有 ES/OpenSearch/Meilisearch provider
- [ ] M3-T3.6 结构化源查询：执行 SQL `SELECT * FROM {table} WHERE {where_clauses} ORDER BY {order_by} LIMIT {top_k}`，参数化 WHERE 条件（安全铁律）
- [ ] M3-T3.7 部分源降级处理：某源超时/失败时标记 `degraded = true`，其他源正常融合（spec 5.3.1 规则 5，design.md `:868`）
- [ ] M3-T3.8 三源均失败时返回 `HybridError::AllSourcesFailed`
- [ ] M3-T3.9 某源查询为 None（未配置）时跳过该源，不标记降级
- [ ] M3-T3.10 计算端到端 `elapsed_ms`，验证 ≤200ms（单机基准，结果集 ≤1000，spec 4.1 规则 3）
- [ ] M3-T3.11 编写单元测试：三源各耗时 50ms/80ms/30ms，并行查询端到端 ≈80ms + 融合开销 ≤200ms（spec 5.3.1 规则 4 验收条件）
- [ ] M3-T3.12 编写单元测试：ES 不可用，向量+结构化可用，返回融合结果标记"fulltext degraded"（spec 5.3.1 规则 5 验收条件）
- [ ] M3-T3.13 编写单元测试：三源均无匹配结果返回空列表，不报错（spec 5.3.3 异常 2）
- [ ] M3-T3.14 编写单元测试：某源超时（> 配置阈值）标记 TIMEOUT，其他源正常融合（spec 5.3.3 异常 1）

**验收标准**：
1. `HybridSearcher::search` 通过 `tokio::join!` 并行查询三源，端到端 ≤200ms
2. 部分源失败时降级为可用源结果，标记降级源
3. 三源均失败返回 `AllSourcesFailed`，均无匹配返回空列表
4. 复用既有 `PgVectorStore:189` + ES/OpenSearch/Meilisearch provider，不重复实现
5. `cargo test -p sz-orm-vector --features hybrid-search` 新增测试全部通过
6. 附 `packages/sz-orm-vector/src/hybrid_search/searcher.rs` 新增代码的 file:line 证据

**依赖**：M3-T1（feature gate）、M3-T2（数据结构）

---

## M3-T4：融合排序（RRF + 加权 + 级联）+ 结构化过滤下推

**任务描述**：实现三种融合排序策略（RRF/加权/级联）与结构化过滤下推（将结构化过滤下推到向量/全文源）。

**涉及文件**：
- `packages/sz-orm-vector/src/hybrid_search/fusion.rs`（新增，融合策略实现）
- `packages/sz-orm-vector/src/hybrid_search/pushdown.rs`（新增，过滤下推实现）

**复用标注**：复用 M3-T2 `SearchResult`/`HybridSearchResult`/`FusionStrategy`

**子任务**：
- [ ] M3-T4.1 实现 `rrf_fusion(results: &[Vec<SearchResult>], k: u32, top_k: usize) -> Vec<HybridSearchResult>`：RRF 公式 `score = Σ 1/(k + rank_i)`，按融合 score 降序取 top_k（spec 5.3.1 规则 2，design.md `:113`）
- [ ] M3-T4.2 实现 `weighted_fusion(results: &[Vec<SearchResult>], weights: &[f32], top_k: usize) -> Vec<HybridSearchResult>`：`score = Σ weight_i × normalized_score_i`，各源 score 归一化到 [0,1]（design.md `:114`）
- [ ] M3-T4.3 实现 `cascade_fusion(vector_results, fulltext_results, structured_results, top_k) -> Vec<HybridSearchResult>`：先向量召回 → 全文精排（重排向量结果）→ 结构化过滤（过滤不满足条件的结果）（design.md `:115`）
- [ ] M3-T4.4 实现 `HybridSearcher::fuse(&self, results: &[Vec<SearchResult>], strategy: FusionStrategy, top_k: usize) -> Vec<HybridSearchResult>`：按 strategy 分发到 rrf/weighted/cascade
- [ ] M3-T4.5 实现 `FilterPushdown::pushdown_to_vector(filter: &StructuredQuery, vector_query: &mut VectorQuery)`：将结构化过滤（如 `price < 1000`）下推到 pgvector WHERE 子句（spec 5.3.1 规则 6，design.md `:116`）
- [ ] M3-T4.6 实现 `FilterPushdown::pushdown_to_fulltext(filter: &StructuredQuery, fulltext_query: &mut FulltextQuery)`：将结构化过滤下推到 ES filter（design.md `:116`）
- [ ] M3-T4.7 过滤下推后融合层不再重复过滤，减少融合层开销
- [ ] M3-T4.8 编写单元测试：配置 strategy=RRF（k=60），三源各返回 top10，RRF 融合后返回 top10，score=Σ1/(60+rank)（spec 5.3.1 规则 2 验收条件）
- [ ] M3-T4.9 编写单元测试：加权融合，vector_w=0.5/fulltext_w=0.3/structured_w=0.2，验证加权 score 正确
- [ ] M3-T4.10 编写单元测试：级联融合，向量召回 top50 → 全文精排 top20 → 结构化过滤 top10
- [ ] M3-T4.11 编写单元测试：查询含 `price < 1000` 过滤，下推到 pgvector WHERE + ES filter，融合层不再过滤（spec 5.3.1 规则 6 验收条件）

**验收标准**：
1. RRF 融合公式 `score = Σ 1/(k + rank_i)` 正确，默认 k=60
2. 加权融合各源 score 归一化后加权求和
3. 级联融合先向量召回→全文精排→结构化过滤
4. 结构化过滤下推到 pgvector WHERE + ES filter，融合层不再过滤
5. `cargo test -p sz-orm-vector --features hybrid-search` 新增测试全部通过
6. 附 `packages/sz-orm-vector/src/hybrid_search/fusion.rs` 与 `pushdown.rs` 新增代码的 file:line 证据

**依赖**：M3-T3（三源并行查询）

---

## M3-T5：M3 集成测试与门禁验证

**任务描述**：对 M3 所有任务进行集成验证。

**涉及文件**：
- `packages/sz-orm-vector/tests/hybrid_search_test.rs`（新增 M3 集成测试，`required-features = ["hybrid-search"]`）

**子任务**：
- [ ] M3-T5.1 运行 `cargo fmt --all -- --check` + `cargo check --workspace --all-targets` + `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] M3-T5.2 运行 `cargo test --workspace`（既有测试基线不回退）
- [ ] M3-T5.3 运行 `cargo test -p sz-orm-vector --features hybrid-search`（M3 新增测试全部通过）
- [ ] M3-T5.4 运行 `cargo check --workspace --all-targets --all-features`（feature 全组合编译）
- [ ] M3-T5.5 扫描新增代码无占位实现、无 unsafe（或有 SAFETY 注释）
- [ ] M3-T5.6 验证既有 `PgVectorStore`（`:189`）/`SearchResult`（`:113`）签名与行为不变
- [ ] M3-T5.7 验证默认 `cargo build` 不引入 hybrid-search 依赖（spec 5.3.1 规则 7）

**验收标准**：
1. M3 相关门禁全部通过
2. 既有测试基线不回退
3. `hybrid-search` feature 全组合编译通过
4. 既有 `PgVectorStore`/`SearchResult` 签名与行为不变
5. 默认编译不引入 hybrid-search 依赖
6. 附门禁运行输出证据

**依赖**：M3-T1、M3-T2、M3-T3、M3-T4

---

# 五、M4：数据 lineage（REQ-V40-004，P1）

**目标**：提供 `LineageTracker` 字段级血缘追踪，解析 SQL 提取表/字段依赖构建 `LineageGraph`（DAG），支持影响分析、溯源分析、导出 DOT/JSON/GraphML，可选写入既有 `HashChainAuditor` 审计链。
**预期工作量**：1.5 周
**对应需求**：REQ-V40-004（spec.md 5.4，design.md 2.2.2 模块 D）
**依赖**：无（M4 独立，仅需 feature gate 体系）

## M4-T1：data-lineage feature gate 搭建

**任务描述**：在 sz-orm-audit 中新增 `data-lineage` feature gate，依赖 sqlparser。

**涉及文件**：
- `packages/sz-orm-audit/Cargo.toml`（新增 `data-lineage` feature）

**子任务**：
- [ ] M4-T1.1 在 `packages/sz-orm-audit/Cargo.toml` `[features]` 新增 `data-lineage = ["dep:sqlparser"]`，默认关闭（design.md `:1490`）
- [ ] M4-T1.2 在 `packages/sz-orm-audit/Cargo.toml` `[dependencies]` 新增 `sqlparser = { version = "0.51", optional = true }`（若未有）
- [ ] M4-T1.3 验证 `cargo check -p sz-orm-audit` 默认编译通过，无 data-lineage 相关代码生效
- [ ] M4-T1.4 验证 `cargo check -p sz-orm-audit --features data-lineage` 编译通过

**验收标准**：
1. `data-lineage` feature 默认关闭，默认编译行为与 v3.9.0 一致
2. feature 全组合编译通过
3. 附 `packages/sz-orm-audit/Cargo.toml` 新增 feature 的 file:line 证据

**依赖**：无

---

## M4-T2：LineageGraph + LineageNode + LineageEdge 数据结构

**任务描述**：在 sz-orm-audit 新增 `lineage` 模块，定义 `LineageGraph`（DAG）/`LineageNode`/`LineageEdge`/`LineageNodeId` 等数据结构，支持环路检测。

**涉及文件**：
- `packages/sz-orm-audit/src/lineage/mod.rs`（新增模块，定义数据结构）
- `packages/sz-orm-audit/src/lineage/graph.rs`（新增，LineageGraph DAG 实现）
- `packages/sz-orm-audit/src/lib.rs`（新增 `#[cfg(feature = "data-lineage")] pub mod lineage;`）

**复用标注**：
- 既有 `HashChainAuditor`（`packages/sz-orm-audit/src/lib.rs:778`）：lineage 变更可选写入审计链，保留不动
- 既有 `HashChainEntry`（`packages/sz-orm-audit/src/lib.rs:691`）：审计条目，lineage 事件可携带

**子任务**：
- [ ] M4-T2.1 定义 `LineageNodeId` 结构：`table: String` + `column: String`，`#[derive(Debug, Clone, PartialEq, Eq, Hash)]`（design.md `:908`）
- [ ] M4-T2.2 定义 `NodeType` 枚举：`Table` / `Column` / `View` / `MaterializedView`
- [ ] M4-T2.3 定义 `LineageNode` 结构：`id: LineageNodeId` + `node_type: NodeType`，`#[derive(Debug, Clone)]`（design.md `:910`）
- [ ] M4-T2.4 定义 `EdgeType` 枚举：`DirectDependency` / `Derived` / `Join` / `Filter` / `Projection`
- [ ] M4-T2.5 定义 `LineageEdge` 结构：`source: LineageNodeId` + `target: LineageNodeId` + `edge_type: EdgeType`，`#[derive(Debug, Clone, PartialEq, Eq, Hash)]`（design.md `:914`）
- [ ] M4-T2.6 定义 `LineageGraph` 结构：`nodes: HashMap<LineageNodeId, LineageNode>` + `edges: HashSet<LineageEdge>`，`#[derive(Debug, Clone, Default)]`（design.md `:901-904`）
- [ ] M4-T2.7 实现 `LineageGraph::add_node(&mut self, node: LineageNode)`：添加节点
- [ ] M4-T2.8 实现 `LineageGraph::add_edge(&mut self, edge: LineageEdge) -> Result<(), LineageError>`：添加边前检测环路（DFS），环路时返回 `LineageError::CycleDetected`（spec 5.4.3 异常 2，design.md `:933`）
- [ ] M4-T2.9 实现 `LineageGraph::incremental_update(&mut self, edges: Vec<LineageEdge>)`：增量更新图，新增/修改边，既有边保留（spec 5.4.1 规则 2）
- [ ] M4-T2.10 在 `packages/sz-orm-audit/src/lib.rs` 新增 `#[cfg(feature = "data-lineage")] pub mod lineage;`
- [ ] M4-T2.11 编写单元测试：添加节点与边，图结构正确
- [ ] M4-T2.12 编写单元测试：添加形成环路的边（A→B→A）返回 `CycleDetected`，不加入 DAG（spec 5.4.3 异常 2）
- [ ] M4-T2.13 编写单元测试：增量更新图，新增边保留既有边

**验收标准**：
1. `LineageGraph` 为 DAG，环路检测正确（A→B→A 返回 CycleDetected）
2. 增量更新图保留既有边
3. `cargo test -p sz-orm-audit --features data-lineage` 新增测试全部通过
4. 附 `packages/sz-orm-audit/src/lineage/graph.rs` 新增代码的 file:line 证据

**依赖**：M4-T1（feature gate）

---

## M4-T3：LineageSqlParser（SQL 依赖解析）

**任务描述**：实现 `LineageSqlParser`，通过 sqlparser 解析 SQL（INSERT/UPDATE/CREATE VIEW/CREATE MATERIALIZED VIEW）提取表/字段依赖关系。

**涉及文件**：
- `packages/sz-orm-audit/src/lineage/parser.rs`（新增，SQL 依赖解析）

**复用标注**：sqlparser crate（M4-T1 新增可选依赖，`data-lineage` feature gate 隔离）

**子任务**：
- [ ] M4-T3.1 实现 `pub struct LineageSqlParser { dialect: Box<dyn Dialect> }`，`new(dialect: DbType) -> Self` 按方言选择 sqlparser Dialect
- [ ] M4-T3.2 实现 `LineageSqlParser::parse(&self, sql: &str) -> Result<Vec<LineageEdge>, LineageError>`：使用 `sqlparser::Parser::parse_sql` 解析 SQL，提取表/字段依赖（design.md `:122`）
- [ ] M4-T3.3 实现 INSERT 语句解析：`INSERT INTO report SELECT user.name, order.amount FROM user JOIN order` → 边 `report.name ← user.name`、`report.amount ← order.amount`（spec 5.4.1 规则 1 验收条件）
- [ ] M4-T3.4 实现 UPDATE 语句解析：`UPDATE report SET name = user.name FROM user` → 边 `report.name ← user.name`
- [ ] M4-T3.5 实现 CREATE VIEW 语句解析：`CREATE VIEW v AS SELECT a, b FROM t` → 边 `v.a ← t.a`、`v.b ← t.b`
- [ ] M4-T3.6 实现 CREATE MATERIALIZED VIEW 语句解析：同 CREATE VIEW
- [ ] M4-T3.7 实现 JOIN 依赖提取：`SELECT * FROM a JOIN b ON a.id = b.aid` → 边 `a.id ← b.aid`（Join 边类型）
- [ ] M4-T3.8 SQL 解析失败处理：语法不支持或方言不支持时返回 `LineageError::ParseFailed`，跳过该 SQL（spec 5.4.3 异常 1）
- [ ] M4-T3.9 编写单元测试：`INSERT INTO report SELECT user.name, order.amount FROM user JOIN order` 解析输出 `report.name ← user.name`、`report.amount ← order.amount`（spec 5.4.1 规则 1 验收条件）
- [ ] M4-T3.10 编写单元测试：`CREATE VIEW v AS SELECT a, b FROM t` 解析输出 `v.a ← t.a`、`v.b ← t.b`
- [ ] M4-T3.11 编写单元测试：SQL 语法错误返回 `ParseFailed`，不 panic

**验收标准**：
1. `LineageSqlParser` 解析 INSERT/UPDATE/CREATE VIEW/CREATE MATERIALIZED VIEW 提取表/字段依赖
2. JOIN 依赖正确提取
3. SQL 解析失败返回 `ParseFailed`，不 panic
4. `cargo test -p sz-orm-audit --features data-lineage` 新增测试全部通过
5. 附 `packages/sz-orm-audit/src/lineage/parser.rs` 新增代码的 file:line 证据

**依赖**：M4-T1（feature gate + sqlparser）、M4-T2（LineageGraph 数据结构）

---

## M4-T4：LineageTracker（追踪 + 影响分析 + 溯源分析）

**任务描述**：实现 `LineageTracker`，编排 SQL 解析 + 增量更新图 + 影响分析 + 溯源分析，可选写入既有 `HashChainAuditor` 审计链。

**涉及文件**：
- `packages/sz-orm-audit/src/lineage/tracker.rs`（新增，LineageTracker 实现）

**复用标注**：
- 既有 `HashChainAuditor`（`packages/sz-orm-audit/src/lib.rs:778`）：lineage 变更可选写入审计链
- 既有 `HashChainEntry`（`packages/sz-orm-audit/src/lib.rs:691`）：审计条目
- M4-T2 `LineageGraph` + M4-T3 `LineageSqlParser`

**子任务**：
- [ ] M4-T4.1 实现 `pub struct LineageTracker { graph: Arc<RwLock<LineageGraph>>, parser: LineageSqlParser, auditor: Option<Arc<HashChainAuditor>> }`（design.md `:894-897`）
- [ ] M4-T4.2 实现 `LineageTracker::new(dialect, auditor: Option<Arc<HashChainAuditor>>) -> Self`
- [ ] M4-T4.3 实现 `LineageTracker::track_sql(&self, sql: &str) -> Result<LineageUpdate, LineageError>`：调用 `parser.parse(sql)` 提取边，`graph.incremental_update(edges)` 增量更新图（spec 5.4.1 规则 2，design.md `:918`）
- [ ] M4-T4.4 实现 `LineageTracker::impact_analysis(&self, node: &LineageNodeId) -> Vec<LineageNode>`：正向图遍历（BFS/DFS），输出下游受影响表/字段/报表（spec 5.4.1 规则 3，design.md `:921`）
- [ ] M4-T4.5 实现 `LineageTracker::origin_analysis(&self, node: &LineageNodeId) -> Vec<LineageNode>`：反向图遍历，输出源头表/字段（spec 5.4.1 规则 4，design.md `:924`）
- [ ] M4-T4.6 lineage 审计集成：`auditor` 存在时，lineage 变更（新依赖建立）写入审计链 `HashChainAuditor`，`verify()` 通过（spec 5.4.1 规则 5，design.md `:935`）
- [ ] M4-T4.7 编写单元测试：执行 `INSERT INTO report SELECT user.name, order.amount FROM user JOIN order`，lineage 图记录 `report.name ← user.name`、`report.amount ← order.amount`（spec 5.4.1 规则 1 验收条件）
- [ ] M4-T4.8 编写单元测试：`impact_analysis(user.name)` 输出下游受影响列表 `report.name`、`dashboard.user_name_widget` 等（spec 5.4.1 规则 3 验收条件）
- [ ] M4-T4.9 编写单元测试：`origin_analysis(report.amount)` 输出源头 `order.amount`（spec 5.4.1 规则 4 验收条件）
- [ ] M4-T4.10 编写单元测试：启用审计，执行新 SQL 建立 lineage，审计链记录变更事件，`verify()` 通过（spec 5.4.1 规则 5 验收条件）
- [ ] M4-T4.11 编写单元测试：SQL 解析失败跳过该 SQL，不影响其他 SQL lineage 追踪（spec 5.4.3 异常 1）

**验收标准**：
1. `LineageTracker` 解析 SQL 增量更新 lineage 图，复用 `LineageSqlParser` + `LineageGraph`
2. `impact_analysis` 正向遍历输出下游受影响列表
3. `origin_analysis` 反向遍历输出源头列表
4. lineage 变更可选写入既有 `HashChainAuditor:778` 审计链
5. SQL 解析失败跳过，不影响其他 SQL
6. `cargo test -p sz-orm-audit --features data-lineage` 新增测试全部通过
7. 附 `packages/sz-orm-audit/src/,lineage/tracker.rs` 新增代码的 file:line 证据

**依赖**：M4-T2（LineageGraph）、M4-T3（LineageSqlParser）

---

## M4-T5：lineage 图导出（DOT/JSON/GraphML）

**任务描述**：实现 `LineageTracker::export`，支持导出 lineage 图为 DOT/JSON/GraphML 标准格式，可被 Graphviz/D3.js 可视化。

**涉及文件**：
- `packages/sz-orm-audit/src/lineage/export.rs`（新增，图导出实现）

**子任务**：
- [ ] M4-T5.1 定义 `LineageExportFormat` 枚举：`Dot` / `Json` / `GraphMl`
- [ ] M4-T5.2 实现 `export_dot(graph: &LineageGraph) -> String`：生成 DOT 格式（`digraph lineage { "user.name" -> "report.name"; ... }`），可被 Graphviz 渲染（spec 5.4.1 规则 6，design.md `:927`）
- [ ] M4-T5.3 实现 `export_json(graph: &LineageGraph) -> String`：生成 JSON 格式（`{ nodes: [...], edges: [...] }`），可被 D3.js 解析
- [ ] M4-T5.4 实现 `export_graphml(graph: &LineageGraph) -> String`：生成 GraphML XML 格式
- [ ] M4-T5.5 实现 `LineageTracker::export(&self, format: LineageExportFormat) -> Result<String, LineageError>`：按 format 分发到 dot/json/graphml
- [ ] M4-T5.6 编写单元测试：导出 DOT 格式，含 `digraph` + 节点 + 边，可被 Graphviz 渲染（spec 5.4.1 规则 6 验收条件）
- [ ] M4-T5.7 编写单元测试：导出 JSON 格式，含 nodes + edges 数组，可被 `serde_json::from_str` 解析
- [ ] M4-T5.8 编写单元测试：导出 GraphML 格式，含 `<graphml>` + `<node>` + `<edge>` XML 元素

**验收标准**：
1. 导出 DOT 格式可被 Graphviz 渲染为可视化 DAG 图
2. 导出 JSON 格式可被 `serde_json::from_str` 解析
3. 导出 GraphML 格式为标准 XML
4. `cargo test -p sz-orm-audit --features data-lineage` 新增测试全部通过
5. 附 `packages/sz-orm-audit/src/lineage/export.rs` 新增代码的 file:line 证据

**依赖**：M4-T2（LineageGraph）

---

## M4-T6：M4 集成测试与门禁验证

**任务描述**：对 M4 所有任务进行集成验证。

**涉及文件**：
- `packages/sz-orm-audit/tests/lineage_test.rs`（新增 M4 集成测试，`required-features = ["data-lineage"]`）

**子任务**：
- [ ] M4-T6.1 运行 `cargo fmt --all -- --check` + `cargo check --workspace --all-targets` + `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] M4-T6.2 运行 `cargo test --workspace`（既有测试基线不回退）
- [ ] M4-T6.3 运行 `cargo test -p sz-orm-audit --features data-lineage`（M4 新增测试全部通过）
- [ ] M4-T6.4 运行 `cargo check --workspace --all-targets --all-features`（feature 全组合编译）
- [ ] M4-T6.5 扫描新增代码无占位实现、无 unsafe（或有 SAFETY 注释）
- [ ] M4-T6.6 验证既有 `HashChainAuditor`（`:778`）/`HashChainEntry`（`:691`）/`verify()`（`:862`）签名与行为不变

**验收标准**：
1. M4 相关门禁全部通过
2. 既有测试基线不回退
3. `data-lineage` feature 全组合编译通过
4. 既有 `HashChainAuditor`/`HashChainEntry`/`verify()` 签名与行为不变
5. 附门禁运行输出证据

**依赖**：M4-T1、M4-T2、M4-T3、M4-T4、M4-T5

---

# 六、M5：分片自动 rebalance（REQ-V40-005，P1）

**目标**：提供 `ShardRebalancer`，扩缩容时计算最小数据搬迁计划，分批迁移数据到新分片，支持断点续传、双写/影子读保证查询不中断、进度可观测，复用既有 `ShardingRouter`。
**预期工作量**：1.5 周
**对应需求**：REQ-V40-005（spec.md 5.5，design.md 2.2.2 模块 E）
**依赖**：无（M5 独立，仅需 feature gate 体系）

## M5-T1：shard-rebalance feature gate 搭建

**任务描述**：在 sz-orm-sharding 中新增 `shard-rebalance` feature gate。

**涉及文件**：
- `packages/sz-orm-sharding/Cargo.toml`（新增 `shard-rebalance` feature）

**子任务**：
- [ ] M5-T1.1 在 `packages/sz-orm-sharding/Cargo.toml` `[features]` 新增 `shard-rebalance = ["dep:tokio"]`，默认关闭（design.md `:1496`）
- [ ] M5-T1.2 验证 `cargo check -p sz-orm-sharding` 默认编译通过，无 shard-rebalance 相关代码生效
- [ ] M5-T1.3 验证 `cargo check -p sz-orm-sharding --features shard-rebalance` 编译通过

**验收标准**：
1. `shard-rebalance` feature 默认关闭，默认编译行为与 v3.9.0 一致
2. feature 全组合编译通过
3. 附 `packages/sz-orm-sharding/Cargo.toml` 新增 feature 的 file:line 证据

**依赖**：无

---

## M5-T2：RebalancePlan + RebalanceProgress 数据结构

**任务描述**：在 sz-orm-sharding 新增 `rebalancer` 模块，定义 `RebalancePlan`/`ShardMigration`/`RebalanceProgress`/`RebalanceReport` 等数据结构。

**涉及文件**：
- `packages/sz-orm-sharding/src/rebalancer/mod.rs`（新增模块，定义数据结构）
- `packages/sz-orm-sharding/src/lib.rs`（新增 `#[cfg(feature = "shard-rebalance")] pub mod rebalancer;`）

**复用标注**：
- 既有 `ShardingRouter`（`packages/sz-orm-sharding/src/lib.rs:130`）：路由器，rebalance 完成后更新路由表
- 既有 `ShardingStrategy`（`packages/sz-orm-sharding/src/lib.rs:60`）：分片策略（Hash/Range/Date/Enum/List/Directory/Composite）

**子任务**：
- [ ] M5-T2.1 定义 `ShardMigration` 结构：`source_shard: String` + `target_shard: String` + `row_count: u64` + `estimated_time: Duration`，`#[derive(Debug, Clone)]`（design.md `:958-963`）
- [ ] M5-T2.2 定义 `RebalancePlan` 结构：`migrations: Vec<ShardMigration>` + `total_rows: u64` + `estimated_time: Duration` + `strategy: ShardingStrategy`，`#[derive(Debug, Clone)]`（design.md `:950-955`）
- [ ] M5-T2.3 定义 `RebalanceProgress` 结构：`migrated_rows: u64` + `remaining_rows: u64` + `percentage: f64` + `eta: Duration` + `is_paused: bool`，`#[derive(Debug, Clone)]`（design.md `:967-973`）
- [ ] M5-T2.4 定义 `RebalanceReport` 结构：`total_migrated: u64` + `elapsed: Duration` + `consistency_passed: bool` + `new_router: ShardingRouter`
- [ ] M5-T2.5 定义 `RebalanceError` 枚举（thiserror）：`NodeFailed { shard: String }` / `ConsistencyFailed` / `CheckpointFailed` / `InvalidPlan`
- [ ] M5-T2.6 在 `packages/sz-orm-sharding/src/lib.rs` 新增 `#[cfg(feature = "shard-rebalance")] pub mod rebalancer;`
- [ ] M5-T2.D7 编写单元测试：`RebalancePlan` 构造含迁移列表 + 总行数 + 预估时间；`RebalanceProgress` 计算 percentage = migrated / (migrated + remaining)

**验收标准**：
1. `RebalancePlan`/`ShardMigration`/`RebalanceProgress`/`RebalanceReport` 数据结构完整可用
2. `cargo test -p sz-orm-sharding --features shard-rebalance` 新增测试全部通过
3. 附 `packages/sz-orm-sharding/src/rebalancer/mod.rs` 新增代码的 file:line 证据

**依赖**：M5-T1（feature gate）

---

## M5-T3：ShardRebalancer（最小搬迁计划计算）

**任务描述**：实现 `ShardRebalancer::plan_migration`，计算最小数据搬迁计划（一致性哈希环相邻区间/范围分片边界），仅搬迁必要数据非全量重哈希。

**涉及文件**：
- `packages/sz-orm-sharding/src/rebalancer/planner.rs`（新增，迁移计划计算）

**复用标注**：
- 既有 `ShardingRouter`（`packages/sz-orm-sharding/src/lib.rs:130`）：获取当前路由+策略
- 既有 `ShardingStrategy`（`packages/sz-orm-sharding/src/lib.rs:60`）：策略适配

**子任务**：
- [ ] M5-T3.1 实现 `pub struct ShardRebalancer { router: Arc<RwLock<ShardingRouter>>, checkpoint_store: Arc<dyn CheckpointStore> }`（design.md `:943-946`）
- [ ] M5-T3.2 实现 `ShardRebalancer::new(router, checkpoint_store) -> Self`
- [ ] M5-T3.3 实现 `ShardRebalancer::plan_migration(&self, current: &[String], target: &[String]) -> RebalancePlan`：按策略计算最小搬迁计划（spec 5.5.1 规则 2，design.md `:977`）
- [ ] M5-T3.4 一致性哈希策略最小搬迁：仅搬迁哈希环上新增节点相邻区间的数据，非全量 1/N（spec 5.5.1 规则 2 验收条件）
- [ ] M5-T3.5 范围分片策略最小搬迁：仅搬迁范围边界变更涉及的数据
- [ ] M5-T3.6 枚举/列表/目录/复合策略：按策略特性计算搬迁计划
- [ ] M5-T3.7 估算迁移时间：`estimated_time = total_rows / migration_speed`（可配置迁移速度）
- [ ] M5-T3.8 编写单元测试：一致性哈希 3→4 分片，仅搬迁新增节点相邻区间数据，非全量 1/4（spec 5.5.1 规则 2 验收条件）
- [ ] M5-T3.9 编写单元测试：范围分片 3→4 分片，仅搬迁范围边界变更数据
- [ ] M5-T3.10 编写单元测试：缩容 4→3 分片，计算移除分片数据的搬迁计划

**验收标准**：
1. `plan_migration` 计算最小搬迁量，非全量重哈希
2. 一致性哈希仅搬迁新增节点相邻区间数据
3. 范围分片仅搬迁边界变更数据
4. `cargo test -p sz-orm-sharding --features shard-rebalance` 新增测试全部通过
5. 附 `packages/sz-orm-sharding/src/rebalancer/planner.rs` 新增代码的 file:line 证据

**依赖**：M5-T1（feature gate）、M5-T2（数据结构）

---

## M5-T4：迁移执行（双写 + 影子读 + 断点续传 + 进度可观测）

**任务描述**：实现 `ShardRebalancer::execute`，分批迁移数据，双写保证查询不中断，断点续传支持中断恢复，进度可查询可中止。

**涉及文件**：
- `packages/sz-orm-sharding/src/rebalancer/executor.rs`（新增，迁移执行）
- `packages/sz-orm-sharding/src/rebalancer/checkpoint.rs`（新增，位点管理）

**复用标注**：
- 既有 `ShardingRouter`（`packages/sz-orm-sharding/src/lib.rs:130`）：rebalance 完成后更新路由表
- 既有 `cross_shard_tx.rs`（`packages/sz-orm-sharding/src/cross_shard_tx.rs`）：迁移过程跨分片事务

**子任务**：
- [ ] M5-T4.1 实现 `ShardRebalancer::execute(&self, plan: &RebalancePlan) -> Result<RebalanceReport, RebalanceError>`：分批迁移数据（spec 5.5.1 规则 1，design.md `:980`）
- [ ] M5-T4.2 双写开启：迁移过程中新旧分片同时写，保证一致性（spec 5.5.1 规则 4，design.md 活动图 `:593`）
- [ ] M5-T4.3 影子读：迁移过程中读旧分片（不读新分片），查询不中断（spec 5.5.1 规则 4）
- [ ] M5-T4.4 分批迁移：按 batch_size 分批迁移数据，每批更新位点
- [ ] M5-T4.5 实现 `CheckpointStore` trait：`save_checkpoint(task_id, position)` + `load_checkpoint(task_id) -> Option<Position>`，位点持久化
- [ ] M5-T4.6 断点续传：迁移中断后 `resume(task_id)` 从持久化位点继续，不重迁已迁移数据（spec 5.5.1 规则 3，design.md `:989`）
- [ ] M5-T4.7 实现 `ShardRebalancer::progress(&self, task_id: &str) -> Option<RebalanceProgress>`：查询迁移进度（已迁移/剩余/百分比/ETA）（spec 5.5.1 规则 6，design.md `:983`）
- [ ] M5-T4.8 实现 `ShardRebalancer::pause(&self, task_id: &str) -> Result<(), RebalanceError>`：中止迁移（design.md `:986`）
- [ ] M5-T4.9 实现 `ShardRebalancer::resume(&self, task_id: &str) -> Result<RebalanceReport, RebalanceError>`：恢复迁移（design.md `:989`）
- [ ] M5-T4.10 一致性校验：迁移完成后校验新旧分片数据一致性，不一致则不切换路由告警人工介入（spec 5.5.3 异常 2，design.md 活动图 `:606-614`）
- [ ] M5-T4.11 迁移完成更新 `ShardingRouter` 路由表，关闭双写（旧分片停读）（design.md 活动图 `:607-608`）
- [ ] M5-T4.12 迁移过程中节点故障处理：暂停迁移，标记故障分片，等待恢复后断点续传（spec 5.5.3 异常 1）
- [ ] M5-T4.13 编写单元测试：3→4 分片扩容，运行 rebalance，迁移约 25% 数据到新分片，路由表更新（spec 5.5.1 规则 1 验收条件）
- [ ] M5-T4.14 编写单元测试：迁移 50% 时中断，恢复 rebalance 从 50% 断点继续，不重迁已迁移数据（spec 5.5.1 规则 3 验收条件）
- [ ] M5-T4.15 编写单元测试：迁移过程中查询/写入，查询返回正确结果，写入双写到新旧分片，不中断（spec 5.5.1 规则 4 验收条件）
- [ ] M5-T4.16 编写单元测试：rebalance 迁移中查询进度，返回已迁移 60%、剩余 40%、预估 5 分钟，可中止（spec 5.5.1 规则 6 验收条件）
- [ ] M5-T4.17 编写单元测试：一致性校验失败时不切换路由，告警人工介入（spec 5.5.3 异常 2）

**验收标准**：
1. `execute` 分批迁移数据，双写 + 影子读保证查询不中断
2. 断点续传：中断后从持久化位点继续，不重迁已迁移数据
3. `progress` 查询迁移进度，`pause`/`resume` 中止/恢复
4. 一致性校验失败不切换路由，告警人工介入
5. 迁移完成更新 `ShardingRouter:130` 路由表，不修改路由策略
6. `cargo test -p sz-orm-sharding --features shard-rebalance` 新增测试全部通过
7. 附 `packages/sz-orm-sharding/src/rebalancer/executor.rs` 新增代码的 file:line 证据

**依赖**：M5-T2（数据结构）、M5-T3（迁移计划）

---

## M5-T5：M5 集成测试与门禁验证

**任务描述**：对 M5 所有任务进行集成验证。

**涉及文件**：
- `packages/sz-orm-sharding/tests/rebalancer_test.rs`（新增 M5 集成测试，`required-features = ["shard-rebalance"]`）

**子任务**：
- [ ] M5-T5.1 运行 `cargo fmt --all -- --check` + `cargo check --workspace --all-targets` + `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] M5-T5.2 运行 `cargo test --workspace`（既有测试基线不回退）
- [ ] M5-T5.3 运行 `cargo test -p sz-orm-sharding --features shard-rebalance`（M5 新增测试全部通过）
- [ ] M5-T5.4 运行 `cargo check --workspace --all-targets --all-features`（feature 全组合编译）
- [ ] M5-T5.5 扫描新增代码无占位实现、无 unsafe（或有 SAFETY 注释）
- [ ] M5-T5.6 验证既有 `ShardingRouter`（`:130`）/`ShardingStrategy`（`:60`）签名与行为不变

**验收标准**：
1. M5 相关门禁全部通过
2. 既有测试基线不回退
3. `shard-rebalance` feature 全组合编译通过
4. 既有 `ShardingRouter`/`ShardingStrategy` 签名与行为不变
5. 附门禁运行输出证据

**依赖**：M5-T1、M5-T2、M5-T3、M5-T4

---

# 七、M6：数据库 failover 自动化（REQ-V40-006，P1）

**目标**：提供 `AutoFailoverManager`，持续监控主库健康，故障时自动选择最佳 slave 提升为新主库，更新路由，通知上层，记录审计，含数据丢失风险评估与脑裂检测，30 秒内切换完成。
**预期工作量**：1 周
**对应需求**：REQ-V40-006（spec.md 5.6，design.md 2.2.2 模块 F）
**依赖**：无（M6 独立，仅需 feature gate 体系）

## M6-T1：auto-failover feature gate 搭建

**任务描述**：在 sz-orm-rw 中新增 `auto-failover` feature gate。

**涉及文件**：
- `packages/sz-orm-rw/Cargo.toml`（新增 `auto-failover` feature）

**子任务**：
- [ ] M6-T1.1 在 `packages/sz-orm-rw/Cargo.toml` `[features]` 新增 `auto-failover = ["dep:tokio"]`，默认关闭（design.md `:1502`）
- [ ] M6-T1.2 验证 `cargo check -p sz-orm-rw` 默认编译通过，无 auto-failover 相关代码生效
- [ ] M6-T1.3 验证 `cargo check -p sz-orm-rw --features auto-failover` 编译通过

**验收标准**：
1. `auto-failover` feature 默认关闭，默认编译行为与 v3.9.0 一致
2. feature 全组合编译通过
3. 附 `packages/sz-orm-rw/Cargo.toml` 新增 feature 的 file:line 证据

**依赖**：无

---

## M6-T2：FailoverConfig + FailoverEvent + DataLossRisk 数据结构

**任务描述**：在 sz-orm-rw 新增 `auto_failover` 模块，定义 `FailoverConfig`/`FailoverEvent`/`DataLossRisk`/`SplitBrainStatus` 等数据结构。

**涉及文件**：
- `packages/sz-orm-rw/src/auto_failover/mod.rs`（新增模块，定义数据结构）
- `packages/sz-orm-rw/src/lib.rs`（新增 `#[cfg(feature = "auto-failover")] pub mod auto_failover;`）

**复用标注**：
- 既有 `SlaveHealth`（`packages/sz-orm-rw/src/lib.rs:37`）：Healthy/Unhealthy/Drained
- 既有 `HealthChecker`（`packages/sz-orm-rw/src/lib.rs:219`）：failure_threshold/recovery_cooldown
- 既有 `ReadWriteRouter`（`packages/sz-orm-rw/src/lib.rs:331`）：master/slaves/strategy/health_checker

**子任务**：
- [ ] M6-T2.1 定义 `FailoverConfig` 结构：`check_interval: Duration` + `failure_threshold: u32`（默认 3）+ `lag_threshold: Duration`（默认 1s）+ `switch_timeout: Duration`（默认 30s），`#[derive(Debug, Clone)]`（design.md `:1014-1019`）
- [ ] M6-T2.2 定义 `FailoverOperator` 枚举：`Auto` / `Manual`
- [ ] M6-T2.3 定义 `DataLossRisk` 结构：`lag: Duration` + `estimated_lost_rows: u64` + `is_safe: bool`，`#[derive(Debug, Clone)]`（design.md `:1034`）
- [ ] M6-T2.4 定义 `FailoverEvent` 结构：`failure_time: DateTime` + `detection_confirms: u32` + `promoted_slave: String` + `old_master: String` + `data_loss_assessment: DataLossRisk` + `recovery_time: Option<Duration>` + `operator: FailoverOperator`，`#[derive(Debug, Clone)]`（design.md `:1023-1031`）
- [ ] M6-T2.5 定义 `SplitBrainStatus` 枚举：`NoSplitBrain` / `Detected { old_master, new_master }` / `Resolved`
- [ ] M6-T2.6 定义 `FailoverError` 枚举（thiserror）：`NoHealthySlave` / `PromotionFailed { slave: String, reason: String }` / `SplitBrain` / `LagTooHigh { lag: Duration }` / `SwitchTimeout`
- [ ] M6-T2.7 实现 `FailoverConfig::default()`：check_interval=1s、failure_threshold=3、lag_threshold=1s、switch_timeout=30s
- [ ] M6-T2.8 在 `packages/sz-orm-rw/src/lib.rs` 新增 `#[cfg(feature = "auto-failover")] pub mod auto_failover;`
- [ ] M6-T2.9 编写单元测试：`FailoverConfig::default()` 返回合理默认值；`DataLossRisk { lag: 0.5s, is_safe: true }` 安全切换

**验收标准**：
1. `FailoverConfig`/`FailoverEvent`/`DataLossRisk`/`SplitBrainStatus` 数据结构完整可用
2. `FailoverConfig::default()` 返回合理默认值（3 次确认，1s 延迟阈值，30s 切换超时）
3. `cargo test -p sz-orm-rw --features auto-failover` 新增测试全部通过
4. 附 `packages/sz-orm-rw/src/auto_failover/mod.rs` 新增代码的 file:line 证据

**依赖**：M6-T1（feature gate）

---

## M6-T3：AutoFailoverManager（自动检测 + slave 提升 + 路由更新）

**任务描述**：实现 `AutoFailoverManager`，持续监控主库健康，连续失败达阈值触发 failover，选择最佳 slave 提升为新主库，更新 `ReadWriteRouter` 路由，通知上层。

**涉及文件**：
- `packages/sz-orm-rw/src/auto_failover/manager.rs`（新增，AutoFailoverManager 实现）

**复用标注**：
- 既有 `HealthChecker`（`packages/sz-orm-rw/src/lib.rs:219`）：健康检测，`AutoFailoverManager` 复用检测主库健康
- 既有 `ReadWriteRouter`（`packages/sz-orm-rw/src/lib.rs:331`）：路由器，`AutoFailoverManager` 更新路由（提升 slave 为新 master）
- 既有 `SlaveHealth`（`packages/sz-orm-rw/src/lib.rs:37`）：slave 健康状态

**子任务**：
- [ ] M6-T3.1 实现 `pub struct AutoFailoverManager { router: Arc<RwLock<ReadWriteRouter>>, health_checker: Arc<HealthChecker>, config: FailoverConfig, auditor: Option<Arc<HashChainAuditor>> }`（design.md `:1005-1010`）
- [ ] M6-T3.2 实现 `AutoFailoverManager::new(router, health_checker, config, auditor) -> Self`
- [ ] M6-T3.3 实现 `AutoFailoverManager::start(&self) -> Result<(), FailoverError>`：启动持续监控循环，按 `check_interval` 定期检测主库健康（design.md `:1038`）
- [ ] M6-T3.4 自动故障检测：主库连续故障达 `failure_threshold`（默认 3）次时触发 failover（spec 5.6.1 规则 1，design.md 状态机 `:555-557`）
- [ ] M6-T3.5 实现 `AutoFailoverManager::select_best_slave(&self) -> Result<String, FailoverError>`：选择复制延迟最小 + 数据最完整的 slave（spec 5.6.1 规则 2，design.md `:1044`）
- [ ] M6-T3.6 实现 `AutoFailoverManager::assess_data_loss(&self, slave: &str) -> DataLossRisk`：评估 slave 复制延迟，延迟 ≤ 阈值 `is_safe = true`，> 阈值 `is_safe = false`（spec 5.6.1 规则 3，design.md `:1047`）
- [ ] M6-T3.7 数据丢失风险评估：延迟 > 阈值时告警人工确认，不盲目切换；延迟 ≤ 阈值时自动切换（spec 5.6.1 规则 3）
- [ ] M6-T3.8 实现 `AutoFailoverManager::trigger(&self) -> Result<FailoverEvent, FailoverError>`：手动/自动触发 failover，提升 slave 为新主库，更新 `ReadWriteRouter` 路由，通知上层（design.md `:1041`）
- [ ] M6-T3.9 slave 提升：执行 `PROMOTE_SLAVE` 命令（方言适配），提升 slave 为新主库
- [ ] M6-T3.10 路由更新：更新 `ReadWriteRouter.master = promoted_slave`，通知上层应用新主库地址（spec 5.6.1 规则 2）
- [ ] M6-T3.11 30 秒切换超时：从故障检测到路由切换完成不超过 30 秒（含 3 次确认 + slave 提升 + 路由更新 + 通知）（spec 5.6.1 规则 7）
- [ ] M6-T3.12 failover 事件审计：记录 `FailoverEvent` 到审计日志（故障时间/检测确认/提升 slave/丢失评估/操作者）（spec 5.6.1 规则 4）
- [ ] M6-T3.13 所有 slave 不可用处理：告警人工介入，服务降级（只读或拒绝）（spec 5.6.3 异常 1）
- [ ] M6-T3.14 slave 提升失败处理：尝试下一个候选 slave，全部失败则告警（spec 5.6.3 异常 2）
- [ ] M6-T3.15 编写单元测试：主库故障，连续 3 次健康检测失败，触发自动 failover（spec 5.6.1 规则 1 验收条件）
- [ ] M6-T3.16 编写单元测试：触发 failover，slave-2 延迟最小，slave-2 提升为新主库，路由更新，上层通知（spec 5.6.1 规则 2 验收条件）
- [ ] M6-T3.17 编写单元测试：slave 延迟 2s > 阈值 1s，告警人工确认；延迟 0.5s ≤ 阈值，自动切换（spec 5.6.1 规则 3 验收条件）
- [ ] M6-T3.18 编写单元测试：failover 完成，审计日志含完整事件记录，可查询追溯（spec 5.6.1 规则 4 验收条件）
- [ ] M6-T3.19 编写单元测试：主库故障，自动 failover 30 秒内路由切换完成，上层收到通知（spec 5.6.1 规则 7 验收条件）
- [ ] M6-T3.20 编写单元测试：所有 slave 不可用，告警人工介入（spec 5.6.3 异常 1）

**验收标准**：
1. `AutoFailoverManager` 复用既有 `HealthChecker:219` 检测 + `ReadWriteRouter:331` 路由更新，不修改既有逻辑
2. 连续失败达阈值（默认 3）触发 failover
3. 选择复制延迟最小的 slave 提升，延迟 > 阈值告警人工确认
4. 30 秒内路由切换完成，failover 事件记录审计
5. 所有 slave 不可用告警人工介入，slave 提升失败尝试下一候选
6. `cargo test -p sz-orm-rw --features auto-failover` 新增测试全部通过
7. 附 `packages/sz-orm-rw/src/auto_failover/manager.rs` 新增代码的 file:line 证据

**依赖**：M6-T1（feature gate）、M6-T2（数据结构）

---

## M6-T4：脑裂检测

**任务描述**：实现 `AutoFailoverManager::detect_split_brain`，检测双主（旧主库恢复但新主库已提升），将旧主库降级为 slave 或隔离。

**涉及文件**：
- `packages/sz-orm-rw/src/auto_failover/split_brain.rs`（新增，脑裂检测）

**复用标注**：既有 `HealthChecker`（`packages/sz-orm-rw/src/lib.rs:219`）：检测旧主库恢复

**子任务**：
- [ ] M6-T4.1 实现 `AutoFailoverManager::detect_split_brain(&self) -> SplitBrainStatus`：检测旧主库恢复但新主库已提升（双主）（spec 5.6.3 异常 3，design.md `:1050`）
- [ ] M6-T4.2 脑裂处理：检测到脑裂时将旧主库降级为 slave 或隔离，告警"split-brain detected, old master demoted"（spec 5.6.3 异常 3）
- [ ] M6-T4.3 failover 后定期检测脑裂：新主库提升后持续监控旧主库是否恢复
- [ ] M6-T4.4 编写单元测试：旧主库恢复但新主库已提升，检测到脑裂，旧主库降级为 slave（spec 5.6.3 异常 3）

**验收标准**：
1. 脑裂检测正确识别双主情况
2. 检测到脑裂时旧主库降级为 slave 或隔离，告警
3. `cargo test -p sz-orm-rw --features auto-failover` 新增测试全部通过
4. 附 `packages/sz-orm-rw/src/auto_failover/split_brain.rs` 新增代码的 file:line 证据

**依赖**：M6-T3（AutoFailoverManager）

---

## M6-T5：M6 集成测试与门禁验证

**任务描述**：对 M6 所有任务进行集成验证。

**涉及文件**：
- `packages/sz-orm-rw/tests/auto_failover_test.rs`（新增 M6 集成测试，`required-features = ["auto-failover"]`）

**子任务**：
- [ ] M6-T5.1 运行 `cargo fmt --all -- --check` + `cargo check --workspace --all-targets` + `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] M6-T5.2 运行 `cargo test --workspace`（既有测试基线不回退）
- [ ] M6-T5.3 运行 `cargo test -p sz-orm-rw --features auto-failover`（M6 新增测试全部通过）
- [ ] M6-T5.4 运行 `cargo check --workspace --all-targets --all-features`（feature 全组合编译）
- [ ] M6-T5.5 扫描新增代码无占位实现、无 unsafe（或有 SAFETY 注释）
- [ ] M6-T5.6 验证既有 `ReadWriteRouter`（`:331`）/`HealthChecker`（`:219`）/`SlaveHealth`（`:37`）签名与行为不变
- [ ] M6-T5.7 验证既有手动 failover 测试 `test_router_failover_to_master_when_all_unhealthy`（`:911`）行为不变（spec 5.6.1 规则 6）

**验收标准**：
1. M6 相关门禁全部通过
2. 既有测试基线不回退
3. `auto-failover` feature 全组合编译通过
4. 既有 `ReadWriteRouter`/`HealthChecker`/`SlaveHealth` 签名与行为不变
5. 既有手动 fail2 failover 测试行为不变
6. 附门禁运行输出证据

**依赖**：M6-T1、M6-T2、M6-T3、M6-T4

---

# 八、M7：CDC 变更数据捕获（REQ-V40-009，P2）

**目标**：提供 `CdcCapturer` 捕获数据库变更（INSERT/UPDATE/DELETE）为 `ChangeEvent`，从 WAL/binlog/触发器/LogMiner/CDC 五方言读取，Exactly-Once 语义，断点续传，下游分发到消息队列（复用既有 sz-orm-queue 6 provider）+ HTTP webhook，可选脱敏。
**预期工作量**：2 周
**对应需求**：REQ-V40-009（spec.md 5.9，design.md 2.2.2 模块 I）
**依赖**：无（M7 独立，先于 M8 GraphQL，提供 Subscription 数据源）

## M7-T1：cdc feature gate 搭建

**任务描述**：在 sz-orm-queue 中新增 `cdc` feature gate，依赖 tokio 与 sz-orm-masking。

**涉及文件**：
- `packages/sz-orm-queue/Cargo.toml`（新增 `cdc` feature）

**子任务**：
- [ ] M7-T1.1 在 `packages/sz-orm-queue/Cargo.toml` `[features]` 新增 `cdc = ["dep:tokio", "sz-orm-masking"]`，默认关闭（design.md `:1521`）
- [ ] M7-T1.2 验证 `cargo check -p sz-orm-queue` 默认编译通过，无 cdc 相关代码生效
- [ ] M7-T1.3 验证 `cargo check -p sz-orm-queue --features cdc` 编译通过

**验收标准**：
1. `cdc` feature 默认关闭，默认编译行为与 v3.9.0 一致
2. feature 全组合编译通过
3. 附 `packages/sz-orm-queue/Cargo.toml` 新增 feature 的 file:line 证据

**依赖**：无

---

## M7-T2：ChangeEvent + CdcConfig + CdcCheckpoint 数据结构

**任务描述**：在 sz-orm-queue 新增 `cdc` 模块，定义 `ChangeEvent`/`ChangeOp`/`CdcConfig`/`CdcCheckpoint`/`CheckpointPosition` 等数据结构。

**涉及文件**：
- `packages/sz-orm-queue/src/cdc/mod.rs`（新增模块，定义数据结构）
- `packages/sz-orm-queue/src/lib.rs`（新增 `#[cfg(feature = "cdc")] pub mod cdc;`）

**复用标注**：
- 既有 `sz-orm-queue` 6 provider（`packages/sz-orm-queue/src/real_kafka.rs`/`real_nats.rs`/`real_pulsar.rs`/`real_activemq.rs`/`lapin_rabbitmq.rs`/`rocketmq.rs`）：下游分发复用
- 既有 `DataMasker`（`packages/sz-orm-masking/src/lib.rs`）：ChangeEvent 脱敏复用

**子任务**：
- [ ] M7-T2.1 定义 `ChangeOp` 枚举：`Insert` / `Update` / `Delete`，`#[derive(Debug, Clone, Copy, Serialize, Deserialize)]`（design.md `:1194`）
- [ ] M7-T2.2 定义 `Row` 类型：`HashMap<String, serde_json::Value>`（变更前/后数据）
- [ ] M7-T2.3 定义 `ChangeEvent` 结构：`op: ChangeOp` + `before: Option<Row>` + `after: Option<Row>` + `timestamp: u64` + `transaction_id: String` + `table: String` + `schema: String`，`#[derive(Debug, Clone, Serialize, Deserialize)]`（design.md `:1183-1191`）
- [ ] M7-T2.4 定义 `CheckpointPosition` 枚举：`WalLsn(u64)`（PostgreSQL）+ `BinlogGtid(String)`（MySQL）+ `TriggerSeq(u64)`（SQLite）+ `LogMinerScn(u64)`（Oracle）+ `CdcLsn(u64)`（MSSQL）
- [ ] M7-T2.5 定义 `CdcCheckpoint` 结构：`dialect: DbType` + `position: CheckpointPosition` + `updated_at: u64`，`#[derive(Debug, Clone, Serialize, Deserialize)]`（design.md `:1208-1212`）
- [ ] M7-T2.6 定义 `DownstreamConfig` 枚举：`Kafka { topic, .. }` / `RabbitMq { exchange, .. }` / `Nats { subject, .. }` / `Pulsar { topic, .. }` / `RocketMq { topic, .. }` / `ActiveMq { queue, .. }` / `HttpWebhook { url, headers }`
- [ ] M7-T2.7 定义 `CdcConfig` 结构：`tables: Vec<String>` + `dialect: DbType` + `downstream: Vec<DownstreamConfig>` + `checkpoint_store: CheckpointStoreConfig` + `masking: Option<MaskingRuleMap>`，`#[derive(Debug, Clone)]`（design.md `:1198-1204`）
- [ ] M7-T2.8 定义 `CdcError` 枚举（thiserror）：`WalNot:NotConfigured` / `BinlogNotEnabled` / `DownstreamUnavailable { downstream: String }` / `CheckpointFailed` / `CaptureError { reason: String }`
- [ ] M7-T2.9 在 `packages/sz-orm-queue/src/lib.rs` 新增 `#[cfg(feature = "cdc")] pub mod cdc;`
- [ ] M7-T2.10 编写单元测试：`ChangeEvent` 序列化/反序列化正确；`CdcCheckpoint` 含 WAL LSN 位点

**验收标准**：
1. `ChangeEvent`/`ChangeOp`/`CdcConfig`/`CdcCheckpoint`/`CheckpointPosition` 数据结构完整可用
2. `ChangeEvent` 支持 serde 序列化/反序列化
3. `cargo test -p sz-orm-queue --features cdc` 新增测试全部通过
4. 附 `packages/sz-orm-queue/src/cdc/mod.rs` 新增代码的 file:line 证据

**依赖**：M7-T1（feature gate）

---

## M7-T3：DialectCapturer trait + PostgreSQL WAL 捕获器

**任务描述**：定义 `DialectCapturer` trait，实现 PostgreSQL WAL 逻辑复制捕获器（通过 tokio-postgres replication 协议读取 WAL）。

**涉及文件**：
- `packages/sz-orm-queue/src/cdc/capturer.rs`（新增，DialectCapturer trait + WalCapturer）

**复用标注**：tokio-postgres replication（PostgreSQL 逻辑复制协议）

**子任务**：
- [ ] M7-T3.1 定义 `#[async_trait] pub trait DialectCapturer: Send + Sync`：`async fn start_capture(&self, checkpoint: Option<CdcCheckpoint>) -> Result<Pin<Box<dyn Stream<Item = ChangeEvent> + Send>>, CdcError>` + `fn dialect(&self) -> DbType`（design.md `:1216-1219`）
- [ ] M7-T3.2 实现 `pub struct WalCapturer { config: CdcConfig, conn_config: PostgresConfig }`（PostgreSQL WAL 逻辑复制，design.md `:1222`）
- [ ] M7-T3.3 实现 `impl DialectCapturer for WalCapturer`：`start_capture` 通过 PostgreSQL 逻辑复制槽（`START_REPLICATION SLOT ... LOGICAL ...`）读取 WAL 变更，构造 `ChangeEvent`（design.md `:176`）
- [ ] M7-T3.4 WAL 变更解析：解析逻辑复制协议消息（INSERT/UPDATE/DELETE），提取 before/after 数据
- [ ] M7-T3.5 从 checkpoint 恢复：`start_capture` 传入 checkpoint 时从 WAL LSN 续传
- [ ] M7-T3.6 WAL 未配置逻辑复制处理：`wal_level != logical` 时返回 `CdcError::WalNotConfigured`，提示配置（spec 5.9.3 异常 1）
- [ ] M7-T3.7 编写单元测试：执行 `UPDATE users SET name='new' WHERE id=1`，WalCapturer 捕获 `ChangeEvent { op: Update, before: {name: 'old'}, after: {name: 'new'}, ts, txid }`（spec 5.9.1 规则 1 验收条件）
- [ ] M7-T3.8 编写单元测试：WAL 未配置逻辑复制（`wal_level != logical`）返回 `WalNotConfigured` 错误提示（spec 5.9.3 异常 1）

**验收标准**：
1. `DialectCapturer` trait 定义统一捕获接口
2. `WalCapturer` 通过 PostgreSQL 逻辑复制协议读取 WAL，构造 ChangeEvent
3. 从 checkpoint 恢复时从 WAL LSN 续传
4. WAL 未配置时返回 `WalNotConfigured` 错误提示
5. `cargo test -p sz-orm-queue --features cdc` 新增测试全部通过
6. 附 `packages/sz-orm-queue/src/cdc/capturer.rs` 新增代码的 file:line 证据

**依赖**：M7-T1（feature gate）、M7-T2（数据结构）

---

## M7-T4：MySQL binlog + SQLite 触发器 + Oracle LogMiner + MSSQL CDC 捕获器

**任务描述**：实现 MySQL binlog 捕获器、SQLite 触发器捕获器、Oracle LogMiner 捕获器、MSSQL CDC 捕获器，覆盖五方言变更捕获。

**涉及文件**：
- `packages/sz-orm-queue/src/cdc/binlog.rs`（新增，BinlogCapturer）
- `packages/sz-orm-queue/src/cdc/trigger.rs`（新增，TriggerCapturer）
- `packages/sz-orm-queue/src/cdc/logminer.rs`（新增，LogMinerCapturer）
- `packages/sz-orm-queue/src/cdc/mssql_cdc.rs`（新增，MssqlCdcCapturer）

**子任务**：
- [ ] M7-T4.1 实现 `BinlogCapturer`（MySQL binlog，design.md `:1223`）：通过 binlog 协议读取变更，解析 binlog event（WRITE_ROWS/UPDATE_ROWS/DELETE_ROWS）构造 ChangeEvent
- [ ] M7-T4.2 BinlogCapturer 从 checkpoint 恢复：从 binlog GTID 续传
- [ ] M7-T4.3 MySQL binlog 未开启处理：返回 `CdcError::BinlogNotEnabled`，提示开启 binlog
- [ ] M7-T4.4 实现 `TriggerCapturer`（SQLite 触发器/更新钩子，design.md `:1224`）：通过 SQLite `update_hook` 捕获变更，构造 ChangeEvent
- [ ] M7-T4.5 实现 `LogMinerCapturer`（Oracle LogMiner，design.md `:1225`）：通过 Oracle LogMiner 读取 redo log，构造 ChangeEvent
- [ ] M7-T4.6 实现 `MssqlCdcCapturer`（MSSQL CDC/变更跟踪，design.md `:1226`）：通过 MSSQL CDC 表读取变更，构造 ChangeEvent
- [ ] M7-T4.7 各捕获器实现 `DialectCapturer` trait，`dialect()` 返回对应方言
- [ ] M7-T4.8 编写单元测试：MySQL 执行 UPDATE，BinlogCapturer 捕获 ChangeEvent；SQLite 执行 UPDATE，TriggerCapturer 捕获 ChangeEvent
- [ ] M7-T4.9 编写单元测试：MySQL binlog 未开启返回 `BinlogNotEnabled` 错误提示
- [ ] M7-T4.10 编写单元测试：五方言捕获器 `dialect()` 返回正确方言类型（spec 5.9.1 规则 7 验收条件）

**验收标准**：
1. `BinlogCapturer`/`TriggerCapturer`/`LogMinerCapturer`/`MssqlCdcCapturer` 四方言捕获器实现 `DialectCapturer` trait
2. 各捕获器从对应变更源（binlog/触发器/LogMiner/CDC）读取变更构造 ChangeEvent
3. 从 checkpoint 恢复时从对应位点（GTID/TriggerSeq/SCN/Lsn）续传
4. `cargo test -p sz-orm-queue --features cdc` 新增测试全部通过
5. 附 `packages/sz-orm-queue/src/cdc/binlog.rs` 等新增代码的 file:line 证据

**依赖**：M7-T3（DialectCapturer trait）

---

## M7-T5：Exactly-Once 去重 + 断点续传

**任务描述**：实现 `ExactlyOnceDedup` 通过 TransactionId 幂等去重，`CheckpointManager` 管理消费位点持久化支持断点续传。

**涉及文件**：
- `packages/sz-orm-queue/src/cdc/dedup.rs`（新增，ExactlyOnceDedup）
- `packages/sz-orm-queue/src/cdc/checkpoint.rs`（新增，CheckpointManager）

**子任务**：
- [ ] M7-T5.1 实现 `ExactlyOnceDedup`：通过 `transaction_id` 幂等去重，已处理事务的事件不再分发（spec 5.9.1 规则 3，design.md `:181`）
- [ ] M7-T5.2 `ExactlyOnceDedup` 使用 `HashSet<String>`（或 LRU 缓存）存储已处理 transaction_id
- [ ] M7-T5.3 实现 `CheckpointManager`：`save_checkpoint(checkpoint: &CdcCheckpoint)` + `load_checkpoint() -> Option<CdcCheckpoint>`，位点持久化（spec 5.9.1 规则 4，design.md `:182`）
- [ ] M7-T5.4 位点持久化存储：文件/数据库/Redis（可配置 `CheckpointStoreConfig`）
- [ ] M7-T5.5 CDC 服务重启后从持久化位点续传，不丢事件不重复（spec 5.9.1 规则 4）
- [ ] M7-T5.6 位点持久化失败处理：暂停捕获，告警人工介入，避免重启后丢事件（spec 5.9.3 异常 3）
- [ ] M7-T5.7 编写单元测试：同一事务变更重发，下游去重，不重复消费（spec 5.9.1 规则 3 验收条件）
- [ ] M7-T5.8 编写单元测试：CDC 服务重启，从持久化位点续传，无丢失无重复（spec 5.9.1 规则 4 验收条件）
- [ ] M7-T5.9 编写单元测试：位点持久化失败，暂停捕获，告警人工介入（spec 5.9.3 异常 3）

**验收标准**：
1. `ExactlyOnceDedup` 通过 TransactionId 幂等去重，下游不出现重复事件
2. `CheckpointManager` 位点持久化，CDC 重启从断点续传
3. 位点持久化失败暂停捕获告警
4. `cargo test -p sz-orm-queue --features cdc` 新增测试全部通过
5. 附 `packages/sz-orm-queue/src/cdc/dedup.rs` 与 `checkpoint.rs` 新增代码的 file:line 证据

**依赖**：M7-T2（数据结构）

---

## M7-T6：下游分发 + ChangeEvent 脱敏

**任务描述**：实现 `CdcCapturer` 下游分发到消息队列（复用既有 sz-orm-queue 6 provider）+ HTTP webhook，并行分发，可选 ChangeEvent 脱敏。

**涉及文件**：
- `packages/sz-orm-queue/src/cdc/downstream.rs`（新增，下游分发）
- `packages/sz-orm-queue/src/cdc/masking.rs`（新增，ChangeEvent 脱敏）

**复用标注**：
- 既有 `sz-orm-queue` 6 provider（`real_kafka.rs`/`real_nats.rs`/`real_pulsar.rs`/`real_activemq.rs`/`lapin_rabbitmq.rs`/`rocketmq.rs`）：下游分发复用，不重复实现
- 既有 `DataMasker`（`packages/sz-orm-masking/src/lib.rs:44`）：12 种脱敏规则，ChangeEvent 脱敏复用

**子任务**：
- [ ] M7-T6.1 实现 `DownstreamSink` trait：`async fn send(&self, event: &ChangeEvent) -> Result<(), CdcError>`，适配既有 6 provider + HTTP webhook
- [ ] M7-T6.2 为既有 6 provider 实现 `DownstreamSink`：KafkaSink/RabbitMqSink/NatsSink/PulsarSink/RocketMqSink/ActiveMqSink，复用既有 provider 客户端
- [ ] M7-T6.3 实现 `HttpWebhookSink`：POST ChangeEvent JSON 到 webhook URL，支持 headers 鉴权
- [ ] M7-T6.4 实现 `CdcCapturer::distribute(&self, event: &ChangeEvent) -> Result<(), CdcError>`：并行分发到所有配置的下游（`tokio::join!`）（spec 5.9.1 规则 2，design.md `:183`）
- [ ] M7-T6.5 下游分发失败处理：事件缓冲到本地有界队列，下游恢复后重发，缓冲满则告警（spec 5.9.3 异常 2）
- [ ] M7-T6.6 实现 `apply_masking(event: &mut ChangeEvent, rules: &MaskingRuleMap)`：对 before/after 敏感字段应用 `DataMasker::apply` 脱敏（spec 5.9.1 规则 5，design.md `:184`）
- [ ] M7-T6.7 CDC 消费鉴权：消息队列 ACL / HTTP webhook 签名验证，禁止未授权消费（spec 4.3 规则 6）
- [ ] M7-T6.8 编写单元测试：配置下游=Kafka topic=users_cdc + HTTP webhook，变更事件同时分发到 Kafka 与 webhook（spec 5.9.1 规则 2 验收条件）
- [ ] M7-T6.9 编写单元测试：变更含手机号字段，启用脱敏，分发到 Kafka，Kafka 事件中手机号显示为 `138****8888`（spec 5.9.1 规则 5 验收条件）
- [ ] M7-T6.10 编写单元测试：Kafka 不可用，事件缓冲到本地队列，下游恢复后重发（spec 5.9.3 异常 2）

**验收标准**：
1. 下游分发复用既有 `sz-orm-queue` 6 provider，不重复实现消息队列客户端
2. 并行分发到所有配置下游，分发失败缓冲重发
3. 启用脱敏时 ChangeEvent 敏感字段脱敏后分发
4. CDC 消费鉴权（消息队列 ACL / webhook 签名验证）
5. `cargo test -p sz-orm-queue --features cdc` 新增测试全部通过
6. 附 `packages/sz-orm-queue/src/cdc/downstream.rs` 与 `masking.rs` 新增代码的 file:line 证据

**依赖**：M7-T2（数据结构）、M7-T5（ExactlyOnce + Checkpoint）

---

## M7-T7：CdcCapturer 编排 + M7 集成测试与门禁验证

**任务描述**：实现 `CdcCapturer` 编排捕获 + 去重 + 脱敏 + 分发 + 位点管理，并进行 M7 集成验证。

**涉及文件**：
- `packages/sz-orm-queue/src/cdc/capturer.rs`（扩展，CdcCapturer 编排）
- `packages/sz-orm-queue/tests/cdc_test.rs`（新增 M7 集成测试，`required-features = ["cdc"]`）

**子任务**：
- [ ] M7-T7.1 实现 `pub struct CdcCapturer { capturer: Box<dyn DialectCapturer>, downstream: Vec<Box<dyn DownstreamSink>>, checkpoint: Arc<CheckpointManager>, dedup: ExactlyOnceDedup, masker: Option<Arc<DataMasker>> }`（design.md `:1173-1179`）
- [ ] M7-T7.2 实现 `CdcCapturer::new(config: CdcConfig) -> Result<Self, CdcError>`：按 `config.dialect` 构造对应方言捕获器
- [ ] M7-T7.3 实现 `CdcCapturer::start(&self) -> Result<(), CdcError>`：启动捕获 + 去重 + 脱敏 + 分发 + 位点更新循环（design.md `:1230`）
- [ ] M7-T7.4 实现 `CdcCapturer::resume_from_checkpoint(&self) -> Result<(), CdcError>`：从持久化位点续传（design.md `:1233`）
- [ ] M7-T7.5 CDC 捕获开销 ≤5ms/事件（从 WAL/binlog 读取 + 序列化 ChangeEvent，spec 4.1 规则 7）
- [ ] M7-T7.6 CDC 分发吞吐 ≥10,000 事件/秒（spec 4.1 规则 7）
- [ ] M7-T7.7 运行 `cargo fmt --all -- --check` + `cargo check --workspace --all-targets` + `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] M7-T7.8 运行 `cargo test --workspace`（既有测试基线不回退）
- [ ] M7-T7.9 运行 `cargo test -p sz-orm-queue --features cdc`（M7 新增测试全部通过）
- [ ] M7-T7.10 运行 `cargo check --workspace --all-targets --all-features`（feature 全组合编译）
- [ ] M7-T7.11 扫描新增代码无占位实现、无 unsafe（或有 SAFETY 注释）
- [ ] M7-T7.12 验证既有 `sz-orm-queue` 6 provider 签名与行为不变

**验收标准**：
1. `CdcCapturer` 编排捕获 + 去重 + 脱敏 + 分发 + 位点管理
2. CDC 捕获开销 ≤5ms/事件，分发吞吐 ≥10,000 事件/秒
3. M7 相关门禁全部通过，既有测试基线不回退
4. `cdc` feature 全组合编译通过
5. 既有 `sz-orm-queue` 6 provider 签名与行为不变
6. 附门禁运行输出证据

**依赖**：M7-T1、M7-T2、M7-T3、M7-T4、M7-T5、M7-T6

---

# 九、M8：GraphQL 深度集成 async-graphql（REQ-V40-008，P2）

**目标**：将既有自研 GraphQL 深度对接 async-graphql 生态，复用既有 `DataLoader` 消除 N+1，支持 Subscription（基于 CDC ChangeEvent）/Relay 分页/Federation 联邦/工单化错误处理。
**预期工作量**：2 周
**对应需求**：REQ-V40-008（spec.md 5.8，design.md 2.2.2 模块 H）
**依赖**：M7（CDC，提供 ChangeEvent 作为 Subscription 数据源）

## M8-T1：async-graphql-integration feature gate 搭建

**任务描述**：在 sz-orm-graphql 中新增 `async-graphql-integration` feature gate，扩展既有 `real` feature（已引入 async-graphql = "7"）。

**涉及文件**：
- `packages/sz-orm-graphql/Cargo.toml`（新增 `async-graphql-integration` feature）

**复用标注**：既有 `real` feature 已引入 `async-graphql = { version = "7", optional = true }`（`packages/sz-orm-graphql/Cargo.toml:31`）+ `async-graphql-axum`（`:32`）

**子任务**：
- [ ] M8-T1.1 在 `packages/sz-orm-graphql/Cargo.toml` `[features]` 新增 `async-graphql-integration = ["real", "sz-orm-queue/cdc"]`，默认关闭（design.md `:1515`）
- [ ] M8-T1.2 验证 `cargo check -p sz-orm-graphql` 默认编译通过，无 async-graphql-integration 相关代码生效
- [ ] M8-T1.3 验证 `cargo check -p sz-orm-graphql --features async-graphql-integration` 编译通过
- [ ] M8-T1.4 验证既有 `real` feature 行为不变（`async-graphql-integration` 扩展而非替换）

**验收标准**：
1. `async-graphql-integration` feature 默认关闭，扩展既有 `real` feature
2. feature 全组合编译通过
3. 既有 `real` feature 行为不变
4. 附 `packages/sz-orm-graphql/Cargo.toml` 新增 feature 的 file:line 证据

**依赖**：M7-T1（cdc feature gate，Subscription 数据源）

---

## M8-T2：AsyncGraphqlBridge（async-graphql Schema 对接 + DataLoader 复用）

**任务描述**：实现 `AsyncGraphqlBridge`，将既有 `GraphQLSchema` 转换为 `async_graphql::Schema`，复用既有 `DataLoader` 消除 N+1。

**涉及文件**：
- `packages/sz-orm-graphql/src/async_graphql_integration/mod.rs`（新增模块）
- `packages/sz-orm-graphql/src/async_graphBraphql_integration/bridge.rs`（新增，AsyncGraphqlBridge 实现）
- `packages/sz-orm-graphql/src/lib.rs`（新增 `#[cfg(feature = "async-graphql-integration")] pub mod async_graphql_integration;`）

**复用标注**：
- 既有 `GraphQLSchema`（`packages/sz-orm-graphql/src/lib.rs:36`）：types/queries/mutations
- 既有 `GraphQLServer`（`packages/sz-orm-graphql/src/lib.rs:182`）：已含 `#[cfg(feature="real")] dynamic::Schema`
- 既有 `DataLoader<K, V>`（`packages/sz-orm-graphql/src/dataloader.rs:89`）：N+1 消除，单 tick 合并
- 既有 `BatchLoader<K, V>` trait（`packages/sz-orm-graphql/src/dataloader.rs:74`）：批量加载
- 既有 `async-graphql = "7"`（`packages/sz-orm-graphql/Cargo.toml:31`）

**子任务**：
- [ ] M8-T2.1 实现 `pub struct AsyncGraphqlBridge { schema: async_graphql::Schema<Query, Mutation, Subscription>, dataloader: Arc<DataLoader<String, ModelRow>> }`（design.md `:1116-1119`）
- [ ] M8-T2.2 实现 `AsyncGraphqlBridge::from_schema(schema: &GraphQLSchema, resolver: SharedDbResolver) -> Result<Self, GraphqlError>`：将既有 `GraphQLSchema:36` 转换为 `async_graphql::Schema`，复用既有 `DataLoader:89`（design.md `:1123`）
- [ ] M8-T2.3 实现 `AsyncGraphqlBridge::execute(&self, query: &str) -> Result<serde_json::Value, GraphqlError>`：执行查询，DataLoader 批量加载消除 N+1（design.md `:1126`）
- [ ] M8-T2.4 查询关联字段时 DataLoader 批量加载：`query { users { name, orders { amount } } }` → DataLoader 批量加载 users + 批量加载 orders（`SELECT * FROM orders WHERE user_id IN (...)`），无 N+1（spec 5.8.1 规则 1 验收条件）
- [ ] M8-T2.5 在 `packages/sz-orm-graphql/src/lib.rs` 新增 `#[cfg(feature = "async-graphql-integration")] pub mod async_graphql_integration;`
- [ ] M8-T2.6 定义 `GraphqlError` 枚举（thiserror）：`DataLoaderFailed { reason: String }` / `SubscriptionDisconnected` / `SchemaConversionFailed` / `QueryExecutionFailed`
- [ ] M8-T2.7 编写单元测试：查询 `users { name, orders { amount } }`，DataLoader 批量加载 users + orders，无 N+1（spec 5.8.1 规则 1 验收条件）
- [ ] M8-T2.8 编写单元测试：DataLoader 批量加载失败返回部分结果 + 错误，不整体失败（spec 5.8.3 异常 1）
- [ ] M8-T2.9 编写单元测试：既有 `GraphQLServer`（`:182`）/`DataLoader`（`:89`）/`BatchLoader`（`:74`）签名与行为不变

**验收标准**：
1. `AsyncGraphqlBridge` 将既有 `GraphQLSchema:36` 转换为 `async_graphql::Schema`，复用 `DataLoader:89` 消除 N+1
2. 查询关联字段 DataLoader 批量加载，无 N+1
3. DataLoader 失败返回部分结果 + 错误
4. 既有 `GraphQLServer`/`DataLoader`/`BatchLoader` 签名与行为不变
5. `cargo test -p sz-orm-graphql --features async-graphql-integration` 新增测试全部通过
6. 附 `packages/sz-orm-graphql/src/async_graphql_integration/bridge.rs` 新增代码的 file:line 证据

**依赖**：M8-T1（feature gate）

---

## M8-T3：Subscription 支持（基于 CDC ChangeEvent）

**任务描述**：实现 GraphQL Subscription，基于 WebSocket/SSE，订阅数据变更（复用 M7 CDC ChangeEvent 作为 Subscription 数据源）。

**涉及文件**：
- `packages/sz-orm-graphql/src/async_graphql_integration/subscription.rs`（新增，Subscription 实现）

**复用标注**：
- M7 `CdcCapturer`（`packages/sz-orm-queue/src/cdc/capturer.rs`）：提供 ChangeEvent 作为 Subscription 数据源
- 既有 `async-graphql = "7"` Subscription 支持

**子任务**：
- [ ] M8-T3.1 定义 `SubscriptionSource { cdc: Arc<CdcCapturer> }`，订阅 CDC ChangeEvent 流（design.md `:1132-1134`）
- [ ] M8-T3.2 实现 `impl async_graphql::Subscription for Subscription`：`user_updated` 订阅用户变更事件，返回 `impl Stream<Item = UserUpdatedEvent>`（design.md `:1136-1139`）
- [ ] M8-T3.3 实现 WebSocket 传输：基于 `async-graphql-axum` WebSocket 协议推送 Subscription 事件
- [ ] M8-T3.4 实现 SSE 传输：基于 Server-Sent Events 推送 Subscription 事件（备选）
- [ ] M8-T3.5 客户端订阅 `userUpdated` 事件，用户数据变更时 CDC 捕获 ChangeEvent → 推送 Subscription 事件（spec 5.8.1 规则 2 验收条件）
- [ ] M8-T3.6 Subscription 连接断开处理：清理订阅，停止推送，资源释放（spec 5.8.3 异常 2）
- [ ] M8-T3.7 编写单元测试：客户端订阅 `userUpdated`，用户数据变更时推送 Subscription 事件（spec 5.8.1 规则 2 验收条件）
- [ ] M8-T3.8 编写单元测试：客户端 WebSocket 连接断开，清理订阅，停止推送，资源释放（spec 5.8.3 异常 2）

**验收标准**：
1. Subscription 基于 CDC ChangeEvent 推送数据变更
2. 支持 WebSocket/SSE 传输
3. 客户端订阅 `userUpdated`，用户变更时推送事件
4. 连接断开清理订阅，资源释放
5. `cargo test -p sz-orm-graphql --features async-graphql-integration` 新增测试全部通过
6. 附 `packages/sz-orm-graphql/src/async_graphql_integration/subscription.rs` 新增代码的 file:line 证据

**依赖**：M8-T2（AsyncGraphqlBridge）、M7（CDC ChangeEvent 数据源）

---

## M8-T4：Relay 分页规范

**任务描述**：实现 Relay 分页规范（Connection/Edge/PageInfo，cursor-based 分页），复用既有 `keyset_after`。

**涉及文件**：
- `packages/sz-orm-graphql/src/async_graphql_integration/relay.rs`（新增，Relay 分页实现）

**复用标注**：既有 `QueryBuilder::keyset_after`（`packages/sz-orm-core/src/query.rs:986`）：cursor-based 分页，100% 复用

**子任务**：
- [ ] M8-T4.1 定义 `RelayConnection<T>` 结构：`edges: Vec<RelayEdge<T>>` + `page_info: PageInfo`，`#[derive(Debug, Clone)]`（design.md `:1143-1146`）
- [ ] M8-T4.2 定义 `RelayEdge<T>` 结构：`node: T` + `cursor: String`
- [ ] M8-T4.3 定义 `PageInfo` 结构：`has_next_page: bool` + `has_previous_page: bool` + `start_cursor: Option<String>` + `end_cursor: Option<String>`，`#[derive(Debug, Clone)]`（design.md `:1148`）
- [ ] M8-T4.4 实现 `relay_paginate<T>(query: QueryBuilder<T>, first: usize, after: Option<String>) -> Result<RelayConnection<T>, GraphqlError>`：复用既有 `keyset_after:986` 实现 cursor-based 分页（spec 5.8.1 规则 3，design.md `:1166`）
- [ ] M8-T4.5 `first` 参数控制每页数量，`after` cursor 控制起始位置
- [ ] M8-T4.6 `has_next_page` = 结果数 > first，`end_cursor` = 最后一条记录的 cursor
- [ ] M8-T4.7 编写单元测试：查询 `users(first: 10, after: "cursor")`，返回 `Connection { edges, pageInfo { hasNextPage, endCursor } }`（spec 5.8.1 规则 3 验收条件）
- [ ] M8-T4.8 编写单元测试：`has_next_page` 在有更多数据时为 true，无更多数据时为 false
- [ ] M8-T4.9 编写单元测试：既有 `QueryBuilder::keyset_after`（`:986`）签名与行为不变

**验收标准**：
1. Relay 分页返回 `Connection { edges, pageInfo }`，cursor-based 分页
2. 复用既有 `keyset_after:986`，不重复实现
3. `has_next_page`/`end_cursor` 正确计算
4. 既有 `keyset_after` 签名与行为不变
5. `cargo test -p sz-orm-graphql --features async-graphql-integration` 新增测试全部通过
6. 附 `packages/sz-orm-graphql/src/async_graphql_integration/relay.rs` 新增代码的 file:line 证据

**依赖**：M8-T2（AsyncGraphqlBridge）

---

## M8-T5：Federation 联邦 schema

**任务描述**：实现 GraphQL Federation（联邦 schema，多服务 schema 合并，`_entities`/`_service` 查询）。

**涉及文件**：
- `packages/sz-orm-graphql/src/async_graphql_integration/federation.rs`（新增，Federation 实现）

**复用标注**：`async-graphql` Federation 扩展

**子任务**：
- [ ] M8-T5.1 实现 `FederationGateway`：合并多个子服务 schema 为联邦网关 schema（spec 5.8.1 规则 4，design.md `:167`）
- [ ] M8-T5.2 实现 `_entities` 查询：跨服务实体解析
- [ ] M8-T5.3 实现 `_service` 查询：返回服务 sdl
- [ ] M8-T5.4 跨服务查询：联邦网关合并 schema 后跨服务查询正常
- [ ] M8-T5.5 编写单元测试：配置 Federation，两个子服务 schema，联邦网关合并 schema，跨服务查询正常（spec 5.8.1 规则 4 验收条件）

**验收标准**：
1. Federation 联邦网关合并多服务 schema
2. `_entities`/`_service` 查询正确
3. 跨服务查询正常
4. `cargo test -p sz-orm-graphql --features async-graphql-integration` 新增测试全部通过
5. 附 `packages/sz-orm-graphql/src/async_graphql_integration/federation.rs` 新增代码的 file:line 证据

**依赖**：M8-T2（AsyncGraphqlBridge）

---

## M8-T6：工单化错误处理

**任务描述**：实现工单化错误处理（async-graphql Error extensions），错误含错误码/分类/工单 ID，便于前端统一处理。

**涉及文件**：
- `packages/sz-orm-graphql/src/async_graphql_integration/error.rs`（新增，工单化错误）

**复用标注**：既有 `extensions.rs`（`packages/sz-orm-graphql/src/extensions.rs`）：错误扩展

**子任务**：
- [ ] M8-T6.1 定义 `TicketError` 结构：`code: String` + `category: String` + `ticket_id: String` + `message: String`，`#[derive(Debug, Clone)]`（design.md `:1152-1157`）
- [ ] M8-T6.2 实现 `impl async_graphql::ErrorExtension for TicketError`：错误扩展含 code/category/ticket_id（design.md `:1158`）
- [ ] M8-T6.3 实现 `TicketError::new(code, category, message) -> Self`：生成错误并分配工单 ID（UUID）
- [ ] M8-T6.4 错误分类：`ValidationError` / `AuthError` / `NotFoundError` / `InternalError` / `RateLimitError`
- [ ] M8-T6.5 编写单元测试：查询错误返回含 code/category/ticket_id，前端可据 code 统一处理（spec 5.8.1 规则 5 验收条件）
- [ ] M8-T6.6 编写单元测试：每个错误分配唯一工单 ID（UUID）

**验收标准**：
1. 工单化错误含 code/category/ticket_id，前端可据 code 统一处理
2. 每个错误分配唯一工单 ID
3. `cargo test -p sz-orm-graphql --features async-graphql-integration` 新增测试全部通过
4. 附 `packages/sz-orm-graphql/src/async_graphql_integration/error.rs` 新增代码的 file:line 证据

**依赖**：M8-T2（AsyncGraphqlBridge）

---

## M8-T7：M8 集成测试与门禁验证

**任务描述**：对 M8 所有任务进行集成验证。

**涉及文件**：
- `packages/sz-orm-graphql/tests/async_graphql_integration_test.rs`（新增 M8 集成测试，`required-features = ["async-graphql-integration"]`）

**子任务**：
- [ ] M8-T7.1 运行 `cargo fmt --all -- --check` + `cargo check --workspace --all-targets` + `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] M8-T7.2 运行 `cargo test --workspace`（既有测试基线不回退）
- [ ] M8-T7.3 运行 `cargo test -p sz-orm-graphql --features async-graphql-integration`（M8 新增测试全部通过）
- [ ] M8-T7.4 运行 `cargo check --workspace --all-targets --all-features`（feature 全组合编译，含 async-graphql-integration + cdc）
- [ ] M8-T7.5 扫描新增代码无占位实现、无 unsafe（或有 SAFETY 注释）
- [ ] M8-T7.6 验证既有 `GraphQLServer`（`:182`）/`DataLoader`（`:89`）/`BatchLoader`（`:74`）签名与行为不变
- [ ] M8-T7.7 验证默认 `cargo build` 不引入 async-graphql-integration 依赖（spec 5.8.1 规则 7）

**验收标准**：
1. M8 相关门禁全部通过
2. 既有测试基线不回退
3. `async-graphql-integration` + `cdc` 联合 feature 组合编译与测试通过
4. 既有 `GraphQLServer`/`DataLoader`/`BatchLoader` 签名与行为不变
5. 默认编译不引入 async-graphql-integration 依赖
&6. 附门禁运行输出证据

**依赖**：M8-T1、M8-T2、M8-T3、M8-T4、M8-T5、M8-T6

---

# 十、M9：服务网格集成（REQ-V40-007，P2）

**目标**：提供 `ServiceMeshAdapter` trait，支持 Istio/Linkerd 两类服务网格，生成 xDS/CRD 配置，mTLS STRICT 默认，流量治理（金丝雀/蓝绿/熔断/重试），复用既有 `MetricsRegistry` + `sz-orm-tracing`。
**预期工作量**：1 周
**对应需求**：REQ-V40-007（spec.md 5.7，design.md 2.2.2 模块 G）
**依赖**：无（M9 独立，仅需 feature gate 体系）

## M9-T1：service-mesh feature gate 搭建

**任务描述**：在 sz-orm-observability 中新增 `service-mesh` feature gate，依赖 sz-orm-limit。

**涉及文件**：
- `packages/sz-orm-observability/Cargo.toml`（新增 `service-mesh` feature）

**子任务**：
- [ ] M9-T1.1 在 `packages/sz-orm-observability/Cargo.toml` `[features]` 新增 `service-mesh = ["sz-orm-limit"]`，默认关闭（design.md `:1508`）
- [ ] M9-T1.2 验证 `cargo check -p sz-orm-observability` 默认编译通过，无 service-mesh 相关代码生效
- [ ] M9-T1.3 验证 `cargo check -p sz-orm-observability --features service-mesh` 编译通过

**验收标准**：
1. `service-mesh` feature 默认关闭，默认编译行为与 v3.9.0 一致
2. feature 全组合编译通过
3. 附 `packages/sz-orm-observability/Cargo.toml` 新增 feature 的 file:line 证据

**依赖**：无

---

## M9-T2：ServiceMeshAdapter trait + MeshConfig 数据结构

**任务描述**：在 sz-orm-observability 新增 `service_mesh` 模块，定义 `ServiceMeshAdapter` trait、`MeshConfig`/`MtlsMode`/`TrafficGovernance` 等数据结构。

**涉及文件**：
- `packages/sz-orm-observability/src/service_mesh/mod.rs`（新增模块，定义数据结构）
- `packages/sz-orm-observability/src/lib.rs`（新增 `#[cfg(feature = "service-mesh")] pub mod service_mesh;`）

**复用标注**：
- 既有 `MetricsRegistry`（`packages/sz-orm-observability/src/lib.rs:250`）：Prometheus Counter/Gauge/Histogram
- 既有 `MetricsAccessControl`（`packages/sz-orm-observability/src/lib.rs:443`）：metrics ACL
- 既有 `sz-orm-tracing`（OTLP 分布式追踪）
- 既有 `sz-orm-limit`（限流熔断器）

**子任务**：
- [ ] M9-T2.1 定义 `MeshType` 枚举：`Istio` / `Linkerd`，`#[derive(Debug, Clone, Copy)]`（design.md `:1083`）
- [ ] M9-T2.2 定义 `MtlsMode` 枚举：`Strict`（默认） / `Permissive`，`#[derive(Debug, Clone, Copy, Default)]`（design.md `:1085`）
- [ ] M9-T2.3 定义 `CanaryConfig`/`BlueGreenConfig`/`CircuitConfig`/`RetryConfig` 结构：金丝雀（percentage）、蓝绿（version）、熔断（复用 sz-orm-limit）、重试（status codes/timeout）
- [ ] M9-T2.4 定义 `TrafficGovernance` 结构：`canary: Option<CanaryConfig>` + `blue_green: Option<BlueGreenConfig>` + `circuit_breaker: Option<CircuitConfig>` + `retry: Option<RetryConfig>`（design.md `:1088-1093`）
- [ ] M9-T2.5 定义 `SidecarConfig` 结构：`namespace_label: Option<String>` + `annotation: Option<String>`
- [ ] M9-T2.6 定义 `MeshConfig` 结构：`mesh: MeshType` + `mtls: MtlsMode` + `traffic: TrafficGovernance` + `sidecar_injection: SidecarConfig`，`#[derive(Debug, Clone)]`（design.mdE.md `:1075-1080`）
- [ ] M9-T2.7 定义 `MeshConfigOutput` 结构：`yaml: String` + `resources: Vec<String>`（生成的 YAML + 资源列表）
- [ ] M9-T2.8 定义 `MeshError` 枚举（thiserror）：`ControlPlaneUnavailable` / `MtlsConflict { existing: MtlsMode, new: MtlsMode }` / `ConfigInvalid`
- [ ] M9-T2.9 定义 `pub trait ServiceMeshAdapter: Send + Sync`：`fn generate_config(&self, config: &MeshConfig) -> Result<MeshConfigOutput, MeshError>` + `fn mesh_type(&self) -> &'static str`（design.md `:1066-1071`）
- [ ] M9-T2.10 在 `packages/sz-orm-observability/src/lib.rs` 新增 `#[cfg(feature = "service-mesh")] pub mod service_mesh;`
- [ ] M9-T2.11 编写单元测试：`MeshConfig::default()` 含 mTLS STRICT + Istio mesh；`MtlsMode::default()` 为 Strict

**验收标准**：
1. `ServiceMeshAdapter` trait + `MeshConfig`/`MtlsMode`/`TrafficGovernance` 数据结构完整可用
2. `MtlsMode::default()` 为 Strict（安全优先）
3. `cargo test -p sz-orm-observability --features service-mesh` 新增测试全部通过
4. 附 `packages/sz-orm-observability/src/service_mesh/mod.rs` 新增代码的 file:line 证据

**依赖**：M9-T1（feature gate）

---

## M9-T3：IstioAdapter + LinkerdAdapter 实现

**任务描述**：实现 `ServiceMeshAdapter` trait 的 IstioAdapter（生成 Istio CRD：VirtualService/DestinationRule/PeerAuthentication）与 LinkerdAdapter（生成 Linkerd policy）。

**涉及文件**：
- `packages/sz-orm-observability/src/service_mesh/istio.rs`（新增，IstioAdapter）
- `packages/sz-orm-observability/src/service_mesh/linkerd.rs`（新增，LinkerdAdapter）

**复用标注**：既有 `MetricsRegistry`（`packages/sz-orm-observability/src/lib.rs:250`）：接入网格 metrics

**子任务**：
- [ ] M9-T3.1 实现 `pub struct IstioAdapter { metrics: Arc<MetricsRegistry> }`，`new(metrics) -> Self`（design.md `:1096`）
- [ ] M9-T3.2 实现 `impl ServiceMeshAdapter for IstioAdapter`：`generate_config` 生成 Istio CRD YAML（VirtualService/DestinationRule/PeerAuthentication）（design.md `:1097`）
- [ ] M9-T3.3 生成 `VirtualService`：含路由规则（金丝雀按百分比、蓝绿按版本、重试按状态码/超时）
- [ ] M9-T3.4 生成 `DestinationRule`：含熔断配置（复用既有 `sz-orm-limit` 熔断器参数）
- [ ] M9-T3.5 生成 `PeerAuthentication`：mTLS STRICT/PERMISSIVE 模式（spec 5.7.1 规则 2）
- [ ] M9-T3.6 sidecar 自动注入：生成 namespace label `istio-injection=enabled`（spec 5.7.1 规则 4）
- [ ] M9-T3.7; M9-T3.7 实现 `pub struct LinkerdAdapter { metrics: Arc<MetricsRegistry> }`（design.md `:1100`）
- [ ] M9-T3.8 实现 `impl ServiceMeshAdapter for LinkerdAdapter`：`generate_config` 生成 Linkerd policy YAML（design.md `:1101`）
- [ ] M9-T3.9 Linkerd mTLS 策略：生成 `Server` + `ServerAuthorization` policy
- [ ] M9-T3.10 Linkerd 流量治理：生成 `ServiceProfile`（重试/超时/熔断）
- [ ] M9-T3.11 Linkerd sidecar 注入：生成 annotation `linkerd.io/inject: enabled`
- [ ] M9-T3.12 网格平台不可用处理：控制平面不可用时配置生成正常，部署标记"mesh control plane unavailable"（spec 5.7.3 异常 1）
- [ ] M9-T3.13 mTLS 配置冲突处理：已有 PERMISSIVE mTLS，新配置 STRICT 时提示配置冲突，需人工确认覆盖（spec 5.7.3 异常 2）
- [ ] M9-T3.14 编写单元测试：配置 mesh=istio，生成 Istio CRD（VirtualService/DestinationRule/PeerAuthentication）（spec 5.7.1 规则 1 验收条件）
- [ ] M9-T3.15 编写单元测试：配置 mesh=linkerd，生成 Linkerd policy（spec 5.7.1 规则 1 验收条件）
- [ ] M9-T3.16 编写单元测试：生成网格配置含 mTLS STRICT 策略，服务间通信加密（spec 5.7.1 规则 2 验收条件）
- [ ] M9-T3.17 编写单元测试：配置金丝雀 10% 流量到 v2，生成 VirtualService 90% v1 + 10% v2 路由规则（spec 5.7.1 规则 3 验收条件）
- [ ] M9-T3.18 编写单元测试：配置 namespace istio-injection=enabled，Pod 自动注入 Istio sidecar（spec 5.7.1 规则 4 验收条件）
- [ ] M9-T3.19 编写单元测试：mTLS 配置冲突（已有 PERMISSIVE vs 新 STRICT）提示需人工确认（spec 5.7.3 异常 2）

**验收标准**：
1. `IstioAdapter` 生成 Istio CRD（VirtualService/DestinationRule/PeerAuthentication），`LinkerdAdapter` 生成 Linkerd policy
2. mTLS 默认 STRICT，服务间通信加密
3. 流量治理：金丝雀按百分比路由、蓝绿按版本切换、熔断复用 sz-orm-limit、重试按状态码/超时
4. sidecar 自动注入配置正确
5. 网格平台不可用/mTLS 冲突正确处理
6. `cargo test -p sz-orm-observability --features service-mesh` 新增测试全部通过
7. 附 `packages/sz-orm-observability/src/service_mesh/istio.rs` 与 `linkerd.rs` 新增代码的 file:line 证据

**依赖**：M9-T2（数据结构）

---

## M9-T4：可观测性接入（复用 MetricsRegistry + sz-orm-tracing）

**任务描述**：服务网格 metrics/traces 接入既有 `MetricsRegistry`（Prometheus）+ `sz-orm-tracing`（OTLP）。

**涉及文件**：
- `packages/sz-orm-observability/src/service_mesh/observability.rs`（新增，可观测性接入）

**复用标注**：
- 既有 `MetricsRegistry`（`packages/sz-orm-observability/src/lib.rs:250`）：Prometheus 抓取
- 既有 `sz-orm-tracing`（OTLP 分布式追踪）

**子任务**：
- [ ] M9-T4.1 实现 `ServiceMeshAdapter::integrate_metrics(&self, registry: &MetricsRegistry)`：网格 metrics 接入既有 `MetricsRegistry`，Prometheus 抓取（spec 5.7.1 规则 5）
- [ ] M9-T4.2 实现 `ServiceMeshAdapter::integrate_traces(&self)`：网格 traces 接入既有 `sz-orm-tracing` OTLP
- [ ] M9-T4.3 网格 metrics 含请求计数/延迟直方图/熔断次数/重试次数
- [ ] M9-T4.4 网格 traces 含分布式追踪 span（sidecar 代理 span + 应用 span）
- [ ] M9-T4.5 编写单元测试：服务网格 metrics 接入既有 MetricsRegistry，Prometheus 抓取；traces 接入既有 OTLP（spec 5.7.1 规则 5 验收条件）

**验收标准**：
1. 网格 metrics 接入既有 `MetricsRegistry:250`，Prometheus 抓取
2. 网格 traces 接入既有 `sz-orm-tracing` OTLP
3. `cargo test -p sz-orm-observability --features service-mesh` 新增测试全部通过
4. 附 `packages/sz-orm-observability/src/service_mesh/observability.rs` 新增代码的 file:line 证据

**依赖**：M9-T3（Istio/Linkerd adapter）

---

## M9-T5：M9 集成测试与门禁验证

**任务描述**：对 M9 所有任务进行集成验证。

**涉及文件**：
- `packages/sz-orm-observability/tests/service_mesh_test.rs`（新增 M9 集成测试，`required-features = ["service-mesh"]`）

**子任务**：
- [ ] M9-T5.1 运行 `cargo fmt --all -- --check` + `cargo check --workspace --all-targets` + `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] M9-T5.2 运行 `cargo test --workspace`（既有测试基线不回退）
- [ ] M9-T5.3 运行 `cargo test -p sz-orm-observability --features service-mesh`（M9 新增测试全部通过）
- [ ] M9-T5.4 运行 `cargo check --workspace --all-targets --all-features`（feature 全组合编译）
- [ ] M9-T5.5 扫描新增代码无占位实现、无 unsafe（或有 SAFETY 注释）
- [ ] M9-T5.6 验证既有 `MetricsRegistry`（`:250`）/`MetricsAccessControl`（`:443`）/`sz-orm-tracing` 签名与行为不变
- [ ] M9-T5.7 验证默认 `cargo build` 不引入 service-mesh 依赖（spec 5.7.1 规则 6）

**验收标准**：
1. M9 相关门禁全部通过
2. 既有测试基线不回退
3. `service-mesh` feature 全组合编译通过
4. 既有 `MetricsRegistry`/`MetricsAccessControl`/`sz-orm-tracing` 签名与行为不变
5. 默认编译不引入 service-mesh 依赖
6. 附门禁运行输出证据

**依赖**：M9-T1、M9-T2、M9-T3、M9-T4

---

# 十一、M10：最终验证与文档同步

**目标**：v4.0.0 须通过 AGENTS.md 定义的 14 道门禁，更新版本号、CHANGELOG、README，同步文档，验证 sz-pay 兼容性。
**预期工作量**：0.5 周
**依赖**：M1-M9 全部完成

## M10-T1：14 道门禁最终验证

**任务描述**：v4.0.0 须通过 AGENTS.md 定义的 14 道门禁，确保整体质量与不回退。

**涉及文件**：
- `scripts/gate.ps1`（复用既有门禁脚本）
- `scripts/check-sql-injection.ps1`（复用既有 SQL 注入扫描）
- `scripts/check-doc-consistency.py`（复用既有文档一致性检查）
- `scripts/audit-verify.sh`（复用既有审计证据验证）

**子任务**：
- [ ] M10-T1.1 门禁 1：`cargo fmt --all -- --check`（fmt 格式检查）通过
- [ ] M10-T1.2 门禁 2：`cargo check --workspace --all-targets`（默认 feature 编译检查）通过
- [ ] M10-T1.3 门禁 3：`cargo clippy --workspace --all-targets -- -D warnings`（clippy 静态分析）通过
- [ ] M10-T1.4 门禁 4：`cargo test --workspace`（单元/集成测试）通过，既有测试基线不回退（6760+ passed）
- [ ] M10-T1.5 门禁 5：`cargo doc --workspace --no-deps --all-features`（文档构建）通过
- [ ] M10-T1.6 门禁 6：`cargo audit` + `cargo deny check`（安全审计）通过
- [ ] M10-T1.7 门禁 7：`cargo test --workspace -- --ignored`（真实服务集成测试）通过
- [ ] M10-T1.8 门禁 8：`grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs'`（禁止占位实现检查）无命中
- [ ] M10-T1.9 门禁 9：`scripts/check-sql-injection.ps1`（SQL 注入扫描）通过
- [ ] M10-T1.10 门禁 10：`cargo check --workspace --all-targets --all-features`（feature 全组合编译，含 9 个新 feature）通过
- [ ] M10-T1.11 门禁 11：`git diff --name-only HEAD`（上游仓库未修改检查，ADR-0001）确认未修改 sz-pay/sz-rust 下游
- [ ] M10-T1.12 门禁 12：`python scripts/check-doc-consistency.py`（文档与代码一致性检查）通过
- [ ] M10-T1.13 门禁 13：`bash scripts/audit-verify.sh <审计报告.md>`（审计证据验证）通过，所有 file:line 引用真实存在
- [ ] M10-T1.14 门禁 14：`python scripts/check-doc-sync.py --diff HEAD`（文档同步更新检查）通过

**验收标准**：
1. 14 道门禁全部通过
2. 既有测试基线不回退（6760+ passed）
3. 新增代码无占位实现、无 unsafe（或有 SAFETY 注释）
4. 审计证据所有 file:line 引用真实存在
5. sz-pay/sz-rust 下游仓库未修改（ADR-0001）
6. 附 14 道门禁运行输出证据

**依赖**：M1-T6、M2-T5、M3-T5、M4-T6、M5-T5、M6-T5、M7-T7、M8-T7、M9-T5（所有里程碑集成测试完成）

---

## M10-T2：文档同步与版本号更新

**任务描述**：更新版本号、CHANGELOG、README，同步 v4.0.0 文档，验证 sz-pay 兼容性。

**涉及文件**：
- `Cargo.toml`（workspace.package.version 从 3.9.0 更新至 4.0.0）
- `CHANGELOG.md`（新增 v4.0.0 变更记录）
- `README.md`（更新 v4.0.0 新能力说明）
- `docs/spec/v4.0.0/spec.md`、`docs/spec/v4.0.0/design.md`、`docs/spec/v4.0.0/tasks.md`（本文档）

**子任务**：
- [ ] M10-T2.1 更新 `Cargo.toml` workspace.package.version 从 3.9.0 至 4.0.0（集中管理版本号）
- [ ] M10-T2.2 更新 `CHANGELOG.md` 新增 v4.0.0 变更记录：9 项新能力（AI 调优闭环/多 LLM/混合搜索/数据 lineage/分片 rebalance/failover 自动化/服务网格/GraphQL 深度集成/CDC）、9 个新 feature gate
- [ ] M10-T2.3 更新 `README.md` 新增 v4.0.0 能力说明：9 个 feature gate 启用方式、`AutoTuningPipeline` 使用示例、`LlmProvider` 多 provider 配置示例、`HybridSearcher` 混合搜索示例、`LineageTracker` lineage 示例、`ShardRebalancer` rebalance 示例、`AutoFailoverManager` failover 示例、`ServiceMeshAdapter` 服务网格示例、`AsyncGraphqlBridge` GraphQL 示例、`CdcCapturer` CDC 示例
- [ ] M10-T2.4 运行 `python scripts/check-doc-sync.py --diff HEAD` 验证文档与代码同步
- [ ] M10-T2.5 运行 `python scripts/check-doc-consistency.py` 验证文档与代码一致性
- [ ] M10-T2.6 验证 sz-pay 兼容性：sz-pay 不启用 v4.0.0 新 feature，行为与 v3.9.0 一致（sz-pay 路径 `E:\vue\test\sz-pay`）
- [ ] M10-T2.7 生成 v4.0.0 验收报告：9 项需求验收结果，每项附 file:line 证据

**验收标准**：
1. 版本号更新至 4.0.0，集中管理
2. CHANGELOG/README 文档同步更新
3. 文档与代码一致性检查通过
4. sz-pay 兼容性验证通过（不启用新 feature 时行为与 v3.9.0 一致）
5. 9 项需求验收报告附 file:line 证据
6. 附文档更新 file:line 证据

**依赖**：M10-T1（14 道门禁通过）

---

## M10-T3：feature gate 逐步启用计划与全组合验证

**任务描述**：验证 9 个新 feature gate 的逐步启用计划，确保各 feature 可独立启用、组合启用、全启用均编译通过。

**涉及文件**：
- 各包 Cargo.toml（feature gate 验证）

**子任务**：
- [ ] M10-T3.1 验证 9 个 feature 独立启用编译通过：`cargo check -p sz-orm-ai --features multi-llm`、`cargo check. -p sz-orm-ai --features ai-auto-tuning`、`cargo check -p sz-orm-vector --features hybrid-search`、`cargo check -p sz-orm-audit --features data-lineage`、`cargo check -p sz-orm-sharding --features shard-rebalance`、`cargo check -p sz-orm-rw --features auto-failover`、`cargo check -p sz-orm-observability --features service-mesh`、`cargo check -p sz-orm-graphql --features async-graphql-integration`、`cargo check -p sz-orm-queue --features cdc`
- [ ] M10-T3.2 验证依赖 feature 组合启用编译通过：`cargo check -p sz-orm-ai --features ai-auto-tuning,multi-llm`（AI 调优 + 多 LLM）、`cargo check -p sz-orm-graphql --features async-graphql-integration`（含 cdc 依赖）
- [ ] M10-T3.3 验证全 feature 组合编译通过：`cargo check --workspace --all-targets --all-features`
- [ ] M10-T3.4 验证默认 feature 行为不变：`cargo check --workspace`（不启用任何 v4.0.0 新 feature，行为与 v3.9.0 一致）
- [ ] M10-T3.5 验证 feature gate 隔离正确：各 feature 默认关闭，`cargo build` 不引入新 feature 相关依赖
- [ ] M10-T3.6 生成 feature gate 启用矩阵文档：9 个 feature × 独立/组合/全启用 × 编译结果

**验收标准**：
1. 9 个 feature 独立启用均编译通过
2. 依赖 feature 组合启用编译通过（ai-auto-tuning + multi-llm、async-graphql-integration + cdc）
3. 全 feature 组合编译通过
4. 默认 feature 行为与 v3.9.0 一致
5. feature gate 隔离正确，默认不引入新依赖
6. 附 feature gate 启用矩阵文档

**依赖**：M10-T1（14 道门禁通过）

---

# 十二、任务依赖关系图

```plantuml
@startuml
title sz-orm v4.0.0 任务依赖关系图

package "M1: 多 LLM 模型支持" as m1 {
  usecase "M1-T1: feature gate" as m1t1
  usecase "M1-T2: LlmProvider trait" as m1t2
  usecase "M1-T3: Claude/Gemini provider" as m1t3
  usecase "M1-T4: Ollama/OpenAI provider" as m1t4
  usecase "M1-T5: LlmRouter" as m1t5
  usecase "M1-T6: M1 集成测试" as m1t6
}

package "M2: AI 自动调优闭环" as m2 {
  usecase "M2-T1: feature gate" as m2t1
  usecase "M2-T2: 数据结构" as m2t2
  usecase "M2-T3: SlowQueryDetector" as m2t3
  usecase "M2-T4: AutoTuningPipeline" as m2t4
  usecase "M2-T5: M2 集成测试" as m2t5
}

package "M3: 混合搜索" as m3 {
  usecase "M3-T1: feature gate" as m3t1
  usecase "M3-T2: 数据结构" as m3t2
  usecase "M3-T3: 三源并行查询" as m3#t3
  usecase "M3-T4: 融合排序+下推" as m3t4
  usecase "M3-T5: M3 集成测试" as m3t5
}

package "M4: 数据 lineage" as m4 {
  usecase "M4-T1:A: feature gate" as m4t1
  usecase "M4-T2: LineageGraph" as m4t2
  usecase "M4-T3: SQL 解析" as m4t3
  usecase "M4-T4: LineageTracker" as m4t4
  usecase "M4-T5: 图导出" as m4t5
  usecase "M4-T6: M4 集成测试" as m4t6
}

package "M5: 分片 rebalance" as m5 {
  usecase "M5-T1: feature gate" as m5t1
  usecase "M5-T2: 数据结构" as m5t2
  usecase "M5-T3: 迁移计划" as m5t3
  usecase "M5-T4: 迁移执行" as m5t4
  usecase "M5-T5: M5 集成测试" as m5t5
}

package "M6: failover 自动化" as m6 {
  usecase "M6-T1: feature gate" as m6t1
  usecase "M6-T2: 数据结构" as m6t2
  usecase "M6-T3: AutoFailoverManager" as m6t3
  usecase "M6-T4: 脑裂检测" as m6t4
  usecase "M6-T5: M6 集成测试" as m6t5
}

package "M7: CDC 变更数据捕获" as m7 {
  usecase "M7-T1: feature gate" as m7t1
  usecase "M7-T2: 数据结构" as m7t2
  usecase "M7-T3: PG WAL 捕获" as m7t3
  usecase "M7-T4: 四方言捕获" as m7t4
  usecase "M7-T5: Exactly-Once+断点" as m7t5
  usecase "M7-T6: 下游分发+脱敏" as m7t6
  usecase "M7-T7: CdcCapturer+集成" as m7t7
}

package "M8: GraphQL 深度集成" as m8 {
  usecase "M8-T1: feature gate" as m8t1
  usecase "M8-T2: AsyncGraphqlBridge" as m8t2
  usecase "M8-T3: Subscription" as m8t3
  usecase "M8-T4: Relay 分页" as m8t4
  usecase "M8-T5: Federation" as m8t5
  usecase "M8-T6: 工单化错误" as m8t6
  usecase "M8-T7: M8 集成测试" as m8t7
}

package "M9: 服务网格集成" as m9 {
  usecase "M9-T1: feature gate" as m9t1
  usecase "M9-T2: 数据结构" as m9t2
  usecase "M9-T3: Istio/Linkerd" as m9t3
  usecase "M9-T4: 可观测性接入" as m9t4
  usecase "M9-T5: M9 集成测试" as m9t5
}

package "M10: 最终验证" as m10 {
  usecase "M10-T1: 14 道门禁" as m10t1
  usecase "M10-T2: 文档同步" as m10t2
  usecase "M10-T3: feature 启用计划" as m10t3
}

' M1 内部依赖
m1t2 --> m1t1
m1t3 --> m1t1
m1t3 --> m1t2
m1t4 --> m1t1
m1t4 --> m1t2
m1t5 --> m1t1
m1t5 --> m1t2
m1t5 --> m1t3
m1t5 --> m1t4
m1t6 --> m1t5

' M2 依赖 M1 (LlmProvider)
m2t1 --> m1t1
m2t2 --> m2t1
m2t3 --> m2t2
m2t4 --> m2t3
m2t4 --> m1t5
m2t5 --> m2t4

' M3 独立
m3t2 --> m3t1
m3t3 --> m3t2
m3t4 --> m3t3
m3t5 --> m3t4

' M4 独立
m4t2 --> m4t1
m4t3 --> m4t2
m4t4 --> m4t3
m4t5 --> m4t2
m4t6 --> m4t4
m4t6 --> m4t5

' M5 独立
m5t2 --> m5t1
m5t3 --> m5t2
m5t4 --> m5t3
m5t5 --> m5t4

' M6 独立
m6t2 --> m6t1
m6t3 --> m6t2
m6t4 --> m6t3
m6t5 --> m6t4

' M7 独立 (先于 M8)
m7t2 --> m7t1
m7t3 --> m7t2
m7t4 --> m7t3
m7t5 --> m7t2
m7t6 --> m7t5
m7t7 --> m7t6
m7t7 --> m7t4

' M8 依赖 M7 (CDC Subscription 数据源)
m8t1 --> m7t1
m8t2 --> m8t1
m8t3 --> m8t2
m8t3 --> m7t7
m8t4 --> m8t2
m8t5 --> m8t2
m8t6 --> m8t2
m8t7 --> m8t3
m8t7 --> m8t4
m8t7 --> m8t5
m8t7 --> m8t6

' M9 独立
m9t2 --> m9t1
m9t3 --> m9t2
m9t4 --> m9t3
m9t5 --> m9t4

' M10 依赖所有里程碑
m10t1 --> m1t6
m10t1 --> m2t5
m10t1 --> m3t5
m10t1 --> m4t6
m10t1 --> m5t5
m10t1 --> m6t5
m10t1 --> m7t7
m10t1 --> m8t7
m10t1 --> m9t5
m10t2 --> m10t1
m10t3 --> m10t1

@enduml
```

**依赖关系说明**：
1. **M1（多 LLM）是 P0 基座**：`LlmProvider` trait + 4 provider + `LlmRouter` 必须先实现，M2（AI 调优）依赖 M1 提供 LlmProvider 用于 LLM 增强建议
2. **M2（AI 调优）依赖 M1**：`AutoTuningPipeline` 的 Advise 阶段可选调用 `LlmRouter` 增强，M1 必须先于 M2
3. **M3/M4/M5/M6/M9 相互独立可并行**：混合搜索/数据 lineage/分片 rebalance/failover/服务网格互不依赖，可并行开发
4. **M7（CDC）先于 M8（GraphQL）**：GraphQL Subscription 需要 CDC ChangeEvent 作为数据源，M7 必须先于 M8
5. **M8（GraphQL）依赖 M7**：`AsyncGraphqlBridge` 的 Subscription 基于 CDC `CdcCapturer` 推送数据变更
6. **M10 必须最后执行**：14 道门禁最终验证依赖所有里程碑集成测试完成，文档同步与版本号更新依赖门禁通过

---

# 十三、验收标准汇总

## 13.1 多 LLM 模型支持（M1，P0）

| 任务 | 对应需求 | 验收标准（节选） | 验证方法 |
|------|---------|----------------|---------|
| M1-T1 | — | multi-llm feature gate 搭建，默认 feature 行为不变 | `cargo check` + `--all-features` 编译通过 |
| M1-T2 | REQ-V40-002 | LlmProvider trait + LlmConfig + LlmError 完整可用 | `cargo test --features multi-llm` 通过 |
| M1-T3 | REQ-V40-002 | ClaudeProvider + GeminiProvider 实现 LlmProvider trait | 单元测试验证 provider_name + API 调用结构 |
| M1-T4 | REQ-V40-002 | LocalLlamaProvider + OpenAIProvider 实现，既有 with_llm 不变 | 单元测试验证既有 with_llm 行为不变 |
| M1-T5 | REQ-V40-002 | LlmRouter 热切换 + 按能力路由 + fallback | 运行时切换验证 + fallback 验证 |
| M1-T6 | — | M1 集成测试与门禁验证 | M1 相关门禁全部通过 |

## 13.2 AI 自动调优闭环（M2，P0）

| 任务 | 对应需求 | 验收标准（节选） | 验证方法 |
|------|---------|----------------|---------|
| M2-T1 | — | ai-auto-tuning feature gate 搭建 | `cargo check` 编译通过 |
| M2-T2 | REQ-V40-001 | AutoTuningConfig + TuningSuggestion + AutoTuningReport 数据结构 | `cargo test --features ai-auto-tuning` 通过 |
| M2-T3 | REQ-V40-001 | SlowQueryDetector 复用 ExplainPlanParser 识别慢查询 | EXPLAIN 解析验证全表扫描/索引缺失 |
| M2-T4 | REQ-V40-001 | AutoTuningPipeline 四阶段闭环 + 回归回滚 | 四阶段报告验证 + 回归自动回滚验证 |
| M2-T5 | — | M2 集成测试与门禁验证 | M2 相关门禁全部通过 |

## 13.3 混合搜索（M3，P1）

| 任务 | 对应需求 | 验收标准（节选） | 验证方法 |
|------|---------|----------------|---------|
| M3-T1 | — | hybrid-search feature gate 搭建 | `cargo check` 编译通过 |
| M3-T2 | REQ-V40-003 | HybridSearcher + HybridQuery + FusionStrategy 数据结构 | `cargo test --features hybrid-search` 通过 |
| M3-T3 | REQ-V40-003 | 三源并行查询 ≤200ms + 部分降级 | 并行查询验证 + 降级验证 |
| M3-T4 | REQ-V40-003 | RRF/加权/级联融合 + 结构化过滤下推 | RRF 公式验证 + 下推验证 |
| M3-T5 | — | M3 集成测试与门禁验证 | M3 相关门禁全部通过 |

## 13.4 数据 lineage（M4，P1）

| 任务 | 对应需求 | 验收标准（节选） | 验证方法 |
|------|---------|----------------|---------|
| M4-T1 | — | data-lineage feature gate 搭建 | `cargo check` 编译通过 |
| M4-T2 | REQ-V40-004 | LineageGraph DAG + 环路检测 | 环路验证返回 CycleDetected |
| M4-T3 | REQ-V40-004 | SQL 解析提取表/字段依赖 | INSERT/CREATE VIEW 解析验证 |
| M4-T4 | REQ-V40-004 | LineageTracker + 影响分析 + 溯源分析 | impact_analysis + origin_analysis 验证 |
| M4-T5 | REQ-V40-004 | 导出 DOT/JSON/GraphML | Graphviz 渲染验证 |
| M4-T6 | — | M4 集成测试与门禁验证 | M4 相关门禁全部通过 |

## 13.5 分片 rebalance（M5，P1）

| 任务 | 对应需求 | 验收标准（节选） | 验证方法 |
|------|---------|----------------|---------|
| M5-T1 | — | shard-rebalance feature gate 搭建 | `cargo check` 编译通过 |
| M5-T2 | REQ-V40-005 | RebalancePlan + RebalanceProgress 数据结构 | `cargo test --features shard-rebalance` 通过 |
| M5-T3 | REQ-V40-005 | 最小搬迁计划计算 | 一致性哈希 3→4 验证仅搬迁相邻区间 |
| M5-T4 | REQ-V40-005 | 双写 + 影子读 + 断点续传 + 进度可观测 | 迁移中查询不中断 + 断点续传验证 |
| M5-T5 | — | M5 集成测试与门禁验证 | M5 相关门禁全部通过 |

## 13.6 failover 自动化（M6，P1）

| 任务 | 对应需求 | 验收标准（节选） | 验证方法 |
|------|---------|----------------|---------|
| M6-T1 | — | auto-failover feature gate 搭建 | `cargo check` 编译通过 |
| M6-T2 | REQ-V40-006 | FailoverConfig + FailoverEvent 数据结构 | `cargo test --features auto-failover` 通过 |
| M6-T3 | REQ-V40-006 | 自动检测 + slave 提升 + 30s 切换 | 主库故障验证自动 failover + 30s 切换 |
| M6-T4 | REQ-V40-006 | 脑裂检测 | 双主验证旧主降级 |
| M6-T5 | — | M6 集成测试与门禁验证 | M6 相关门禁全部通过 |

## 13.7 CDC 变更数据捕获（M7，P2）

| 任务 | 对应需求 | 验收标准（节选） | 验证方法 |
|------|---------|----------------|---------|
| M7-T1 | — | cdc feature gate 搭建 | `cargo check` 编译通过 |
| M7-T2 | REQ-V40-009 | ChangeEvent + CdcConfig + CdcCheckpoint 数据结构 | `cargo test --features cdc` 通过 |
| M7-T3 | REQ-V40-009 | PostgreSQL WAL 捕获 | UPDATE 验证 ChangeEvent 捕获 |
| M7-T4 | REQ-V40-009 | MySQL/SQLite/Oracle/MSSQL 四方言捕获 | 五方言捕获器 dialect() 验证 |
| M7-T5 | REQ-V40-009 | Exactly-Once 去重 + 断点续传 | 同事务重发验证去重 + 重启验证续传 |
| M7-T6 | REQ-V40-009 | 下游分发 + ChangeEvent 脱敏 | Kafka+webhook 分发验证 + 脱敏验证 |
| M7-T7 | — | CdcCapturer 编排 + M7 集成测试 | M7 相关门禁全部通过 |

## 13.8 GraphQL 深度集成（M8，P2）

| 任务 | 对应需求 | 验收标准（节选） | 验证方法 |
|------|---------|----------------|---------|
| M8-T1 | — | async-graphql-integration feature gate 搭建 | `cargo check` 编译通过 |
| M8-T2 | REQ-V40-008 | AsyncGraphqlBridge + DataLoader 复用消除 N+1 | 关联查询验证 DataLoader 批量加载 |
| M8-T3 | REQ-V40-008 | Subscription 基于 CDC ChangeEvent | 订阅 userUpdated 验证推送 |
| M8-T4 | REQ-V40-008 | Relay 分页复用 keyset_after | users(first:10, after:"cursor") 验证 |
| M8-T5 | REQ-V40-008 | Federation 联邦 schema | 两子服务合并验证跨服务查询 |
| M8-T6 | REQ-V40-008 | 工单化错误处理 | 错误含 code/category/ticket_id 验证 |
| M8-T7 | — | M8 集成测试与门禁验证 | M8 相关门禁全部通过 |

## 13.9 服务网格集成（M9，P2）

| 任务 | 对应需求 | 验收标准（节选） | 验证方法 |
|------|---------|----------------|---------|
| M9-T1 | — | service-mesh feature gate 搭建 | `cargo check` 编译通过 |
| M9-T2 | REQ-V40-007 | ServiceMeshAdapter trait + MeshConfig 数据结构 | `cargo test --features service-mesh` 通过 |
| M9-T3 | REQ-V40-007 | Istio/Linkerd adapter + mTLS STRICT + 流量治理 | CRD 生成验证 + 金丝雀路由验证 |
| M9-T4 | REQ-V40-007 | 可观测性接入复用 MetricsRegistry + OTLP | metrics/traces 接入验证 |
| M9-T5 | — | M9 集成测试与门禁验证 | M9 相关门禁全部通过 |

## 13.10 最终验证（M10）

| 任务 | 对应需求 | 验收标准（节选） | 验证方法 |
|------|---------|----------------|---------|
| M10-T1 | 全局 | 14 道门禁全部通过 | 运行 14 道门禁脚本 |
| M10-T2 | 全局 | 文档同步，版本号更新，sz-pay 兼容 | 文档一致性检查 + sz-pay 兼容性验证 |
| M10-T3 | 全局 | 9 个 feature gate 逐步启用计划 | feature 独立/组合/全启用编译验证 |

## 13.11 全局验收条件

1. **API 兼容性**：v4.0.0 既有公开 API 完全向后兼容，sz-pay 既有代码不受影响（sz-pay 不启用 v4.0.0 新 feature，行为与 v3.9.0 一致）
2. **feature gate 隔离**：所有新能力通过 feature gate 隔离（`ai-auto-tuning` / `multi-llm` / `hybrid-search` / `data-lineage` / `shard-rebalance` / `auto-failover` / `service-mesh` / `async-graphql-integration` / `cdc`），默认 feature 行为不变
3. **测试基线不回退**：v3.9.0 已验收测试基线（6760+ passed）不回退，v4.0.0 仅增不减
4. **五方言一致**：新增能力在 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言上行为一致（AI 调优/failover/CDC 按方言能力适配）
5. **审计证据**：每项需求结论附 file:line 证据，遵循 AGENTS.md 审计合规铁律（`bash scripts/audit-verify.sh <审计报告.md>` 验证通过）
6. **14 道门禁通过**：v4.0.0 须通过 AGENTS.md 定义的 14 道门禁
7. **unsafe 零容忍**：所有新增代码无 `unsafe` 块，或必须有 `// SAFETY:` 注释
8. **禁止占位实现**：所有新增代码无 `todo!`/`unimplemented!`/`unreachable!`
9. **复用优先**：优先复用既有能力，不重复实现（AI 调优复用 UnifiedQueryOptimizer/IndexAdvisor/RewriteAdvisor，混合搜索复用 PgVectorStore/ES provider，CDC 复用 sz-orm-queue，GraphQL 复用 DataLoader，failover 复用 HealthChecker/ReadWriteRouter，rebalance 复用 ShardingRouter，服务网格复用 MetricsRegistry/OTLP）

---

# 十四、已验证的 file:line 代码证据清单

> 本清单所有 file:line 引用均来自 spec.md/design.md 既有代码证据（非编造），已通过源码读取验证（2026-08-11），遵循 AGENTS.md 审计合规铁律。

## 14.1 REQ-V40-001 AI 自动调优闭环

| 代码位置 | 内容 | 来源 |
|---------|------|------|
| `packages/sz-orm-ai/src/query_plan_optimizer.rs:515` | `UnifiedQueryOptimizer`（统一优化器，rule + llm hint 聚合） | spec.md `:27` / design.md `:21` |
| `packages/sz-orm-ai/src/query_plan_optimizer.rs:177` | `OptimizerConfig`（优化器配置） | spec.md `:27` / design.md `:22` |
| `packages/sz-orm-ai/src/query_plan_optimizer.rs:207` | `with_llm`（OpenAI 兼容构造） | spec.md `:27` / design.md `:23` |
| `packages/sz-orm-ai/src/index_advisor.rs:100` | `IndexAdvisor`（索引建议器） | spec.md `:27` / design.md `:24` |
| `packages/sz-orm-ai/src/rewrite_advisor.rs:89` | `RewriteAdvisor`（重写建议器） | spec.md `:27` / design.md `:25` |
| `packages/sz-orm-ai/src/explain_parser.rs:50` | `ExplainPlanParser` trait（5 方言解析器） | spec.md `:7` / design.md `:26` |

## 14.2 REQ-V40-002 多 LLM 模型支持

| 代码位置 | 内容 | 来源 |
|---------|------|------|
| `packages/sz-orm-ai/src/query_plan_optimizer.rs:177` | `OptimizerConfig`（既有配置） | spec.md `:27` / design.md `:27` |
| `packages/sz-orm-ai/src/query_plan_optimizer.rs:207` | `with_llm`（既有 OpenAI 兼容，包装为 OpenAIProvider） | spec.md `:27` / design.md `:28` |
| `packages/sz-orm-ai/src/real_embedding.rs` | 既有 OpenAI 兼容 embedding | spec.md `:27` / design.md `:29` |
| `packages/sz-orm-ai/Cargo.toml:13-24` | 既有 feature gate 体系（real/llm-optimizer/plan-cache 等） | design.md `:30` |

## 14.3 REQ-V40-003 混合搜索

| 代码位置 | 内容 | 来源 |
|---------|------|------|
| `packages/sz-orm-vector/src/lib.rs:189` | `PgVectorStore` trait（pgvector 向量存储） | spec.md `:28` / design.md `:29` |
| `packages/sz-orm-vector/src/lib.rs:113` | `SearchResult`（向量搜索结果） | spec.md `:28` / design.md `:30` |
| `packages/sz-orm-vector/src/lib.rs:145` | `VectorMetric`（Cosine/Euclidean/DotProduct） | spec.md `:28` / design.md `:31` |
| `packages/sz-orm-search/src/elasticsearch_provider.rs` | ES 全文搜索 provider | spec.md `:28` / design.md `:32` |
| `packages/sz-orm-search/src/opensearch_provider.rs` | OpenSearch 全文搜索 provider | spec.md `:28` / design.md `:33` |
| `packages/sz-orm-search/src/meilisearch_provider.rs` | Meilisearch 全文搜索 provider | spec.md `:28` / design.md `:34` |

## 14.4 REQ-V40-004 数据 lineage

| 代码位置 | 内容 | 来源 |
|---------|------|------|
| `packages/sz-orm-audit/src/lib.rs:691` | `HashChainEntry`（哈希链审计条目） | spec.md `:29` / design.md `:35` |
| `packages/sz-orm-audit/src/lib.rs:778` | `HashChainAuditor`（哈希链审计器） | spec.md `:29` / design.md `:36` |
| `packages/sz-orm-audit/src/lib.rs:862` | `verify()`（审计链验证） | spec.md `:29` / design.md `:37` |

## 14.5 REQ-V40-005 分片 rebalance

| 代码位置 | 内容 | 来源 |
|---------|------|------|
| `packages/sz-orm-sharding/src/lib.rs:60` | `ShardingStrategy`（分片策略枚举） | spec.md `:30` / design.md `:38` |
| `packages/sz-orm-sharding/src/lib.rs:130` | `ShardingRouter`（分片路由器） | spec.md `:30` / design.md `:39` |
| `packages/sz-orm-sharding/src/enhanced.rs` | 增强分片能力 | spec.md `:30` / design.md `:40` |
| `packages/sz-orm-sharding/src/cross_shard_tx.rs` | 跨分片事务协调 | spec.md `:30` / design.md `:41` |

## 14.6 REQ-V40-006 failover 自动化

| 代码位置 | 内容 | 来源 |
|---------|------|------|
| `packages/sz-orm-rw/src/lib.rs:37` | `SlaveHealth`（Healthy/Unhealthy/Drained） | spec.md `:31` / design.md `:42` |
| `packages/sz-orm-rw/src/lib.rs:219` | `HealthChecker`（健康检查器） | spec.md `:31` / design.md `:43` |
| `packages/sz-orm-rw/src/lib.rs:331` | `ReadWriteRouter`（读写分离路由器） | spec.md `:31` / design.md `:44` |
| `packages/sz-orm-rw/src/lib.rs:911`&quot; | `test_router_failover_to_master_when_all_unhealthy`（既有手动 failover 测试） | spec.md `:31` / design.md `:45` |

## 14.7 REQ-V40-007 服务网格集成

| 代码位置 | 内容 | 来源 |
|---------|------|------|
| `packages/sz-orm-observability/src/lib.rs:250` | `MetricsRegistry`（Prometheus Counter/Gauge/Histogram） | spec.md `:32` / design.md `:46` |
| `packages/sz-orm-observability/src/lib.rs:443` | `MetricsAccessControl`（metrics ACL） | spec.md `:32` / design.md `:47` |
| `packages/sz-orm-tracing/src/lib.rs` | `sz-orm-tracing`（OTLP 分布式追踪） | spec.md `:32` / design.md `:49` |
| `packages/sz-orm-limit/src/lib.rs` | `sz-orm-limit`（限流熔断器） | spec.md `:32` / design.md `:48` |

## 14.8 REQ-V40-008 GraphQL 深度集成

| 代码位置 | 内容 | 来源 |
|---------|------|------|
| `packages/sz-orm-graphql/src/lib.rs:36` | `GraphQLSchema`（types/queries/mutations） | spec.md `:33` / design.md `:50` |
| `packages/sz-orm-graphql/src/lib.rs:182` | `GraphQLServer`（含 `#[cfg(feature="real")]` async-graphql dynamic::Schema） | spec.md `:33` / design.md `:51` |
| `packages/sz-orm-graphql/src/dataloader.rs:74` | `BatchLoader<K, V>` trait | spec.md `:33` / design.md `:52` |
| `packages/sz-orm-graphql/src/dataloader.rs:89` | `DataLoader<K, V>`（N+1 消除） | spec.md `:33` / design.md `:53` |
| `packages/sz-orm-graphql/Cargo.toml:31` | `async-graphql = { version = "7", optional = true }` | spec.md `:33` / design.md `:54` |
| `packages/sz-orm-core/src/query.rs:986` | `QueryBuilder::keyset_after`（cursor-based 分页） | spec.md `:33` / design.md `:65` |

## 14.9 REQ-V40-009 CDC 变更数据捕获

| 代码位置 | 内容 | 来源 |
|---------|------|------|
| `packages/sz-orm-queue/src/real_kafka.rs` | Kafka 消息队列 provider | spec.md `:34` / design.md `:58` |
| `packages/sz-orm-queue/src/real_nats.rs` | NATS 消息队列 provider | spec.md `:34` / design.md `:59` |
| `packages/sz-orm-queue/src/real_pulsar.rs` | Pulsar 消息队列 provider | spec.md `:34` / design.md `:60` |
| `packages/sz-orm-queue/src/real_activemq.rs` | ActiveMQ 消息队列 provider | spec.md `:34` / design.md `:61` |
| `packages/sz-orm-queue/src/lapin_rabbitmq.rs` | RabbitMQ 消息队列 provider | spec.md `:34` / design.md `:62` |
| `packages/sz-orm-queue/src/rocketmq.rs` | RocketMQ 消息队列 provider | spec.md `:34` / design.md `:63` |
| `packages/sz-orm-audit/src/lib.rs` | SQL 审计日志（可作 CDC 数据源） | spec.md `:34` / design.md `:64` |

## 14.10 feature gate 体系

| 代码位置 | 内容 | 来源 |
|---------|------|------|
| `packages/sz-orm-core/Cargo.toml:85-115` | prod-ready 14 子 feature + 总 feature 聚合 | spec.md `:38` / design.md `:66` |
| `packages/sz-orm-ai/Cargo.toml:13-24` | sz-orm-ai 既有 feature gate（real/llm-optimizer/plan-cache 等） | design.md `:30` |
| `packages/sz-orm-graphql/Cargo.toml:15` | sz-orm-graphql `real` feature（含 async-graphql） | design.md `:54` |

---

> 本任务规划文档遵循 AGENTS.md 审计合规铁律，所有 file:line 引用均来自 spec.md/design.md 既有代码证据（非编造），已通过源码读取验证（2026-08-11）。任务按里程碑 M1-M10 组织，对应 design.md 第二章依赖关系（M1 多 LLM（P0 基座）→ M2 AI 调优（P0 依赖 M1）→ M3-M6（P1 可并行）→ M7 CDC（P2 先于 GraphQL）→ M8 GraphQL（P2 依赖 M7）→ M9 服务网格（P2 独立）→ M10 最终验证）。每个任务含 ID、描述、涉及文件、复用标注、子任务、验收标准、依赖关系。后续由 spec-implementation-agent 按任务顺序编码实现，每项实施后运行 14 道门禁确保不回退。
