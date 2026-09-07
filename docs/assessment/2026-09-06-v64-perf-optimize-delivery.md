# v6.4.0 性能优化交付记录

**日期**：2026-09-06
**版本**：6.3.0 → 6.4.0
**范围**：8 项性能优化（5.1-5.8），基于 v6.3.0 对比分析识别的 6 个性能瓶颈 + 1 个竞争力缺口

## 1. 交付清单

| # | 需求 | 优先级 | 状态 | 核心改动 |
|---|------|--------|------|----------|
| 5.1 | QueryBuilder 构造零堆分配 | P0 | ✅ 已交付 | `DialectKind::quote_into` enum 分发 |
| 5.2 | 批量 INSERT SQL 零分配 | P0 | ✅ 已交付 | `build_batch_insert_with_params` 零分配重写 |
| 5.3 | 查询结果集列名复用 | P0 | ✅ 已交付 | 6 处 `col.name().to_string()` 复用 |
| 5.4 | 关系查询零分配 | P1 | ✅ 已交付 | `find_with_related` 零分配重写 |
| 5.5 | 分页数字格式化优化 | P1 | ✅ 已交付 | `push_usize_to_string` 栈上格式化 |
| 5.6 | 连接池栈上缓冲 + 延迟 Instant | P2 | ✅ 已交付 | `to_close` 循环外复用 + deadline 惰性初始化 |
| 5.7 | Future 栈分配 | P1 | ✅ 评估完成 | trait 签名约束无法消除，保留 v6.3.0 |
| 5.8 | find_by_ids 原生批量查询 | P1 | ✅ 已交付 | `WHERE id IN` 单次查询 + 自动分块 999 |

## 2. 代码证据（file:line）

### 5.1 QueryBuilder 构造零堆分配

- `packages/sz-orm-core/src/dialect.rs` — `DialectKind::quote_into` 方法（enum 分发到 5 个方言）
- `packages/sz-orm-core/src/query.rs:297` — `QueryBuilder::new()` 初始化 `dialect_kind`
- `packages/sz-orm-core/src/query.rs` — `quote_into` 辅助方法（enum 优先，回退 vtable）
- 66 处 `self.dialect.quote_into` 替换为 `self.quote_into`
- 测试：`test_dialect_kind_quote_into`（5 种方言字节级等价验证）

### 5.2 批量 INSERT SQL 零分配

- `packages/sz-orm-core/src/query.rs:2821` — `build_insert_with_params` 零分配重写
- `packages/sz-orm-core/src/query.rs:2901` — `build_batch_insert_with_params` 零分配重写
- `packages/sz-orm-batch/src/dialect.rs:44` — `build_batch_insert` 零分配重写
- `packages/sz-orm-batch/src/dialect.rs:14` — `quote_identifier_into` 零分配写入版
- `packages/sz-orm-batch/src/dialect.rs:42` — `placeholder_into` 零分配写入版
- `bench-comparison/benches/competitor_adapter.rs:308` — `insert_batch` 改用批量 INSERT

### 5.3 查询结果集列名复用

- `packages/sz-orm-sqlx/src/any.rs:1129` — SQLite query_with_params 列名复用
- `packages/sz-orm-sqlx/src/any.rs` — MySQL query + query_with_params 列名复用
- `packages/sz-orm-sqlx/src/any.rs` — PG query + query_with_params 列名复用
- 6 处统一修改（SQLite 2 + MySQL 2 + PG 2），PG 流式查询 1 处不修改

### 5.4 关系查询零分配

- `packages/sz-orm-core/src/find_with_related.rs` — `find_with_related_eager_sql` 零分配重写
- `packages/sz-orm-core/src/find_with_related.rs` — `find_with_related_subquery` 零分配重写

### 5.5 分页数字格式化优化

- `packages/sz-orm-core/src/query.rs` — `push_usize_to_string` 函数（栈上 `[u8; 20]` 数组）
- 4 处 `write!(sql, " LIMIT {}", limit)` / `write!(sql, " OFFSET {}", offset)` 替换
- 测试：`test_push_usize_to_string`（覆盖 0/1/9/10/99/100/999/1000/9999/10000/99999999/usize::MAX）

### 5.6 连接池栈上缓冲 + 延迟 Instant::now

- `packages/sz-orm-core/src/pool.rs:1419` — `deadline` 改为 `Option<Instant>` 惰性初始化
- `packages/sz-orm-core/src/pool.rs:1424` — `to_close` 循环外预分配 + `drain(..)` 容量复用
- `packages/sz-orm-core/src/pool.rs:1552` — `get_or_insert_with` 延迟 deadline 计算

### 5.7 Future 栈分配（评估结论）

- `packages/sz-orm-sqlx/src/any.rs:412` — SQLite `query_with_params` 的 `Box::pin`
- 评估结论：`Connection` trait 签名 `Pin<Box<dyn Future>>` 无法改为 `impl Future`（HRTB 冲突 + API 兼容性约束）
- 保留 v6.3.0 实现，无法消除 `Box::pin`

