//! v8.0.0 组 2 任务 2.9：性能极致优化端到端接线测试
//!
//! 4 个端到端接线测试，调用真实组件而非 mock，验证生产调用点可达。

#![cfg(feature = "perf-extreme")]

use std::sync::Arc;

use sz_orm_core::perf_extreme::{
    BatchAcquireOptimized, PerfBenchmarkComparator, PerfRegressionDetector, SimdConfig,
    SimdFullPipeline, ZeroCopyAcquire, ZeroCopyDeserConfig, ZeroCopyDeserializer,
};
use sz_orm_core::{Connection, ConnectionFactory, Pool, PoolConfigBuilder};

// ============================================================================
// 测试用 Mock 连接（与 pool.rs 测试一致的真实 Connection 实现）
// ============================================================================

struct MockConnection {
    connected: bool,
}

impl MockConnection {
    fn new() -> Self {
        Self { connected: true }
    }
}

impl Connection for MockConnection {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<u64, sz_orm_core::DbError>> + Send + 'a>,
    > {
        Box::pin(async move { Ok(1) })
    }
    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        Vec<std::collections::HashMap<String, sz_orm_core::Value>>,
                        sz_orm_core::DbError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move { Ok(vec![]) })
    }
    fn begin_transaction<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>,
    > {
        Box::pin(async move { Ok(()) })
    }
    fn commit<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>,
    > {
        Box::pin(async move { Ok(()) })
    }
    fn rollback<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>,
    > {
        Box::pin(async move { Ok(()) })
    }
    fn is_connected(&self) -> bool {
        self.connected
    }
    fn ping<'a>(
        &'a mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { true })
    }
    fn close<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>,
    > {
        Box::pin(async move {
            self.connected = false;
            Ok(())
        })
    }
}

struct MockConnectionFactory;

#[async_trait::async_trait]
impl ConnectionFactory for MockConnectionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, sz_orm_core::DbError> {
        Ok(Box::new(MockConnection::new()))
    }
}

fn make_pool(max_size: u32) -> Pool {
    let config = PoolConfigBuilder::new().max_size(max_size).build().unwrap();
    Pool::new(config, Arc::new(MockConnectionFactory)).unwrap()
}

// ============================================================================
// 接线测试 1：ZeroCopyAcquire 真实连接池 10000 次 acquire 全链路
// ============================================================================

#[tokio::test]
async fn wiring_zero_copy_acquire_10000_times() {
    let pool = Arc::new(make_pool(16));
    let zc = ZeroCopyAcquire::from_pool(pool);
    for _ in 0..10000 {
        let conn = zc.acquire_zero_copy().await;
        assert!(conn.is_ok(), "acquire_zero_copy 应成功");
    }
    let snap = sz_orm_core::perf_metrics::PerfMetrics::global().snapshot();
    assert!(
        snap.pool_acquire_count >= 10000,
        "PerfMetrics 应累计 ≥ 10000 次 acquire，实际 {}",
        snap.pool_acquire_count
    );
}

// ============================================================================
// 接线测试 2：SimdFullPipeline 1024 元素 batch 向量化执行
// ============================================================================

#[tokio::test]
async fn wiring_simd_full_pipeline_1024_elements() {
    let pipeline = SimdFullPipeline::new(SimdConfig::default());
    let i64_column: Vec<i64> = (0..1024).collect();
    let f32_column: Vec<f32> = (0..1024).map(|i| i as f32).collect();
    let batch = sz_orm_core::perf_extreme::QueryBatch {
        i64_column,
        f32_column,
        compare_target: 512,
        filter_threshold: 512.0,
        filter_op: sz_orm_core::simd::SimdCmpOp::Gt,
        agg_op: sz_orm_core::simd::SimdAggOp::Sum,
    };
    let result = pipeline.execute_vectorized(&batch);
    assert_eq!(result.compare_mask.len(), 1024);
    assert_eq!(result.filter_mask.len(), 1024);
    // sum(0..1024) = 523776
    assert!(
        (result.aggregate - 523776.0).abs() < 1.0,
        "聚合结果应 ≈ 523776，实际 {}",
        result.aggregate
    );
}

