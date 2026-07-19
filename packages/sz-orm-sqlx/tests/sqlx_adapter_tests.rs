//! sqlx 适配器单元测试（使用 SQLite 内存数据库，无需外部 DB）

use std::collections::HashMap;
use std::sync::Arc;
use sz_orm_core::Value;
use sz_orm_core::{ConnectionFactory, PoolConfigBuilder};
use sz_orm_sqlx::{SqlitePoolHandle, SqlxSqliteConnectionFactory};

async fn setup_sqlite_pool() -> Arc<SqlitePoolHandle> {
    let pool = SqlitePoolHandle::connect("sqlite::memory:")
        .await
        .expect("sqlite memory connect failed");
    Arc::new(pool)
}

async fn setup_sqlite_factory() -> Arc<SqlxSqliteConnectionFactory> {
    let pool = setup_sqlite_pool().await;
    Arc::new(SqlxSqliteConnectionFactory::new(pool))
}

#[tokio::test]
async fn test_sqlx_adapter_basic_connect() {
    let pool = SqlitePoolHandle::connect("sqlite::memory:").await;
    assert!(pool.is_ok(), "sqlite memory should connect");
}

#[tokio::test]
async fn test_sqlx_adapter_factory_create() {
    let factory = setup_sqlite_factory().await;
    let conn = factory.create().await;
    assert!(conn.is_ok(), "factory.create should succeed");
    let conn = conn.unwrap();
    assert!(conn.is_connected(), "new connection should be connected");
}

#[tokio::test]
async fn test_sqlx_adapter_execute_create_table() {
    let factory = setup_sqlite_factory().await;
    let mut conn = factory.create().await.unwrap();
    let result = conn
        .execute("CREATE TABLE test_adapt (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .await;
    assert!(result.is_ok(), "create table should succeed");
    assert_eq!(result.unwrap(), 0, "DDL rows_affected should be 0");
}

#[tokio::test]
async fn test_sqlx_adapter_insert_and_query() {
    let factory = setup_sqlite_factory().await;
    let mut conn = factory.create().await.unwrap();
    conn.execute("CREATE TABLE test_iq (id INTEGER PRIMARY KEY, name TEXT, value INTEGER)")
        .await
        .unwrap();
    conn.execute("INSERT INTO test_iq (id, name, value) VALUES (1, 'alice', 100)")
        .await
        .unwrap();
    conn.execute("INSERT INTO test_iq (id, name, value) VALUES (2, 'bob', 200)")
        .await
        .unwrap();

    let rows = conn
        .query("SELECT id, name, value FROM test_iq ORDER BY id")
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].get("name"),
        Some(&Value::String("alice".to_string()))
    );
    assert_eq!(rows[1].get("name"), Some(&Value::String("bob".to_string())));
}

#[tokio::test]
async fn test_sqlx_adapter_transaction_commit() {
    let factory = setup_sqlite_factory().await;
    let mut conn = factory.create().await.unwrap();
    conn.execute("CREATE TABLE test_tc (id INTEGER PRIMARY KEY, name TEXT)")
        .await
        .unwrap();

    conn.begin_transaction().await.unwrap();
    conn.execute("INSERT INTO test_tc (id, name) VALUES (1, 'tx_commit')")
        .await
        .unwrap();
    conn.commit().await.unwrap();

    let rows = conn.query("SELECT * FROM test_tc").await.unwrap();
    assert_eq!(rows.len(), 1, "row should persist after commit");
}

#[tokio::test]
async fn test_sqlx_adapter_transaction_rollback() {
    let factory = setup_sqlite_factory().await;
    let mut conn = factory.create().await.unwrap();
    conn.execute("CREATE TABLE test_tr (id INTEGER PRIMARY KEY, name TEXT)")
        .await
        .unwrap();

    conn.begin_transaction().await.unwrap();
    conn.execute("INSERT INTO test_tr (id, name) VALUES (1, 'tx_rollback')")
        .await
        .unwrap();
    conn.rollback().await.unwrap();

    let rows = conn.query("SELECT * FROM test_tr").await.unwrap();
    assert_eq!(rows.len(), 0, "row should be rolled back");
}

#[tokio::test]
async fn test_sqlx_adapter_double_begin_fails() {
    let factory = setup_sqlite_factory().await;
    let mut conn = factory.create().await.unwrap();
    conn.begin_transaction().await.unwrap();
    let result = conn.begin_transaction().await;
    assert!(result.is_err(), "double begin should fail");
    conn.rollback().await.unwrap();
}

#[tokio::test]
async fn test_sqlx_adapter_commit_without_begin() {
    let factory = setup_sqlite_factory().await;
    let mut conn = factory.create().await.unwrap();
    // commit without begin should be no-op (success)
    let result = conn.commit().await;
    assert!(
        result.is_ok(),
        "commit without begin should succeed (no-op)"
    );
}

#[tokio::test]
async fn test_sqlx_adapter_rollback_without_begin() {
    let factory = setup_sqlite_factory().await;
    let mut conn = factory.create().await.unwrap();
    let result = conn.rollback().await;
    assert!(
        result.is_ok(),
        "rollback without begin should succeed (no-op)"
    );
}

#[tokio::test]
async fn test_sqlx_adapter_ping() {
    let factory = setup_sqlite_factory().await;
    let mut conn = factory.create().await.unwrap();
    assert!(
        conn.ping().await,
        "ping should return true on healthy connection"
    );
}

#[tokio::test]
async fn test_sqlx_adapter_close() {
    let factory = setup_sqlite_factory().await;
    let mut conn = factory.create().await.unwrap();
    conn.close().await.unwrap();
    assert!(
        !conn.is_connected(),
        "connection should be marked disconnected after close"
    );
    // operations after close should fail
    let result = conn.execute("SELECT 1").await;
    assert!(result.is_err(), "execute after close should fail");
}

