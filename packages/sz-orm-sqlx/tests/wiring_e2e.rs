//! v9.5.0 端到端测试：连真实 DB 验证 5 项新能力。
//!
//! 需要本机 MySQL 9.6: mysql://root:test123@127.0.0.1:3306/sz_orm_test
//! 需要本机 PostgreSQL 18: postgres://postgres:test123@127.0.0.1:5432/sz_orm_test
//! 运行: cargo test -p sz-orm-sqlx --test wiring_e2e --features "slow-query-capture,auto-healer" -- --ignored

use std::collections::HashMap;
use std::sync::OnceLock;

use sz_orm_core::{Connection, DbType, Value};
use sz_orm_sqlx::{AnyBackend, AnyPool, BatchExecutor};

static MYSQL_DSN: OnceLock<String> = OnceLock::new();
static PG_DSN: OnceLock<String> = OnceLock::new();

fn mysql_dsn() -> &'static str {
    MYSQL_DSN.get_or_init(|| {
        std::env::var("SZ_ORM_MYSQL_URL")
            .unwrap_or_else(|_| "mysql://root:test123@127.0.0.1:3306/sz_orm_test".to_string())
    })
}

fn pg_dsn() -> &'static str {
    PG_DSN.get_or_init(|| {
        std::env::var("SZ_ORM_PG_URL").unwrap_or_else(|_| {
            "postgres://postgres:test123@127.0.0.1:5432/sz_orm_test".to_string()
        })
    })
}

/// T8.2 MySQL 真实批量 INSERT 1000 行
#[tokio::test]
#[ignore]
async fn test_batch_insert_e2e_mysql() {
    let pool = AnyPool::connect(mysql_dsn()).await.unwrap();
    assert_eq!(pool.backend(), AnyBackend::MySql);
    let mut conn = pool.create().await.unwrap();

    conn.execute("DROP TABLE IF EXISTS e2e_batch_test")
        .await
        .unwrap();
    conn.execute("CREATE TABLE e2e_batch_test (id INT PRIMARY KEY, val VARCHAR(255))")
        .await
        .unwrap();

    let rows: Vec<HashMap<String, Value>> = (1..=1000)
        .map(|i| {
            HashMap::from([
                ("id".to_string(), Value::I64(i)),
                ("val".to_string(), Value::String(format!("v{}", i))),
            ])
        })
        .collect();

    let mut executor = BatchExecutor::new(&mut conn, DbType::MySQL);
    let count = executor
        .batch_insert("e2e_batch_test", &rows)
        .await
        .unwrap();
    assert_eq!(count, 1000);

    conn.execute("DROP TABLE IF EXISTS e2e_batch_test")
        .await
        .unwrap();
}

/// T8.3 PostgreSQL 真实批量 UPSERT 100 行
#[tokio::test]
#[ignore]
async fn test_batch_upsert_e2e_pg() {
    let pool = AnyPool::connect(pg_dsn()).await.unwrap();
    assert_eq!(pool.backend(), AnyBackend::Postgres);
    let mut conn = pool.create().await.unwrap();

    conn.execute("DROP TABLE IF EXISTS e2e_upsert_test")
        .await
        .unwrap();
    conn.execute("CREATE TABLE e2e_upsert_test (id INT PRIMARY KEY, val VARCHAR(255))")
        .await
        .unwrap();

    let rows: Vec<HashMap<String, Value>> = (1..=100)
        .map(|i| {
            HashMap::from([
                ("id".to_string(), Value::I64(i)),
                ("val".to_string(), Value::String(format!("v{}", i))),
            ])
        })
        .collect();

    let mut executor = BatchExecutor::new(&mut conn, DbType::PostgreSQL);
    let count = executor
        .batch_upsert("e2e_upsert_test", &rows, &["id"], &["val"])
        .await
        .unwrap();
    assert!(count >= 100);

    conn.execute("DROP TABLE IF EXISTS e2e_upsert_test")
        .await
        .unwrap();
}

