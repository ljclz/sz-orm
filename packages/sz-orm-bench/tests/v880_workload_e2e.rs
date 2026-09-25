//! v8.8.0 端到端测试 — 4 种新 WorkloadType × 4 种数据库
//!
//! 覆盖 ConcurrentReadWrite / PoolStress / LongTransaction / LargeResultSet
//! 在 MySQL / PostgreSQL / Oracle / SQLite 上的端到端验证。
//! 每个测试标记 #[ignore]（需真实 DB），通过 DATABASE_URL 环境变量指定连接。

#![cfg(feature = "real-bench")]

use std::sync::atomic::{AtomicU64, Ordering};
use sz_orm_bench::{BenchConfig, DbBackend, FrameworkType, WorkloadType};

const MYSQL_URL: &str = "mysql://root:test123@127.0.0.1:3306/sz_orm_test";
const POSTGRES_URL: &str = "postgres://postgres:test123@127.0.0.1:5432/sz_orm_test";
const ORACLE_URL: &str = "oracle://sz_orm_test:SzOrmTest2026@127.0.0.1:1521/freepdb1.FALSE";

static SQLITE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn sqlite_url() -> (String, std::path::PathBuf) {
    let id = SQLITE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("v880_e2e_{id}.sqlite"));
    let path_str = path.to_string_lossy().replace('\\', "/");
    (format!("sqlite://{path_str}?mode=rwc"), path)
}

fn cleanup_sqlite(path: &std::path::PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
}

fn make_config(backend: DbBackend, conn: &str) -> BenchConfig {
    BenchConfig {
        seed: 42,
        warmup_rounds: 1,
        measure_rounds: 5,
        pool_size: 5,
        dataset_size: 100,
        concurrency: 2,
        db_backend: backend,
        db_connection: conn.to_string(),
        compare_frameworks: vec![],
        init_once: false,
        save_baseline: None,
        compare_baseline: None,
    }
}

async fn run_e2e(backend: DbBackend, conn: &str, workload: WorkloadType) {
    let config = make_config(backend, conn);
    let framework = match backend {
        DbBackend::Sqlite => FrameworkType::SzOrm,
        DbBackend::Oracle => FrameworkType::SeaOrm,
        _ => FrameworkType::Sqlx,
    };
    let result = sz_orm_bench::run_workload_real(framework, workload, &config)
        .await
        .unwrap_or_else(|e| panic!("{backend:?}/{workload:?} 失败: {e}"));
    assert!(
        result.is_real_db,
        "{backend:?}/{workload:?}: is_real_db 应为 true"
    );
    assert_eq!(
        result.raw_latencies.len(),
        config.measure_rounds as usize,
        "{backend:?}/{workload:?}: raw_latencies 长度应 == measure_rounds"
    );
    assert!(
        result.p50_us >= 0.0,
        "{backend:?}/{workload:?}: P50 应 >= 0"
    );
}

// ── ConcurrentReadWrite ───────────────────────────────────────────

#[tokio::test]
#[ignore = "需要 MySQL: mysql://root:test123@127.0.0.1:3306/sz_orm_test"]
async fn e2e_concurrent_read_write_mysql() {
    run_e2e(
        DbBackend::Mysql,
        MYSQL_URL,
        WorkloadType::ConcurrentReadWrite,
    )
    .await;
}

#[tokio::test]
#[ignore = "需要 PostgreSQL: postgres://postgres:test123@127.0.0.1:5432/sz_orm_test"]
async fn e2e_concurrent_read_write_postgres() {
    run_e2e(
        DbBackend::Postgres,
        POSTGRES_URL,
        WorkloadType::ConcurrentReadWrite,
    )
    .await;
}

#[tokio::test]
#[ignore = "需要 Oracle: oracle://sz_orm_test:SzOrmTest2026@127.0.0.1:1521/freepdb1.FALSE"]
async fn e2e_concurrent_read_write_oracle() {
    run_e2e(
        DbBackend::Oracle,
        ORACLE_URL,
        WorkloadType::ConcurrentReadWrite,
    )
    .await;
}

