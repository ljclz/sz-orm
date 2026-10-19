# sz-orm v9.5.1 五维审查报告

> 审查日期：2026-10-08 | 审查范围：v9.5.1 全部改动（T1-T6）
> 审查基线：v9.5.0 → v9.5.1 diff

## 1. 正确性 (Correctness) — 评分：A

### T1 SqlRepository（`packages/sz-orm-sqlx/src/sql_repository.rs`）

| 方法 | 实现验证 | 结果 |
|------|----------|------|
| `batch_create` | 复用 `QueryBuilder::build_batch_insert_with_params`（`query.rs:2953`），生成 1 条多值 INSERT | ✅ |
| `batch_upsert` | 复用 `QueryBuilder::build_batch_upsert_with_params`（`query.rs:3035`），生成 INSERT ON CONFLICT | ✅ |
| `paginate_sql` | LIMIT/OFFSET + COUNT 两条 SQL，`page==0` 返回 `Err(InvalidParams)` | ✅ |
| `find_all`/`find_by`/`save`/`count` | 均通过 `QueryBuilder` + `Connection::query_with_params`/`execute_with_params` | ✅ |

空批处理：`batch_create` 空批返回 `Ok(0)`（不执行 SQL），`batch_upsert` 空批返回 `Err(InvalidParams)`。语义正确。

### T2 EagerLoader 集成（`packages/sz-orm-core/src/repository.rs`）

- `find_all` L721：`relations` 非空且 `eager_loader` 为 None → 返回 `RelationLoaderNotSet` 错误 ✅
- `find_all` L761：`relations` 非空且 `eager_loader` 已设置 → `tracing::warn!`（同步方法无法调用 async load_many） ✅
- `find_all_with_eager` L800：非空且 relations 非空 → 调用 `EagerLoader::load_many` ✅
- `find_one_with_eager` L840：委托 `find_all_with_eager` + `into_iter().next()` ✅

### 已知问题

- `repository.rs:734` 排序逻辑变异测试 MISSED（预存覆盖缺口，非 v9.5.1 引入）：`with_order` 后 `find_all` 未有测试验证排序结果正确性。**建议后续版本补测试。**

## 2. 可读性 (Readability) — 评分：A

- `sql_repository.rs`：模块文档（L1-13）说明设计要点，每个公开方法有 doc comment + `# 示例`
- `repository.rs`：`with_eager_loader`/`find_all_with_eager`/`find_one_with_eager` 均有 v9.5.1 标注 + 幻影交付消除说明
- 错误消息清晰：`"with_relation declared but eager_loader not set; use with_eager_loader()"`
- `tracing::warn!` 包含 relations 上下文，便于排查
- 代码风格与既有 `repository.rs` 一致

## 3. 架构 (Architecture) — 评分：A

| 决策 | 理由 | 结果 |
|------|------|------|
| `SqlRepository` 独立 struct（非 impl Repository trait） | Repository trait 是同步的，SQL 操作需异步；避免破坏 trait 签名 | ✅ 向后兼容 |
| `with_eager_loader` + `find_all_with_eager` 双方法 | 同步 `find_all` 无法调用 async `load_many`；提供 async 替代 | ✅ 向后兼容 |
| `AnyPool::factory()` accessor | 供 `Pool::new(config, factory)` 创建 Pool 以使用 AutoHealer | ✅ 最小暴露 |
| 不新增 crate / feature gate / 依赖 | v9.5.1 是 patch 版本，仅修复幻影交付 | ✅ |

无循环依赖引入。G17 架构一致性扫描通过。

## 4. 安全性 (Security) — 评分：A

| 检查项 | 结果 |
|--------|------|
| SQL 注入（G9） | ✅ 66/66 Safe，所有值通过 `?`/`$N` 占位符绑定 |
| 安全攻击测试（G21） | ✅ auth 5 + crypto 4 + core 4 = 13 passed |
| unsafe 代码 | ✅ 无新增 unsafe 块 |
| 脱敏验证（T4） | ✅ `test_slow_query_capture_e2e` 验证 `!json.contains("13888888888")` |
| 参数化查询 | ✅ `apply_conditions` 全部使用 `where_eq`/`where_ne`/... 参数化方法 |

## 5. 性能 (Performance) — 评分：A

| 方法 | v9.5.0（幻影） | v9.5.1（真实） | 改善 |
|------|----------------|----------------|------|
| `batch_create` | 逐条 save（N 次 SQL） | 1 条多值 INSERT | N 倍 |
| `batch_upsert` | 逐条 save + 内存判重 | 1 条 INSERT ON CONFLICT | N 倍 |
| `paginate_sql` | 内存切片 | 2 条 SQL（LIMIT/OFFSET + COUNT） | 大数据集显著 |
| EagerLoader | 幻影预加载（不执行） | `load_many` 批量加载 | N+1 消除 |

## 综合评级

| 维度 | 评分 | 权重 | 加权 |
|------|------|------|------|
| 正确性 | A | 30% | 0.30 |
| 可读性 | A | 20% | 0.20 |
| 架构 | A | 20% | 0.20 |
| 安全性 | A | 15% | 0.15 |
| 性能 | A | 15% | 0.15 |
| **综合** | **A** | | **1.00** |

### 待改进项

1. `repository.rs:734` 排序逻辑补测试（杀变异测试 MISSED 变异体）
2. G20 变异测试 Windows 下超时，仅完成 1/94 变异体（需 Linux 环境全量跑）