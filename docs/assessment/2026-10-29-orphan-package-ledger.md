# 孤儿包治理清单（2026-10-29）

> 依据：`docs/assessment/2026-10-28-architecture-roadmap-review.md` P2「17 个孤儿包逐个裁定」。
> 口径：遍历 `packages/sz-orm-*/Cargo.toml`，统计每个 `sz-orm-*` 包被其他 workspace 包依赖（含 dev/optional）的次数，**无任何依赖方**的包列入本清单（本口径 30 个，架构评测的 17 个为其更严格口径，本清单按类型分组便于裁定）。

## 一、独立交付物（非孤儿，建议保留）

这些包是面向外部用户的独立产物，不需要被其他 workspace 包依赖，**不构成架构债**：

| 包 | 类型 |
|----|------|
| sz-orm-js / sz-orm-python / sz-orm-go / sz-orm-cpp / sz-orm-java / sz-orm-wasm | 跨语言绑定层 |
| sz-orm-bench / sz-orm-studio / sz-orm-lsp | 独立工具（基准/Studio/LSP） |

## 二、宣称能力的扩展包（需裁定）

这些包宣称提供扩展能力，但仓库内无下游依赖方，属于"提供组件但未接线"的候选：

| 包 | 宣称能力 | 裁定建议 |
|----|----------|----------|
| sz-orm-actix / sz-orm-axum | Web 框架集成 | 提供示例应用接线，或标注"需手动接入" |
| sz-orm-advisor | 性能顾问 | 接入 sz-orm-bench 或 CLI |
| sz-orm-agent / sz-orm-governance / sz-orm-multimodal / sz-orm-mcp | AI 智能体/治理/多模态/MCP | 在 README 明确"独立可选包，需手动接入"，或提供示例 |
| sz-orm-back / sz-orm-es | 备份/ES 搜索 | 提供接线示例（sz-orm-search 已接 ES？复核） |
| sz-orm-batch / sz-orm-parallel / sz-orm-stream | 批处理/并行/流 | 提供示例或接入 core 测试 |
| sz-orm-lc / sz-orm-model-ops | 负载控制/模型运维 | 评估是否并入相关包 |
| sz-orm-mqtt / sz-orm-scheduler / sz-orm-sharding / sz-orm-storage / sz-orm-swagger | MQTT/调度/分片/存储/Swagger | 各包登记为"独立可选（需手动接入）"，或用 example 接线 |

## 三、处置原则（对齐架构评测）

1. **非孤儿**（第一类 9 包）：在架构台账中标注"独立交付物，非孤儿"，不做改动。
2. **扩展包**：逐包裁定 —— 有真实示例/文档即保留为"可选组件"；无任何调用与测试的评估删除或合并。
3. 裁定结果登记到 `AGENTS.md` 或架构台账，避免重复评估。

*本清单为治理输入，不修改任何代码。*