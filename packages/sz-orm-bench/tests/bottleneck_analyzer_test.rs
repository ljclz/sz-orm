use sz_orm_bench::{
    BenchConfig, BenchResult, BottleneckAnalyzer, BottleneckPoint, FrameworkType,
    RegressionBaseline, WorkloadType,
};

fn make_result(fw: FrameworkType, wl: WorkloadType, p50: f64, p95: f64, qps: f64) -> BenchResult {
    BenchResult {
        framework: fw,
        workload: wl,
        p50_us: p50,
        p95_us: p95,
        p99_us: p95 * 1.5,
        throughput_ops: qps,
        peak_rss_kb: 0,
        alloc_count: 0,
        alloc_bytes: 0,
        raw_latencies: vec![],
        is_real_db: false,
        db_backend: sz_orm_bench::DbBackend::Sqlite,
        simd_speedup: None,
        dataset_size: 0,
    }
}

fn make_baseline(results: Vec<BenchResult>) -> RegressionBaseline {
    RegressionBaseline::new("test_baseline", BenchConfig::new(), results, "test_db")
}

#[test]
fn test_bottleneck_analyzer_identifies_slowest() {
    let results = vec![
        make_result(FrameworkType::SzOrm, WorkloadType::SingleRowQuery, 50.0, 60.0, 100.0),
        make_result(FrameworkType::SzOrm, WorkloadType::ComplexJoin, 2000.0, 2500.0, 10.0),
    ];
    let baseline = make_baseline(results);
    let report = BottleneckAnalyzer::analyze(&baseline);
    assert!(report.bottleneck_workloads.contains(&WorkloadType::ComplexJoin));
}

#[test]
fn test_bottleneck_analyzer_points_include_defaults() {
    let results = vec![
        make_result(FrameworkType::SzOrm, WorkloadType::SingleRowQuery, 50.0, 60.0, 100.0),
    ];
    let baseline = make_baseline(results);
    let report = BottleneckAnalyzer::analyze(&baseline);
    assert!(report.bottleneck_points.contains(&BottleneckPoint::ConnectionPool));
    assert!(report.bottleneck_points.contains(&BottleneckPoint::Cache));
}

#[test]
fn test_bottleneck_analyzer_high_p95_adds_simd() {
    let results = vec![
        make_result(FrameworkType::SzOrm, WorkloadType::ComplexJoin, 1500.0, 2000.0, 10.0),
    ];
    let baseline = make_baseline(results);
    let report = BottleneckAnalyzer::analyze(&baseline);
    assert!(report.bottleneck_points.contains(&BottleneckPoint::Simd));
    assert!(report.optimization_directions.iter().any(|d| d.contains("SIMD")));
}

#[test]
fn test_bottleneck_analyzer_very_high_p95_adds_zero_copy() {
    let results = vec![
        make_result(FrameworkType::SzOrm, WorkloadType::ComplexJoin, 2000.0, 3000.0, 5.0),
    ];
    let baseline = make_baseline(results);
    let report = BottleneckAnalyzer::analyze(&baseline);
    assert!(report.bottleneck_points.contains(&BottleneckPoint::ZeroCopy));
}

#[test]
fn test_bottleneck_analyzer_low_p95_no_bottleneck() {
    let results = vec![
        make_result(FrameworkType::SzOrm, WorkloadType::SingleRowQuery, 10.0, 15.0, 1000.0),
    ];
    let baseline = make_baseline(results);
    let report = BottleneckAnalyzer::analyze(&baseline);
    assert!(report.optimization_directions.iter().any(|d| d.contains("good") || d.contains("良好")));
}

#[test]
fn test_bottleneck_point_as_str() {
    assert_eq!(BottleneckPoint::ConnectionPool.as_str(), "connection_pool");
    assert_eq!(BottleneckPoint::Cache.as_str(), "cache");
    assert_eq!(BottleneckPoint::Simd.as_str(), "simd");
    assert_eq!(BottleneckPoint::ZeroCopy.as_str(), "zero_copy");
    assert_eq!(BottleneckPoint::PlanCache.as_str(), "plan_cache");
}
