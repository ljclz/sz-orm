# SZ-ORM `performance` Feature 实测验证报告

> 验证日期：2026-08-19
> 验证环境：Windows MSVC，x86_64，AVX2 可用
> 验证方法：`tests/performance_validation.rs`，直接对比标量路径 vs 优化路径耗时
> 代码证据：[simd.rs:92](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/simd.rs#L92)

---

## 1. 验证结论

**`performance` feature 此前从未被实际验证。** 经实测发现 SIMD 路径在所有基准中均比标量路径慢（反优化），已修复。

### 修复前（SIMD 路径 vs 标量路径）

| 基准 | n | 标量耗时 | SIMD 耗时 | 加速比 | 结论 |
|------|---|----------|-----------|--------|------|
| `compare_eq` | 50,000 | 386μs | 1,865μs | **0.21x** | SIMD 慢 4.8 倍 |
| `decode_integers` | 50,000 | 1,179μs | 1,467μs | **0.80x** | SIMD 慢 1.25 倍 |
| `compare_in` | 50,000 (set=500) | 155ms | 775ms | **0.20x** | SIMD 慢 5.0 倍 |

### 修复后（优化路径 vs 标量路径）

| 基准 | n | 标量耗时 | 优化耗时 | 加速比 | 结论 |
|------|---|----------|-----------|--------|------|
| `compare_eq` | 50,000 | 294μs | 249μs | **1.18x** | 持平（编译器自动向量化） |
| `decode_integers` | 50,000 | 1,061μs | 942μs | **1.13x** | 持平（编译器自动向量化） |
| `compare_in` | 50,000 (set=500) | 179ms | 10ms | **17.66x** | HashSet 碾压线性扫描 |

> 数据来源：`cargo test -p sz-orm-core --test performance_validation --features simd -- --nocapture` 实测输出

---

## 2. 根因分析

### 2.1 `simd_compare_eq`（原慢 4.8x）

[simd.rs:178](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/simd.rs#L178)（已移除）

**根因**：编译器对标量 `values.iter().map(|&v| v == target).collect()` 已自动向量化（auto-vectorization），生成 SSE2/AVX2 指令。显式 `wide::i64x4` 代码在自动向量化基础上增加了寄存器加载/提取开销，净效果为负。

**修复**：移除显式 SIMD 路径，始终使用标量路径（编译器自动向量化更优）。

### 2.2 `simd_decode_integers`（原慢 1.25x）

**根因**：同上。标量 `i64::from_le_bytes` 循环已被编译器自动向量化。

**修复**：移除显式 SIMD 路径。

### 2.3 `simd_compare_in`（原慢 5.0x → 修复后快 17.66x）

[simd.rs:218](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/simd.rs#L218)

**根因**：算法复杂度为 O(n × set_size)，SIMD 只加速最内层比较，外层嵌套循环不变。对 set=500，每个 chunk 要做 500 次 SIMD 比较，开销巨大。

**修复**：当 `set.len() >= 8` 时使用 `HashSet<i64>` 做 O(1) 查找，复杂度从 O(n × k) 降为 O(n)。这是算法级优化，远超 SIMD 向量化收益。

```rust
pub fn batch_compare_in(values: &[i64], set: &[i64], _avail: SimdAvailability) -> Vec<bool> {
    if set.len() >= 8 {
        let hash_set: std::collections::HashSet<i64> = set.iter().copied().collect();
        values.iter().map(|&v| hash_set.contains(&v)).collect()
    } else {
        scalar_compare_in(values, set)
    }
}
```

---

## 3. 修复清单

| 文件 | 修改 | 证据 |
|------|------|------|
| [simd.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/simd.rs) | 移除 3 个 `simd_*` 函数（显式 SIMD 路径），`batch_compare_in` 改用 HashSet | 第 92, 130, 155 行 |
| [Cargo.toml](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/Cargo.toml) | `simd` feature 改为空 gate（不再依赖 `wide` crate） | 第 34 行 |
| [Cargo.toml](file:///E:/vue/test/鲜视达/rust/sz-orm/Cargo.toml) | 移除 workspace `wide = "0.7"` 依赖 | 第 66 行 |
| [performance_validation.rs](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/tests/performance_validation.rs) | 新增性能验证测试（6 个测试） | 新文件 |

---

## 4. 验证命令

```bash
# 运行性能验证测试
cargo test -p sz-orm-core --test performance_validation --features simd -- --nocapture

# 运行 SIMD 单元测试
cargo test -p sz-orm-core --lib simd::tests --features simd

# 运行全量测试（确认无回归）
cargo test -p sz-orm-core --lib --features simd
```

---

## 5. 总结

**`performance` feature gate 此前从未被实际验证过。** 经实测发现：

1. **SIMD 路径是反优化**：在所有基准中均比标量路径慢（0.20x~0.80x），根因是编译器已对标量循环自动向量化，显式 SIMD 代码只增加寄存器开销。
2. **`compare_in` 的正确优化方向是 HashSet**：将 O(n×k) 降为 O(n)，实测获得 **17.66x 加速**。
3. **已移除 `wide` crate 依赖**：减少构建依赖，`simd` feature 保留用于 `detect()` CPU 检测。
4. **`performance` feature 现在提供真实价值**：`batch_compare_in` 的 HashSet 优化是可测量的性能改进。
