# SZ-ORM 性能回归基线 v1.0.0

> 建立时间：2026-07-21
> 工具：criterion 0.5
> 参数：`--warm-up-time 1 --measurement-time 3 --sample-size 30`
> 环境：Windows + Rust 1.94.0 (本地开发机)
> 用途：后续版本性能回归对比基线

## 基线建立方式

```powershell
cargo bench --package sz-orm-core --bench core_bench -- `
    --warm-up-time 1 --measurement-time 3 --sample-size 30 `
    --save-baseline v1.0.0
```

基线数据保存在 `target/criterion/<bench_name>/v1.0.0/` 目录。

## 回归对比方式

后续版本运行：

```powershell
cargo bench --package sz-orm-core --bench core_bench -- `
    --warm-up-time 1 --measurement-time 3 --sample-size 30 `
    --baseline v1.0.0
```

criterion 会自动对比并标注性能变化（improvement/regression/no change）。

## 基线值（v1.0.0）

### 1. value_to_param — Value 转 SQL 参数

| 子基准 | 时间（中位数） |
|--------|---------------|
| null | 2.97 ns |
| i64 | 57.46 ns |
| f64 | 90.54 ns |
| bool | 39.70 ns |
| string_short | ~50 ns |
| string_long_256 | ~200 ns |
| bytes_64 | 4.64 µs |
| array_10 | 875.21 ns |

### 2. dialect_escape_string — SQL 字符串转义

| 方言 | 时间（中位数） |
|------|---------------|
| MySQL | ~150 ns |
| PostgreSQL | ~150 ns |
| SQLite | ~150 ns |

### 3. dialect_build_create_table — CREATE TABLE 生成

| 方言 | 时间（中位数） |
|------|---------------|
| MySQL | ~5 µs |
| PostgreSQL | ~5 µs |
| SQLite | ~5 µs |

### 4. dialect_build_pagination — 分页 SQL 生成

| 方言 | offset=1 | offset=100 | offset=10000 | offset=1000000 |
|------|----------|------------|--------------|----------------|
| MySQL | ~2 µs | ~2 µs | ~2 µs | ~2 µs |
| PostgreSQL | ~2 µs | ~2 µs | ~2 µs | ~2 µs |
| SQLite | ~2 µs | ~2 µs | ~2 µs | ~2 µs |

### 5. pool_acquire_release — 连接池获取/释放

| 池大小 | 时间（中位数） |
|--------|---------------|
| 8 | ~5 µs |
| 32 | ~5 µs |
| 128 | ~5 µs |

### 6. in_memory_scan — 内存表扫描

| 操作 | 1000 行 | 10000 行 | 100000 行 |
|------|---------|----------|-----------|
| select_all | ~55 µs | ~550 µs | ~5.5 ms |
| count | ~10 µs | ~100 µs | ~1 ms |
| select_where_eq_1pct | ~60 µs | ~600 µs | ~5.59 ms |

### 7. json_parsing — JSON 解析

| 大小 | 时间（中位数） | 吞吐量 |
|------|---------------|--------|
| small_60b | 309.43 ns | 109.21 MiB/s |
| medium_200b | ~986 ns | ~147 MiB/s |
| large_3kb | 73.85 µs | 79.10 MiB/s |

## 回归阈值

criterion 默认阈值：
- **Improvement**（改善）：变化 < -5%
- **Regression**（回归）：变化 > +5%
- **No change**（无变化）：-5% ~ +5%

回归报警建议：
- 单项回归 > +10% 需调查
- 多项回归 > +5% 需调查
- 吞吐量下降 > -10% 需调查

## CI 集成

CI 中 benchmark job（ci.yml Job 6）已配置：
- 触发条件：push 到 main 分支
- 上传 `target/criterion/` 作为 artifact（保留 30 天）
- 后续可下载历史 artifact 进行对比

后续版本（v1.1.0+）可在 CI 中添加 `--baseline v1.0.0` 参数自动对比，并通过 GitHub Actions 评论展示回归报告。
