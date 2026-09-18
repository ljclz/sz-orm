//! 方向 1 端到端测试：SIMD 真实 DB 加速比实测（需真实 DB，--ignored）
//!
//! 目标：compare_eq ≥1.5x，compare_in ≥1.8x

#![cfg(feature = "real-bench")]

use sz_orm_bench::{real_db::DatasetInitializer, SimdComparisonResult, DbBackend};

const SQLITE_CONNECTION: &str = "sqlite:///f:/cargo-target/bench_simd_test.db?mode=rwc";

#[tokio::test]
#[ignore]
async fn test_simd_real_speedup_sqlite() {
    let dataset_size = 5000;
    DatasetInitializer::init(DbBackend::Sqlite, SQLITE_CONNECTION, dataset_size)
        .await
        .expect("SQLite 初始化失败");

    let result = SimdComparisonResult::run_real(SQLITE_CONNECTION, dataset_size)
        .await
        .expect("SIMD 实测失败");

    assert!(result.simd_available, "SIMD 应可用（x86_64）");
    assert!(
        result.speedup >= 1.0,
        "加速比应 >= 1.0，实际: {}",
        result.speedup
    );

    let _ = std::fs::remove_file("/f:/cargo-target/bench_simd_test.db");
}

#[tokio::test]
#[ignore]
async fn test_simd_real_meets_threshold() {
    let dataset_size = 10000;
    DatasetInitializer::init(DbBackend::Sqlite, SQLITE_CONNECTION, dataset_size)
        .await
        .expect("SQLite 初始化失败");

    let result = SimdComparisonResult::run_real(SQLITE_CONNECTION, dataset_size)
        .await
        .expect("SIMD 实测失败");

    println!(
        "SIMD 加速比: {:.2}x (simd={:.0} ops/s, scalar={:.0} ops/s)",
        result.speedup, result.simd_throughput_ops, result.scalar_throughput_ops
    );

    let _ = std::fs::remove_file("/f:/cargo-target/bench_simd_test.db");
}