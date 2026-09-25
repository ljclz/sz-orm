//! v8.8.0 生产冒烟测试 — 连接→查询→批量→释放全链路
//!
//! 验证 v8.8.0 优化后的连接池获取/释放、查询构建、批量操作端到端可用。
//! 在 4 种数据库执行冒烟测试，每测试标记 #[ignore]（需真实 DB）。

#![cfg(feature = "real-bench")]

use std::sync::atomic::{AtomicU64, Ordering};
use sz_orm_bench::{BenchConfig, DbBackend, FrameworkType, WorkloadType};

const MYSQL_URL: &str = "mysql://root:test123@127.0.0.1:3306/sz_orm_test";
const POSTGRES_URL: &str = "postgres://postgres:test123@127.0.0.1:5432/sz_orm_test";
const ORACLE_URL: &str = "oracle://sz_orm_test:SzOrmTest2026@127.0.0.1:1521/freepdb1.FALSE";

static SQLITE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn sqlite_url() -> (String, std::path::PathBuf) {
    let id = SQLITE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("v880_smoke_{id}.sqlite"));
    let path_str = path.to_string_lossy().replace('\\', "/");
    (format!("sqlite://{path_str}?mode=rwc"), path)
}

fn cleanup_sqlite(path: &std::path::PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
}

fn smoke_config(backend: DbBackend, conn: &str) -> BenchConfig {
    BenchConfig {
        seed: 42,
        warmup_rounds: 1,
        measure_rounds: 5,
        pool_size: 3,
        dataset_size: 100,
        concurrency: 1,
        db_backend: backend,
        db_connection: conn.to_string(),
        compare_frameworks: vec![],
        init_once: false,
        save_baseline: None,
        compare_baseline: None,
    }
}

async fn run_smoke_chain(backend: DbBackend, conn: &str) {
    let config = smoke_config(backend, conn);
    let framework = match backend {
        DbBackend::Sqlite => FrameworkType::SzOrm,
        DbBackend::Oracle => FrameworkType::SeaOrm,
        _ => FrameworkType::Sqlx,
    };

    let workloads = [
        WorkloadType::SingleRowQuery,
        WorkloadType::BatchQuery,
        WorkloadType::ConcurrentReadWrite,
        WorkloadType::PoolStress,
        WorkloadType::LongTransaction,
        WorkloadType::LargeResultSet,
    ];

    for wl in &workloads {
        let result = sz_orm_bench::run_workload_real(framework, *wl, &config)
            .await
            .unwrap_or_else(|e| panic!("smoke {backend:?}/{wl:?} 失败: {e}"));
        assert!(
            result.is_real_db,
            "smoke {backend:?}/{wl:?}: is_real_db 应为 true"
        );
        assert!(
            result.raw_latencies.len() >= 1,
            "smoke {backend:?}/{wl:?}: 至少 1 次延迟采样"
        );
    }
}

#[tokio::test]
#[ignore = "需要 MySQL: mysql://root:test123@127.0.0.1:3306/sz_orm_test"]
async fn smoke_mysql_full_chain() {
    run_smoke_chain(DbBackend::Mysql, MYSQL_URL).await;
}

#[tokio::test]
#[ignore = "需要 PostgreSQL: postgres://postgres:test123@127.0.0.1:5432/sz_orm_test"]
async fn smoke_postgres_full_chain() {
    run_smoke_chain(DbBackend::Postgres, POSTGRES_URL).await;
}

#[tokio::test]
#[ignore = "需要 Oracle: oracle://sz_orm_test:SzOrmTest2026@127.0.0.1:1521/freepdb1.FALSE"]
async fn smoke_oracle_full_chain() {
    run_smoke_chain(DbBackend::Oracle, ORACLE_URL).await;
}

#[tokio::test]
async fn smoke_sqlite_full_chain() {
    let (conn, path) = sqlite_url();
    run_smoke_chain(DbBackend::Sqlite, &conn).await;
    cleanup_sqlite(&path);
}
