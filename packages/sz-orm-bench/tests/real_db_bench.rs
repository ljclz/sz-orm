//! 真实 DB 基准集成测试（v7.2.0）
//!
//! 验证 `run_workload_real()` 在 SQLite/MySQL 上的正确性、可复现性和性能。

#![cfg(feature = "real-bench")]

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use sz_orm_bench::{
    validate_db_connection, BenchConfig, BenchError, DbBackend, FrameworkType, WorkloadType,
};

/// 全局计数器：为每个测试分配独立的 SQLite 文件，避免并行测试 UNIQUE 冲突
static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 创建 SQLite 临时文件连接串，返回 (连接串, 文件路径)
fn sqlite_test_connection() -> (String, std::path::PathBuf) {
    let id = TEST_DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("sz_orm_bench_real_db_test_{id}.sqlite"));
    // Windows 路径需正斜杠（sqlx URL 解析要求）
    let path_str = path.to_string_lossy().replace('\\', "/");
    let conn = format!("sqlite://{path_str}?mode=rwc");
    (conn, path)
}

/// 清理指定 SQLite 临时文件
fn cleanup_sqlite(path: &std::path::PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
}

/// 小规模测试配置（快速验证）
fn test_config(conn: String) -> BenchConfig {
    BenchConfig {
        seed: 42,
        warmup_rounds: 1,
        measure_rounds: 3,
        pool_size: 5,
        dataset_size: 200,
        concurrency: 2,
        db_backend: DbBackend::Sqlite,
        db_connection: conn,
    }
}

#[tokio::test]
async fn test_run_workload_real_sqlite() {
    let (conn, db_path) = sqlite_test_connection();
    let config = test_config(conn);

    let frameworks = [
        FrameworkType::SzOrm,
        FrameworkType::Sqlx,
        FrameworkType::SeaOrm,
    ];
    let workloads = [
        WorkloadType::SingleRowQuery,
        WorkloadType::BatchQuery,
        WorkloadType::ComplexJoin,
        WorkloadType::Transaction,
        WorkloadType::PoolConcurrency,
    ];

    for fw in &frameworks {
        for wl in &workloads {
            let result = sz_orm_bench::run_workload_real(*fw, *wl, &config)
                .await
                .unwrap_or_else(|e| panic!("{fw:?}/{wl:?} 失败: {e}"));

            assert!(result.is_real_db, "{fw:?}/{wl:?}: is_real_db 应为 true");
            assert_eq!(
                result.raw_latencies.len(),
                config.measure_rounds as usize,
                "{fw:?}/{wl:?}: raw_latencies 长度应 == measure_rounds"
            );
            assert!(
                result.p99_us >= result.p95_us,
                "{fw:?}/{wl:?}: P99({}) 应 >= P95({})",
                result.p99_us,
                result.p95_us
            );
            assert!(
                result.p95_us >= result.p50_us,
                "{fw:?}/{wl:?}: P95({}) 应 >= P50({})",
                result.p95_us,
                result.p50_us
            );
            assert!(result.p50_us >= 0.0, "{fw:?}/{wl:?}: P50 应 >= 0");
        }
    }

    cleanup_sqlite(&db_path);
}

#[tokio::test]
#[ignore = "需要 MySQL 服务: mysql://root:test123@127.0.0.1:3306/sz_orm_test"]
async fn test_run_workload_real_mysql() {
    let config = BenchConfig {
        seed: 42,
        warmup_rounds: 1,
        measure_rounds: 3,
        pool_size: 5,
        dataset_size: 200,
        concurrency: 2,
        db_backend: DbBackend::Mysql,
        db_connection: "mysql://root:test123@127.0.0.1:3306/sz_orm_test".to_string(),
    };

    let frameworks = [FrameworkType::Sqlx, FrameworkType::SeaOrm];
    let workloads = [
        WorkloadType::SingleRowQuery,
        WorkloadType::BatchQuery,
        WorkloadType::ComplexJoin,
        WorkloadType::Transaction,
        WorkloadType::PoolConcurrency,
    ];

    for fw in &frameworks {
        for wl in &workloads {
            let result = sz_orm_bench::run_workload_real(*fw, *wl, &config)
                .await
                .unwrap_or_else(|e| panic!("{fw:?}/{wl:?} 失败: {e}"));
            assert!(result.is_real_db, "{fw:?}/{wl:?}: is_real_db 应为 true");
            assert_eq!(result.raw_latencies.len(), config.measure_rounds as usize);
        }
    }
}