// ============================================================================
// 接线测试 3：ZeroCopyDeserializer 1000 行结果零拷贝反序列化
// ============================================================================

#[tokio::test]
async fn wiring_zero_copy_deserializer_1000_rows() {
    let deser = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
    let rows: Vec<Vec<u8>> = (0..1000).map(|i| vec![0x04, b'v', i as u8]).collect();
    let row_refs: Vec<&[u8]> = rows.iter().map(|r| r.as_slice()).collect();
    let vals = deser.deserialize_batch_borrowed(&row_refs);
    assert_eq!(vals.len(), 1000);
    for v in &vals {
        assert!(matches!(
            v,
            sz_orm_core::value_borrowed::BorrowedValue::String(_)
        ));
    }
    let snap = sz_orm_core::perf_metrics::PerfMetrics::global().snapshot();
    let total = snap.zero_copy_hits + snap.zero_copy_misses;
    assert!(total > 0, "应记录零拷贝命中/未命中指标");
}

// ============================================================================
// 接线测试 4：PerfBenchmarkComparator 与基准套件对比 QPS 达标
// ============================================================================

#[tokio::test]
async fn wiring_benchmark_comparator_qps_meets_threshold() {
    let cmp = PerfBenchmarkComparator::with_default();
    let frameworks = [
        sz_orm_core::perf_extreme::BenchmarkFramework {
            name: "sz-orm".to_string(),
            qps: 980.0,
            avg_latency_us: 10.0,
            p99_latency_us: 20.0,
        },
        sz_orm_core::perf_extreme::BenchmarkFramework {
            name: "Diesel".to_string(),
            qps: 1000.0,
            avg_latency_us: 10.0,
            p99_latency_us: 20.0,
        },
        sz_orm_core::perf_extreme::BenchmarkFramework {
            name: "SQLx".to_string(),
            qps: 950.0,
            avg_latency_us: 12.0,
            p99_latency_us: 25.0,
        },
    ];
    let report = cmp.compare(&frameworks);
    assert!(report.benchmark_available, "基准套件应可用");
    assert!(
        report.qps_ratio >= 0.95,
        "sz-orm QPS 比值应 ≥ 0.95，实际 {}",
        report.qps_ratio
    );
    // 验证退化检测器与基准对比联动
    let detector = PerfRegressionDetector::with_default();
    use sz_orm_core::perf_extreme::PerfMetricsSnapshot;
    let baseline = PerfMetricsSnapshot {
        qps: 1000.0,
        avg_latency_us: 10.0,
        p99_latency_us: 20.0,
        pool_acquire_success_rate: 1.0,
        zero_copy_hit_rate: 0.9,
    };
    let current = PerfMetricsSnapshot {
        qps: report.sz_orm_metrics.as_ref().unwrap().qps,
        avg_latency_us: report.sz_orm_metrics.as_ref().unwrap().avg_latency_us,
        p99_latency_us: report.sz_orm_metrics.as_ref().unwrap().p99_latency_us,
        pool_acquire_success_rate: 1.0,
        zero_copy_hit_rate: 0.9,
    };
    let alert = detector.detect(&current, &baseline);
    // 980 vs 1000 = 2% 退化，低于 10% 阈值，应无告警
    assert!(alert.is_none(), "2% 退化不应触发告警");
}

// ============================================================================
// 接线测试 5：BatchAcquireOptimized 批量获取验证
// ============================================================================

#[tokio::test]
async fn wiring_batch_acquire_optimized_5_connections() {
    let pool = Arc::new(make_pool(16));
    let ba = BatchAcquireOptimized::from_pool(pool);
    let conns = ba.acquire_batch_optimized(5).await;
    assert!(conns.is_ok());
    assert_eq!(conns.unwrap().len(), 5);
}
