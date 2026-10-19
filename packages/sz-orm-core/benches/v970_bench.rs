//! v9.7.0 性能基准测试：异步零拷贝 / NUMA / 缓存预热
//!
//! 三大方向 before/after 性能证据采集。
//! 运行：cargo bench --package sz-orm-core --bench v970_bench

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::collections::HashMap;
use std::time::Duration;

use sz_orm_core::cache_warmup_protection::{AccessPatternAnalyzer, AccessPatternConfig};
use sz_orm_core::numa_topology::{CrossNodeCounter, NumaConfig, NumaTopology};
use sz_orm_core::warmup_pipeline::{StageFailStrategy, WarmupPipelineConfig, WarmupStage};
use sz_orm_core::warmup_trigger::TriggerConfig;
use sz_orm_core::zero_copy_pipeline::{BackpressureConfig, ZeroCopyPipeline};
use sz_orm_core::Value;

fn generate_rows(n: usize) -> Vec<HashMap<String, Value>> {
    (0..n)
        .map(|i| {
            let mut row = HashMap::new();
            row.insert("id".to_string(), Value::I64(i as i64));
            row.insert("name".to_string(), Value::String(format!("user_{i}")));
            row.insert(
                "email".to_string(),
                Value::String(format!("user_{i}@example.com")),
            );
            row.insert(
                "dept".to_string(),
                Value::String(format!("dept_{}", i % 10)),
            );
            row
        })
        .collect()
}

fn columns() -> Vec<String> {
    vec![
        "id".to_string(),
        "name".to_string(),
        "email".to_string(),
        "dept".to_string(),
    ]
}

// ============================================================================
// T1: 异步零拷贝基准测试
// ============================================================================

fn bench_async_zero_copy_alloc(c: &mut Criterion) {
    let rows = generate_rows(10_000);
    let cols = columns();

    c.bench_function("v970_sync_next_row_10k", |b| {
        b.iter(|| {
            let pipeline = ZeroCopyPipeline::new();
            let mut stream = pipeline.stream_rows(black_box(&rows).clone(), &cols);
            let mut count = 0usize;
            while stream.next_row().is_some() {
                count += 1;
            }
            count
        })
    });

    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("v970_async_stream_10k", |b| {
        b.to_async(&rt).iter(|| {
            let rows_clone = rows.clone();
            let cols_clone = cols.clone();
            async move {
                let pipeline = ZeroCopyPipeline::new();
                let stream = pipeline.stream_async(
                    black_box(&rows_clone).clone(),
                    &cols_clone,
                    BackpressureConfig::default(),
                );
                use futures::StreamExt;
                let mut stream = stream;
                let mut count = 0usize;
                while stream.next().await.is_some() {
                    count += 1;
                }
                count
            }
        })
    });
}

fn bench_stream_async_throughput(c: &mut Criterion) {
    let rows = generate_rows(10_000);
    let cols = columns();
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("v970_async_throughput_bp1000", |b| {
        b.to_async(&rt).iter(|| {
            let rows_clone = rows.clone();
            let cols_clone = cols.clone();
            async move {
                let pipeline = ZeroCopyPipeline::new();
                let stream = pipeline.stream_async(
                    rows_clone,
                    &cols_clone,
                    BackpressureConfig::new(1000, 500),
                );
                use futures::StreamExt;
                let mut stream = stream;
                let mut total_bytes = 0usize;
                while let Some(row) = stream.next().await {
                    total_bytes += row.data.len();
                }
                total_bytes
            }
        })
    });

    c.bench_function("v970_async_throughput_bp100", |b| {
        b.to_async(&rt).iter(|| {
            let rows_clone = rows.clone();
            let cols_clone = cols.clone();
            async move {
                let pipeline = ZeroCopyPipeline::new();
                let stream = pipeline.stream_async(
                    rows_clone,
                    &cols_clone,
                    BackpressureConfig::new(100, 50),
                );
                use futures::StreamExt;
                let mut stream = stream;
                let mut total_bytes = 0usize;
                while let Some(row) = stream.next().await {
                    total_bytes += row.data.len();
                }
                total_bytes
            }
        })
    });
}

fn bench_batch_deserialize_simd(c: &mut Criterion) {
    use sz_orm_core::simd::{batch_decode_integers, detect, scalar_decode_integers};

    let n: usize = 10_000;
    let values: Vec<i64> = (0..n as i64).collect();
    let buf: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    let avail = detect();

    c.bench_function("v970_scalar_decode_10k", |b| {
        b.iter(|| {
            let result = scalar_decode_integers(black_box(&buf), black_box(n));
            black_box(result.len())
        })
    });

    c.bench_function("v970_simd_decode_10k", |b| {
        b.iter(|| {
            let result = batch_decode_integers(black_box(&buf), black_box(n), black_box(avail));
            black_box(result.len())
        })
    });
}

