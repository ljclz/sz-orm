//! Serverless 适配示例
//!
//! 演示 ColdStartOptimizer + GracefulShutdown + MeteringCollector 完整流程。

use std::time::Duration;

use sz_orm_core::{CdcCheckpoint, ColdStartOptimizer, GracefulShutdown};
use sz_orm_observability::MeteringCollector;

fn main() {
    let optimizer = ColdStartOptimizer::default();
    println!(
        "冷启动优化器: 目标延迟={:?}, 最小预热={}",
        optimizer.target_latency(),
        optimizer.min_prewarm_connections()
    );

    let stats = optimizer.on_cold_start();
    println!(
        "冷启动完成: 预热={}, 耗时={:?}, 部分可用={}",
        stats.warmed_connections, stats.elapsed, stats.partial_available
    );

    optimizer.record_latency(Duration::from_millis(80));
    optimizer.record_latency(Duration::from_millis(120));
    optimizer.record_latency(Duration::from_millis(95));
    println!("P95 延迟: {:?}", optimizer.p95_latency());

    let shutdown = GracefulShutdown::default();
    let checkpoints = vec![
        CdcCheckpoint {
            source: "mysql".into(),
            position: 100,
        },
        CdcCheckpoint {
            source: "postgres".into(),
            position: 200,
        },
    ];
    let result = shutdown
        .on_scale_to_zero(stats.warmed_connections, &checkpoints)
        .unwrap();
    println!(
        "缩容完成: 释放={}, 水位={}, 耗时={:?}",
        result.released_connections, result.persisted_checkpoints, result.elapsed
    );

    if let Some(advice) = shutdown.scale_up_advice(5, 20) {
        println!("扩容建议: {} -> {}", 5, advice);
    }

    let metering = MeteringCollector::default();
    metering.record_request(Duration::from_millis(50), Duration::from_millis(100));
    metering.record_request(Duration::from_millis(30), Duration::from_millis(80));
    let report = metering.export();
    println!(
        "计费度量: 请求={}, 总耗时={}ms, 连接驻留={}ms",
        report.request_count, report.total_duration_ms, report.total_connection_resident_ms
    );
    let status = metering.try_report(true);
    println!("上报结果: {:?}", status);
}
