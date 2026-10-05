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
        measure_rounds: 5,
        pool_size: 5,
        dataset_size: 200,
        concurrency: 2,
        db_backend: DbBackend::Sqlite,
        db_connection: conn,
        compare_frameworks: vec![],
        init_once: false,
        save_baseline: None,
        compare_baseline: None,
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
        measure_rounds: 5,
        pool_size: 5,
        dataset_size: 200,
        concurrency: 2,
        db_backend: DbBackend::Mysql,
        db_connection: "mysql://root:test123@127.0.0.1:3306/sz_orm_test".to_string(),
        compare_frameworks: vec![],
        init_once: false,
        save_baseline: None,
        compare_baseline: None,
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
        Err(BenchError::ProductionDatabaseRejected(_))
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
    config.measure_rounds = 5;
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
// ── v8.6.0 Oracle benchmark 测试 ──────────────────────────────────────

/// Oracle 测试串行锁：多个 Oracle 测试共享同一 bench_users 表，
/// 并行 init 会导致 DDL 竞态
static ORACLE_TEST_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

/// Oracle 连接串（本机 23ai Free 监听器以 freepdb1.FALSE 注册）
fn oracle_test_connection() -> String {
    "oracle://sz_orm_test:SzOrmTest2026@127.0.0.1:1521/freepdb1.FALSE".to_string()
}

/// 计算百分位数
fn percentile(sorted: &[u64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx] as f64
}

