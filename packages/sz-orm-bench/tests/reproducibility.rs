//! 可复现性验证测试（v7.2.0）
//!
//! 验证相同 seed 连续多次运行的真实 DB 查询吞吐量偏差在可接受范围内。
//!
//! 策略：一次性初始化数据集 + 创建工作负载实例，多次运行同一实例的 execute()，
//! 消除初始化和连接池创建开销对可复现性评估的干扰。

#![cfg(feature = "real-bench")]

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use sz_orm_bench::{
    real_db::{DatasetInitializer, SeaOrmWorkload, SqlxWorkload, SzOrmWorkload},
    BenchConfig, DbBackend, WorkloadType,
};

/// 全局计数器：为每个测试分配独立的 SQLite 文件，避免并行测试冲突
static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

fn sqlite_test_connection() -> (String, std::path::PathBuf) {
    let id = TEST_DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("sz_orm_bench_reproducibility_test_{id}.sqlite"));
    let path_str = path.to_string_lossy().replace('\\', "/");
    let conn = format!("sqlite://{path_str}?mode=rwc");
    (conn, path)
}

fn cleanup_sqlite(path: &std::path::PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
}

fn reproducibility_config(conn: String) -> BenchConfig {
    BenchConfig {
        seed: 42,
        warmup_rounds: 5,
        measure_rounds: 20,
        pool_size: 5,
        dataset_size: 500,
        concurrency: 2,
        db_backend: DbBackend::Sqlite,
        db_connection: conn,
    }
}

fn throughput_deviation(throughputs: &[f64]) -> f64 {
    if throughputs.is_empty() {
        return 0.0;
    }
    let avg = throughputs.iter().sum::<f64>() / throughputs.len() as f64;
    if avg == 0.0 {
        return 0.0;
    }
    throughputs
        .iter()
        .map(|&t| ((t - avg).abs() / avg) * 100.0)
        .fold(0.0_f64, f64::max)
}

fn assert_reproducible(label: &str, throughputs: &[f64]) {
    let deviation = throughput_deviation(throughputs);
    if deviation > 30.0 {
        panic!("{label} 吞吐量偏差 {deviation:.1}% > 30%，不可复现");
    }
    if deviation > 5.0 {
        eprintln!("REPRODUCIBILITY_WARNING: {label} 吞吐量偏差 {deviation:.1}% > 5%（警告级，真实 DB 正常波动）");
    } else {
        println!("{label} 可复现性验证通过: 偏差 {deviation:.1}% ≤ 5%");
    }
}

async fn init_and_warmup(config: &BenchConfig) {
    DatasetInitializer::init(
        config.db_backend,
        &config.db_connection,
        config.dataset_size,
    )
    .await
    .expect("数据集初始化应成功");
}

#[tokio::test]
async fn test_reproducibility_sqlx() {
    let (conn, db_path) = sqlite_test_connection();
    let config = reproducibility_config(conn);
    init_and_warmup(&config).await;

    let wl = SqlxWorkload::new_sqlite(&config.db_connection, config.pool_size)
        .await
        .expect("SqlxWorkload 创建应成功");

    // 预热
    let _ = wl
        .execute(WorkloadType::SingleRowQuery, config.warmup_rounds)
        .await
        .expect("预热应成功");

    let mut throughputs = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        let latencies = wl
            .execute(WorkloadType::SingleRowQuery, config.measure_rounds)
            .await
            .expect("测量应成功");
        let elapsed = start.elapsed();
        let throughput = latencies.len() as f64 / elapsed.as_secs_f64();
        throughputs.push(throughput);
    }

    assert_reproducible("Sqlx", &throughputs);
    cleanup_sqlite(&db_path);
}

#[tokio::test]
async fn test_reproducibility_sz_orm() {
    let (conn, db_path) = sqlite_test_connection();
    let config = reproducibility_config(conn);
    init_and_warmup(&config).await;

    let wl = SzOrmWorkload::new_sqlite(&config.db_connection, config.pool_size)
        .await
        .expect("SzOrmWorkload 创建应成功");

    // 预热
    let _ = wl
        .execute(WorkloadType::SingleRowQuery, config.warmup_rounds)
        .await
        .expect("预热应成功");

    let mut throughputs = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        let latencies = wl
            .execute(WorkloadType::SingleRowQuery, config.measure_rounds)
            .await
            .expect("测量应成功");
        let elapsed = start.elapsed();
        let throughput = latencies.len() as f64 / elapsed.as_secs_f64();
        throughputs.push(throughput);
    }

    assert_reproducible("sz-orm", &throughputs);
    cleanup_sqlite(&db_path);
}

#[tokio::test]
async fn test_reproducibility_sea_orm() {
    let (conn, db_path) = sqlite_test_connection();
    let config = reproducibility_config(conn);
    init_and_warmup(&config).await;

    let wl = SeaOrmWorkload::new(&config.db_connection)
        .await
        .expect("SeaOrmWorkload 创建应成功");

    // 预热
    let _ = wl
        .execute(WorkloadType::SingleRowQuery, config.warmup_rounds)
        .await
        .expect("预热应成功");

    let mut throughputs = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        let latencies = wl
            .execute(WorkloadType::SingleRowQuery, config.measure_rounds)
            .await
            .expect("测量应成功");
        let elapsed = start.elapsed();
        let throughput = latencies.len() as f64 / elapsed.as_secs_f64();
        throughputs.push(throughput);
    }

    assert_reproducible("SeaOrm", &throughputs);
    cleanup_sqlite(&db_path);
}