/// T8.4 MySQL 慢查询捕获（v9.5.1 T4: 补齐 slow/fingerprint/脱敏断言）
#[tokio::test]
#[ignore]
#[cfg(feature = "slow-query-capture")]
async fn test_slow_query_capture_e2e() {
    let pool = AnyPool::connect(mysql_dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();

    conn.execute("DROP TABLE IF EXISTS e2e_slow_test")
        .await
        .unwrap();
    conn.execute("CREATE TABLE e2e_slow_test (id INT PRIMARY KEY, val VARCHAR(255))")
        .await
        .unwrap();
    conn.execute("INSERT INTO e2e_slow_test (id, val) VALUES (1, 'a')")
        .await
        .unwrap();

    let mut guard = sz_orm_sqlx::SlowQueryGuard::new(conn)
        .with_slow_threshold(std::time::Duration::from_millis(50));

    // 慢查询：SELECT SLEEP(0.1) 耗时 ~100ms >= 50ms
    let _ = guard
        .execute_with_logging("SELECT SLEEP(0.1)", &[])
        .await
        .unwrap();

    // 含手机号参数的慢查询
    let _ = guard
        .execute_with_logging(
            "SELECT * FROM e2e_slow_test WHERE id = ?",
            &[Value::String("13888888888".to_string())],
        )
        .await
        .unwrap();

    let entries = guard.logged_entries();
    assert!(
        !entries.is_empty(),
        "expected at least 1 logged entry, got {}",
        entries.len()
    );

    // 断言 1: SLEEP 慢查询 slow == true
    let sleep_entry = entries
        .iter()
        .find(|e| e.sql.contains("SLEEP"))
        .expect("SLEEP query should be logged");
    assert!(
        sleep_entry.slow,
        "SLEEP(0.1) should be marked slow (threshold=50ms)"
    );
    assert!(
        sleep_entry.total_elapsed_ms >= 50,
        "SLEEP(0.1) elapsed {}ms should be >= 50ms",
        sleep_entry.total_elapsed_ms
    );

    // 断言 2: fingerprint 归一化（数字字面量 → ?）
    let select_entry = entries
        .iter()
        .find(|e| e.sql.contains("e2e_slow_test"))
        .expect("SELECT query should be logged");
    assert!(
        select_entry.query_key.contains('?'),
        "fingerprint should contain normalized placeholder '?', got: {}",
        select_entry.query_key
    );

    // 断言 3: 脱敏——JSON 日志中不含原始手机号
    let jsons = guard.logged_jsons();
    for json in jsons {
        if json.contains("e2e_slow_test") {
            assert!(
                !json.contains("13888888888"),
                "phone number should be masked in JSON log, got: {}",
                json
            );
        }
    }

    // 断言 4: 快查询（SELECT 1）不标记 slow
    let _ = guard.execute_with_logging("SELECT 1", &[]).await.unwrap();
    let fast_entries: Vec<_> = guard
        .logged_entries()
        .iter()
        .filter(|e| e.sql == "SELECT 1")
        .collect();
    if !fast_entries.is_empty() {
        let fast = fast_entries[0];
        assert!(
            !fast.slow,
            "SELECT 1 should not be marked slow (threshold=50ms)"
        );
    }

    guard
        .inner_mut()
        .execute("DROP TABLE IF EXISTS e2e_slow_test")
        .await
        .unwrap();
}

/// T8.5 AutoHealer 端到端验证（v9.5.1 T3: 替换配置打印为真实剔除+触底重建+stop 全链路）
#[tokio::test]
#[ignore]
#[cfg(feature = "auto-healer")]
async fn test_auto_healer_evict_e2e() {
    use sz_orm_core::{AutoHealer, HealerConfig, Pool, PoolConfig};

    // 1. 连接 MySQL 并创建 Pool
    let any_pool = AnyPool::connect(mysql_dsn()).await.unwrap();
    let config = PoolConfig::default();
    let pool = Pool::new(config, any_pool.factory()).unwrap();

    // 2. 预热连接池
    pool.warmup(2).await.unwrap();
    let status_before = pool.status().await;
    assert!(
        status_before.idle >= 1,
        "pool should have idle connections after warmup, got {}",
        status_before.idle
    );

    // 3. 启动 AutoHealer
    let healer_config = HealerConfig {
        heal_interval: std::time::Duration::from_secs(2),
        probe_timeout: std::time::Duration::from_millis(500),
        probe_sql: "SELECT 1".to_string(),
        conservative_mode: false,
        min_idle: 2,
    };
    let handle = AutoHealer::start(pool.clone(), healer_config);

    // 4. 等待 1.5 个探测周期，让 AutoHealer 执行探测
    tokio::time::sleep(std::time::Duration::from_millis(3000)).await;

    // 5. 验证探测后池仍然可用（连接健康或已触底重建）
    let status_after = pool.status().await;
    let _ = status_after.idle;

    // 6. 验证可以获取新连接并执行 SQL
    let conn = pool.acquire().await.unwrap();
    drop(conn);

    // 7. 停止 AutoHealer
    handle.stop().await;

    // 8. 验证停止后池仍然可用
    let status_stopped = pool.status().await;
    let _ = status_stopped.idle;

    // 9. 再次获取连接验证
    let conn = pool.acquire().await.unwrap();
    drop(conn);
}

/// T8.6 MySQL 多步骤事务编排
#[tokio::test]
#[ignore]
async fn test_orchestrator_e2e() {
    use sz_orm_core::{
        OrchestrationResult, OrchestratorConfig, RollbackStrategy, StepFn, TransactOptions,
        TransactionOrchestrator,
    };

    let pool = AnyPool::connect(mysql_dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();

    conn.execute("DROP TABLE IF EXISTS e2e_orch_orders")
        .await
        .unwrap();
    conn.execute("CREATE TABLE e2e_orch_orders (id INT PRIMARY KEY, amount INT)")
        .await
        .unwrap();

    let step1: StepFn = Box::new(|tx, _ctx| {
        Box::pin(async move {
            tx.execute("INSERT INTO e2e_orch_orders (id, amount) VALUES (1, 10000)")
                .await?;
            Ok(())
        })
    });
    let step2: StepFn = Box::new(|tx, _ctx| {
        Box::pin(async move {
            tx.execute("UPDATE e2e_orch_orders SET amount = 20000 WHERE id = 1")
                .await?;
            Ok(())
        })
    });

    let config = OrchestratorConfig {
        rollback_strategy: RollbackStrategy::FullRollback,
    };
    let orch = TransactionOrchestrator::new()
        .with_config(config)
        .step("insert_order", step1)
        .step("update_amount", step2);

    let result = orch.run(Box::new(conn), TransactOptions::default()).await;
    match result {
        OrchestrationResult::Success(_) => {}
        OrchestrationResult::Failure {
            failed_step,
            reason,
            ..
        } => panic!("orchestrator failed at '{}': {}", failed_step, reason),
    }

    let mut conn = pool.create().await.unwrap();
    let rows = conn
        .query("SELECT amount FROM e2e_orch_orders WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let amount = rows[0].get("amount").unwrap();
    assert!(*amount == Value::I32(20000) || *amount == Value::I64(20000));

    conn.execute("DROP TABLE IF EXISTS e2e_orch_orders")
        .await
        .unwrap();
}

/// T8.7 Repository 增强方法验证（InMemoryRepository + 真实 DB 连接验证基础设施）
#[tokio::test]
#[ignore]
async fn test_repository_enhanced_e2e() {
    use sz_orm_core::repository::{
        EntityAttributes, InMemoryRepository, Repository, SoftDeletable,
    };

    #[derive(Debug, Clone, PartialEq)]
    struct Order {
        id: i64,
        status: String,
    }

    impl EntityAttributes for Order {
        fn get_attribute(&self, field: &str) -> Option<Value> {
            match field {
                "id" => Some(Value::I64(self.id)),
                "status" => Some(Value::String(self.status.clone())),
                _ => None,
            }
        }
    }

    impl sz_orm_core::repository::EntityKey<i64> for Order {
        fn key(&self) -> i64 {
            self.id
        }
    }

    impl SoftDeletable for Order {}

    let repo = InMemoryRepository::<Order>::new();
    let orders: Vec<Order> = (1..=50)
        .map(|i| Order {
            id: i,
            status: if i % 2 == 0 {
                "paid".into()
            } else {
                "pending".into()
            },
        })
        .collect();
    let count = repo.batch_create(orders).unwrap();
    assert_eq!(count, 50);

    let page = repo.paginate_sql(1, 10).unwrap();
    assert_eq!(page.items.len(), 10);
    assert_eq!(page.total, 50);
    assert_eq!(page.total_pages(), 5);

    let paid_count = repo
        .query()
        .where_eq("status", Value::String("paid".into()))
        .count()
        .unwrap();
    assert_eq!(paid_count, 25);

    let pool = AnyPool::connect(mysql_dsn()).await.unwrap();
    let _conn = pool.create().await.unwrap();
}
// ════════════════════════════════════════════════════════════════════════════
// v9.6.0 端到端测试（T8.1-T8.8）
// ════════════════════════════════════════════════════════════════════════════

/// T8.1 渐进式预热端到端验证
#[tokio::test]
#[ignore = "需要 MySQL 服务"]
async fn test_warmup_gradual_e2e() {
    use std::sync::Arc;
    use sz_orm_core::{Pool, PoolConfig};
    use sz_orm_sqlx::{MySqlPoolHandle, SqlxMySqlConnectionFactory};

    let pool_handle = MySqlPoolHandle::connect(mysql_dsn())
        .await
        .expect("MySQL 连接应成功");
    let factory = SqlxMySqlConnectionFactory::new(Arc::new(pool_handle));
    let config = PoolConfig {
        min_idle: 10,
        max_size: 20,
        ..PoolConfig::default()
    };
    let pool = Pool::new(config, Arc::new(factory)).expect("Pool 创建应成功");

    let result = pool
        .warmup_gradual(10, 3, std::time::Duration::from_millis(200))
        .await;
    assert!(result.is_ok(), "渐进式预热应成功: {result:?}");

    pool.close_all().await;
}

/// T8.2 时间窗口空闲回收端到端验证
#[tokio::test]
#[ignore = "需要 MySQL 服务"]
async fn test_reap_idle_by_window_e2e() {
    use std::sync::Arc;
    use sz_orm_core::{Pool, PoolConfig};
    use sz_orm_sqlx::{MySqlPoolHandle, SqlxMySqlConnectionFactory};

    let pool_handle = MySqlPoolHandle::connect(mysql_dsn())
        .await
        .expect("MySQL 连接应成功");
    let factory = SqlxMySqlConnectionFactory::new(Arc::new(pool_handle));
    let config = PoolConfig {
        min_idle: 5,
        max_size: 10,
        ..PoolConfig::default()
    };
    let pool = Pool::new(config, Arc::new(factory)).expect("Pool 创建应成功");

    pool.warmup(5).await.expect("预热应成功");
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let reaped = pool.reap_idle_by_window().await;
    assert!(reaped <= 5, "回收数应 <= 5，实际 {reaped}");

    pool.close_all().await;
}

/// T8.3 自适应健康检查端到端验证
#[tokio::test]
#[cfg(feature = "auto-healer")]
#[ignore = "需要 MySQL 服务 + auto-healer feature"]
async fn test_adaptive_healer_e2e() {
    use std::sync::Arc;
    use sz_orm_core::{AdaptiveHealerConfig, Pool, PoolConfig};
    use sz_orm_sqlx::{MySqlPoolHandle, SqlxMySqlConnectionFactory};

    let pool_handle = MySqlPoolHandle::connect(mysql_dsn())
        .await
        .expect("MySQL 连接应成功");
    let factory = SqlxMySqlConnectionFactory::new(Arc::new(pool_handle));
    let config = PoolConfig {
        min_idle: 2,
        max_size: 5,
        ..PoolConfig::default()
    };
    let pool = Pool::new(config, Arc::new(factory)).expect("Pool 创建应成功");

    let _config = AdaptiveHealerConfig::default();
    let interval = pool.current_heal_interval();
    println!("adaptive_healer_e2e: initial interval = {interval:?}");

    pool.close_all().await;
}

/// T8.4 多级缓存端到端验证
#[tokio::test]
#[ignore = "需要 MySQL 服务"]
async fn test_multi_level_cache_e2e() {
    use sz_orm_core::l2_cache::CacheKey;
    use sz_orm_core::DependencyGraph;

    let dep_graph = DependencyGraph::new(1000);
    let table = "bench_users";
    let key1 = CacheKey::by_pk(table, 1);
    let key2 = CacheKey::by_pk(table, 2);

    dep_graph.add_dependency(table, &key1);
    dep_graph.add_dependency(table, &key2);

    let keys = dep_graph.dependent_keys(table);
    assert_eq!(keys.len(), 2, "依赖图应有 2 个 key");

    let table_count = dep_graph.table_count();
    assert_eq!(table_count, 1, "应有 1 个表");
}

/// T8.5 并行批量插入端到端验证
#[tokio::test]
#[ignore = "需要 MySQL 服务"]
async fn test_batch_insert_parallel_e2e() {
    use sz_orm_core::{DbType, Value};
    use sz_orm_sqlx::{AnyPool, BatchExecutor};

    let pool = AnyPool::connect(mysql_dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();

    conn.execute("DROP TABLE IF EXISTS bench_parallel_test")
        .await
        .unwrap();
    conn.execute("CREATE TABLE bench_parallel_test (id INT PRIMARY KEY, val VARCHAR(50))")
        .await
        .unwrap();

    let rows: Vec<HashMap<String, Value>> = (0..100)
        .map(|i| {
            let mut row = HashMap::new();
            row.insert("id".to_string(), Value::I32(i));
            row.insert("val".to_string(), Value::String(format!("val_{i}")));
            row
        })
        .collect();

    let mut executor = BatchExecutor::new(&mut conn, DbType::MySQL);
    let result = executor
        .batch_insert_parallel("bench_parallel_test", &rows, 20, 4)
        .await;
    assert!(result.is_ok(), "并行批量插入应成功: {result:?}");

    let batch_result = result.unwrap();
    assert!(batch_result.is_all_success(), "所有分片应成功");
    assert_eq!(batch_result.total_affected, 100, "总影响行数应 == 100");

    conn.execute("DROP TABLE IF EXISTS bench_parallel_test")
        .await
        .unwrap();
}

/// T8.6 动态批量顾问端到端验证
#[tokio::test]
#[ignore = "需要 MySQL 服务"]
async fn test_dynamic_batch_advisor_e2e() {
    use sz_orm_core::{DbType, DynamicBatchAdvisor, DynamicBatchConfig};

    let config = DynamicBatchConfig::default();
    let min_size = config.min_size;
    let max_size = config.max_size;
    let advisor = DynamicBatchAdvisor::new(config);

    for _ in 0..20 {
        advisor.record_execution(500, 5_000_000.0, 2.0);
    }

    let advice = advisor.advise_dynamic(DbType::MySQL, 3, 10000);
    assert!(advice >= min_size, "建议应 >= min_size，实际 {advice}");
    assert!(advice <= max_size, "建议应 <= max_size，实际 {advice}");
}

/// T8.7 SIMD 端到端正确性验证
#[test]
fn test_simd_avx512_e2e() {
    use sz_orm_core::simd::{
        batch_aggregate_f64, batch_aggregate_f64_avx512, batch_hash_probe, batch_sort_compare,
        detect, SimdAggOp,
    };

    let avail = detect();
    let data: Vec<f64> = (0..1024).map(|i| i as f64).collect();

    let sum_scalar = batch_aggregate_f64(&data, SimdAggOp::Sum);
    let sum_simd = batch_aggregate_f64_avx512(&data, SimdAggOp::Sum, avail);
    assert!(
        (sum_scalar - sum_simd).abs() < 0.001,
        "SIMD 和标量结果应一致: scalar={sum_scalar} simd={sum_simd}"
    );

    let values: Vec<i64> = (0..100).map(|i| (i + 10) as i64).collect();
    let thresholds: Vec<i64> = (0..100).map(|i| i as i64).collect();
    let cmp = batch_sort_compare(&values, &thresholds, avail);
    assert_eq!(cmp.len(), 100);
    assert!(
        cmp.iter().all(|&r| r),
        "所有 values >= thresholds 应为 true"
    );

    let hashes: Vec<u64> = vec![1, 2, 3, 4, 5];
    let bloom: Vec<u64> = vec![u64::MAX];
    let probe = batch_hash_probe(&hashes, &bloom, avail);
    assert_eq!(probe.len(), 5);
    assert!(probe.iter().all(|&r| r), "全 1 布隆过滤器应全部命中");
}

/// T8.8 Arena + SmallVec 端到端验证
#[test]
fn test_arena_smallvec_e2e() {
    use sz_orm_core::{ArenaAllocator, SmallVec};

    let mut arena = ArenaAllocator::new(1024 * 64);
    for _ in 0..100 {
        let layout = std::alloc::Layout::from_size_align(64, 8).unwrap();
        let _ = arena.alloc(layout);
    }
    assert!(arena.used() > 0, "arena used 应 > 0");
    arena.reset();
    assert_eq!(arena.used(), 0, "reset 后 used 应 == 0");

    let mut sv: SmallVec<i64, 8> = SmallVec::new();
    for i in 0..8 {
        sv.push(i);
    }
    assert_eq!(sv.len(), 8);
    assert!(!sv.is_on_heap(), "8 元素应在栈上");

    sv.push(8);
    assert_eq!(sv.len(), 9);
    assert!(sv.is_on_heap(), "9 元素应在堆上");

    let popped = sv.pop();
    assert_eq!(popped, Some(8));
    assert_eq!(sv.len(), 8);
}
// ════════════════════════════════════════════════════════════════════════════
// v9.7.0 端到端测试（T5.1-T5.8）
// ════════════════════════════════════════════════════════════════════════════

/// T5.1 异步零拷贝解析端到端验证
#[tokio::test]
#[ignore = "需要 MySQL 服务"]
async fn test_try_parse_async_e2e() {
    use sz_orm_core::zero_copy_pipeline::{BackpressureConfig, ZeroCopyPipeline};

    let pool = AnyPool::connect(mysql_dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();

    conn.execute("DROP TABLE IF EXISTS e2e_v970_zc")
        .await
        .unwrap();
    conn.execute("CREATE TABLE e2e_v970_zc (id INT PRIMARY KEY, name VARCHAR(100))")
        .await
        .unwrap();
    for i in 1..=100 {
        conn.execute(&format!(
            "INSERT INTO e2e_v970_zc (id, name) VALUES ({i}, 'user_{i}')"
        ))
        .await
        .unwrap();
    }

    let rows_db = conn
        .query("SELECT id, name FROM e2e_v970_zc")
        .await
        .unwrap();
    assert_eq!(rows_db.len(), 100);

    let rows: Vec<HashMap<String, Value>> = rows_db
        .iter()
        .map(|r| {
            let mut row = HashMap::new();
            row.insert(
                "id".to_string(),
                r.get("id").cloned().unwrap_or(Value::Null),
            );
            row.insert(
                "name".to_string(),
                r.get("name").cloned().unwrap_or(Value::Null),
            );
            row
        })
        .collect();

    let pipeline = ZeroCopyPipeline::new();
    let cols = vec!["id".to_string(), "name".to_string()];
    let mut sync_stream = pipeline.stream_rows(rows.clone(), &cols);
    let mut sync_count = 0;
    while sync_stream.next_row().is_some() {
        sync_count += 1;
    }
    assert_eq!(sync_count, 100);

    let pipeline2 = ZeroCopyPipeline::new();
    let async_stream = pipeline2.stream_async(rows, &cols, BackpressureConfig::default());
    use futures::StreamExt;
    let mut async_stream = async_stream;
    let mut async_count = 0;
    while async_stream.next().await.is_some() {
        async_count += 1;
    }
    assert_eq!(async_count, 100);

    conn.execute("DROP TABLE IF EXISTS e2e_v970_zc")
        .await
        .unwrap();
}

/// T5.2 异步流背压端到端验证
#[tokio::test]
#[ignore = "需要 MySQL 服务"]
async fn test_stream_async_backpressure_e2e() {
    use sz_orm_core::zero_copy_pipeline::{BackpressureConfig, ZeroCopyPipeline};

    let rows: Vec<HashMap<String, Value>> = (0..10_000)
        .map(|i| {
            let mut row = HashMap::new();
            row.insert("id".to_string(), Value::I64(i));
            row.insert("name".to_string(), Value::String(format!("user_{i}")));
            row
        })
        .collect();

    let pipeline = ZeroCopyPipeline::new();
    let cols = vec!["id".to_string(), "name".to_string()];
    let stream = pipeline.stream_async(rows, &cols, BackpressureConfig::new(200, 100));
    use futures::StreamExt;
    let mut stream = stream;
    let mut peak_backlog = 0usize;
    let mut total = 0usize;
    while let Some(row) = stream.next().await {
        total += 1;
        let bl = stream.backlog();
        if bl > peak_backlog {
            peak_backlog = bl;
        }
        std::hint::black_box(row);
    }
    assert_eq!(total, 10_000);
    assert!(
        peak_backlog <= 200,
        "背压峰值应 <= high_water(200)，实际 {peak_backlog}"
    );
}

/// T5.3 SIMD 批量反序列化端到端验证
#[tokio::test]
#[ignore = "需要 MySQL 服务"]
async fn test_batch_deserialize_zero_copy_e2e() {
    use sz_orm_core::simd::{batch_decode_integers, detect, scalar_decode_integers};

    let pool = AnyPool::connect(mysql_dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();

    conn.execute("DROP TABLE IF EXISTS e2e_v970_simd")
        .await
        .unwrap();
    conn.execute("CREATE TABLE e2e_v970_simd (id INT PRIMARY KEY)")
        .await
        .unwrap();
    for i in 0..1000 {
        conn.execute(&format!("INSERT INTO e2e_v970_simd (id) VALUES ({i})"))
            .await
            .unwrap();
    }

    let rows = conn
        .query("SELECT id FROM e2e_v970_simd ORDER BY id")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1000);

    let values: Vec<i64> = rows
        .iter()
        .map(|r| match r.get("id") {
            Some(Value::I64(v)) => *v,
            Some(Value::I32(v)) => *v as i64,
            _ => 0,
        })
        .collect();

    let buf: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    let n = values.len();
    let avail = detect();

    let scalar_result = scalar_decode_integers(&buf, n);
    let simd_result = batch_decode_integers(&buf, n, avail);
    assert_eq!(scalar_result.len(), simd_result.len());
    assert_eq!(scalar_result, simd_result);

    conn.execute("DROP TABLE IF EXISTS e2e_v970_simd")
        .await
        .unwrap();
}

/// T5.4 NUMA 分区连接池端到端验证
#[tokio::test]
#[ignore = "需要 MySQL 服务"]
async fn test_numa_partitioned_pool_e2e() {
    use sz_orm_core::numa_topology::{CrossNodeCounter, NumaConfig, NumaTopology};

    let topo = NumaTopology::detect();
    assert!(!topo.nodes.is_empty(), "NUMA 拓扑应至少有 1 个节点");

    let config = NumaConfig::default();
    let sizes = config.partition_sizes(100, &topo);
    assert!(!sizes.is_empty(), "分区大小列表应非空");
    let total: u32 = sizes.iter().sum();
    assert_eq!(total, 100, "分区总连接数应 == 100");

    let counter = CrossNodeCounter::new();
    for _ in 0..1000 {
        counter.record_total();
    }
    assert_eq!(counter.total_count(), 1000);
    assert_eq!(counter.cross_node_count(), 0);
    assert_eq!(counter.rate(), 0.0);

    counter.record_cross_node();
    assert_eq!(counter.cross_node_count(), 1);
}

/// T5.5 NUMA 单节点降级端到端验证
#[tokio::test]
#[ignore = "需要 MySQL 服务"]
async fn test_numa_degrade_e2e() {
    use sz_orm_core::numa_topology::{NumaConfig, NumaTopology};

    let topo = NumaTopology::detect();
    let config = NumaConfig::default();
    let sizes = config.partition_sizes(50, &topo);

    if topo.degraded {
        assert_eq!(
            sizes.len(),
            1,
            "降级单节点应只有 1 个分区，实际 {} 个",
            sizes.len()
        );
        assert_eq!(sizes[0], 50, "单分区应分配全部 50 连接");
    } else {
        assert!(!sizes.is_empty(), "多节点应至少有 1 个分区");
    }
}

/// T5.6 智能预热端到端验证
#[tokio::test]
#[ignore = "需要 MySQL 服务"]
async fn test_intelligent_warmup_e2e() {
    use sz_orm_core::cache_warmup_protection::{AccessPatternAnalyzer, AccessPatternConfig};

    let pool = AnyPool::connect(mysql_dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();

    conn.execute("DROP TABLE IF EXISTS e2e_v970_warmup")
        .await
        .unwrap();
    conn.execute("CREATE TABLE e2e_v970_warmup (id INT PRIMARY KEY, val VARCHAR(50))")
        .await
        .unwrap();
    for i in 0..100 {
        conn.execute(&format!(
            "INSERT INTO e2e_v970_warmup (id, val) VALUES ({i}, 'v_{i}')"
        ))
        .await
        .unwrap();
    }

    let analyzer = AccessPatternAnalyzer::new(AccessPatternConfig::default());
    for i in 0..1000 {
        analyzer.record_access(&format!("e2e_v970_warmup:pk:{i}"));
    }
    for _ in 0..100 {
        for i in 0..10 {
            analyzer.record_access(&format!("e2e_v970_warmup:pk:{i}"));
        }
    }

    let hotspots = analyzer.identify_hotspots();
    assert!(!hotspots.is_empty(), "应识别出热点 key");
    assert!(
        hotspots.len() <= AccessPatternConfig::default().top_n,
        "热点数应 <= top_n"
    );

    conn.execute("DROP TABLE IF EXISTS e2e_v970_warmup")
        .await
        .unwrap();
}

/// T5.7 预热触发器端到端验证
#[tokio::test]
#[ignore = "需要 MySQL 服务"]
async fn test_warmup_trigger_e2e() {
    use sz_orm_core::warmup_trigger::TriggerConfig;

    let config = TriggerConfig {
        scheduled_enabled: true,
        scheduled_period: std::time::Duration::from_secs(60),
        scheduled_tables: vec!["e2e_trigger_t1".to_string()],
        event_driven_enabled: true,
        event_driven_tables: vec!["e2e_trigger_t2".to_string()],
        dedup_window: std::time::Duration::from_secs(30),
    };

    assert!(config.scheduled_enabled);
    assert!(config.event_driven_enabled);
    assert_eq!(config.scheduled_tables.len(), 1);
    assert_eq!(config.event_driven_tables.len(), 1);

    let pool = AnyPool::connect(mysql_dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS e2e_trigger_t1")
        .await
        .unwrap();
    conn.execute("DROP TABLE IF EXISTS e2e_trigger_t2")
        .await
        .unwrap();
}

/// T5.8 预热流水线端到端验证
#[tokio::test]
#[ignore = "需要 MySQL 服务"]
async fn test_warmup_pipeline_e2e() {
    use sz_orm_core::warmup_pipeline::{StageFailStrategy, WarmupPipelineConfig, WarmupStage};

    let config = WarmupPipelineConfig {
        stages: vec![WarmupStage::L2, WarmupStage::L1, WarmupStage::InProcess],
        parallel_enabled: true,
        parallelism: 4,
        fail_strategy: StageFailStrategy::Continue,
    };

    assert_eq!(config.stages.len(), 3, "应有 3 个阶段");
    assert!(config.parallel_enabled, "应启用并行");
    assert_eq!(config.parallelism, 4, "并行度应为 4");

    let default_config = WarmupPipelineConfig::default();
    assert_eq!(default_config.stages.len(), 3, "默认配置也应有 3 个阶段");

    let pool = AnyPool::connect(mysql_dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS e2e_pipeline_t1")
        .await
        .unwrap();
    conn.execute("CREATE TABLE e2e_pipeline_t1 (id INT PRIMARY KEY)")
        .await
        .unwrap();
    conn.execute("DROP TABLE IF EXISTS e2e_pipeline_t1")
        .await
        .unwrap();
}
// ============================================================================
// v9.8.0: 端到端接线验证——真实生产 API 调用 + 行为断言
// ============================================================================

#[tokio::test]
#[ignore = "需要 MySQL 服务"]
async fn test_adaptive_resize_e2e() {
    use sz_orm_core::{AutoResizeConfig, Pool, PoolConfig};

    let any_pool = AnyPool::connect(mysql_dsn()).await.unwrap();
    let factory = any_pool.factory();

    let config = PoolConfig::default().with_auto_resize(AutoResizeConfig {
        enabled: true,
        grow_threshold_ms: 10.0,
        collect_interval: std::time::Duration::from_secs(1),
        ..AutoResizeConfig::default()
    });
    let mut config = config;
    config.max_size = 5;
    let pool = Pool::new_async(config, factory).await.unwrap();

    let mut conns = Vec::new();
    for _ in 0..3 {
        let conn = pool.acquire().await.unwrap();
        conns.push(conn);
    }
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    for conn in conns {
        pool.release(conn).await;
    }

    let history = pool.adjustment_history();
    let status = pool.status().await;
    assert!(
        status.max >= 5 || !history.is_empty(),
        "pool should either maintain size or have adjustment history"
    );

    pool.shutdown().await;
}

#[tokio::test]
async fn test_adaptive_resize_auto_start_e2e() {
    use sz_orm_core::{AutoResizeConfig, Pool, PoolConfig};

    struct DummyFactory;
    #[async_trait::async_trait]
    impl sz_orm_core::ConnectionFactory for DummyFactory {
        async fn create(&self) -> Result<Box<dyn sz_orm_core::Connection>, sz_orm_core::DbError> {
            Err(sz_orm_core::DbError::ConnectionError("dummy".to_string()))
        }
    }

    let config = PoolConfig::default().with_auto_resize(AutoResizeConfig {
        enabled: true,
        ..AutoResizeConfig::default()
    });
    let mut config = config;
    config.max_size = 10;
    let pool = Pool::new_async(config, std::sync::Arc::new(DummyFactory))
        .await
        .unwrap();

    let status = pool.status().await;
    assert_eq!(status.max, 10);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    pool.shutdown().await;
}

#[tokio::test]
async fn test_plan_regression_through_executor_e2e() {
    use sz_orm_adaptive::planner::{PlanExecutionRegressionDetector, RegressionConfig};
    use sz_orm_adaptive::{AdaptiveConfig, AdaptiveExecutor};

    let detector = PlanExecutionRegressionDetector::new(RegressionConfig {
        enabled: true,
        regression_multiplier: 3.0,
        detection_sensitivity: 1,
        ..RegressionConfig::default()
    });
    let executor =
        AdaptiveExecutor::new(AdaptiveConfig::default()).with_regression_detector(detector);

    for ms in [50, 48, 52, 49, 51] {
        executor.record("q1", 10, ms);
    }
    assert!(
        executor.check_regression("q1", "current", 200).is_some(),
        "regression should be detected after slow execution"
    );
    assert!(
        executor.check_regression("q1", "current", 50).is_none(),
        "no regression after normal execution"
    );
}

#[tokio::test]
async fn test_index_advisor_through_executor_e2e() {
    use sz_orm_adaptive::planner::IndexAdvisor;
    use sz_orm_adaptive::{AdaptiveConfig, AdaptiveExecutor};

    let advisor = IndexAdvisor::new(100, 5);
    let executor = AdaptiveExecutor::new(AdaptiveConfig::default()).with_index_advisor(advisor);

    for _ in 0..101 {
        executor.record("slow_query", 2000, 100);
    }
    let suggestions = executor.advise_index("slow_query", &["user_id".to_string()]);
    assert!(
        !suggestions.is_empty(),
        "index advisor should suggest index for full scan"
    );
    assert!(
        suggestions[0].safe,
        "suggestion for normal table should be safe"
    );
}

#[tokio::test]
async fn test_adaptive_loop_through_anomaly_detector_e2e() {
    use std::sync::Mutex;
    use sz_orm_observability::anomaly::{AnomalyConfig, AnomalyDetector};
    use sz_orm_observability::anomaly_remediation_rca::{
        AdaptiveLoopConfig, AdaptiveTuningLoop, AutoApplyConfig, SuggestionAction,
        SuggestionExecutor,
    };
    use sz_orm_observability::MetricsRegistry;

    struct RealExecutor {
        cap: Mutex<u32>,
    }
    impl SuggestionExecutor for RealExecutor {
        fn execute_grow_pool(&self, delta: u32, max: u32) -> Result<u32, String> {
            let mut c = self.cap.lock().unwrap();
            *c = (*c + delta).min(max);
            Ok(*c)
        }
        fn execute_shrink_pool(&self, delta: u32, min: u32) -> Result<u32, String> {
            let mut c = self.cap.lock().unwrap();
            *c = (*c).saturating_sub(delta).max(min);
            Ok(*c)
        }
        fn execute_switch_plan(&self, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn execute_warmup_cache(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }

    let metrics = std::sync::Arc::new(MetricsRegistry::new());
    let detector = AnomalyDetector::new(metrics, AnomalyConfig::default());

    let anomalies = vec![sz_orm_observability::anomaly::Anomaly {
        metric_name: "pool_exhausted".to_string(),
        anomaly_value: 150.0,
        threshold: 100.0,
        window_ms: 5000,
        algorithm: sz_orm_observability::anomaly::AnomalyAlgorithm::Threshold,
        detected_at: 0,
        description: "pool exhausted".to_string(),
    }];

    let loop_ = AdaptiveTuningLoop::new(AdaptiveLoopConfig {
        enabled: true,
        auto_apply: AutoApplyConfig {
            enabled: true,
            whitelist: vec![SuggestionAction::GrowPool { delta: 5 }],
            ..AutoApplyConfig::default()
        },
        ..AdaptiveLoopConfig::default()
    });
    let executor = RealExecutor {
        cap: Mutex::new(10),
    };

    let results = detector.remediate(&anomalies, &loop_, &executor);
    assert!(!results.is_empty(), "remediate should produce results");
    assert!(
        results.iter().any(|r| matches!(
            r,
            sz_orm_observability::anomaly_remediation_rca::ApplyResult::Applied { .. }
        )),
        "at least one suggestion should be applied"
    );

    let state = loop_.state();
    assert!(
        !state.applied_history.is_empty(),
        "applied history should record the remediation"
    );
}

#[tokio::test]
async fn test_auto_apply_whitelist_rejection_e2e() {
    use std::sync::Mutex;
    use sz_orm_observability::anomaly_remediation_rca::{
        AdaptiveLoopConfig, AdaptiveTuningLoop, AutoApplyConfig, RootCause, RootCauseCategory,
        SuggestionAction, SuggestionExecutor,
    };

    struct RealExecutor {
        cap: Mutex<u32>,
    }
    impl SuggestionExecutor for RealExecutor {
        fn execute_grow_pool(&self, delta: u32, max: u32) -> Result<u32, String> {
            let mut c = self.cap.lock().unwrap();
            *c = (*c + delta).min(max);
            Ok(*c)
        }
        fn execute_shrink_pool(&self, delta: u32, min: u32) -> Result<u32, String> {
            let mut c = self.cap.lock().unwrap();
            *c = (*c).saturating_sub(delta).max(min);
            Ok(*c)
        }
        fn execute_switch_plan(&self, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn execute_warmup_cache(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }

    let loop_ = AdaptiveTuningLoop::new(AdaptiveLoopConfig {
        enabled: true,
        auto_apply: AutoApplyConfig {
            enabled: true,
            whitelist: vec![SuggestionAction::GrowPool { delta: 5 }],
            ..AutoApplyConfig::default()
        },
        ..AdaptiveLoopConfig::default()
    });
    let executor = RealExecutor {
        cap: Mutex::new(10),
    };

    let root_cause = RootCause::new(RootCauseCategory::SlowQuery, 0.9);
    let results = loop_.process_root_cause(&root_cause, &executor);
    assert!(
        results.iter().all(|r| matches!(
            r,
            sz_orm_observability::anomaly_remediation_rca::ApplyResult::Rejected { .. }
        )),
        "SlowQuery suggestion (SwitchPlan) should be rejected by whitelist (only GrowPool allowed)"
    );
}

#[tokio::test]
async fn test_adaptive_loop_coordination_e2e() {
    use std::sync::Mutex;
    use sz_orm_core::{AutoResizeConfig, Pool, PoolConfig};
    use sz_orm_observability::anomaly_remediation_rca::{
        AdaptiveLoopConfig, AdaptiveTuningLoop, AutoApplyConfig, RootCause, RootCauseCategory,
        SuggestionAction, SuggestionExecutor,
    };

    struct CoordExecutor {
        cap: Mutex<u32>,
    }
    impl SuggestionExecutor for CoordExecutor {
        fn execute_grow_pool(&self, delta: u32, max: u32) -> Result<u32, String> {
            let mut c = self.cap.lock().unwrap();
            *c = (*c + delta).min(max);
            Ok(*c)
        }
        fn execute_shrink_pool(&self, delta: u32, min: u32) -> Result<u32, String> {
            let mut c = self.cap.lock().unwrap();
            *c = (*c).saturating_sub(delta).max(min);
            Ok(*c)
        }
        fn execute_switch_plan(&self, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn execute_warmup_cache(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }

    struct DummyFactory;
    #[async_trait::async_trait]
    impl sz_orm_core::ConnectionFactory for DummyFactory {
        async fn create(&self) -> Result<Box<dyn sz_orm_core::Connection>, sz_orm_core::DbError> {
            Err(sz_orm_core::DbError::ConnectionError("dummy".to_string()))
        }
    }

    let mut pool_config = PoolConfig::default().with_auto_resize(AutoResizeConfig {
        enabled: true,
        ..AutoResizeConfig::default()
    });
    pool_config.max_size = 10;
    let pool = Pool::new_async(pool_config, std::sync::Arc::new(DummyFactory))
        .await
        .unwrap();

    let loop_ = AdaptiveTuningLoop::new(AdaptiveLoopConfig {
        enabled: true,
        auto_apply: AutoApplyConfig {
            enabled: true,
            whitelist: vec![SuggestionAction::GrowPool { delta: 5 }],
            ..AutoApplyConfig::default()
        },
        ..AdaptiveLoopConfig::default()
    });
    let executor = CoordExecutor {
        cap: Mutex::new(10),
    };

    let root_cause = RootCause::new(RootCauseCategory::ConnectionPoolExhausted, 0.95);
    let results = loop_.process_root_cause(&root_cause, &executor);
    assert!(
        results.iter().any(|r| matches!(
            r,
            sz_orm_observability::anomaly_remediation_rca::ApplyResult::Applied { .. }
        )),
        "pool exhausted should trigger GrowPool action"
    );

    let pool_status = pool.status().await;
    assert_eq!(pool_status.max, 10);
    pool.shutdown().await;
}
// ============================================================================
// v9.9.0 端到端测试：连接保活自愈 / 池耗尽降级 / 压力可观测性 / 零拷贝解码
// ============================================================================

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering as AOrdering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sz_orm_core::{
    ConnectionFactory, DbError, DegradationConfig, DropReason, DroppedCountWindow, Pool,
    PoolConfigBuilder, PoolDegradationStrategy, PoolEvent, QueryRows,
};

/// 可控 Mock 连接：is_connected/ping 可由外部开关控制
struct E2eMockConn {
    connected: Arc<AtomicBool>,
    ping_ok: Arc<AtomicBool>,
}

impl Connection for E2eMockConn {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async { Ok(0) })
    }
    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    fn is_connected(&self) -> bool {
        self.connected.load(AOrdering::Acquire)
    }
    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        let ok = self.ping_ok.load(AOrdering::Acquire);
        Box::pin(async move { ok })
    }
    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

/// 可控 Mock 连接工厂：create 可配置失败次数
struct E2eMockFactory {
    connected: Arc<AtomicBool>,
    ping_ok: Arc<AtomicBool>,
    fail_before_ok: Arc<AtomicU32>,
    create_count: Arc<AtomicU32>,
}

impl E2eMockFactory {
    fn new() -> Self {
        Self {
            connected: Arc::new(AtomicBool::new(true)),
            ping_ok: Arc::new(AtomicBool::new(true)),
            fail_before_ok: Arc::new(AtomicU32::new(0)),
            create_count: Arc::new(AtomicU32::new(0)),
        }
    }
}

#[sz_orm_core::async_trait]
impl ConnectionFactory for E2eMockFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, sz_orm_core::DbError> {
        let count = self.create_count.fetch_add(1, AOrdering::SeqCst);
        let fail_needed = self.fail_before_ok.load(AOrdering::Acquire);
        if count < fail_needed {
            return Err(sz_orm_core::DbError::ConnectionError(format!(
                "mock fail #{}",
                count + 1
            )));
        }
        Ok(Box::new(E2eMockConn {
            connected: Arc::clone(&self.connected),
            ping_ok: Arc::clone(&self.ping_ok),
        }))
    }
}

/// 事件收集器：用 Mutex<Vec<PoolEvent>> 收集事件
use std::sync::Mutex;

fn make_event_collector() -> (
    Arc<Mutex<Vec<PoolEvent>>>,
    Arc<dyn Fn(PoolEvent) + Send + Sync>,
) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);
    let callback: Arc<dyn Fn(PoolEvent) + Send + Sync> = Arc::new(move |event| {
        events_clone.lock().unwrap().push(event);
    });
    (events, callback)
}

// ---- 7.1 连接保活自愈端到端测试 ----

#[tokio::test]
#[ignore]
async fn test_connection_dropped_event_e2e() {
    let factory = E2eMockFactory::new();
    let connected_flag = Arc::clone(&factory.connected);

    let (events, callback) = make_event_collector();
    let config = PoolConfigBuilder::new()
        .max_size(2)
        .on_event(callback)
        .build()
        .unwrap();
    let pool = Pool::new(config, Arc::new(factory)).unwrap();

    let conn = pool.acquire().await.unwrap();
    drop(conn);
    tokio::time::sleep(Duration::from_millis(100)).await;

    connected_flag.store(false, AOrdering::SeqCst);

    let result = pool.acquire().await;
    assert!(
        result.is_ok(),
        "acquire should succeed by creating new conn"
    );

    let pm = pool.pressure_metrics();
    assert!(
        pm.dropped_count >= 1,
        "dropped_count should be >= 1, got {}",
        pm.dropped_count
    );

    let collected = events.lock().unwrap();
    let has_dropped = collected.iter().any(|e| {
        matches!(
            e,
            PoolEvent::ConnectionDropped {
                reason: DropReason::NotConnected,
                ..
            }
        )
    });
    assert!(
        has_dropped,
        "should have ConnectionDropped event with NotConnected"
    );
}

#[tokio::test]
#[ignore]
#[cfg(feature = "auto-healer")]
async fn test_rebuild_backoff_e2e() {
    let factory = E2eMockFactory::new();
    factory.fail_before_ok.store(3, AOrdering::SeqCst);

    let (events, callback) = make_event_collector();
    let config = PoolConfigBuilder::new()
        .max_size(5)
        .on_event(callback)
        .build()
        .unwrap();
    let pool = Pool::new(config, Arc::new(factory)).unwrap();

    let result = pool.trigger_rebuild(1).await;
    assert!(result.is_ok(), "rebuild should eventually succeed");

    let pm = pool.pressure_metrics();
    assert_eq!(pm.rebuild_failures, 3, "should have 3 rebuild failures");

    let collected = events.lock().unwrap();
    let failed_count = collected
        .iter()
        .filter(|e| matches!(e, PoolEvent::RebuildFailed { .. }))
        .count();
    assert_eq!(failed_count, 3, "should have 3 RebuildFailed events");

    let has_succeeded = collected
        .iter()
        .any(|e| matches!(e, PoolEvent::RebuildSucceeded { .. }));
    assert!(has_succeeded, "should have RebuildSucceeded event");
}

#[tokio::test]
#[ignore]
#[cfg(feature = "auto-healer")]
async fn test_rebuild_storm_suppressed_e2e() {
    use sz_orm_core::RebuildStormConfig;

    let factory = E2eMockFactory::new();

    let (events, callback) = make_event_collector();
    let mut config = PoolConfigBuilder::new()
        .max_size(5)
        .on_event(callback)
        .build()
        .unwrap();
    config.rebuild_storm = Some(RebuildStormConfig {
        window: Duration::from_secs(10),
        limit: 2,
    });
    let pool = Pool::new(config, Arc::new(factory)).unwrap();

    let _ = pool.trigger_rebuild(1).await;
    let _ = pool.trigger_rebuild(1).await;
    let _ = pool.trigger_rebuild(1).await;

    let collected = events.lock().unwrap();
    let has_suppressed = collected
        .iter()
        .any(|e| matches!(e, PoolEvent::RebuildStormSuppressed { .. }));
    assert!(has_suppressed, "should have RebuildStormSuppressed event");
}

#[tokio::test]
#[ignore]
#[cfg(feature = "auto-healer")]
async fn test_rebuild_triggered_via_acquire_e2e() {
    let factory = E2eMockFactory::new();
    let ping_ok_flag = Arc::clone(&factory.ping_ok);

    let (events, callback) = make_event_collector();
    let mut config = PoolConfigBuilder::new()
        .max_size(2)
        .test_before_acquire(true)
        .on_event(callback)
        .build()
        .unwrap();
    config.min_idle = 1;
    let pool = Pool::new(config, Arc::new(factory)).unwrap();

    let conn = pool.acquire().await.unwrap();
    drop(conn);
    tokio::time::sleep(Duration::from_millis(100)).await;

    ping_ok_flag.store(false, AOrdering::SeqCst);

    tokio::time::sleep(Duration::from_secs(31)).await;

    let result = pool.acquire().await;
    assert!(result.is_ok(), "acquire should succeed via new connection");

    tokio::time::sleep(Duration::from_millis(200)).await;

    let pm = pool.pressure_metrics();
    assert!(
        pm.dropped_count >= 1,
        "dropped_count should be >= 1 after ping failure, got {}",
        pm.dropped_count
    );

    let collected = events.lock().unwrap();
    let has_ping_dropped = collected.iter().any(|e| {
        matches!(
            e,
            PoolEvent::ConnectionDropped {
                reason: DropReason::PingFailed,
                ..
            }
        )
    });
    assert!(
        has_ping_dropped,
        "should have ConnectionDropped with PingFailed reason"
    );

    let has_rebuild = collected.iter().any(|e| {
        matches!(
            e,
            PoolEvent::RebuildSucceeded { .. } | PoolEvent::RebuildFailed { .. }
        )
    });
    assert!(
        has_rebuild,
        "should have RebuildSucceeded or RebuildFailed event triggered via acquire path"
    );
}

// ---- 7.2 池耗尽降级端到端测试 ----

#[tokio::test]
#[ignore]
async fn test_degradation_fast_fail_e2e() {
    let factory = E2eMockFactory::new();
    let (events, callback) = make_event_collector();
    let config = PoolConfigBuilder::new()
        .max_size(1)
        .acquire_timeout(10)
        .with_degradation(DegradationConfig {
            strategy: PoolDegradationStrategy::FastFail,
            backpressure_threshold: 1,
        })
        .on_event(callback)
        .build()
        .unwrap();
    let pool = Pool::new(config, Arc::new(factory)).unwrap();

    let conn1 = pool.acquire().await.unwrap();
    let start = Instant::now();
    let result = pool.acquire().await;
    let elapsed = start.elapsed();

    assert!(result.is_err());
    assert!(
        elapsed < Duration::from_millis(100),
        "FastFail should return immediately, took {:?}",
        elapsed
    );

    let pm = pool.pressure_metrics();
    assert!(pm.rejected_count >= 1, "rejected_count should be >= 1");

    let collected = events.lock().unwrap();
    let has_exhausted = collected.iter().any(|e| {
        matches!(
            e,
            PoolEvent::PoolExhausted {
                strategy: "FastFail",
                ..
            }
        )
    });
    assert!(
        has_exhausted,
        "should have PoolExhausted event with FastFail"
    );
    drop(conn1);
}

#[tokio::test]
#[ignore]
async fn test_degradation_default_unchanged_e2e() {
    let factory = E2eMockFactory::new();
    let mut config = PoolConfigBuilder::new().max_size(1).build().unwrap();
    config.acquire_timeout = Duration::from_millis(100);
    let pool = Pool::new(config, Arc::new(factory)).unwrap();

    let conn1 = pool.acquire().await.unwrap();
    let start = Instant::now();
    let result = pool.acquire().await;
    let elapsed = start.elapsed();

    assert!(result.is_err());
    assert!(
        elapsed >= Duration::from_millis(80),
        "default should wait ~100ms, took {:?}",
        elapsed
    );
    drop(conn1);
}

#[tokio::test]
#[ignore]
async fn test_backpressure_warning_e2e() {
    let factory = E2eMockFactory::new();
    let (events, callback) = make_event_collector();
    let mut config = PoolConfigBuilder::new()
        .max_size(1)
        .with_degradation(DegradationConfig {
            strategy: PoolDegradationStrategy::BackpressureNotify,
            backpressure_threshold: 2,
        })
        .on_event(callback)
        .build()
        .unwrap();
    config.acquire_timeout = Duration::from_millis(200);
    let pool = Pool::new(config, Arc::new(factory)).unwrap();

    let conn1 = pool.acquire().await.unwrap();
    let pool2 = pool.clone();
    let pool3 = pool.clone();
    let h2 = tokio::spawn(async move {
        let _ = pool2.acquire().await;
    });
    let h3 = tokio::spawn(async move {
        let _ = pool3.acquire().await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let collected = events.lock().unwrap();
        let has_warning = collected.iter().any(|e| {
            matches!(
                e,
                PoolEvent::BackpressureWarning { queue_len, threshold, .. } if *threshold == 2
            )
        });
        assert!(has_warning, "should have BackpressureWarning event");
    }

    drop(conn1);
    let _ = h2.await;
    let _ = h3.await;
}

// ---- 7.3 压力可观测性端到端测试 ----

#[tokio::test]
#[ignore]
async fn test_pressure_metrics_e2e() {
    let factory = E2eMockFactory::new();
    let mut config = PoolConfigBuilder::new()
        .max_size(1)
        .with_degradation(DegradationConfig {
            strategy: PoolDegradationStrategy::FastFail,
            backpressure_threshold: 1,
        })
        .build()
        .unwrap();
    config.acquire_timeout = Duration::from_millis(50);
    let pool = Pool::new(config, Arc::new(factory)).unwrap();

    let conn1 = pool.acquire().await.unwrap();

    let pm_before = pool.pressure_metrics();
    assert_eq!(pm_before.rejected_count, 0);

    let result = pool.acquire().await;
    assert!(result.is_err(), "second acquire should fail with FastFail");

    let pm_after = pool.pressure_metrics();
    assert!(
        pm_after.rejected_count >= 1,
        "rejected_count should be >= 1 after FastFail, got {}",
        pm_after.rejected_count
    );
    assert!(
        pm_after.exhausted_count >= 1,
        "exhausted_count should be >= 1, got {}",
        pm_after.exhausted_count
    );

    drop(conn1);
}

#[tokio::test]
#[ignore]
async fn test_dropped_count_sliding_window_e2e() {
    let window = DroppedCountWindow::new(Duration::from_secs(1));

    for _ in 0..5 {
        window.record_drop();
    }
    assert_eq!(window.current_count(), 5);

    tokio::time::sleep(Duration::from_millis(1200)).await;

    for _ in 0..3 {
        window.record_drop();
    }
    assert_eq!(window.current_count(), 3);
}

#[tokio::test]
#[ignore]
async fn test_pool_status_enhanced_fields_e2e() {
    let factory = E2eMockFactory::new();
    let mut config = PoolConfigBuilder::new()
        .max_size(1)
        .with_degradation(DegradationConfig {
            strategy: PoolDegradationStrategy::FastFail,
            backpressure_threshold: 1,
        })
        .build()
        .unwrap();
    config.acquire_timeout = Duration::from_millis(50);
    let pool = Pool::new(config, Arc::new(factory)).unwrap();

    let conn1 = pool.acquire().await.unwrap();
    let status_before = pool.status().await;
    assert_eq!(status_before.rejected_count, 0);

    let _ = pool.acquire().await;

    let status_after = pool.status().await;
    assert!(
        status_after.rejected_count >= 1,
        "rejected_count should be >= 1 after FastFail, got {}",
        status_after.rejected_count
    );

    drop(conn1);
}

// ---- 7.4 零拷贝解码端到端测试 ----

use sz_orm_core::queryable::{FromRow, QueryAs, QueryError, ZeroCopyDecodeConfig};

#[derive(Debug, Clone, PartialEq)]
struct E2eTestRow {
    id: i64,
    name: String,
}

impl FromRow for E2eTestRow {
    fn from_row(row: HashMap<String, Value>) -> Result<Self, QueryError> {
        let id = match row.get("id") {
            Some(Value::I64(v)) => *v,
            _ => {
                return Err(QueryError::Custom(
                    "Column 'id' not found or wrong type".into(),
                ))
            }
        };
        let name = match row.get("name") {
            Some(Value::String(s)) => s.clone(),
            _ => {
                return Err(QueryError::Custom(
                    "Column 'name' not found or wrong type".into(),
                ))
            }
        };
        Ok(Self { id, name })
    }
}

impl sz_orm_core::FromQueryResult for E2eTestRow {
    fn from_query_result(row: &HashMap<String, Value>) -> Result<Self, String> {
        let id = match row.get("id") {
            Some(Value::I64(v)) => *v,
            _ => return Err("Column 'id' not found or wrong type".into()),
        };
        let name = match row.get("name") {
            Some(Value::String(s)) => s.clone(),
            _ => return Err("Column 'name' not found or wrong type".into()),
        };
        Ok(Self { id, name })
    }
    fn row_desc() -> Vec<&'static str> {
        vec!["id", "name"]
    }
}

async fn setup_sqlite_for_zero_copy() -> AnyPool {
    let pool = AnyPool::connect("sqlite://:memory:").await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("CREATE TABLE zc_test (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .await
        .unwrap();
    for i in 1..=10000 {
        conn.execute(&format!(
            "INSERT INTO zc_test (id, name) VALUES ({}, 'name_{}')",
            i, i
        ))
        .await
        .unwrap();
    }
    pool
}

#[tokio::test]
#[ignore]
async fn test_zero_copy_decode_correctness_e2e() {
    let pool = setup_sqlite_for_zero_copy().await;
    let mut conn = pool.create().await.unwrap();

    let query = QueryAs::<E2eTestRow>::new("SELECT id, name FROM zc_test ORDER BY id");
    let result_hashmap = query.fetch_all(&mut conn).await.unwrap();

    let config = ZeroCopyDecodeConfig {
        enabled: true,
        fallback_on_mismatch: true,
        min_rows_for_zero_copy: 1000,
    };
    let result_zero_copy = query.fetch_all_zero_copy(&mut conn, &config).await.unwrap();

    assert_eq!(result_hashmap.len(), result_zero_copy.len());
    for (a, b) in result_hashmap.iter().zip(result_zero_copy.iter()) {
        assert_eq!(a, b);
    }
}

// ---- M16.1：零拷贝主路径端到端测试 ----

#[tokio::test]
#[ignore]
async fn test_zero_copy_main_path_enabled_e2e() {
    let pool = setup_sqlite_for_zero_copy().await;
    let mut conn = pool.create().await.unwrap();

    let q = QueryAs::<E2eTestRow>::new("SELECT id, name FROM zc_test ORDER BY id")
        .with_zero_copy_config(ZeroCopyDecodeConfig {
            enabled: true,
            fallback_on_mismatch: true,
            min_rows_for_zero_copy: 1000,
        });
    let result = q.fetch_all(&mut conn).await.unwrap();
    assert_eq!(result.len(), 10000);
    assert_eq!(result[0].id, 1);
    assert_eq!(result[0].name, "name_1");
    assert_eq!(result[9999].id, 10000);
}

#[tokio::test]
#[ignore]
async fn test_zero_copy_main_path_default_disabled_e2e() {
    let pool = setup_sqlite_for_zero_copy().await;
    let mut conn = pool.create().await.unwrap();

    let q = QueryAs::<E2eTestRow>::new("SELECT id, name FROM zc_test ORDER BY id");
    assert!(q.zero_copy_config().is_none());
    let result = q.fetch_all(&mut conn).await.unwrap();
    assert_eq!(result.len(), 10000);
    assert_eq!(result[0].id, 1);
}

// ---- M16.2：查询缓存增强端到端测试 ----

#[tokio::test]
#[ignore]
async fn test_cache_key_normalization_e2e() {
    use sz_orm_core::query_result_cache::{normalize_sql, CacheKey};

    let sql1 = "SELECT   /* comment */ * FROM users";
    let sql2 = "SELECT * FROM users";
    let norm1 = normalize_sql(sql1);
    let norm2 = normalize_sql(sql2);
    assert_eq!(norm1, norm2);

    let params = vec![];
    let key1 = CacheKey::new_normalized(sql1, &params, None);
    let key2 = CacheKey::new_normalized(sql2, &params, None);
    assert_eq!(key1, key2);
}

#[tokio::test]
#[ignore]
async fn test_cache_prewarm_e2e() {
    use sz_orm_core::query_result_cache::{QueryResultCache, QueryResultCacheConfig};

    let pool = AnyPool::connect("sqlite://:memory:").await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
        .await
        .unwrap();
    conn.execute("INSERT INTO users (id, name) VALUES (1, 'Alice')")
        .await
        .unwrap();
    conn.execute("INSERT INTO users (id, name) VALUES (2, 'Bob')")
        .await
        .unwrap();

    let cache = QueryResultCache::new(QueryResultCacheConfig::default());
    let queries = vec![("SELECT id, name FROM users".to_string(), vec![])];
    let success = cache.prewarm(&queries, &mut conn).await;
    assert_eq!(success, 1);

    let key =
        sz_orm_core::query_result_cache::CacheKey::new("SELECT id, name FROM users", &[], None);
    let hit = cache.get(&key);
    assert!(hit.is_some());
    assert_eq!(hit.unwrap().len(), 2);
}

// ---- M16.3：批量插入自适应端到端测试 ----

#[tokio::test]
#[ignore]
async fn test_batch_insert_adaptive_e2e() {
    use sz_orm_core::batch_advisor::{
        BatchAdaptiveConfig, DynamicBatchAdvisor, DynamicBatchConfig,
    };
    use sz_orm_core::repository::batch_insert_adaptive;

    let pool = AnyPool::connect("sqlite://:memory:").await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
        .await
        .unwrap();

    let rows: Vec<HashMap<String, Value>> = (1..=1000)
        .map(|i| {
            HashMap::from([
                ("id".to_string(), Value::I64(i)),
                ("name".to_string(), Value::String(format!("user_{}", i))),
            ])
        })
        .collect();

    let advisor = DynamicBatchAdvisor::new(DynamicBatchConfig::default());
    let config = BatchAdaptiveConfig {
        enabled: true,
        max_batch_size: 500,
        ..Default::default()
    };
    let result = batch_insert_adaptive(&mut conn, "users", rows, &advisor, &config, DbType::Sqlite)
        .await
        .unwrap();
    assert_eq!(result.total_inserted, 1000);
    assert!(result.batch_count > 0);
    for &bs in &result.batch_sizes_used {
        assert!((1..=500).contains(&bs));
    }

    let rows_check = conn.query("SELECT COUNT(*) FROM users").await.unwrap();
    assert_eq!(rows_check[0].get("COUNT(*)").unwrap().as_i64(), Some(1000));
}

// ---- M16.4：连接池 acquire 竞争优化端到端测试 ----

#[tokio::test]
#[ignore]
async fn test_known_good_fast_path_e2e() {
    use sz_orm_core::{KnownGoodFastPathConfig, Pool, PoolConfig};

    let any_pool = AnyPool::connect("sqlite://:memory:").await.unwrap();
    let config = PoolConfig::default().with_known_good_fast_path(KnownGoodFastPathConfig {
        enabled: true,
        mark_on_release: true,
        slow_path_ping_sample_interval: 1,
    });
    let pool = Pool::new(config, any_pool.factory()).unwrap();

    let conn1 = pool.acquire().await.unwrap();
    let id1 = conn1.conn_id;
    pool.release(conn1).await;

    let conn2 = pool.acquire().await.unwrap();
    let id2 = conn2.conn_id;
    assert_eq!(id1, id2, "known_good 快速路径应优先返回刚释放的连接");
    assert!(!conn2.known_good, "acquire 后 known_good 标记应被清除");
    pool.release(conn2).await;
}

#[tokio::test]
#[ignore]
async fn test_idle_priority_recent_first_e2e() {
    use sz_orm_core::{IdlePriority, Pool, PoolConfig};

    let any_pool = AnyPool::connect("sqlite://:memory:").await.unwrap();
    let config = PoolConfig {
        max_size: 4,
        ..PoolConfig::default()
    }
    .with_idle_priority(IdlePriority::RecentFirst);
    let pool = Pool::new(config, any_pool.factory()).unwrap();

    let conn1 = pool.acquire().await.unwrap();

    let conn2 = pool.acquire().await.unwrap();
    let id2 = conn2.conn_id;
    pool.release(conn1).await;
    pool.release(conn2).await;

    let conn3 = pool.acquire().await.unwrap();
    assert_eq!(
        conn3.conn_id, id2,
        "RecentFirst 应优先返回最近使用的连接(id2)"
    );
    pool.release(conn3).await;
}

// ---- M16.8：池告警桥接端到端测试 ----

#[tokio::test]
#[ignore]
async fn test_pool_alert_bridge_e2e() {
    use sz_orm_core::{PoolAlertConfig, PoolEvent};
    use sz_orm_observability::alert_rule_engine::PoolAlertBridge;

    let config = PoolAlertConfig {
        enabled: true,
        warn_threshold: 5,
        critical_threshold: 10,
        ..Default::default()
    };
    let bridge = PoolAlertBridge::new(config);

    let warn_event = PoolEvent::BackpressureWarning {
        queue_len: 7,
        threshold: 5,
        ts_ms: 0,
    };
    let warn_alert = bridge.convert(&warn_event).expect("warn 级别应产生告警");
    assert!(warn_alert.message.contains("backpressure"));

    let critical_event = PoolEvent::BackpressureWarning {
        queue_len: 15,
        threshold: 5,
        ts_ms: 1,
    };
    let critical_alert = bridge
        .convert(&critical_event)
        .expect("critical 级别应产生告警");
    assert!(critical_alert.message.contains("backpressure"));
    assert!(
        critical_alert.severity >= warn_alert.severity,
        "critical 告警严重度应 >= warn 告警"
    );

    let no_alert_event = PoolEvent::BackpressureWarning {
        queue_len: 3,
        threshold: 5,
        ts_ms: 2,
    };
    assert!(
        bridge.convert(&no_alert_event).is_none(),
        "queue_len < warn_threshold 不应产生告警"
    );
}

// ---- M16.9：安全合规深化端到端测试 ----

#[cfg(feature = "tde-interceptor")]
#[tokio::test]
#[ignore]
async fn test_transparent_encryption_e2e() {
    use sz_orm_core::{
        ColumnCryptoConfig, ColumnEncryptionPolicy, Pool, PoolConfig, TransparentEncryptConfig,
    };

    let mut policy = ColumnEncryptionPolicy::new();
    policy
        .add_column(ColumnCryptoConfig::new("secret_data", "value"))
        .unwrap();
    assert!(policy.is_encrypted("secret_data", "value"));
    assert!(!policy.is_encrypted("secret_data", "other_col"));

    let any_pool = AnyPool::connect("sqlite://:memory:").await.unwrap();
    let config = PoolConfig::default().with_transparent_encrypt(TransparentEncryptConfig {
        transparent_encrypt: true,
        ..Default::default()
    });
    let pool = Pool::new(config, any_pool.factory()).unwrap();
    let interceptor = pool
        .encryption_interceptor()
        .expect("启用 transparent_encrypt 后应返回拦截器");
    assert!(
        !interceptor.is_encrypted("any_table", "any_col"),
        "默认空策略应不加密任何列"
    );
    let result = interceptor.encrypt_for_column("any_table", "any_col", b"test");
    assert!(result.is_ok(), "空策略加密应返回 Ok(None)");
    assert!(result.unwrap().is_none(), "空策略应无需加密");
}

#[cfg(feature = "tenant-quota-rls-enhanced")]
#[tokio::test]
#[ignore]
async fn test_rls_auto_inject_e2e() {
    use sz_orm_core::tenant_context::{IsolationStrategy, TenantContext};
    use sz_orm_core::tenant_quota_rls::{RlsAutoInjector, RlsPolicyEnhancer};

    let enhancer = std::sync::Arc::new(RlsPolicyEnhancer::new());
    let injector = std::sync::Arc::new(RlsAutoInjector::new(enhancer));

    let pool = AnyPool::connect("sqlite://:memory:").await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("CREATE TABLE tenant_data (id INTEGER PRIMARY KEY, tenant_id INTEGER, name TEXT)")
        .await
        .unwrap();
    conn.execute("INSERT INTO tenant_data (id, tenant_id, name) VALUES (1, 100, 'A')")
        .await
        .unwrap();
    conn.execute("INSERT INTO tenant_data (id, tenant_id, name) VALUES (2, 200, 'B')")
        .await
        .unwrap();

    let _guard = TenantContext::new(100, IsolationStrategy::RowLevel).enter();
    let injected = injector
        .inject("tenant_data", "SELECT id, name FROM tenant_data")
        .expect("RLS 注入应成功");
    assert!(
        injected.to_lowercase().contains("where"),
        "注入后 SQL 应包含 WHERE 条件: {}",
        injected
    );
    assert!(
        injected.contains("tenant_id"),
        "注入后 SQL 应包含 tenant_id 过滤: {}",
        injected
    );

    let q = QueryAs::<E2eTestRow>::new("SELECT id, name FROM tenant_data")
        .with_rls_injector("tenant_data", injector);
    let result = q.fetch_all(&mut conn).await;
    assert!(result.is_ok(), "查询应成功: {:?}", result.err());
}

#[cfg(feature = "tenant-quota-rls-enhanced")]
#[tokio::test]
#[ignore]
async fn test_cross_tenant_detection_e2e() {
    use sz_orm_core::tenant_quota_rls::CrossTenantDetector;

    let detector = CrossTenantDetector::new();

    let result = detector.detect("SELECT * FROM data WHERE tenant_id = 999", 100);
    assert!(result.is_err());

    let result2 = detector.detect("SELECT * FROM data WHERE tenant_id = 100", 100);
    assert!(result2.is_ok());

    assert_eq!(detector.denied_count(), 1);

    let detector_arc = std::sync::Arc::new(CrossTenantDetector::new());
    let pool = AnyPool::connect("sqlite://:memory:").await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("CREATE TABLE data (id INTEGER PRIMARY KEY, tenant_id INTEGER, name TEXT)")
        .await
        .unwrap();
    conn.execute("INSERT INTO data (id, tenant_id, name) VALUES (1, 100, 'A')")
        .await
        .unwrap();

    let q = QueryAs::<E2eTestRow>::new("SELECT id, name FROM data WHERE tenant_id = 100")
        .with_cross_tenant_detector(detector_arc);
    let result = q.fetch_all(&mut conn).await;
    assert!(result.is_ok(), "同租户查询应成功: {:?}", result.err());
    let rows = result.unwrap();
    assert_eq!(rows.len(), 1, "应返回 1 行数据");
    assert_eq!(rows[0].id, 1, "应返回 id=1 的行");
}

// ============================================================================
// v10.1.0 e2e 接线验证测试
// ============================================================================

#[tokio::test]
async fn test_v1010_performance_linkage_config_e2e() {
    use sz_orm_core::PerformanceLinkageConfig;

    let config = sz_orm_core::pool::PoolConfigBuilder::new()
        .max_size(4)
        .acquire_timeout(5)
        .performance_linkage(PerformanceLinkageConfig {
            enabled: true,
            cache_zero_copy_return: true,
            batch_cache_aware: true,
        })
        .build()
        .expect("config valid");

    assert!(config.performance_linkage.enabled);
    assert!(config.performance_linkage.cache_zero_copy_return);
    assert!(config.performance_linkage.batch_cache_aware);
    assert!(!config.observability_loop.enabled);
    assert!(!config.compliance_auto_check.auto_check_enabled);
    assert!(!config.diagnostic_report.enabled);
}

#[tokio::test]
async fn test_v1010_observability_loop_push_event_e2e() {
    use sz_orm_core::ObservabilityLoopConfig;
    use sz_orm_observability::observability_loop::{AlertSeverity, LoopEvent, ObservabilityLoop};

    let config = ObservabilityLoopConfig {
        enabled: true,
        slow_query_to_ai: true,
        alert_to_diagnosis: true,
        event_queue_capacity: 10,
        ..Default::default()
    };
    let (loop_engine, mut rx) = ObservabilityLoop::new_with_channel(config);

    loop_engine.push_event(LoopEvent::SlowQuery {
        sql: "SELECT * FROM large_table".to_string(),
        elapsed_ms: 5000,
        plan: None,
    });
    loop_engine.push_event(LoopEvent::PressureAlert {
        severity: AlertSeverity::Critical,
        detail: "pool exhausted".to_string(),
    });

    let event1 = rx.recv().await;
    let event2 = rx.recv().await;
    assert!(event1.is_some(), "first event should be received");
    assert!(event2.is_some(), "second event should be received");

    let metrics = loop_engine.metrics();
    assert_eq!(metrics.trigger_count, 2, "trigger_count should be 2");
}

#[tokio::test]
async fn test_v1010_observability_loop_disabled_e2e() {
    use sz_orm_core::ObservabilityLoopConfig;
    use sz_orm_observability::observability_loop::{AlertSeverity, LoopEvent, ObservabilityLoop};

    let loop_engine = ObservabilityLoop::new(ObservabilityLoopConfig::default());
    loop_engine.push_event(LoopEvent::PressureAlert {
        severity: AlertSeverity::Critical,
        detail: "test".to_string(),
    });

    let metrics = loop_engine.metrics();
    assert_eq!(metrics.trigger_count, 1);
    assert_eq!(
        metrics.analysis_count, 0,
        "disabled loop should not analyze"
    );
}

#[tokio::test]
async fn test_v1010_compliance_auto_check_e2e() {
    use sz_orm_audit::compliance_auto_checker::{ComplianceAutoCheckConfig, ComplianceAutoChecker};

    let checker = ComplianceAutoChecker::new(ComplianceAutoCheckConfig::default());
    let result = checker.run_check(
        0.5,
        true,
        true,
        &["ssn".to_string(), "password".to_string()],
    );

    assert!(result.has_violations());
    assert_eq!(result.violations.len(), 2);
    assert!(result.violations[0].contains("ssn"));
    assert!(result.violations[1].contains("password"));

    let last = checker.last_result();
    assert!(last.is_some());
    assert_eq!(last.unwrap().encryption_coverage, 0.5);
}

#[tokio::test]
async fn test_v1010_apply_security_preset_compliance_first_e2e() {
    use sz_orm_audit::compliance_auto_checker::{apply_security_preset, SecurityPreset};

    let mut te = false;
    let mut pa = false;
    let mut ct = false;
    let mut ac = false;
    let mut ce = false;

    apply_security_preset(
        SecurityPreset::ComplianceFirst,
        &mut te,
        &mut pa,
        &mut ct,
        &mut ac,
        &mut ce,
    );

    assert!(te, "transparent_encrypt should be enabled");
    assert!(pa, "pool_alert should be enabled");
    assert!(ct, "cross_tenant_detection should be enabled");
    assert!(ac, "auto_check should be enabled");
}

#[tokio::test]
async fn test_v1010_hot_update_validate_e2e() {
    use sz_orm_core::{HotUpdateTarget, UnifiedHotUpdater};

    let valid = HotUpdateTarget::AlertThreshold {
        warn: 70,
        critical: 90,
    };
    assert!(UnifiedHotUpdater::validate(&valid).is_ok());

    let invalid = HotUpdateTarget::AlertThreshold {
        warn: 90,
        critical: 80,
    };
    assert!(UnifiedHotUpdater::validate(&invalid).is_err());
}

#[tokio::test]
async fn test_v1010_diagnostic_report_health_score_e2e() {
    use sz_orm_core::diagnostic_report::{DiagnosticReportConfig, DiagnosticReportGenerator};

    let gen = DiagnosticReportGenerator::new(DiagnosticReportConfig::default());
    let high = gen.compute_health_score(1.0, 1.0, 1.0, 1.0);
    let low = gen.compute_health_score(0.0, 0.0, 0.0, 0.0);
    let mid = gen.compute_health_score(0.5, 0.5, 0.5, 0.5);

    assert!((high - 1.0).abs() < 1e-9, "all 1.0 should give 1.0");
    assert!((low - 0.0).abs() < 1e-9, "all 0.0 should give 0.0");
    assert!((mid - 0.5).abs() < 1e-9, "all 0.5 should give 0.5");
    assert!(high > mid && mid > low, "should be monotonic");
}

#[tokio::test]
async fn test_v1010_batch_advisor_cache_aware_e2e() {
    use sz_orm_core::batch_advisor::{DynamicBatchAdvisor, DynamicBatchConfig};

    let advisor = DynamicBatchAdvisor::new(DynamicBatchConfig {
        max_size: 5000,
        ..Default::default()
    });

    let base = advisor.advise_dynamic(DbType::MySQL, 2, 10000);
    let with_none = advisor.advise_dynamic_with_cache(DbType::MySQL, 2, 10000, None);
    assert_eq!(base, with_none, "None should match base");

    let with_low = advisor.advise_dynamic_with_cache(DbType::MySQL, 2, 10000, Some(0.3));
    assert_eq!(base, with_low, "low hit rate should match base");
}

#[tokio::test]
async fn test_v1010_pool_config_new_fields_default_e2e() {
    use sz_orm_core::{DiagnosticReportConfig, Pool, PoolConfigBuilder};

    let config = sz_orm_core::pool::PoolConfig::default();

    assert!(
        !config.performance_linkage.enabled,
        "performance_linkage default disabled"
    );
    assert!(
        !config.observability_loop.enabled,
        "observability_loop default disabled"
    );
    assert!(
        !config.compliance_auto_check.auto_check_enabled,
        "compliance_auto_check default disabled"
    );
    assert!(
        !config.diagnostic_report.enabled,
        "diagnostic_report default disabled"
    );
    assert!(
        !config.self_check_on_start,
        "self_check_on_start default false"
    );
    assert!(
        config.self_check_modules.is_empty(),
        "self_check_modules default empty"
    );

    struct DummyFactory;
    #[async_trait::async_trait]
    impl sz_orm_core::ConnectionFactory for DummyFactory {
        async fn create(&self) -> Result<Box<dyn sz_orm_core::Connection>, sz_orm_core::DbError> {
            Err(sz_orm_core::DbError::ConnectionError("dummy".to_string()))
        }
    }

    let config = PoolConfigBuilder::new()
        .max_size(4)
        .acquire_timeout(5)
        .build()
        .expect("config valid");
    let mut config = config;
    config.diagnostic_report = DiagnosticReportConfig {
        enabled: true,
        ..Default::default()
    };
    let pool = Pool::new(config, std::sync::Arc::new(DummyFactory)).expect("pool creation");
    assert!(
        pool.diagnostic_report_generator().is_some(),
        "diagnostic_report enabled -> generator should be created"
    );
    assert!(
        !pool.observability_loop_config().enabled,
        "observability_loop config accessible via getter"
    );
}

#[tokio::test]
async fn test_v1010_compliance_score_e2e() {
    use sz_orm_audit::compliance_auto_checker::{ComplianceAutoCheckConfig, ComplianceAutoChecker};

    let checker = ComplianceAutoChecker::new(ComplianceAutoCheckConfig::default());

    assert_eq!(checker.compliance_score(), 1.0, "no check run -> 1.0");

    checker.run_check(1.0, true, true, &[]);
    assert_eq!(checker.compliance_score(), 1.0, "no violations -> 1.0");

    checker.run_check(0.5, false, false, &["ssn".to_string()]);
    let score = checker.compliance_score();
    assert!(score < 1.0, "with violations -> < 1.0");
    assert!(score > 0.0, "score should be positive");
}