#[tokio::test]
#[ignore = "需要 Oracle 23ai Free: oracle://sz_orm_test:SzOrmTest2026@127.0.0.1:1521/freepdb1.FALSE"]
async fn test_oracle_init_and_crud() {
    use sz_orm_bench::DatasetInitializer;

    let _lock = ORACLE_TEST_LOCK.lock().await;
    let conn = oracle_test_connection();
    let dataset_size = 1000;

    // 一次性数据集初始化
    DatasetInitializer::init(DbBackend::Oracle, &conn, dataset_size)
        .await
        .expect("Oracle 数据集初始化应成功");

    // 通过 sz-orm-oracle 执行 CRUD 并采集延迟
    use std::sync::Arc;
    use sz_orm_core::ConnectionFactory;
    use sz_orm_core::Value;
    use sz_orm_oracle::{OracleConnectionFactory, OraclePoolHandle};

    let handle = OraclePoolHandle::connect(
        "sz_orm_test",
        "SzOrmTest2026",
        "127.0.0.1:1521/freepdb1.FALSE",
    )
    .expect("Oracle 连接池创建应成功");
    let handle = Arc::new(handle);
    let factory = OracleConnectionFactory::new(handle);
    let mut conn_obj = factory.create().await.expect("Oracle 连接创建应成功");

    // INSERT 延迟采集
    let mut insert_latencies = Vec::new();
    for i in 1..=100 {
        let start = std::time::Instant::now();
        let params = [
            Value::I64(i + 10000),
            Value::String(format!("bench_user_{i}")),
            Value::String(format!("bench_user_{i}@oracle.test")),
            Value::I64(i),
        ];
        conn_obj
            .execute_with_params(
                "INSERT INTO bench_users (id, name, email, created_at) VALUES (?, ?, ?, ?)",
                &params,
            )
            .await
            .expect("Oracle INSERT 应成功");
        insert_latencies.push(start.elapsed().as_micros() as u64);
    }

    // SELECT 延迟采集
    let mut select_latencies = Vec::new();
    for i in 1..=100 {
        let start = std::time::Instant::now();
        let params = [Value::I64(i)];
        let rows = conn_obj
            .query_with_params("SELECT * FROM bench_users WHERE id = ?", &params)
            .await
            .expect("Oracle SELECT 应成功");
        assert!(!rows.is_empty(), "SELECT 应返回行");
        select_latencies.push(start.elapsed().as_micros() as u64);
    }

    // UPDATE 延迟采集
    let mut update_latencies = Vec::new();
    for i in 1..=100 {
        let start = std::time::Instant::now();
        let params = [Value::String(format!("updated_user_{i}")), Value::I64(i)];
        conn_obj
            .execute_with_params("UPDATE bench_users SET name = ? WHERE id = ?", &params)
            .await
            .expect("Oracle UPDATE 应成功");
        update_latencies.push(start.elapsed().as_micros() as u64);
    }

    // DELETE 延迟采集
    let mut delete_latencies = Vec::new();
    for i in 1..=100 {
        let start = std::time::Instant::now();
        let params = [Value::I64(i + 10000)];
        conn_obj
            .execute_with_params("DELETE FROM bench_users WHERE id = ?", &params)
            .await
            .expect("Oracle DELETE 应成功");
        delete_latencies.push(start.elapsed().as_micros() as u64);
    }

    conn_obj.close().await.ok();

    // 排序并计算 P50/P95/P99
    insert_latencies.sort();
    select_latencies.sort();
    update_latencies.sort();
    delete_latencies.sort();

    let insert_p50 = percentile(&insert_latencies, 50.0);
    let insert_p95 = percentile(&insert_latencies, 95.0);
    let insert_p99 = percentile(&insert_latencies, 99.0);
    let select_p50 = percentile(&select_latencies, 50.0);
    let select_p95 = percentile(&select_latencies, 95.0);
    let select_p99 = percentile(&select_latencies, 99.0);

    println!("Oracle INSERT: P50={insert_p50}μs P95={insert_p95}μs P99={insert_p99}μs");
    println!("Oracle SELECT: P50={select_p50}μs P95={select_p95}μs P99={select_p99}μs");
    println!(
        "Oracle UPDATE: P50={}μs P95={}μs P99={}μs",
        percentile(&update_latencies, 50.0),
        percentile(&update_latencies, 95.0),
        percentile(&update_latencies, 99.0)
    );
    println!(
        "Oracle DELETE: P50={}μs P95={}μs P99={}μs",
        percentile(&delete_latencies, 50.0),
        percentile(&delete_latencies, 95.0),
        percentile(&delete_latencies, 99.0)
    );

    // 吞吐量计算
    let insert_total: u64 = insert_latencies.iter().sum();
    let insert_throughput = 100.0 * 1_000_000.0 / (insert_total as f64);
    println!("Oracle INSERT 吞吐量: {insert_throughput:.2} ops/s");

    assert!(insert_p50 > 0.0, "INSERT P50 应 > 0");
    assert!(select_p50 > 0.0, "SELECT P50 应 > 0");
    assert!(insert_p99 >= insert_p50, "P99 应 >= P50");
    assert!(select_p99 >= select_p50, "P99 应 >= P50");
}

