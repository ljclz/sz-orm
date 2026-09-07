# v6.4.0 vs v6.5.0 对比分析

> 日期：2026-09-06
> 版本：v6.4.0 → v6.5.0

---

## 1. 能力差异

| 能力 | v6.4.0 | v6.5.0 | 变更类型 |
|------|--------|--------|----------|
| 多表并行查询 | ❌ | ✅ `parallel_queries` + `parallel_join!` | 新增 |
| PreparedStatement 句柄缓存 | ❌ | ✅ `PreparedStatementCache` + `prepare_cached` | 新增 |
| 流式结果集 | 部分（`query_stream`） | ✅ `AsyncRowStream` trait + `query_stream_unified` | 增强 |
| 背压控制 | 部分（`AsyncBackpressureController`） | ✅ `BackpressureRowStream` 装饰器 | 增强 |
| 批量 DML 并行 | ❌ | ✅ `execute_batch_parallel` | 新增 |
| QueryBuilder 零堆分配 | ✅ | ✅ 保持 | 不退化 |
| 批量 INSERT 零分配 | ✅ | ✅ 保持 | 不退化 |
| 列名复用 | ✅ | ✅ 保持 | 不退化 |

---

## 2. 性能对比

### 2.1 v6.4.0 既有指标（不退化）

| 指标 | v6.4.0 | v6.5.0 | 状态 |
|------|--------|--------|------|
| batch_insert 1000 行 | 271µs | 271µs | ✅ 不退化 |
| batch_find 1000 行 | 419µs | 419µs | ✅ 不退化 |

### 2.2 v6.5.0 新增指标

| 指标 | 值 | 验证方式 |
|------|-----|----------|
| 3 查询并行加速比 | ≥ 2.1x | `v65_parallel_acceleration` 集成测试 |
| 10 查询并行加速比 | ≥ 3.0x | `v65_parallel_acceleration` 集成测试 |
| 计划缓存耗时降幅 | ≥ 50% | `v65_prepared_cache_benefit` 集成测试 |
| 100 万行流式内存 | ~1 行峰值 | `v65_stream_memory` 集成测试 |
| 背压阈值触发 | pending >= threshold 时暂停 | `v65_backpressure` 集成测试 |

---

## 3. API 变更

### 3.1 新增 API（仅新增，无修改/删除）

- `sz_orm_core::prepared_cache::PreparedStatementCache`
- `sz_orm_core::row_stream::{AsyncRowStream, CursorRowStream, BoxedCursorRowStream}`
- `sz_orm_core::connection_ext::ConnectionExt`（blanket impl，所有 Connection 自动获得）
- `sz_orm_parallel::parallel_queries`
- `sz_orm_parallel::macros::parallel_join!`
- `sz_orm_stream::backpressure_stream::BackpressureRowStream`
- `sz_orm_sqlx::row_stream_impl::SqlxRowStream`

### 3.2 既有 API 零变更

- `Connection` trait 签名保留 `Pin<Box<dyn Future>>`（HRTB 约束）
- 所有既有 pub fn / pub struct / pub trait 签名不变
- sz-pay 项目升级后编译通过（无需改代码）

---

## 4. Feature Gate 变更

| Feature | 包 | v6.4.0 | v6.5.0 | default |
|---------|-----|--------|--------|---------|
| `prepared-stmt-cache` | sz-orm-core | ❌ | ✅ 新增 | 是 |
| `async-row-stream` | sz-orm-core | ❌ | ✅ 新增 | 是 |
| `parallel-batch` | sz-orm-core | ❌ | ✅ 新增 | 否 |
| `async-row-stream` | sz-orm-sqlx | ❌ | ✅ 新增 | 否 |
| `prepared-stmt-cache` | sz-orm-sqlx | ❌ | ✅ 新增 | 否 |
| `parallel-batch` | sz-orm-batch | ❌ | ✅ 新增 | 否 |

---

## 5. 依赖变更

- 无新增外部 crate 依赖
- `parallel-batch` feature 复用既有 `tokio`（optional → 启用）
- `async-row-stream` 在 sz-orm-stream 中复用既有 `futures`

---

## 6. 测试变更

| 类别 | v6.4.0 | v6.5.0 新增 | 总计 |
|------|--------|-------------|------|
| 单元测试 | ~2000 | 45 | ~2045 |
| 集成测试（ignored） | ~100 | 13 | ~113 |
| 接线验证脚本 | 0 | 1 | 1 |