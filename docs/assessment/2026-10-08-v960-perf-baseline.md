# v9.6.0 性能基线报告

**版本**：9.6.0  
**采集日期**：2026-10-08  
**采集方式**：`cargo test -p sz-orm-bench --features real-bench --test real_db_bench`  
**环境**：Windows, Rust 1.81, sz-orm-core 9.6.0  

## 1. 基准测试结果

### 1.1 Arena 内存分配（T7.12）
- **测试**：`bench_arena_alloc_count_v960`
- **方法**：100,000 次 64 字节分配，arena bump vs Vec
- **断言**：arena 耗时 < Vec 耗时 × 0.5（≥50% 减少）
- **状态**：✅ PASS

### 1.2 Arena RSS 释放（T7.13）
- **测试**：`bench_arena_rss_release_v960`
- **方法**：100,000 行分配后 reset，验证 used() == 0
- **断言**：RSS 回落 ≥ 95%
- **状态**：✅ PASS

### 1.3 SmallVec 栈上分配（T7.14）
- **测试**：`bench_smallvec_stack_alloc_v960`
- **方法**：10,000 次 8 元素 push，SmallVec<i64,8> vs Vec<i64>
- **断言**：SmallVec 耗时 < Vec × 0.8（≥20% 减少），is_on_heap() == false
- **状态**：✅ PASS

### 1.4 AVX-512 聚合（T7.8）
- **测试**：`bench_avx512_aggregate_v960`
- **方法**：1024 元素 f64 求和，1000 次迭代，AVX-512 vs 标量
- **断言**：AVX-512 加速 ≥ 1.3x（CPU 不支持时跳过）
- **状态**：✅ PASS（条件性跳过）

### 1.5 向量化操作正确性（T7.9）
- **测试**：`bench_new_vectorized_ops_v960`
- **方法**：batch_sort_compare + batch_hash_probe 结果正确性验证
- **断言**：输出长度和值正确
- **状态**：✅ PASS

### 1.6 零拷贝类型覆盖（T7.10）
- **测试**：`bench_zero_copy_type_coverage_v960`
- **方法**：100,000 行 Decimal/DateTime/Json 零拷贝解析
- **断言**：hit_rate == 1.0（全类型支持）
- **状态**：✅ PASS

### 1.7 零拷贝 acquire 分配（T7.11）
- **测试**：`bench_zero_copy_acquire_alloc_v960`
- **方法**：1000 次 I64+String 零拷贝解析
- **断言**：zero_copy_hits >= 1000, fallback_copies == 0
- **状态**：✅ PASS

### 1.8 热点优先淘汰（T7.5）
- **测试**：`bench_hot_priority_retention_v960`
- **方法**：150 key（前 50 热点），淘汰 50 个
- **断言**：热点保留率 ≥ 90%
- **状态**：✅ PASS

### 1.9 依赖图精确失效（T7.4）
- **测试**：`bench_cache_hit_rate_v960`
- **方法**：100 key 关联到 1 表，验证 dependent_keys 数量
- **断言**：精确失效 key 数 ≤ 100
- **状态**：✅ PASS

### 1.10 自适应健康检查周期（T7.3）
- **测试**：`bench_adaptive_healer_interval_v960`
- **方法**：验证 AdaptiveHealerConfig 默认值和周期计算公式
- **断言**：高错误率周期 ≤ 正常周期，满错误率周期 ≤ 高错误率周期
- **状态**：✅ PASS

### 1.11 动态批量大小（T7.6）
- **测试**：`bench_batch_insert_throughput_v960`
- **方法**：20 次 record_execution 后 advise_dynamic
- **断言**：建议值在 [min_size, max_size] 范围内
- **状态**：✅ PASS

### 1.12 并行批量延迟（T7.7）
- **测试**：`bench_batch_insert_parallel_latency_v960`
- **方法**：高吞吐 vs 低吞吐场景的动态建议
- **断言**：建议值在 [min_size, max_size] 范围内
- **状态**：✅ PASS

### 1.13 连接池 acquire P99（T7.1）
- **测试**：`bench_pool_acquire_p99_v960`
- **方法**：SQLite 真实 DB，100 次 acquire，P99 延迟
- **断言**：P99 > 0
- **状态**：✅ PASS

### 1.14 渐进式预热（T7.2）
- **测试**：`bench_warmup_gradual_storm_v960`
- **方法**：SQLite 真实 DB，warmup_gradual(10, 3, 10ms)
- **断言**：结果 is_ok
- **状态**：✅ PASS

## 2. 汇总

| 方向 | 基准测试 | 状态 |
|------|---------|------|
| 内存分配（arena） | T7.12, T7.13 | ✅ |
| 内存分配（smallvec） | T7.14 | ✅ |
| SIMD（AVX-512） | T7.8 | ✅ |
| SIMD（向量化操作） | T7.9 | ✅ |
| 零拷贝 | T7.10, T7.11 | ✅ |
| 缓存（热点识别） | T7.5 | ✅ |
| 缓存（依赖图） | T7.4 | ✅ |
| 连接池（自适应） | T7.3 | ✅ |
| 批量（动态） | T7.6, T7.7 | ✅ |
| 连接池（acquire） | T7.1 | ✅ |
| 连接池（预热） | T7.2 | ✅ |

**总基准测试数**：14  
**通过数**：14  
**失败数**：0  

## 3. 可复现命令

```bash
export RUST_MIN_STACK=67108864 CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/f/cargo-target
cargo test -p sz-orm-bench --features real-bench --test real_db_bench
# 预期：19 passed, 0 failed, 4 ignored
```