#[tokio::test]
#[ignore = "需要 Oracle 23ai Free: oracle://sz_orm_test:SzOrmTest2026@127.0.0.1:1521/freepdb1.FALSE"]
async fn test_oracle_transaction_commit_rollback() {
    use std::sync::Arc;
    use sz_orm_bench::DatasetInitializer;
    use sz_orm_core::ConnectionFactory;
    use sz_orm_core::Value;
    use sz_orm_oracle::{OracleConnectionFactory, OraclePoolHandle};

    let _lock = ORACLE_TEST_LOCK.lock().await;
    let conn = oracle_test_connection();
    DatasetInitializer::init(DbBackend::Oracle, &conn, 100)
        .await
        .expect("Oracle 数据集初始化应成功");

    let handle = OraclePoolHandle::connect(
        "sz_orm_test",
        "SzOrmTest2026",
        "127.0.0.1:1521/freepdb1.FALSE",
    )
    .expect("Oracle 连接池创建应成功");
    let handle = Arc::new(handle);
    let factory = OracleConnectionFactory::new(handle);

    // 事务 commit 测试
    let mut conn1 = factory.create().await.expect("连接创建应成功");
    conn1.begin_transaction().await.expect("开启事务应成功");
    let params = [
        Value::I64(99991),
        Value::String("tx_commit".into()),
        Value::String("tx@oracle".into()),
        Value::I64(1),
    ];
    conn1
        .execute_with_params(
            "INSERT INTO bench_users (id, name, email, created_at) VALUES (?, ?, ?, ?)",
            &params,
        )
        .await
        .expect("事务内 INSERT 应成功");
    conn1.commit().await.expect("COMMIT 应成功");
    conn1.close().await.ok();

    // 验证 commit 后数据存在
    let mut conn2 = factory.create().await.expect("连接创建应成功");
    let rows = conn2
        .query_with_params(
            "SELECT * FROM bench_users WHERE id = ?",
            &[Value::I64(99991)],
        )
        .await
        .expect("SELECT 应成功");
    assert!(!rows.is_empty(), "COMMIT 后数据应存在");

    // 事务 rollback 测试
    conn2.begin_transaction().await.expect("开启事务应成功");
    let params2 = [
        Value::I64(99992),
        Value::String("tx_rollback".into()),
        Value::String("tx@oracle".into()),
        Value::I64(2),
    ];
    conn2
        .execute_with_params(
            "INSERT INTO bench_users (id, name, email, created_at) VALUES (?, ?, ?, ?)",
            &params2,
        )
        .await
        .expect("事务内 INSERT 应成功");
    conn2.rollback().await.expect("ROLLBACK 应成功");
    conn2.close().await.ok();

    // 验证 rollback 后数据不存在
    let mut conn3 = factory.create().await.expect("连接创建应成功");
    let rows = conn3
        .query_with_params(
            "SELECT * FROM bench_users WHERE id = ?",
            &[Value::I64(99992)],
        )
        .await
        .expect("SELECT 应成功");
    assert!(rows.is_empty(), "ROLLBACK 后数据应不存在");
    conn3.close().await.ok();
}

#[tokio::test]
#[ignore = "需要 Oracle 23ai Free: oracle://sz_orm_test:SzOrmTest2026@127.0.0.1:1521/freepdb1.FALSE"]
async fn test_oracle_aggregate_query() {
    use std::sync::Arc;
    use sz_orm_bench::DatasetInitializer;
    use sz_orm_core::ConnectionFactory;
    use sz_orm_core::Value;
    use sz_orm_oracle::{OracleConnectionFactory, OraclePoolHandle};

    let _lock = ORACLE_TEST_LOCK.lock().await;
    let conn = oracle_test_connection();
    DatasetInitializer::init(DbBackend::Oracle, &conn, 500)
        .await
        .expect("Oracle 数据集初始化应成功");

    let handle = OraclePoolHandle::connect(
        "sz_orm_test",
        "SzOrmTest2026",
        "127.0.0.1:1521/freepdb1.FALSE",
    )
    .expect("Oracle 连接池创建应成功");
    let handle = Arc::new(handle);
    let factory = OracleConnectionFactory::new(handle);
    let mut conn_obj = factory.create().await.expect("连接创建应成功");

    // COUNT(*) 聚合查询
    let start = std::time::Instant::now();
    let rows = conn_obj
        .query("SELECT COUNT(*) AS cnt FROM bench_users")
        .await
        .expect("COUNT 查询应成功");
    let count_elapsed = start.elapsed().as_micros() as u64;
    assert!(!rows.is_empty(), "COUNT 应返回行");
    println!("Oracle COUNT(*): {count_elapsed}μs");

    // 范围查询
    let start = std::time::Instant::now();
    let rows = conn_obj
        .query_with_params(
            "SELECT * FROM bench_users WHERE id >= ? AND id < ?",
            &[Value::I64(1), Value::I64(101)],
        )
        .await
        .expect("范围查询应成功");
    let range_elapsed = start.elapsed().as_micros() as u64;
    assert_eq!(rows.len(), 100, "范围查询应返回 100 行");
    println!("Oracle 范围查询(100行): {range_elapsed}μs");

    conn_obj.close().await.ok();
}