#[tokio::test]
async fn e2e_concurrent_read_write_sqlite() {
    let (conn, path) = sqlite_url();
    run_e2e(DbBackend::Sqlite, &conn, WorkloadType::ConcurrentReadWrite).await;
    cleanup_sqlite(&path);
}

// ── PoolStress ────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "需要 MySQL: mysql://root:test123@127.0.0.1:3306/sz_orm_test"]
async fn e2e_pool_stress_mysql() {
    run_e2e(DbBackend::Mysql, MYSQL_URL, WorkloadType::PoolStress).await;
}

#[tokio::test]
#[ignore = "需要 PostgreSQL: postgres://postgres:test123@127.0.0.1:5432/sz_orm_test"]
async fn e2e_pool_stress_postgres() {
    run_e2e(DbBackend::Postgres, POSTGRES_URL, WorkloadType::PoolStress).await;
}

#[tokio::test]
#[ignore = "需要 Oracle: oracle://sz_orm_test:SzOrmTest2026@127.0.0.1:1521/freepdb1.FALSE"]
async fn e2e_pool_stress_oracle() {
    run_e2e(DbBackend::Oracle, ORACLE_URL, WorkloadType::PoolStress).await;
}

#[tokio::test]
async fn e2e_pool_stress_sqlite() {
    let (conn, path) = sqlite_url();
    run_e2e(DbBackend::Sqlite, &conn, WorkloadType::PoolStress).await;
    cleanup_sqlite(&path);
}

// ── LongTransaction ──────────────────────────────────────────────

#[tokio::test]
#[ignore = "需要 MySQL: mysql://root:test123@127.0.0.1:3306/sz_orm_test"]
async fn e2e_long_transaction_mysql() {
    run_e2e(DbBackend::Mysql, MYSQL_URL, WorkloadType::LongTransaction).await;
}

#[tokio::test]
#[ignore = "需要 PostgreSQL: postgres://postgres:test123@127.0.0.1:5432/sz_orm_test"]
async fn e2e_long_transaction_postgres() {
    run_e2e(
        DbBackend::Postgres,
        POSTGRES_URL,
        WorkloadType::LongTransaction,
    )
    .await;
}

#[tokio::test]
#[ignore = "需要 Oracle: oracle://sz_orm_test:SzOrmTest2026@127.0.0.1:1521/freepdb1.FALSE"]
async fn e2e_long_transaction_oracle() {
    run_e2e(DbBackend::Oracle, ORACLE_URL, WorkloadType::LongTransaction).await;
}

#[tokio::test]
async fn e2e_long_transaction_sqlite() {
    let (conn, path) = sqlite_url();
    run_e2e(DbBackend::Sqlite, &conn, WorkloadType::LongTransaction).await;
    cleanup_sqlite(&path);
}

// ── LargeResultSet ───────────────────────────────────────────────

#[tokio::test]
#[ignore = "需要 MySQL: mysql://root:test123@127.0.0.1:3306/sz_orm_test"]
async fn e2e_large_result_set_mysql() {
    run_e2e(DbBackend::Mysql, MYSQL_URL, WorkloadType::LargeResultSet).await;
}

#[tokio::test]
#[ignore = "需要 PostgreSQL: postgres://postgres:test123@127.0.0.1:5432/sz_orm_test"]
async fn e2e_large_result_set_postgres() {
    run_e2e(
        DbBackend::Postgres,
        POSTGRES_URL,
        WorkloadType::LargeResultSet,
    )
    .await;
}

#[tokio::test]
#[ignore = "需要 Oracle: oracle://sz_orm_test:SzOrmTest2026@127.0.0.1:1521/freepdb1.FALSE"]
async fn e2e_large_result_set_oracle() {
    run_e2e(DbBackend::Oracle, ORACLE_URL, WorkloadType::LargeResultSet).await;
}

#[tokio::test]
async fn e2e_large_result_set_sqlite() {
    let (conn, path) = sqlite_url();
    run_e2e(DbBackend::Sqlite, &conn, WorkloadType::LargeResultSet).await;
    cleanup_sqlite(&path);
}
