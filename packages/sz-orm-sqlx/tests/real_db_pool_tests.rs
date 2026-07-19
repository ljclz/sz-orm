//! 真实 DB Pool/Transaction 行为验证测试（P0-3）
//!
//! 这些测试需要真实 MySQL/PG 实例，默认 ignored
//! 运行方式：cargo test -p sz-orm-sqlx --test real_db_pool_tests -- --ignored --nocapture

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use sz_orm_core::{Pool, PoolConfigBuilder};
use sz_orm_sqlx::{
    MySqlPoolHandle, PgPoolHandle, SqlxMySqlConnectionFactory, SqlxPgConnectionFactory,
};

const MYSQL_URL: &str = "mysql://root:<your-password>@127.0.0.1:3306/sz_orm_test";
const PG_URL: &str = "postgres://postgres:<your-password>@127.0.0.1:5432/sz_orm_test";

fn unique_table(prefix: &str) -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}_{}_{}", prefix, pid, nanos % 1_000_000)
}

// ===================== MySQL 真实 DB 测试 =====================

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0 在 127.0.0.1:3306"]
async fn mysql_pool_acquire_release_roundtrip() {
    let pool_handle = Arc::new(MySqlPoolHandle::connect(MYSQL_URL).await.unwrap());
    let factory = Arc::new(SqlxMySqlConnectionFactory::new(pool_handle));
    let config = PoolConfigBuilder::new()
        .max_size(5)
        .min_idle(0)
        .acquire_timeout(10)
        .build()
        .unwrap();
    let pool = Pool::new(config, factory).unwrap();

    // 多次 acquire/release 应该重用连接
    for i in 0..20u32 {
        let mut conn = pool.acquire().await.unwrap();
        let sql = format!("SELECT {} as n", i);
        let rows = conn.query(&sql).await.unwrap();
        assert_eq!(rows.len(), 1);
        pool.release(conn).await;
    }

    let status = pool.status().await;
    assert!(
        status.idle > 0,
        "should have idle connections after release"
    );
    assert!(status.active <= 5, "active should not exceed max_size");
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0 在 127.0.0.1:3306"]
async fn mysql_pool_concurrent_8_tasks() {
    let pool_handle = Arc::new(MySqlPoolHandle::connect(MYSQL_URL).await.unwrap());
    let table = unique_table("mysql_conc");
    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        sqlx::query(&format!(
            "CREATE TABLE {} (id INT PRIMARY KEY, value INT)",
            table
        ))
        .execute(&mut *conn)
        .await
        .unwrap();
    }

    let factory = Arc::new(SqlxMySqlConnectionFactory::new(pool_handle));
    let config = PoolConfigBuilder::new()
        .max_size(8)
        .min_idle(0)
        .acquire_timeout(15)
        .build()
        .unwrap();
    let pool: &'static Pool = Box::leak(Box::new(Pool::new(config, factory).unwrap()));

    let success = Arc::new(AtomicU32::new(0));
    let mut handles = Vec::new();
    for i in 0..8u32 {
        let sc = success.clone();
        let table = table.clone();
        handles.push(tokio::spawn(async move {
            for j in 0..50u32 {
                if let Ok(mut conn) = pool.acquire().await {
                    let id = i * 1000 + j;
                    let sql = format!("INSERT INTO {} (id, value) VALUES ({}, {})", table, id, id);
                    if conn.execute(&sql).await.is_ok() {
                        sc.fetch_add(1, Ordering::SeqCst);
                    }
                    pool.release(conn).await;
                }
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    assert_eq!(
        success.load(Ordering::SeqCst),
        400,
        "all 8*50 inserts should succeed"
    );

    // 清理
    let mut conn = pool.acquire().await.unwrap();
    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0 在 127.0.0.1:3306"]
async fn mysql_transaction_commit_rollback() {
    let pool_handle = Arc::new(MySqlPoolHandle::connect(MYSQL_URL).await.unwrap());
    let table = unique_table("mysql_tx");
    let factory = Arc::new(SqlxMySqlConnectionFactory::new(pool_handle.clone()));
    let config = PoolConfigBuilder::new().max_size(3).build().unwrap();
    let pool = Pool::new(config, factory).unwrap();

    let mut conn = pool.acquire().await.unwrap();
    conn.execute(&format!(
        "CREATE TABLE {} (id INT PRIMARY KEY, name VARCHAR(100))",
        table
    ))
    .await
    .unwrap();

    // commit 测试
    conn.begin_transaction().await.unwrap();
    conn.execute(&format!(
        "INSERT INTO {} (id, name) VALUES (1, 'commit_test')",
        table
    ))
    .await
    .unwrap();
    conn.commit().await.unwrap();

    // rollback 测试
    conn.begin_transaction().await.unwrap();
    conn.execute(&format!(
        "INSERT INTO {} (id, name) VALUES (2, 'rollback_test')",
        table
    ))
    .await
    .unwrap();
    conn.rollback().await.unwrap();

    let rows = conn
        .query(&format!("SELECT * FROM {}", table))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "only committed row should remain");

    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0 在 127.0.0.1:3306"]
async fn mysql_savepoint_nested() {
    let pool_handle = Arc::new(MySqlPoolHandle::connect(MYSQL_URL).await.unwrap());
    let table = unique_table("mysql_sp");
    let factory = Arc::new(SqlxMySqlConnectionFactory::new(pool_handle.clone()));
    let config = PoolConfigBuilder::new().max_size(3).build().unwrap();
    let pool = Pool::new(config, factory).unwrap();

    let mut conn = pool.acquire().await.unwrap();
    conn.execute(&format!(
        "CREATE TABLE {} (id INT PRIMARY KEY, name VARCHAR(100))",
        table
    ))
    .await
    .unwrap();

    conn.begin_transaction().await.unwrap();
    conn.execute(&format!(
        "INSERT INTO {} (id, name) VALUES (1, 'outer')",
        table
    ))
    .await
    .unwrap();
    conn.execute("SAVEPOINT sp1").await.unwrap();
    conn.execute(&format!(
        "INSERT INTO {} (id, name) VALUES (2, 'inner')",
        table
    ))
    .await
    .unwrap();
    conn.execute("ROLLBACK TO SAVEPOINT sp1").await.unwrap();
    conn.execute("RELEASE SAVEPOINT sp1").await.unwrap();
    conn.commit().await.unwrap();

    let rows = conn
        .query(&format!("SELECT * FROM {} ORDER BY id", table))
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        1,
        "only outer row should remain after savepoint rollback"
    );

    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0 在 127.0.0.1:3306"]
async fn mysql_pool_stress_10k_ops() {
    let pool_handle = Arc::new(MySqlPoolHandle::connect(MYSQL_URL).await.unwrap());
    let table = unique_table("mysql_stress");
    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        sqlx::query(&format!(
            "CREATE TABLE {} (id INT PRIMARY KEY, value INT)",
            table
        ))
        .execute(&mut *conn)
        .await
        .unwrap();
    }

    let factory = Arc::new(SqlxMySqlConnectionFactory::new(pool_handle));
    let config = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(2)
        .acquire_timeout(30)
        .build()
        .unwrap();
    let pool: &'static Pool = Box::leak(Box::new(Pool::new(config, factory).unwrap()));

    let success = Arc::new(AtomicU32::new(0));
    let mut handles = Vec::new();
    for i in 0..10u32 {
        let sc = success.clone();
        let table = table.clone();
        handles.push(tokio::spawn(async move {
            for j in 0..1000u32 {
                let id = i * 10000 + j;
                if let Ok(mut conn) = pool.acquire().await {
                    let sql = format!("INSERT INTO {} (id, value) VALUES ({}, {})", table, id, id);
                    if conn.execute(&sql).await.is_ok() {
                        sc.fetch_add(1, Ordering::SeqCst);
                    }
                    pool.release(conn).await;
                }
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    assert_eq!(
        success.load(Ordering::SeqCst),
        10000,
        "all 10*1000 = 10k inserts should succeed"
    );

    // 清理
    let mut conn = pool.acquire().await.unwrap();
    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}

// ===================== PostgreSQL 真实 DB 测试 =====================

#[tokio::test]
#[ignore = "需要 PostgreSQL 18 在 127.0.0.1:5432"]
async fn pg_pool_acquire_release_roundtrip() {
    let pool_handle = Arc::new(PgPoolHandle::connect(PG_URL).await.unwrap());
    let factory = Arc::new(SqlxPgConnectionFactory::new(pool_handle));
    let config = PoolConfigBuilder::new()
        .max_size(5)
        .min_idle(0)
        .acquire_timeout(10)
        .build()
        .unwrap();
    let pool = Pool::new(config, factory).unwrap();

    for i in 0..20u32 {
        let mut conn = pool.acquire().await.unwrap();
        let sql = format!("SELECT {} as n", i);
        let rows = conn.query(&sql).await.unwrap();
        assert_eq!(rows.len(), 1);
        pool.release(conn).await;
    }
    let status = pool.status().await;
    assert!(status.idle > 0, "should have idle connections");
}

#[tokio::test]
#[ignore = "需要 PostgreSQL 18 在 127.0.0.1:5432"]
async fn pg_pool_concurrent_8_tasks() {
    let pool_handle = Arc::new(PgPoolHandle::connect(PG_URL).await.unwrap());
    let table = unique_table("pg_conc");
    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        sqlx::query(&format!(
            "CREATE TABLE {} (id BIGINT PRIMARY KEY, value BIGINT)",
            table
        ))
        .execute(&mut *conn)
        .await
        .unwrap();
    }

    let factory = Arc::new(SqlxPgConnectionFactory::new(pool_handle));
    let config = PoolConfigBuilder::new()
        .max_size(8)
        .min_idle(0)
        .acquire_timeout(15)
        .build()
        .unwrap();
    let pool: &'static Pool = Box::leak(Box::new(Pool::new(config, factory).unwrap()));

    let success = Arc::new(AtomicU32::new(0));
    let mut handles = Vec::new();
    for i in 0..8u32 {
        let sc = success.clone();
        let table = table.clone();
        handles.push(tokio::spawn(async move {
            for j in 0..50u32 {
                let id: i64 = (i as i64) * 1000 + j as i64;
                if let Ok(mut conn) = pool.acquire().await {
                    let sql = format!("INSERT INTO {} (id, value) VALUES ({}, {})", table, id, id);
                    if conn.execute(&sql).await.is_ok() {
                        sc.fetch_add(1, Ordering::SeqCst);
                    }
                    pool.release(conn).await;
                }
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    assert_eq!(
        success.load(Ordering::SeqCst),
        400,
        "all 8*50 inserts should succeed"
    );

    let mut conn = pool.acquire().await.unwrap();
    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}

#[tokio::test]
#[ignore = "需要 PostgreSQL 18 在 127.0.0.1:5432"]
async fn pg_transaction_commit_rollback() {
    let pool_handle = Arc::new(PgPoolHandle::connect(PG_URL).await.unwrap());
    let table = unique_table("pg_tx");
    let factory = Arc::new(SqlxPgConnectionFactory::new(pool_handle.clone()));
    let config = PoolConfigBuilder::new().max_size(3).build().unwrap();
    let pool = Pool::new(config, factory).unwrap();

    let mut conn = pool.acquire().await.unwrap();
    conn.execute(&format!(
        "CREATE TABLE {} (id BIGINT PRIMARY KEY, name TEXT)",
        table
    ))
    .await
    .unwrap();

    conn.begin_transaction().await.unwrap();
    conn.execute(&format!(
        "INSERT INTO {} (id, name) VALUES (1, 'commit_test')",
        table
    ))
    .await
    .unwrap();
    conn.commit().await.unwrap();

    conn.begin_transaction().await.unwrap();
    conn.execute(&format!(
        "INSERT INTO {} (id, name) VALUES (2, 'rollback_test')",
        table
    ))
    .await
    .unwrap();
    conn.rollback().await.unwrap();

    let rows = conn
        .query(&format!("SELECT * FROM {}", table))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "only committed row should remain");

    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}

#[tokio::test]
#[ignore = "需要 PostgreSQL 18 在 127.0.0.1:5432"]
async fn pg_savepoint_nested() {
    let pool_handle = Arc::new(PgPoolHandle::connect(PG_URL).await.unwrap());
    let table = unique_table("pg_sp");
    let factory = Arc::new(SqlxPgConnectionFactory::new(pool_handle.clone()));
    let config = PoolConfigBuilder::new().max_size(3).build().unwrap();
    let pool = Pool::new(config, factory).unwrap();

    let mut conn = pool.acquire().await.unwrap();
    conn.execute(&format!(
        "CREATE TABLE {} (id BIGINT PRIMARY KEY, name TEXT)",
        table
    ))
    .await
    .unwrap();

    conn.begin_transaction().await.unwrap();
    conn.execute(&format!(
        "INSERT INTO {} (id, name) VALUES (1, 'outer')",
        table
    ))
    .await
    .unwrap();
    conn.execute("SAVEPOINT sp1").await.unwrap();
    conn.execute(&format!(
        "INSERT INTO {} (id, name) VALUES (2, 'inner')",
        table
    ))
    .await
    .unwrap();
    conn.execute("ROLLBACK TO SAVEPOINT sp1").await.unwrap();
    conn.execute("RELEASE SAVEPOINT sp1").await.unwrap();
    conn.commit().await.unwrap();

    let rows = conn
        .query(&format!("SELECT * FROM {} ORDER BY id", table))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "only outer row should remain");

    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}

#[tokio::test]
#[ignore = "需要 PostgreSQL 18 在 127.0.0.1:5432"]
async fn pg_pool_stress_10k_ops() {
    let pool_handle = Arc::new(PgPoolHandle::connect(PG_URL).await.unwrap());
    let table = unique_table("pg_stress");
    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        sqlx::query(&format!(
            "CREATE TABLE {} (id BIGINT PRIMARY KEY, value BIGINT)",
            table
        ))
        .execute(&mut *conn)
        .await
        .unwrap();
    }

    let factory = Arc::new(SqlxPgConnectionFactory::new(pool_handle));
    let config = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(2)
        .acquire_timeout(30)
        .build()
        .unwrap();
    let pool: &'static Pool = Box::leak(Box::new(Pool::new(config, factory).unwrap()));

    let success = Arc::new(AtomicU32::new(0));
    let mut handles = Vec::new();
    for i in 0..10u32 {
        let sc = success.clone();
        let table = table.clone();
        handles.push(tokio::spawn(async move {
            for j in 0..1000u32 {
                let id: i64 = (i as i64) * 10000 + j as i64;
                if let Ok(mut conn) = pool.acquire().await {
                    let sql = format!("INSERT INTO {} (id, value) VALUES ({}, {})", table, id, id);
                    if conn.execute(&sql).await.is_ok() {
                        sc.fetch_add(1, Ordering::SeqCst);
                    }
                    pool.release(conn).await;
                }
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    assert_eq!(
        success.load(Ordering::SeqCst),
        10000,
        "all 10k inserts should succeed"
    );

    let mut conn = pool.acquire().await.unwrap();
    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}

// ===================== 超大数据量测试 =====================

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0，10万行批量插入"]
async fn mysql_bulk_insert_100k_via_pool() {
    let pool_handle = Arc::new(MySqlPoolHandle::connect(MYSQL_URL).await.unwrap());
    let table = unique_table("mysql_bulk");
    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        sqlx::query(&format!(
            "CREATE TABLE {} (id BIGINT PRIMARY KEY, value BIGINT, name VARCHAR(50))",
            table
        ))
        .execute(&mut *conn)
        .await
        .unwrap();
    }

    let factory = Arc::new(SqlxMySqlConnectionFactory::new(pool_handle));
    let config = PoolConfigBuilder::new().max_size(5).build().unwrap();
    let pool = Pool::new(config, factory).unwrap();

    let start = std::time::Instant::now();
    let mut conn = pool.acquire().await.unwrap();
    conn.begin_transaction().await.unwrap();
    for i in 0..100_000u32 {
        let sql = format!(
            "INSERT INTO {} (id, value, name) VALUES ({}, {}, 'name_{}')",
            table, i, i, i
        );
        conn.execute(&sql).await.unwrap();
    }
    conn.commit().await.unwrap();
    let elapsed = start.elapsed();

    let rows = conn
        .query(&format!("SELECT COUNT(*) as c FROM {}", table))
        .await
        .unwrap();
    let count = rows[0].get("c").and_then(|v| v.as_i64()).unwrap_or(0);
    assert_eq!(count, 100_000, "should have 100k rows");

    println!("MySQL 10万行插入 via sz-orm-core Pool: {:?}", elapsed);

    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}

#[tokio::test]
#[ignore = "需要 PostgreSQL 18，10万行批量插入"]
async fn pg_bulk_insert_100k_via_pool() {
    let pool_handle = Arc::new(PgPoolHandle::connect(PG_URL).await.unwrap());
    let table = unique_table("pg_bulk");
    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        sqlx::query(&format!(
            "CREATE TABLE {} (id BIGINT PRIMARY KEY, value BIGINT, name TEXT)",
            table
        ))
        .execute(&mut *conn)
        .await
        .unwrap();
    }

    let factory = Arc::new(SqlxPgConnectionFactory::new(pool_handle));
    let config = PoolConfigBuilder::new().max_size(5).build().unwrap();
    let pool = Pool::new(config, factory).unwrap();

    let start = std::time::Instant::now();
    let mut conn = pool.acquire().await.unwrap();
    conn.begin_transaction().await.unwrap();
    for i in 0..100_000u32 {
        let sql = format!(
            "INSERT INTO {} (id, value, name) VALUES ({}, {}, 'name_{}')",
            table, i, i, i
        );
        conn.execute(&sql).await.unwrap();
    }
    conn.commit().await.unwrap();
    let elapsed = start.elapsed();

    let rows = conn
        .query(&format!("SELECT COUNT(*) as c FROM {}", table))
        .await
        .unwrap();
    let count = rows[0].get("c").and_then(|v| v.as_i64()).unwrap_or(0);
    assert_eq!(count, 100_000, "should have 100k rows");

    println!("PG 10万行插入 via sz-orm-core Pool: {:?}", elapsed);

    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}
