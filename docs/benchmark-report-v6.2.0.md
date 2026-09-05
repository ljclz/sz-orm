# sz-orm v6.2.0 基准对比报告

> 自动生成，所有数字来自 criterion JSON，禁止手写。

## 对比表

| 场景 | sz-orm P50 | sz-orm P99 | sz-orm 吞吐量 | SeaORM P50 | SeaORM P99 | SeaORM 吞吐量 | SQLx P50 | SQLx P99 | SQLx 吞吐量 |
|------|-----------|-----------|-------------|-----------|-----------|-------------|---------|---------|------------|
| build_batch_insert_1000/base | 445.8 μs | N/A | 2.2 K ops/s | N/A | N/A | N/A | 445.8 μs | N/A | 2.2 K ops/s |
| build_batch_insert_1000/new | 445.8 μs | N/A | 2.2 K ops/s | N/A | N/A | N/A | 445.8 μs | N/A | 2.2 K ops/s |
| build_batch_upsert_1000/base | 407.4 μs | N/A | 2.4 K ops/s | N/A | N/A | N/A | 407.4 μs | N/A | 2.4 K ops/s |
| build_batch_upsert_1000/new | 407.4 μs | N/A | 2.4 K ops/s | N/A | N/A | N/A | 407.4 μs | N/A | 2.4 K ops/s |
| pool_reuse_rate_steady/base | 1.06 ms | N/A | 853 ops/s | N/A | N/A | N/A | 1.06 ms | N/A | 853 ops/s |
| pool_reuse_rate_steady/new | 1.06 ms | N/A | 853 ops/s | N/A | N/A | N/A | 1.06 ms | N/A | 853 ops/s |
| pool_steady/pool_steady_acquire_release | 1.1 μs | N/A | 905.7 K ops/s | N/A | N/A | N/A | 1.1 μs | N/A | 905.7 K ops/s |
| select_simple/base | 415 ns | N/A | 2.39 M ops/s | N/A | N/A | N/A | 415 ns | N/A | 2.39 M ops/s |
| select_simple/new | 415 ns | N/A | 2.39 M ops/s | N/A | N/A | N/A | 415 ns | N/A | 2.39 M ops/s |
| select_with_join/base | 1.2 μs | N/A | 797.5 K ops/s | N/A | N/A | N/A | 1.2 μs | N/A | 797.5 K ops/s |
| select_with_join/new | 1.2 μs | N/A | 797.5 K ops/s | N/A | N/A | N/A | 1.2 μs | N/A | 797.5 K ops/s |
| select_with_where/base | 1.2 μs | N/A | 793.3 K ops/s | N/A | N/A | N/A | 1.2 μs | N/A | 793.3 K ops/s |
| select_with_where/new | 1.2 μs | N/A | 793.3 K ops/s | N/A | N/A | N/A | 1.2 μs | N/A | 793.3 K ops/s |
| stream_real_backpressure/base | 2.05 ms | N/A | 500 ops/s | N/A | N/A | N/A | 2.05 ms | N/A | 500 ops/s |
| stream_real_backpressure/new | 2.05 ms | N/A | 500 ops/s | N/A | N/A | N/A | 2.05 ms | N/A | 500 ops/s |
| v62_build_select_with_params/build_select_with_params_complex | 5.2 μs | N/A | 189.4 K ops/s | N/A | N/A | N/A | 5.2 μs | N/A | 189.4 K ops/s |
| v62_build_select_with_params/build_select_with_params_simple | 1.2 μs | N/A | 790.0 K ops/s | N/A | N/A | N/A | 1.2 μs | N/A | 790.0 K ops/s |
| v62_sqlx_alignment/sqlx_pool_acquire | 21.5 μs | N/A | 45.2 K ops/s | N/A | N/A | N/A | 21.5 μs | N/A | 45.2 K ops/s |
| v62_sqlx_alignment/sqlx_query_build | 249 ns | N/A | 4.20 M ops/s | N/A | N/A | N/A | 249 ns | N/A | 4.20 M ops/s |
| v62_stream_real/stream_real_batch_1000 | 47.30 ms | N/A | 17 ops/s | N/A | N/A | N/A | 47.30 ms | N/A | 17 ops/s |

## 红线指标

- 池 acquire P99 ≤ 10 μs
- 池复用率 ≥ 90%
- SQL 构建 ≥ 100,000 ops/s
- 流式拉取 ≥ 50,000 elements/s
- 基准回归退化 ≤ 10%
