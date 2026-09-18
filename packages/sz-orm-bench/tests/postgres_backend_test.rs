//! 方向 1 端到端测试：PostgreSQL 18 后端基准执行（需真实 DB，--ignored）

#![cfg(feature = "real-bench")]

use sz_orm_bench::{
    real_db::{run_workload_real, DatasetInitializer},
    BenchConfig, DbBackend, FrameworkType, WorkloadType,
};

const PG_CONNECTION: &str = "postgres://postgres:test123@127.0.0.1:5432/sz_orm_test";

fn pg_config() -> BenchConfig {
    let mut config = BenchConfig::new();
    config.db_backend = DbBackend::Postgres;
    config.db_connection = PG_CONNECTION.to_string();
    config.dataset_size = 500;
    config.measure_rounds = 5;
    config.warmup_rounds = 1;
    config
}

#[tokio::test]
#[ignore]
async fn test_postgres_init_and_query() {
    let config = pg_config();
    DatasetInitializer::init(DbBackend::Postgres, PG_CONNECTION, config.dataset_size)
        .await
        .expect("PostgreSQL 初始化失败");

    let result = run_workload_real(FrameworkType::Sqlx, WorkloadType::SingleRowQuery, &config)
        .await
        .expect("PostgreSQL 基准查询失败");

    assert!(result.is_real_db, "结果应标记为真实 DB");
    assert_eq!(result.db_backend, DbBackend::Postgres);
    assert!(!result.raw_latencies.is_empty(), "应有延迟数据");
    assert!(result.throughput_ops > 0.0, "吞吐量应 > 0");
}

#[tokio::test]
#[ignore]
async fn test_postgres_batch_query() {
    let config = pg_config();
    DatasetInitializer::init(DbBackend::Postgres, PG_CONNECTION, config.dataset_size)
        .await
        .expect("PostgreSQL 初始化失败");

    let result = run_workload_real(FrameworkType::Sqlx, WorkloadType::BatchQuery, &config)
        .await
        .expect("PostgreSQL 批量查询失败");

    assert!(result.is_real_db);
    assert_eq!(result.db_backend, DbBackend::Postgres);
}