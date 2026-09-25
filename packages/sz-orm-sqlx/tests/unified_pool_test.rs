//! M3: unified_pool.rs 补充测试 — 覆盖 Debug impl / prewarm / connect_with_config

use std::sync::Arc;

use sz_orm_core::{Pool, PoolConfigBuilder};
use sz_orm_sqlx::{AnyBackend, SqlitePoolHandle, SqlxSqliteConnectionFactory, UnifiedPool};

#[tokio::test]
async fn test_unified_pool_debug_format() {
    let pool = UnifiedPool::connect("sqlite::memory:").await.unwrap();
    let debug_str = format!("{:?}", pool);
    assert!(debug_str.contains("UnifiedPool"));
    assert!(debug_str.contains("Sqlite"));
}

#[tokio::test]
async fn test_unified_pool_prewarm() {
    let pool = UnifiedPool::connect("sqlite::memory:").await.unwrap();
    pool.prewarm().await;
    let status = pool.status().await;
    assert!(status.max > 0);
}

#[tokio::test]
async fn test_unified_pool_connect_with_custom_config() {
    let config = PoolConfigBuilder::new().max_size(5).build().unwrap();
    let pool = UnifiedPool::connect_with_config("sqlite::memory:", config)
        .await
        .unwrap();
    assert_eq!(pool.backend(), AnyBackend::Sqlite);
    let status = pool.status().await;
    assert_eq!(status.max, 5);
}

#[tokio::test]
async fn test_unified_pool_from_pool_debug() {
    let handle = Arc::new(SqlitePoolHandle::connect("sqlite::memory:").await.unwrap());
    let factory = Arc::new(SqlxSqliteConnectionFactory::new(handle));
    let config = PoolConfigBuilder::new().build().unwrap();
    let pool = Pool::new(config, factory).unwrap();
    let unified = UnifiedPool::from_pool(pool, AnyBackend::Sqlite);
    let debug_str = format!("{:?}", unified);
    assert!(debug_str.contains("UnifiedPool"));
}

#[tokio::test]
async fn test_unified_pool_dialect_after_from_pool() {
    let handle = Arc::new(SqlitePoolHandle::connect("sqlite::memory:").await.unwrap());
    let factory = Arc::new(SqlxSqliteConnectionFactory::new(handle));
    let config = PoolConfigBuilder::new().build().unwrap();
    let pool = Pool::new(config, factory).unwrap();
    let unified = UnifiedPool::from_pool(pool, AnyBackend::Sqlite);
    let d = unified.dialect();
    assert_eq!(d.db_type(), sz_orm_core::DbType::Sqlite);
}

#[tokio::test]
async fn test_unified_pool_resize_after_from_pool() {
    let handle = Arc::new(SqlitePoolHandle::connect("sqlite::memory:").await.unwrap());
    let factory = Arc::new(SqlxSqliteConnectionFactory::new(handle));
    let config = PoolConfigBuilder::new().build().unwrap();
    let pool = Pool::new(config, factory).unwrap();
    let unified = UnifiedPool::from_pool(pool, AnyBackend::Sqlite);
    unified.resize(15);
    let status = unified.status().await;
    assert_eq!(status.max, 15);
}

#[tokio::test]
async fn test_unified_pool_close_all_after_from_pool() {
    let handle = Arc::new(SqlitePoolHandle::connect("sqlite::memory:").await.unwrap());
    let factory = Arc::new(SqlxSqliteConnectionFactory::new(handle));
    let config = PoolConfigBuilder::new().build().unwrap();
    let pool = Pool::new(config, factory).unwrap();
    let unified = UnifiedPool::from_pool(pool, AnyBackend::Sqlite);
    unified.close_all().await;
}

#[tokio::test]
async fn test_unified_pool_acquire_and_query() {
    let pool = UnifiedPool::connect("sqlite::memory:").await.unwrap();
    let mut conn = pool.acquire().await.unwrap();
    conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)")
        .await
        .unwrap();
    conn.execute("INSERT INTO test (id, name) VALUES (1, 'hello')")
        .await
        .unwrap();
    let rows = conn.query("SELECT * FROM test").await.unwrap();
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn test_unified_pool_multiple_queries() {
    let pool = UnifiedPool::connect("sqlite::memory:").await.unwrap();
    let mut conn = pool.acquire().await.unwrap();
    conn.execute("CREATE TABLE t (id INTEGER PRIMARY KEY)")
        .await
        .unwrap();
    for i in 1..=5 {
        conn.execute(&format!("INSERT INTO t (id) VALUES ({})", i))
            .await
            .unwrap();
    }
    let rows = conn.query("SELECT * FROM t").await.unwrap();
    assert_eq!(rows.len(), 5);
}
