//! 端到端测试：SLI 指标超期自动清理

use sz_orm_observability::slo_automation::{SliMetrics, SliRetentionCleaner};

#[tokio::test]
async fn e2e_sli_retention_cleanup() {
    let cleaner = SliRetentionCleaner::new(30);
    let mut metrics = vec![
        SliMetrics {
            timestamp: 0,
            availability: 0.999,
            latency_ms: 10.0,
            throughput: 100.0,
            correctness: 0.999,
        },
        SliMetrics {
            timestamp: 1000 * 86400 * 40,
            availability: 0.999,
            latency_ms: 10.0,
            throughput: 100.0,
            correctness: 0.999,
        },
    ];
    let removed = cleaner.cleanup(&mut metrics, 1000 * 86400 * 40);
    assert_eq!(removed, 1);
}
