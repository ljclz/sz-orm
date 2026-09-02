# sz-orm v5.x → v6.0.0 迁移指南

## 概述

v6.0.0 新增 5 个 AI 方向包，全部以 feature gate 形式提供，默认关闭，对现有代码零影响。

## 新增包

| 包名 | feature gate | 功能 |
|------|-------------|------|
| sz-orm-agent | `agent` | AI Agent 自主运维（感知-决策-执行循环） |
| sz-orm-governance | `governance` | AI 数据治理（血缘/合规/质量） |
| sz-orm-nl-query | `nl-query` | NL 查询闭环（NL→SQL→执行→可视化→洞察） |
| sz-orm-model-ops | `model-ops` | 模型微调本地化（llama.cpp/vLLM） |
| sz-orm-multimodal | `multimodal` | 多模态交互（语音/图表/截图/草图） |

## 迁移步骤

### 1. 更新依赖

```toml
[dependencies]
sz-orm-core = "6.0.0"
sz-orm-sqlx = "6.0.0"
# 按需启用 AI 功能
sz-orm-nl-query = { version = "6.0.0", optional = true }
sz-orm-agent = { version = "6.0.0", optional = true }
```

### 2. 启用 feature gate

```toml
[features]
ai-nl-query = ["dep:sz-orm-nl-query", "sz-orm-nl-query/nl-query"]
ai-agent = ["dep:sz-orm-agent", "sz-orm-agent/agent"]
```

### 3. 使用 NL 查询

```rust
use sz_orm_nl_query::pipeline::NlQueryPipeline;

let pipeline = NlQueryPipeline::new();
let response = pipeline.query("查询所有用户").await?;
// response.sql, response.rows, response.insight
```

### 4. 注入 LLM Generator

```rust
use sz_orm_nl_query::llm_generator::LlmNl2SqlGenerator;
use sz_orm_ai::llm_provider::OpenAIProvider;

let provider = Arc::new(OpenAIProvider::new(...));
let generator = LlmNl2SqlGenerator::new(provider);
let pipeline = NlQueryPipeline::new().with_llm_generator(Arc::new(generator));
```

### 5. 使用 Agent

```rust
use sz_orm_agent::planner::react::ReActPlanner;

let planner = ReActPlanner::new(); // 规则模式
let planner = ReActPlanner::with_provider(provider); // LLM 模式
```

## Breaking Change

v6.0.0 无 Breaking Change。所有新功能通过 feature gate 提供，默认关闭。

## 性能指标

- NL→SQL 规则转换：< 1μs（criterion 基准测试验证）
- 端到端查询（无执行器）：< 1μs
- 端到端查询（含 MySQL 执行）：取决于数据库延迟