#[test]
fn test_validate_db_connection_rejects_production() {
    assert!(matches!(
        validate_db_connection("mysql://root:pass@prod-db:3306/sz_orm_test"),
        Err(BenchError::ProductionDatabaseRejected(_))
    ));
    assert!(matches!(
        validate_db_connection("mysql://root:pass@10.0.0.1:3306/production_db"),
        Err(BenchError::ProductionDatabaseRejected(_))
    ));
    assert!(matches!(
        validate_db_connection(""),
        Err(BenchError::InvalidConnectionString(_))
    ));
    assert!(matches!(
        validate_db_connection("postgres://localhost/test"),
        Err(BenchError::InvalidConnectionString(_))
    ));
    assert_eq!(
        validate_db_connection("sqlite::memory:").unwrap(),
        DbBackend::Sqlite
    );
    assert_eq!(
        validate_db_connection("mysql://root:test123@127.0.0.1:3306/sz_orm_test").unwrap(),
        DbBackend::Mysql
    );
}

#[tokio::test]
async fn test_default_config_runs_real_db() {
    let (conn, db_path) = sqlite_test_connection();
    let mut config = BenchConfig::new();
    config.measure_rounds = 3;
    config.warmup_rounds = 1;
    config.dataset_size = 100;
    config.db_connection = conn;

    let result =
        sz_orm_bench::run_workload_real(FrameworkType::Sqlx, WorkloadType::SingleRowQuery, &config)
            .await
            .expect("默认配置应走真实 DB 路径");

    assert!(result.is_real_db, "默认配置产出的结果 is_real_db 应为 true");

    cleanup_sqlite(&db_path);
}

#[tokio::test]
async fn test_bench_performance_sqlite() {
    let (conn, db_path) = sqlite_test_connection();
    let mut config = test_config(conn);
    config.measure_rounds = 5;

    let start = Instant::now();
    let result =
        sz_orm_bench::run_workload_real(FrameworkType::Sqlx, WorkloadType::SingleRowQuery, &config)
            .await
            .expect("性能测试应成功");
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() <= 30,
        "单框架单负载应 ≤30 秒，实际 {:?}",
        elapsed
    );
    assert!(!result.raw_latencies.is_empty(), "延迟数组不应为空");

    cleanup_sqlite(&db_path);
}

#[tokio::test]
async fn test_bench_result_completeness() {
    let (conn, db_path) = sqlite_test_connection();
    let config = test_config(conn);

    let result =
        sz_orm_bench::run_workload_real(FrameworkType::Sqlx, WorkloadType::SingleRowQuery, &config)
            .await
            .expect("结果完整性测试应成功");

    assert!(!result.raw_latencies.is_empty(), "raw_latencies 不应为空");
    assert!(result.p50_us > 0.0, "p50_us 应 > 0");
    assert!(result.p95_us > 0.0, "p95_us 应 > 0");
    assert!(result.p99_us > 0.0, "p99_us 应 > 0");
    assert!(result.throughput_ops > 0.0, "throughput_ops 应 > 0");
    assert!(result.is_real_db, "is_real_db 应为 true");

    let json = serde_json::to_string(&result).expect("JSON 序列化应成功");
    assert!(
        json.contains("\"is_real_db\":true"),
        "JSON 应含 is_real_db:true"
    );

    cleanup_sqlite(&db_path);
}
