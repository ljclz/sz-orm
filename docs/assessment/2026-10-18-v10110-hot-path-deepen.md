# v10.11.0 方向 1：性能热点路径深化交付记录

- 版本：10.11.0
- 日期：2026-10-18
- 任务范围：T1.1–T1.5
- 代码证据：所有 file:line 引用均经 `grep -n` 验证真实存在

## 1. 交付概要

| 子任务 | 内容 | 新增类型/函数 | 测试数 | feature gate |
|--------|------|--------------|--------|-------------|
| T1.1 | SIMD 热路径内联展开深化 | 34 个 `#[inline]` + `#[cold]` 冷分支分离 | 12 | query-simd, perf-extreme |
| T1.2 | 编译期查询特化 | `QueryShape<const COLS, const PARAMS>` | 3 | perf-extreme |
| T1.3 | 零拷贝行解码深化 | 三协议零拷贝解码函数 | 6 | zero-copy, zero-copy-deep |
| T1.4 | 连接池快速路径深化 | `CachePadded<T>` + `FastPathStats` | 4 | perf-extreme |
| T1.5 | 缓存亲和性优化 | `CacheLayoutOptimizer` | 9 | cache-hit-optimize |
| **合计** | | | **25** | 5 个既有 gate 复用 |

## 2. 详细交付

### T1.1 SIMD 热路径内联展开深化

- 文件：`packages/sz-orm-core/src/simd.rs`
- 改动：24 个热路径函数添加 `#[inline]`，冷分支添加 `#[cold]` 分离
- 证据：
  - `packages/sz-orm-core/src/simd.rs:104` — 首个 `#[inline]` 注解
  - 全文件共 34 处 `#[inline]`（`grep -c "#\[inline\]" simd.rs` = 34）
- 测试：`packages/sz-orm-core/tests/hot_path_deepen_wiring_test.rs`（12 个测试，含 SIMD 内联验证、冷分支分离验证、查询特化验证）

### T1.2 编译期查询特化

- 文件：`packages/sz-orm-core/src/query_specialize.rs`（新增）
- 核心类型：`QueryShape<const COLS: usize, const PARAMS: usize>` — const generics 零运行时开销
- 证据：`packages/sz-orm-core/src/query_specialize.rs:45` — `pub struct QueryShape<const COLS: usize, const PARAMS: usize>`
- 模块声明：`packages/sz-orm-core/src/lib.rs` — `pub mod query_specialize;`
- 测试：`packages/sz-orm-core/tests/hot_path_deepen_wiring_test.rs`（3 个查询特化测试）

### T1.3 零拷贝行解码深化

- 文件：`packages/sz-orm-core/src/zero_copy_pipeline.rs`
- 新增函数：
  - `packages/sz-orm-core/src/zero_copy_pipeline.rs:664` — `mysql_row_decode_zero_copy`
  - `packages/sz-orm-core/src/zero_copy_pipeline.rs:704` — `pg_binary_decode_zero_copy`
  - `packages/sz-orm-core/src/zero_copy_pipeline.rs:749` — `oracle_tns_decode_zero_copy`
- 测试：`packages/sz-orm-core/tests/hot_path_deepen_wiring_test.rs`（6 个零拷贝解码测试，覆盖 MySQL/PostgreSQL/Oracle 三协议）

### T1.4 连接池快速路径深化

- 文件：`packages/sz-orm-core/src/pool.rs`
- 新增类型：
  - `packages/sz-orm-core/src/pool.rs:37` — `pub struct CachePadded<T>` — 64B 缓存行对齐避免 false sharing
  - `packages/sz-orm-core/src/pool.rs:66` — `pub struct FastPathStats` — 取/还连接分离统计
- 测试：`packages/sz-orm-core/tests/pool_fast_path_wiring_test.rs`（4 个测试，验证 CachePadded 对齐和 FastPathStats 统计准确性）

### T1.5 缓存亲和性优化

- 文件：`packages/sz-orm-core/src/cache_warmup_protection.rs`
- 新增类型：`packages/sz-orm-core/src/cache_warmup_protection.rs:592` — `pub struct CacheLayoutOptimizer`
- 功能：缓存布局优化器，支持 pad/align/预取策略
- 测试：`packages/sz-orm-core/tests/cache_affinity_wiring_test.rs`（9 个测试，覆盖布局优化、预取策略、亲和性计算）

## 3. feature gate 复用

| gate | 用途 | 是否新增 |
|------|------|---------|
| query-simd | SIMD 内联展开 | 否（既有） |
| perf-extreme | 编译期特化 + 池快速路径 | 否（既有） |
| zero-copy | 零拷贝解码 | 否（既有） |
| zero-copy-deep | 零拷贝深化 | 否（既有） |
| cache-hit-optimize | 缓存亲和性 | 否（既有） |

## 4. 向后兼容性

- 无破坏性变更：所有新增类型为新增 pub struct，不修改既有公共 API 签名
- `QueryShape` 的 const generics 字段不支持 serde derive（Rust serde 不支持 const generics 数组），已移除 serde derive，不影响序列化兼容性

## 5. 验证结果

- `cargo test -p sz-orm-core --test hot_path_deepen_wiring_test`：12 passed
- `cargo test -p sz-orm-core --test pool_fast_path_wiring_test`：4 passed
- `cargo test -p sz-orm-core --test cache_affinity_wiring_test`：9 passed
- 合计：**25 passed / 0 failed**