#[tokio::test]
async fn test_sqlx_adapter_savepoint() {
    let factory = setup_sqlite_factory().await;
    let mut conn = factory.create().await.unwrap();
    conn.execute("CREATE TABLE test_sp (id INTEGER PRIMARY KEY, name TEXT)")
        .await
        .unwrap();

    conn.begin_transaction().await.unwrap();
    conn.execute("INSERT INTO test_sp (id, name) VALUES (1, 'before_sp')")
        .await
        .unwrap();
    conn.execute("SAVEPOINT sp1").await.unwrap();
    conn.execute("INSERT INTO test_sp (id, name) VALUES (2, 'after_sp')")
        .await
        .unwrap();
    conn.execute("ROLLBACK TO sp1").await.unwrap();
    conn.execute("RELEASE sp1").await.unwrap();
    conn.commit().await.unwrap();

    let rows = conn
        .query("SELECT * FROM test_sp ORDER BY id")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "only row before savepoint should remain");
    assert_eq!(
        rows[0].get("name"),
        Some(&Value::String("before_sp".to_string()))
    );
}

#[tokio::test]
async fn test_sqlx_adapter_with_sz_orm_pool() {
    // 端到端测试：sz-orm-core 的 Pool 使用 sqlx 适配器
    let pool_handle = SqlitePoolHandle::connect("sqlite::memory:").await.unwrap();
    let factory = Arc::new(SqlxSqliteConnectionFactory::new(Arc::new(pool_handle)));
    let config = PoolConfigBuilder::new()
        .max_size(5)
        .min_idle(0)
        .acquire_timeout(5)
        .build()
        .unwrap();
    let pool = sz_orm_core::Pool::new(config, factory).unwrap();

    let mut conn = pool.acquire().await.unwrap();
    conn.execute("CREATE TABLE test_pool (id INTEGER PRIMARY KEY, name TEXT)")
        .await
        .unwrap();
    conn.execute("INSERT INTO test_pool (id, name) VALUES (1, 'via_pool')")
        .await
        .unwrap();
    pool.release(conn).await;

    let mut conn2 = pool.acquire().await.unwrap();
    let rows = conn2.query("SELECT * FROM test_pool").await.unwrap();
    assert_eq!(rows.len(), 1);
    pool.release(conn2).await;
}

#[tokio::test]
async fn test_sqlx_adapter_concurrent_pool() {
    use std::sync::atomic::{AtomicU32, Ordering};
    let pool_handle = Arc::new(SqlitePoolHandle::connect("sqlite::memory:").await.unwrap());

    // 先建表
    {
        let factory = SqlxSqliteConnectionFactory::new(pool_handle.clone());
        let mut conn = factory.create().await.unwrap();
        conn.execute("CREATE TABLE test_conc (id INTEGER PRIMARY KEY, value INTEGER)")
            .await
            .unwrap();
    }

    let factory = Arc::new(SqlxSqliteConnectionFactory::new(pool_handle));
    let config = PoolConfigBuilder::new()
        .max_size(5)
        .min_idle(0)
        .acquire_timeout(5)
        .build()
        .unwrap();
    let pool: &'static sz_orm_core::Pool =
        Box::leak(Box::new(sz_orm_core::Pool::new(config, factory).unwrap()));

    let success_count = Arc::new(AtomicU32::new(0));
    let mut handles = Vec::new();
    for i in 0..10u32 {
        let sc = success_count.clone();
        handles.push(tokio::spawn(async move {
            if let Ok(mut conn) = pool.acquire().await {
                let sql = format!(
                    "INSERT INTO test_conc (id, value) VALUES ({}, {})",
                    i,
                    i * 10
                );
                if conn.execute(&sql).await.is_ok() {
                    sc.fetch_add(1, Ordering::SeqCst);
                }
                pool.release(conn).await;
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    assert_eq!(
        success_count.load(Ordering::SeqCst),
        10,
        "all 10 concurrent inserts should succeed"
    );
}

#[tokio::test]
async fn test_sqlx_adapter_null_values() {
    let factory = setup_sqlite_factory().await;
    let mut conn = factory.create().await.unwrap();
    conn.execute("CREATE TABLE test_null (id INTEGER PRIMARY KEY, name TEXT, opt INTEGER)")
        .await
        .unwrap();
    conn.execute("INSERT INTO test_null (id, name, opt) VALUES (1, 'with_null', NULL)")
        .await
        .unwrap();
    conn.execute("INSERT INTO test_null (id, name, opt) VALUES (2, 'with_val', 42)")
        .await
        .unwrap();

    let rows = conn
        .query("SELECT id, name, opt FROM test_null ORDER BY id")
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get("opt"), Some(&Value::Null));
    assert_eq!(rows[1].get("opt"), Some(&Value::I64(42)));
}

#[tokio::test]
async fn test_sqlx_adapter_error_handling() {
    let factory = setup_sqlite_factory().await;
    let mut conn = factory.create().await.unwrap();
    // syntax error
    let result = conn.execute("INVALID SQL STATEMENT").await;
    assert!(result.is_err(), "invalid SQL should return error");
    // query non-existent table
    let result = conn.query("SELECT * FROM nonexistent_table").await;
    assert!(result.is_err(), "query on non-existent table should fail");
}

// 抑制未使用 import 警告（HashMap 在某些 assert 中可用）
#[allow(dead_code)]
fn _suppress_hashmap_warning() -> HashMap<String, Value> {
    HashMap::new()
}
