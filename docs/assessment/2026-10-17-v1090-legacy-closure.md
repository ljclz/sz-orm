# v10.9.0 方向1 遗留项闭环交付记录

> 版本：v10.9.0
> 日期：2026-10-17
> 方向：1 现有遗留项闭环
> 任务：T1.1 ~ T1.5

## 1. T1.1 deprecated 标记逐项评估（10 处）

扫描命令：`grep -rn "#\[deprecated" packages/ --include='*.rs'`
扫描结果：10 处（8 处实际属性 + 2 处文档注释）

### 1.1 逐项处置结论

| # | file:line | API | 调用点扫描 | 处置结论 | 迁移目标 |
|---|-----------|-----|-----------|---------|---------|
| 1 | [packages/sz-orm-fusion/src/executor.rs:24](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-fusion/src/executor.rs#L24) | `MemoryFusionCache` struct | sz-orm-fusion 内部（executor.rs:245/271/298 测试 + lib.rs:21/59 文档），无 sz-pay 使用 | 保留（向后兼容） | `TtlFusionCache`（db-fusion-v2 feature） |
| 2 | [packages/sz-orm-fusion/src/migration.rs:8](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-fusion/src/migration.rs#L8) | 文档注释提及 `#[deprecated]` | N/A（文档注释） | 保留（文档注释，已提供迁移指引） | N/A |
| 3 | [packages/sz-orm-model-ops/src/evaluator.rs:66](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-model-ops/src/evaluator.rs#L66) | `evaluate()` | sz-orm-model-ops 内部（evaluator.rs:309/329/348/385 + tests/evaluator.rs:33/56/76/93/134），无 sz-pay 使用 | 保留（向后兼容） | `evaluate_with_executor()` |
| 4 | [packages/sz-orm-query-builder/src/lib.rs:219](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-query-builder/src/lib.rs#L219) | 文档注释提及 `#[deprecated]` | N/A（文档注释） | 保留（文档注释） | N/A |
| 5 | [packages/sz-orm-query-builder/src/lib.rs:221](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-query-builder/src/lib.rs#L221) | `Query` struct | sz-pay 声明依赖（Cargo.toml:382）但未使用 `Query` struct | 保留（向后兼容） | `sz_orm_core::QueryBuilder<M>` |
| 6 | [packages/sz-orm-query-builder/src/lib.rs:230](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-query-builder/src/lib.rs#L230) | `Query::select()` | 同上 | 保留（向后兼容） | `QueryBuilder<M>::select()` |
| 7 | [packages/sz-orm-query-builder/src/lib.rs:239](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-query-builder/src/lib.rs#L239) | `Query::insert()` | 同上 | 保留（向后兼容） | `QueryBuilder<M>::insert()` |
| 8 | [packages/sz-orm-query-builder/src/lib.rs:248](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-query-builder/src/lib.rs#L248) | `Query::update()` | 同上 | 保留（向后兼容） | `QueryBuilder<M>::update()` |
| 9 | [packages/sz-orm-query-builder/src/lib.rs:257](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-query-builder/src/lib.rs#L257) | `Query::delete()` | 同上 | 保留（向后兼容） | `QueryBuilder<M>::delete()` |
| 10 | [packages/sz-orm-tracing/src/lib.rs:787](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-tracing/src/lib.rs#L787) | `url()` | 无调用点（全工作空间扫描） | 保留（向后兼容） | `identifier()` |

### 1.2 处置原则

- **全部保留**：10 处 deprecated API 全部保留，不移除
- **理由 1**：向后兼容——移除将导致下游项目编译失败
- **理由 2**：复杂 SQL 场景——UNION/CTE/窗口函数等 `QueryBuilder<M>` 不支持，`Query` 仍有存在价值
- **理由 3**：迁移指引已深化——`docs/query_builder_selection_guide.md` 第 6 节提供完整迁移步骤

### 1.3 验证

```bash
$ grep -rn "#\[deprecated" packages/ --include='*.rs' | wc -l
10  # 预期 10，实际 10 ✅
```

## 2. T1.2 TODO/FIXME 处置（2 处）

扫描命令：`grep -rn "TODO\|FIXME\|__SZORM_TODO__" packages/ --include='*.rs'`
扫描结果：2 处

### 2.1 逐项处置结论

| # | file:line | 内容 | 处置结论 |
|---|-----------|------|---------|
| 1 | [packages/sz-orm-lc/src/lib.rs:2238](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-lc/src/lib.rs#L2238) | `__SZORM_TODO__` 占位符断言 | **测试逻辑保留**：断言 `!code.contains("__SZORM_TODO__")` 确保生成代码不含此占位符，非待办事项 |
| 2 | [packages/sz-orm-vector/tests/ann_e2e.rs:51](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-vector/tests/ann_e2e.rs#L51) | 真实 pgvector HNSW 检索对比 TODO | **未来版本规划**：已更新注释标注 `TODO(v10.10.0)`，依赖 PostgreSQL + pgvector 扩展 + HNSW 索引 |

### 2.2 修改内容

`packages/sz-orm-vector/tests/ann_e2e.rs:51` 注释更新：
```rust
// 修改前：
// TODO: 真实 pgvector HNSW 检索对比
// 需要创建 RealPgVectorStore 并创建 HNSW 索引后执行 ann_search
// 验证两者结果集 ID 交集 ≥ 90%

// 修改后：
// TODO(v10.10.0): 真实 pgvector HNSW 检索对比
// 依赖：PostgreSQL + pgvector 扩展 + HNSW 索引（需 DATABASE_URL 环境变量）
// 计划：创建 RealPgVectorStore 并创建 HNSW 索引后执行 ann_search
// 验收：两者结果集 ID 交集 ≥ 90%
```

### 2.3 验证

```bash
$ cargo test -p sz-orm-vector --features ann-accel,real-pg --test ann_e2e
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out  ✅
```

## 3. T1.3 query-builder 迁移指引深化

### 3.1 修改文件

| 文件 | 修改内容 |
|------|---------|
| [docs/query_builder_selection_guide.md:147](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/query_builder_selection_guide.md#L147) | 新增第 6 节"详细迁移指南（v10.9.0 深化）"：deprecated API 清单 + SELECT/INSERT/UPDATE/DELETE 迁移示例 + 行为差异表 + feature gate 差异表 |
| [docs/qb-migration-roadmap.md:1](file:///E:/vue/test/鲜视达/rust/sz-orm/docs/qb-migration-roadmap.md#L1) | 更新版本 v3.6.0→v10.9.0，撤销 v3.7.0 移除计划，改为长期保留（向后兼容），新增 deprecated API 清单 |

### 3.2 迁移指引内容

- **6.1 deprecated API 清单与迁移路径**：5 处 API 的 file:line 和迁移目标
- **6.2 SELECT 迁移示例**：迁移前/后代码 + 行为差异表（6 维度）
- **6.3 INSERT 迁移示例**：迁移前/后代码
- **6.4 UPDATE 迁移示例**：迁移前/后代码
- **6.5 DELETE 迁移示例**：迁移前/后代码
- **6.6 迁移步骤清单**：6 步迁移流程
- **6.7 feature gate 差异**：4 个 feature 的差异对比

### 3.3 验证

```bash
$ grep -n "详细迁移指南" docs/query_builder_selection_guide.md
147:## 6. 详细迁移指南（v10.9.0 深化）  ✅

$ grep -n "deprecated API 清单" docs/qb-migration-roadmap.md
39:## 5. deprecated API 清单  ✅
```

## 4. T1.4 sz-pay 版本对齐 10.7.0→10.8.0

### 4.1 修改文件

| 文件 | 修改内容 |
|------|---------|
| `E:/vue/test/sz-pay/server/sz-rust/Cargo.toml` | 22 处 sz-orm-* 依赖版本 10.7.0 → 10.8.0 |

### 4.2 修改清单

22 处 sz-orm-* 依赖全部从 `version = "10.7.0"` 升级至 `version = "10.8.0"`：
- sz-orm-sqlx, sz-orm-core, sz-orm-graph, sz-orm-vector, sz-orm-audit, sz-orm-crypto
- sz-orm-masking, sz-orm-auth, sz-orm-ai, sz-orm-nl-query, sz-orm-agent, sz-orm-governance
- sz-orm-multimodal, sz-orm-mcp, sz-orm-queue, sz-orm-batch, sz-orm-observability, sz-orm-storage
- sz-orm-parallel, sz-orm-stream, sz-orm-advisor, sz-orm-dtx

### 4.3 验证

```bash
$ grep "sz-orm-" Cargo.toml | grep "10.7.0" | wc -l
0  # 预期 0 ✅

$ grep "sz-orm-" Cargo.toml | grep "10.8.0" | wc -l
22  # 预期 22 ✅

$ cargo check
Finished `dev` profile [unoptimized + debuginfo] target(s) in 31.44s  ✅

$ cargo test --lib
test result: ok. 5159 passed; 0 failed; 13 ignored  ✅
```

### 4.4 ADR-0001 合规

- 仅修改 sz-pay 自身 Cargo.toml，未修改上游 sz-orm 仓库文件 ✅

## 5. 总结

| 任务 | 状态 | 验证方法 | 结果 |
|------|------|---------|------|
| T1.1 deprecated 评估 | ✅ | `grep -rn "#\[deprecated" packages/` | 10 处全部保留+迁移指引 |
| T1.2 TODO 处置 | ✅ | `cargo test -p sz-orm-vector --features ann-accel,real-pg --test ann_e2e` | 0 failed, 2 ignored |
| T1.3 迁移指引深化 | ✅ | `grep -n "详细迁移指南" docs/query_builder_selection_guide.md` | 第 6 节存在 |
| T1.4 sz-pay 版本对齐 | ✅ | `cargo check && cargo test --lib` | 5159 passed, 0 failed |
| T1.5 交付记录 | ✅ | 本文档 | — |

**向后兼容性**：无破坏性变更，10 处 deprecated 全部保留，sz-pay 编译和测试通过。
**unsafe 零容忍**：无新增 unsafe 代码。
**不新增 crate/feature/依赖**：72 包不变。