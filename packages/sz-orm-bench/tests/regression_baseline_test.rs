//! 方向 1 端到端测试：基线持久化往返 + 对比告警

use sz_orm_bench::{
    regression_baseline::RegressionBaseline, BenchConfig, BenchResult, DbBackend, FrameworkType,
    WorkloadType,
};

fn make_result(workload: WorkloadType, latencies: Vec<u64>) -> BenchResult {
    BenchResult::from_latencies_ext(
        FrameworkType::SzOrm,
        workload,
        latencies,
        1024,
        10,
        1024,
        true,
        DbBackend::Sqlite,
        None,
        1000,
    )
}

#[test]
fn test_regression_baseline_e2e_save_load_compare() {
    let temp_dir = std::env::temp_dir().join("sz_orm_bench_e2e_baseline");
    let config = BenchConfig::new();

    let baseline_results = vec![
        make_result(WorkloadType::SingleRowQuery, vec![100, 150, 200]),
        make_result(WorkloadType::BatchQuery, vec![200, 300, 400]),
    ];
    let baseline = RegressionBaseline::new("e2e_test", config.clone(), baseline_results, "sqlite 3.45");

    let saved_path = baseline.save_to_file(temp_dir.to_str().unwrap()).unwrap();
    assert!(saved_path.exists());

    let loaded = RegressionBaseline::load_from_file(saved_path.to_str().unwrap()).unwrap();
    assert_eq!(loaded.baseline_name, "e2e_test");
    assert_eq!(loaded.results.len(), 2);

    let current_results = vec![
        make_result(WorkloadType::SingleRowQuery, vec![100, 150, 210]),
        make_result(WorkloadType::BatchQuery, vec![200, 300, 500]),
    ];
    let comparison = loaded.compare(&current_results);
    assert!(comparison.has_regression, "BatchQuery P95 退化应触发告警");

    let _ = std::fs::remove_file(&saved_path);
    let _ = std::fs::remove_dir(&temp_dir);
}

#[test]
fn test_db_backend_postgres_serde() {
    let json = serde_json::to_string(&DbBackend::Postgres).unwrap();
    assert_eq!(json, "\"postgres\"");
    let back: DbBackend = serde_json::from_str(&json).unwrap();
    assert_eq!(back, DbBackend::Postgres);
}

#[test]
fn test_bench_config_new_fields_default() {
    let config = BenchConfig::new();
    assert!(config.compare_frameworks.is_empty());
    assert!(!config.init_once);
    assert!(config.save_baseline.is_none());
    assert!(config.compare_baseline.is_none());
}

#[test]
fn test_bench_config_validate_measure_rounds_minimum_5() {
    let mut config = BenchConfig::new();
    config.measure_rounds = 4;
    assert!(config.validate().is_err(), "measure_rounds=4 应失败（需 >= 5）");

    config.measure_rounds = 5;
    assert!(config.validate().is_ok(), "measure_rounds=5 应通过");
}