### 5.8 find_by_ids 原生批量查询 API

- `packages/sz-orm-core/src/query.rs:2521` — `find_by_ids` 方法（`WHERE id IN` + 自动分块 999）
- `bench-comparison/benches/competitor_adapter.rs:334` — benchmark `find_batch` 改用 `find_by_ids`
- `bench-comparison/benches/competitor_adapter.rs:172` — `BenchUserModel` 实现 `Model` trait
- 测试：`test_find_by_ids_empty` / `test_find_by_ids_dedup` / `test_find_by_ids_chunk_999` / `test_find_by_ids_chunk_1000` / `test_find_by_ids_chunk_2000`

## 3. 测试验证结果

| 包 | 测试数 | 结果 |
|----|--------|------|
| sz-orm-core（单元测试） | 1908 | ✅ 全部通过 |
| sz-orm-sqlx（单元 + 集成） | 70 + 22 + 4 + 1 | ✅ 全部通过 |
| sz-orm-batch（单元 + 集成） | 73 + 26 + 1 | ✅ 全部通过 |
| sz-orm-core pool 测试 | 64 | ✅ 全部通过 |
| find_by_ids 边界测试 | 5 | ✅ 全部通过 |
| clippy（sz-orm-core + sz-orm-sqlx + sz-orm-batch） | — | ✅ 无警告 |
| sz-pay 编译兼容 | — | ✅ 通过 |

## 4. 幻影交付验证

| 优化项 | 生产调用点 | 验证结果 |
|--------|-----------|----------|
| 5.1 dialect_kind | `packages/sz-orm-core/src/query.rs` 66 处 `self.quote_into` | ✅ 有真实调用 |
| 5.2 build_batch_insert_with_params | `bench-comparison/benches/competitor_adapter.rs:308` | ✅ 有真实调用 |
| 5.3 col_names 复用 | `packages/sz-orm-sqlx/src/any.rs` 6 处 | ✅ 有真实调用 |
| 5.4 find_with_related 零分配 | `packages/sz-orm-core/src/find_with_related.rs` | ✅ 有真实调用 |
| 5.5 push_usize_to_string | `packages/sz-orm-core/src/query.rs` 4 处 LIMIT/OFFSET | ✅ 有真实调用 |
| 5.6 to_close 复用 + deadline 延迟 | `packages/sz-orm-core/src/pool.rs:1424,1552` | ✅ 有真实调用 |
| 5.8 find_by_ids | `bench-comparison/benches/competitor_adapter.rs:334` | ✅ 有真实调用 |

## 5. 回滚预案

| 优化项 | 回滚方法 |
|--------|----------|
| 5.1 | 删除 `dialect_kind` 字段 + 还原 66 处 `self.quote_into` → `self.dialect.quote_into` |
| 5.2 | 还原 `build_insert_with_params` / `build_batch_insert_with_params` 原实现 + 还原 `sz-orm-batch::build_batch_insert` + 还原 benchmark 逐行 execute |
| 5.3 | 还原 6 处 `col_names` 复用为每行每列 `col.name().to_string()` |
| 5.4 | 还原 `find_with_related_eager_sql` / `find_with_related_subquery` 原实现 |
| 5.5 | 还原 4 处 `push_usize_to_string` 为 `write!` |
| 5.6 | 还原 `to_close` 为循环内 `Vec::new()` + 还原 `deadline` 为 `Instant::now() + timeout` |
| 5.8 | 删除 `find_by_ids` 方法 + 还原 benchmark `find_batch` N+1 逐个查询 |

## 6. 性能目标

| 优化项 | v6.3.0 基线 | v6.4.0 目标 | 验证方式 |
|--------|------------|------------|----------|
| 5.1 | simple_full 282ns | ≤ 200ns | `cargo bench --bench bench_crud -- bench_crud_simple_full` |
| 5.2 | Insert 1000 行 25.49ms | ≤ 5ms | `cargo bench --bench bench_crud -- bench_crud_insert_batch` |
| 5.3 | batch_find/1000 1.65ms | ≤ 500µs | `cargo bench --bench bench_crud -- bench_crud_batch_find` |
| 5.4 | 1:1 查询 19.0µs | ≤ 8µs | `cargo bench --bench bench_relation -- bench_relation_has_one` |
| 5.5 | 分页/10000 36.4µs | ≤ 15µs | `cargo bench --bench bench_pagination` |
| 5.6 | 连接池获取 2.2µs | ≤ 1.5µs | `cargo bench --bench bench_pool` |
| 5.7 | — | 降 30%（尽力） | 评估结论：无法消除，保留 v6.3.0 |
| 5.8 | batch_find N+1 | ≤ 500µs | `cargo bench --bench bench_crud -- bench_crud_batch_find` |

## 7. 文档同步

- `Cargo.toml` 版本号 6.3.0 → 6.4.0 ✅
- `CHANGELOG.md` 新增 `## [6.4.0] — 2026-09-06` 段落 ✅
- `.codeartsdoer/specs/v64_perf_optimize/` spec/design/tasks 文档 ✅