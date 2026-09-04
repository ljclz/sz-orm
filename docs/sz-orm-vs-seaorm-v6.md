# sz-orm vs SeaORM 2.0 能力对照表（v6.0.0 基准）

> 生成日期：2026-09-04
> sz-orm 版本：v6.0.0（tag v6.0.0）
> SeaORM 版本：2.0.2（2026-08-12 发布）

## 对照表

| 能力 | SeaORM 2.0 | sz-orm v6.0.0 | 证据 |
|------|-----------|---------------|------|
| 异步查询 | ✅ 基于 sqlx | ✅ 基于 sqlx + 自研池 | `pool.rs` 自研池 |
| Streaming 查询 | ✅ `find_stream` | ✅ `stream_query` | `sz-orm-stream/src/result_set.rs:54` |
| 连接池指标 | ✅ sqlx pool stats | ✅ `pool_metrics` + `metrics_snapshot_json` | `sz-orm-core/src/pool.rs:1617` |
| 多租户 | ❌ 手写 | ✅ multi-tenant-enhanced (RLS/配额/审计) | `tenant_quota_rls.rs` |
| 信创方言 | ❌ 无 | ✅ 6 种 (达梦/Kingbase/OceanBase/PolarDB/GaussDB/GBase) | `db_type.rs:36-50` |
| AI NL2SQL | ❌ 无 | ✅ NlQueryPipeline + LlmNl2SqlGenerator | `pipeline.rs:115` + `llm_generator.rs:22` |
| AI Agent | ❌ 无 | ✅ ReActPlanner + PlanAndExecutePlanner | `react.rs:15` + `plan_execute.rs:20` |
| MCP Server | ❌ 无 | ✅ sz-orm-mcp (nl_query + execute_sql 工具) | `sz-orm-mcp/src/server.rs:73` |
| 多模态 | ❌ 无 | ✅ 语音/图表/截图/草图/CV | `sz-orm-multimodal/src/` |
| 数据治理 | ❌ 无 | ✅ 血缘/合规/质量规则 | `sz-orm-governance/src/` |
| 编译期 SQL 验证 | ✅ sqlx query! 宏 | ✅ sz-orm-macros query! 宏 (db-verify) | `sz-orm-macros/` |
| 多语言绑定 | ❌ 无 | ✅ C/Java/Go/C++/Python/JS | `sz-orm-cabi/java/go/cpp/python/js` |
| 23 道门禁 | ❌ 无 | ✅ 幻影交付/语义反模式/架构一致性等 | `scripts/check-*.py` |

## 结论

sz-orm v6.0.0 在 SeaORM 2.0 的核心能力（异步查询、streaming、池指标）上已对齐，
在 AI/信创/多租户/多语言绑定/工程门禁上全面领先。

SeaORM 2.0 的优势在于社区成熟度和文档质量，sz-orm 需在 v6.x 持续补强。