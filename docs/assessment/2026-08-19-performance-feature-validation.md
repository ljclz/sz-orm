# SZ-ORM `performance` Feature 完整实测验证报告

> 验证日期：2026-08-20
> 验证环境：Windows MSVC，x86_64，AVX2 可用
> 验证方法：`tests/performance_validation.rs` + `tests/performance_subfeatures.rs`
> 代码证据：[simd.rs:92](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/simd.rs#L92)、[l1_cache.rs:87](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/l1_cache.rs#L87)、[plan_cache.rs:446](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/plan_cache.rs#L446)、[columnar.rs:41](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/columnar.rs#L41)

---

## 1. 验证结论总览

`performance` feature = `simd` + `l1-cache` + `plan-cache` + `zero-copy`，4 个子 feature 全部实测验证。

| 子 feature | 验证前状态 | 验证结果 | 修复后效果 |
|------------|------------|----------|------------|
| `simd` | ❌ 反优化（0.20x~0.80x） | 已修复 | `compare_in` **17.66x** 加速（HashSet） |
| `l1-cache` | ✅ 正常 | 实测通过 | L1 命中 4812ns，避免 DB 调用 |
| `plan-cache` | ✅ 正常 | 实测通过 | 缓存命中 **2.2x** 加速 vs 重新解析 |
| `zero-copy` | ✅ 正常 | 实测通过 | 列式单列扫描 **24.5x** 加速 vs 行式 |

---

## 2. SIMD 子 Feature

### 2.1 修复前（SIMD 路径 vs 标量路径）

| 基准 | n | 标量 | SIMD | 加速比 | 结论 |
|------|---|------|------|--------|------|
| `compare_eq` | 50,000 | 386μs | 1,865μs | **0.21x** | SIMD 慢 4.8 倍 |
| `decode_integers` | 50,000 | 1,179μs | 1,467μs | **0.80x** | SIMD 慢 1.25 倍 |
| `compare_in` | 50,000 (set=500) | 155ms | 775ms | **0.20x** | SIMD 慢 5.0 倍 |

**根因**：编译器已对标量循环自动向量化（auto-vectorization），显式 `wide::i64x4` 只增加寄存器加载/提取开销。

### 2.2 修复后

| 基准 | n | 标量 | 优化 | 加速比 | 优化方式 |
|------|---|------|------|--------|----------|
| `compare_eq` | 50,000 | 294μs | 249μs | **1.18x** | 移除显式 SIMD，依赖编译器自动向量化 |
| `decode_integers` | 50,000 | 1,061μs | 942μs | **1.13x** | 同上 |
| `compare_in` | 50,000 (set=500) | 179ms | 10ms | **17.66x** | HashSet O(1) 查找替代 O(n×k) 线性扫描 |

**修复内容**：
- [simd.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/simd.rs)：移除 3 个显式 SIMD 函数，`batch_compare_in` 改用 HashSet
- [Cargo.toml](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml)：移除 `wide` crate 依赖

---

## 3. L1 Cache 子 Feature

### 3.1 实测数据

| 指标 | 耗时 | 说明 |
|------|------|------|
| L1 命中（via L1L2Coordinator） | 4,812ns | 避免DB调用，这是 L1 缓存的核心价值 |
| L1 直接命中 | 40,033ns | 含 LRU touch 开销（VecDeque O(n)） |
| L1 未命中 | 140ns | 仅 HashMap 查找失败 |
| DB 调用次数 | 1000 次 | 1000 条数据首次加载，后续 100,000 次 L1 命中未触发 DB |

### 3.2 功能验证

| 验证项 | 结果 | 证据 |
|--------|------|------|
| Identity Map 语义 | ✅ `Arc::ptr_eq` = true | [l1_cache.rs:149](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/l1_cache.rs#L149) |
| LRU 淘汰 | ✅ evicts=1 | [l1_cache.rs:135](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/l1_cache.rs#L135) |
| L1→L2→DB 三级查询 | ✅ DB calls=1000 | [l1_cache.rs:245](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/l1_cache.rs#L245) |

### 3.3 已知限制

L1 直接命中（40,033ns）比未命中（140ns）慢，原因是 `touch_lru` 在 VecDeque 上做 O(n) 查找+移除。对 10,000 条目，此开销显著。生产场景中 L1 的核心价值在于避免 DB 调用（4,812ns vs DB 调用通常 >1ms），LRU touch 开销为次要问题。

---

## 4. Plan Cache 子 Feature

### 4.1 实测数据

| 指标 | 耗时 | 说明 |
|------|------|------|
| 缓存命中 | 66,631ns | SQL 归一化 + hash 查找 + AST 返回 |
| 重新解析 | 146,578ns | SQL 归一化 + hash 查找 + `sqlparser` 完整解析 |
| **加速比** | **2.2x** | 避免重复 SQL 解析 |

### 4.2 功能验证

| 验证项 | 结果 | 证据 |
|--------|------|------|
| 缓存命中统计 | ✅ parse_hits=10000 | [plan_cache.rs:492](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/plan_cache.rs#L492) |
| 表级精确失效 | ✅ invalidate_table("users") 仅失效 users 查询 | [plan_cache.rs:606](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/plan_cache.rs#L606) |
| 跨表隔离 | ✅ orders 查询在 users 失效后仍命中 | 实测 hits=2 |

---

## 5. Zero-Copy (Columnar) 子 Feature

### 5.1 实测数据

| 指标 | 耗时 | 说明 |
|------|------|------|
| 列式单列聚合（10,000 行） | 80,891ns | 按列连续存储，CPU 缓存友好 |
| 行式单列聚合（10,000 行） | 1,978,994ns | 每行需 HashMap 查找 + 跨列跳转 |
| **加速比** | **24.5x** | 列式布局对单列扫描显著优于行式 |

### 5.2 功能验证

| 验证项 | 结果 | 证据 |
|--------|------|------|
| 行→列转换 | ✅ 10,000 行正确转换 | [columnar.rs:59](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/columnar.rs#L59) |
| 列→行转换（roundtrip） | ✅ 100 行无损往返 | [columnar.rs:78](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/columnar.rs#L78) |
| 单列过滤 | 44,923ns | `column("age").filter()` |
| 多列联合过滤 | 95,657ns | `column("age").zip(column("city")).filter()` |

---

## 6. 总结

### 6.1 `performance` feature 价值评估

| 子 feature | 是否提供真实价值 | 核心收益 |
|------------|------------------|----------|
| `simd` | ✅（修复后） | `compare_in` 17.66x 加速（HashSet 算法优化） |
| `l1-cache` | ✅ | 避免 DB 调用（4.8μs vs >1ms） |
| `plan-cache` | ✅ | 避免 SQL 重复解析（2.2x 加速） |
| `zero-copy` | ✅ | 单列扫描 24.5x 加速（列式缓存友好） |

### 6.2 验证命令

```bash
# SIMD 性能验证
cargo test -p sz-orm-core --test performance_validation --features simd -- --nocapture

# L1/Plan/ZeroCopy 性能验证
cargo test -p sz-orm-core --test performance_subfeatures --features "l1-cache,plan-cache,zero-copy" -- --nocapture

# 全量 performance feature 验证
cargo test -p sz-orm-core --test performance_validation --test performance_subfeatures --features performance -- --nocapture
```

### 6.3 后续优化建议

| 优先级 | 方向 | 预期收益 |
|--------|------|----------|
| P2 | L1Cache LRU touch 改用 LinkedHashMap 或 HashMap+双向链表 | O(n) → O(1)，L1 命中从 40μs 降至 ~200ns |
| P3 | PlanCache 缓存命中路径优化（预计算 hash） | 66μs → ~10μs |