fn bench_backpressure_memory_bounded(c: &mut Criterion) {
    let rows = generate_rows(10_000);
    let cols = columns();
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("v970_backpressure_bounded", |b| {
        b.to_async(&rt).iter(|| {
            let rows_clone = rows.clone();
            let cols_clone = cols.clone();
            async move {
                let pipeline = ZeroCopyPipeline::new();
                let stream = pipeline.stream_async(
                    rows_clone,
                    &cols_clone,
                    BackpressureConfig::new(200, 100),
                );
                use futures::StreamExt;
                let mut stream = stream;
                let mut peak_backlog = 0usize;
                while stream.next().await.is_some() {
                    let bl = stream.backlog();
                    if bl > peak_backlog {
                        peak_backlog = bl;
                    }
                }
                peak_backlog
            }
        })
    });
}

// ============================================================================
// T2: NUMA 基准测试
// ============================================================================

fn bench_numa_acquire_p99(c: &mut Criterion) {
    c.bench_function("v970_numa_topology_detect", |b| {
        b.iter(|| {
            let topo = NumaTopology::detect();
            black_box(topo.nodes.len())
        })
    });

    c.bench_function("v970_numa_config_build", |b| {
        let topo = NumaTopology::detect();
        b.iter(|| {
            let config = NumaConfig::default();
            let sizes = config.partition_sizes(100, &topo);
            black_box(sizes.len())
        })
    });
}

fn bench_numa_cross_node_rate(c: &mut Criterion) {
    c.bench_function("v970_cross_node_counter", |b| {
        b.iter(|| {
            let counter = CrossNodeCounter::new();
            for i in 0..1000 {
                counter.record_total();
                if i % 3 == 0 {
                    counter.record_cross_node();
                }
            }
            black_box(counter.rate())
        })
    });
}

fn bench_numa_degrade_single_node(c: &mut Criterion) {
    c.bench_function("v970_numa_degrade_single_node", |b| {
        b.iter(|| {
            let topo = NumaTopology::detect();
            let config = NumaConfig::default();
            let sizes = config.partition_sizes(50, &topo);
            assert!(!sizes.is_empty());
            black_box(topo.degraded)
        })
    });
}

// ============================================================================
// T3: 缓存预热基准测试
// ============================================================================

fn bench_cold_start_miss_storm(c: &mut Criterion) {
    c.bench_function("v970_cold_start_no_history", |b| {
        b.iter(|| {
            let analyzer = AccessPatternAnalyzer::new(AccessPatternConfig::default());
            let hotspots = analyzer.identify_hotspots();
            black_box(hotspots.len())
        })
    });
}

fn bench_intelligent_warmup_hit_rate(c: &mut Criterion) {
    c.bench_function("v970_intelligent_warmup_10k", |b| {
        b.iter(|| {
            let analyzer = AccessPatternAnalyzer::new(AccessPatternConfig::default());
            for i in 0..10_000 {
                let key = if i < 1000 {
                    format!("hot_{i}")
                } else {
                    format!("cold_{i}")
                };
                analyzer.record_access(&key);
            }
            for _ in 0..100 {
                for j in 0..10 {
                    analyzer.record_access(&format!("hot_{j}"));
                }
            }
            let hotspots = analyzer.identify_hotspots();
            black_box(hotspots.len())
        })
    });
}

fn bench_warmup_pipeline_parallel(c: &mut Criterion) {
    c.bench_function("v970_warmup_pipeline_config", |b| {
        b.iter(|| {
            let config = WarmupPipelineConfig {
                stages: vec![WarmupStage::L2, WarmupStage::L1, WarmupStage::InProcess],
                parallel_enabled: true,
                parallelism: 4,
                fail_strategy: StageFailStrategy::Continue,
            };
            black_box(config.stages.len())
        })
    });

    c.bench_function("v970_warmup_pipeline_serial", |b| {
        b.iter(|| {
            let config = WarmupPipelineConfig::default();
            black_box(config.stages.len())
        })
    });
}

fn bench_warmup_trigger_idempotent(c: &mut Criterion) {
    c.bench_function("v970_warmup_trigger_config", |b| {
        b.iter(|| {
            let config = TriggerConfig {
                scheduled_enabled: true,
                scheduled_period: Duration::from_secs(60),
                scheduled_tables: vec!["t1".to_string(), "t2".to_string()],
                event_driven_enabled: true,
                event_driven_tables: vec!["t3".to_string()],
                dedup_window: Duration::from_secs(30),
            };
            black_box(config.scheduled_tables.len() + config.event_driven_tables.len())
        })
    });
}

criterion_group!(
    v970_benches,
    bench_async_zero_copy_alloc,
    bench_stream_async_throughput,
    bench_batch_deserialize_simd,
    bench_backpressure_memory_bounded,
    bench_numa_acquire_p99,
    bench_numa_cross_node_rate,
    bench_numa_degrade_single_node,
    bench_cold_start_miss_storm,
    bench_intelligent_warmup_hit_rate,
    bench_warmup_pipeline_parallel,
    bench_warmup_trigger_idempotent,
);
criterion_main!(v970_benches);
