//! 性能测试：采集耗时 < 100μs，判定耗时 < 1ms，内存 < 10MB
//!
//! 使用 `--ignored` 标志运行：`cargo test -p sz-orm-anomaly --test perf -- --ignored`

use std::sync::Arc;
use std::time::{Duration, Instant};

use sz_orm_anomaly::{AlertEmitter, AnomalyConfig, AnomalyDetector, ErrorType, SlidingWindow};

fn fast_config() -> AnomalyConfig {
    AnomalyConfig::default()
        .with_window_size(Duration::from_secs(60))
        .with_alert_cooldown(Duration::from_millis(0))
        .with_min_baseline_samples(5)
        .with_slow_query_spike_count(5)
}

fn now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[test]
#[ignore = "性能测试"]
fn perf_record_slow_query_under_100us() {
    let detector = AnomalyDetector::new(fast_config());
    let ts = now();
    let iterations = 10_000;

    let start = Instant::now();
    for i in 0..iterations {
        detector.record_slow_query(150, "SELECT * FROM users WHERE id = ?", ts + i);
    }
    let elapsed = start.elapsed();

    let avg_ns = elapsed.as_nanos() / iterations as u128;
    assert!(
        avg_ns < 100_000,
        "单次 record_slow_query 平均 {}ns 应 < 100μs",
        avg_ns
    );
}

#[test]
#[ignore = "性能测试"]
fn perf_record_error_under_100us() {
    let detector = AnomalyDetector::new(fast_config());
    let ts = now();
    let iterations = 10_000;

    let start = Instant::now();
    for i in 0..iterations {
        detector.record_error(ErrorType::SqlError, ts + i);
    }
    let elapsed = start.elapsed();

    let avg_ns = elapsed.as_nanos() / iterations as u128;
    assert!(
        avg_ns < 100_000,
        "单次 record_error 平均 {}ns 应 < 100μs",
        avg_ns
    );
}

#[test]
#[ignore = "性能测试"]
fn perf_record_pool_usage_under_100us() {
    let detector = AnomalyDetector::new(fast_config());
    let ts = now();
    let iterations = 10_000;

    let start = Instant::now();
    for i in 0..iterations {
        detector.record_pool_usage(10, 40, 0, 5, ts + i);
    }
    let elapsed = start.elapsed();

    let avg_ns = elapsed.as_nanos() / iterations as u128;
    assert!(
        avg_ns < 100_000,
        "单次 record_pool_usage 平均 {}ns 应 < 100μs",
        avg_ns
    );
}

#[test]
#[ignore = "性能测试"]
fn perf_detect_anomalies_under_1ms() {
    let detector = AnomalyDetector::new(fast_config());
    let ts = now();

    // 预填充指标（200 个慢查询 + 50 个错误 + 50 个连接池，接近真实场景）
    for i in 0..200 {
        detector.record_slow_query(150, "SELECT * FROM t WHERE id = ?", ts + i);
    }
    for i in 0..50 {
        detector.record_error(ErrorType::SqlError, ts + i);
    }
    for i in 0..50 {
        detector.record_pool_usage(10, 40, 0, 5, ts + i);
    }

    let iterations = 100;
    let start = Instant::now();
    for _ in 0..iterations {
        detector.detect_anomalies_raw();
    }
    let elapsed = start.elapsed();

    let avg_ns = elapsed.as_nanos() / iterations as u128;
    assert!(
        avg_ns < 1_000_000,
        "单次 detect_anomalies 平均 {}ns 应 < 1ms",
        avg_ns
    );
}

#[test]
#[ignore = "性能测试"]
fn perf_sliding_window_memory_under_10mb() {
    let window = Arc::new(SlidingWindow::new(Duration::from_secs(30 * 60)));
    let ts = now();

    // 写入 30 分钟数据（每秒 10 条）
    for i in 0..30 * 60 * 10 {
        window.push_slow_query(sz_orm_anomaly::SlowQueryMetric {
            timestamp: ts + i * 100,
            elapsed_ms: 150,
            sql_summary: "SELECT * FROM users WHERE id = ?".to_string(),
        });
    }

    let memory = window.estimated_memory_bytes();
    assert!(
        memory < 10 * 1024 * 1024,
        "30 分钟数据内存 {} 字节应 < 10MB",
        memory
    );
}

#[test]
#[ignore = "性能测试"]
fn perf_concurrent_collection() {
    use std::sync::Arc;
    use std::thread;

    let detector = Arc::new(AnomalyDetector::new(fast_config()));
    let ts = now();
    let threads = 4;
    let per_thread = 1000;

    let start = Instant::now();
    let mut handles = Vec::new();
    for t in 0..threads {
        let detector = Arc::clone(&detector);
        handles.push(thread::spawn(move || {
            for i in 0..per_thread {
                detector.record_slow_query(
                    150,
                    "SELECT * FROM t WHERE id = ?",
                    ts + t * 10_000 + i,
                );
            }
        }));
    }
    for handle in handles {
        handle.join().expect("线程应成功完成");
    }
    let elapsed = start.elapsed();

    let total = threads * per_thread;
    let avg_ns = elapsed.as_nanos() / total as u128;
    assert!(avg_ns < 100_000, "并发采集单次平均 {}ns 应 < 100μs", avg_ns);
    assert_eq!(detector.collector().written_count(), total);
}

#[test]
#[ignore = "性能测试"]
fn perf_alert_emitter_throughput() {
    use sz_orm_anomaly::{Alert, AnomalyType, Severity};
    let emitter = AlertEmitter::new(0); // 无冷却期
    let iterations = 10_000;

    let start = Instant::now();
    for i in 0..iterations {
        let alert = Alert {
            anomaly_type: AnomalyType::SlowQuerySpike,
            severity: Severity::Warn,
            timestamp: i,
            metric_value: 20.0,
            threshold: 10.0,
            baseline: None,
            suggestion: "test".to_string(),
            sql_summary: None,
        };
        emitter.emit(alert);
    }
    let elapsed = start.elapsed();

    let avg_ns = elapsed.as_nanos() / iterations as u128;
    assert!(avg_ns < 10_000, "单次 emit 平均 {}ns 应 < 10μs", avg_ns);
    assert_eq!(emitter.emitted_count(), iterations);
}

#[test]
#[ignore = "性能测试"]
fn perf_baseline_calculation() {
    use sz_orm_anomaly::BaselineCalculator;
    let iterations = 100_000;

    let mut calc = BaselineCalculator::new();
    let start = Instant::now();
    for i in 0..iterations {
        calc.add(i as f64);
    }
    let elapsed = start.elapsed();

    let avg_ns = elapsed.as_nanos() / iterations as u128;
    assert!(
        avg_ns < 1_000,
        "单次 Welford add 平均 {}ns 应 < 1μs",
        avg_ns
    );
}
