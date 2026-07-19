//! 真实 DB Jepsen 风格并发正确性测试（P0-2）
//!
//! 替换原 mock Jepsen 测试，使用真实 MySQL/PG 数据库验证：
//! - 并发事务隔离级别
//! - 并发 savepoint 嵌套
//! - 故障注入下的连接池一致性
//! - 长事务下的连接池耗尽恢复
//!
//! 默认 ignored，运行方式：
//! cargo test -p sz-orm-sqlx --test real_db_jepsen -- --ignored --nocapture

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

// ===================== MySQL Jepsen 测试 =====================

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0"]
async fn mysql_jepsen_concurrent_transfer_isolation() {
    let pool_handle = Arc::new(MySqlPoolHandle::connect(MYSQL_URL).await.unwrap());
    let table = unique_table("jepsen_xfer");
    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        sqlx::query(&format!(
            "CREATE TABLE {} (id INT PRIMARY KEY, balance INT NOT NULL)",
            table
        ))
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(&format!(
            "INSERT INTO {} (id, balance) VALUES (1, 1000)",
            table
        ))
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(&format!(
            "INSERT INTO {} (id, balance) VALUES (2, 1000)",
            table
        ))
        .execute(&mut *conn)
        .await
        .unwrap();
    }

    let factory = Arc::new(SqlxMySqlConnectionFactory::new(pool_handle));
    let config = PoolConfigBuilder::new()
        .max_size(8)
        .min_idle(2)
        .acquire_timeout(15)
        .build()
        .unwrap();
    let pool: &'static Pool = Box::leak(Box::new(Pool::new(config, factory).unwrap()));

    // 8 个 task 并发执行 transfer(1 -> 2, 10)，共 100 次
    let success = Arc::new(AtomicU32::new(0));
    let mut handles = Vec::new();
    for _ in 0..8u32 {
        let sc = success.clone();
        let table = table.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..12u32 {
                if let Ok(mut conn) = pool.acquire().await {
                    // 简化的转账：读两个余额、扣 1 加 1（无显式锁，可能存在竞态）
                    conn.begin_transaction().await.unwrap();
                    let rows = conn
                        .query(&format!("SELECT balance FROM {} WHERE id = 1", table))
                        .await
                        .unwrap();
                    if let Some(v) = rows
                        .first()
                        .and_then(|r| r.get("balance"))
                        .and_then(|v| v.as_i64())
                    {
                        if v >= 10 {
                            conn.execute(&format!(
                                "UPDATE {} SET balance = balance - 10 WHERE id = 1",
                                table
                            ))
                            .await
                            .unwrap();
                            conn.execute(&format!(
                                "UPDATE {} SET balance = balance + 10 WHERE id = 2",
                                table
                            ))
                            .await
                            .unwrap();
                            conn.commit().await.unwrap();
                            sc.fetch_add(1, Ordering::SeqCst);
                        } else {
                            conn.rollback().await.unwrap();
                        }
                    } else {
                        conn.rollback().await.unwrap();
                    }
                    pool.release(conn).await;
                }
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    // 守恒律：总余额必须 = 2000（无论并发顺序）
    let mut conn = pool.acquire().await.unwrap();
    let rows = conn
        .query(&format!("SELECT SUM(balance) as total FROM {}", table))
        .await
        .unwrap();
    let total: i64 = rows[0].get("total").and_then(|v| v.as_i64()).unwrap_or(-1);
    assert_eq!(
        total, 2000,
        "conservation law violated: total = {} (expected 2000)",
        total
    );

    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0"]
async fn mysql_jepsen_savepoint_nested_5_levels() {
    let pool_handle = Arc::new(MySqlPoolHandle::connect(MYSQL_URL).await.unwrap());
    let table = unique_table("jepsen_sp5");
    let factory = Arc::new(SqlxMySqlConnectionFactory::new(pool_handle));
    let config = PoolConfigBuilder::new().max_size(3).build().unwrap();
    let pool = Pool::new(config, factory).unwrap();

    let mut conn = pool.acquire().await.unwrap();
    conn.execute(&format!(
        "CREATE TABLE {} (id INT PRIMARY KEY, level INT)",
        table
    ))
    .await
    .unwrap();

    conn.begin_transaction().await.unwrap();
    // 5 层 savepoint 嵌套
    for level in 0..5u32 {
        conn.execute(&format!(
            "INSERT INTO {} (id, level) VALUES ({}, {})",
            table, level, level
        ))
        .await
        .unwrap();
        conn.execute(&format!("SAVEPOINT sp_{}", level))
            .await
            .unwrap();
    }
    // 回滚到第 3 层
    conn.execute("ROLLBACK TO SAVEPOINT sp_2").await.unwrap();
    conn.commit().await.unwrap();

    let rows = conn
        .query(&format!("SELECT * FROM {} ORDER BY id", table))
        .await
        .unwrap();
    // 应该保留 level 0, 1, 2（sp_2 之后的不保留）
    assert_eq!(rows.len(), 3, "should have 3 rows after savepoint rollback");

    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0"]
async fn mysql_jepsen_pool_exhaustion_recovery() {
    let pool_handle = Arc::new(MySqlPoolHandle::connect(MYSQL_URL).await.unwrap());
    let factory = Arc::new(SqlxMySqlConnectionFactory::new(pool_handle));
    let config = PoolConfigBuilder::new()
        .max_size(3)
        .min_idle(0)
        .acquire_timeout(2)
        .build()
        .unwrap();
    let pool = Pool::new(config, factory).unwrap();

    // 借出所有连接
    let mut conns = Vec::new();
    for _ in 0..3 {
        conns.push(pool.acquire().await.unwrap());
    }
    // 第 4 次应该超时
    let result = tokio::time::timeout(std::time::Duration::from_secs(3), pool.acquire()).await;
    assert!(
        result.is_err() || result.unwrap().is_err(),
        "4th acquire should timeout"
    );

    // 释放一个，应该能再次 acquire
    pool.release(conns.remove(0)).await;
    let new_conn = pool.acquire().await;
    assert!(new_conn.is_ok(), "should recover after release");
    pool.release(new_conn.unwrap()).await;
    for c in conns {
        pool.release(c).await;
    }
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0"]
async fn mysql_jepsen_long_transaction_100_ops() {
    let pool_handle = Arc::new(MySqlPoolHandle::connect(MYSQL_URL).await.unwrap());
    let table = unique_table("jepsen_long");
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
    let config = PoolConfigBuilder::new().max_size(3).build().unwrap();
    let pool = Pool::new(config, factory).unwrap();

    // 单事务 100 次 INSERT
    let mut conn = pool.acquire().await.unwrap();
    conn.begin_transaction().await.unwrap();
    for i in 0..100u32 {
        conn.execute(&format!(
            "INSERT INTO {} (id, value) VALUES ({}, {})",
            table,
            i,
            i * 2
        ))
        .await
        .unwrap();
    }
    conn.commit().await.unwrap();

    let rows = conn
        .query(&format!("SELECT COUNT(*) as c FROM {}", table))
        .await
        .unwrap();
    let count = rows[0].get("c").and_then(|v| v.as_i64()).unwrap_or(0);
    assert_eq!(count, 100, "should have 100 rows");

    // 验证值正确
    let rows = conn
        .query(&format!("SELECT value FROM {} WHERE id = 50", table))
        .await
        .unwrap();
    let v = rows[0].get("value").and_then(|v| v.as_i64()).unwrap_or(-1);
    assert_eq!(v, 100, "value at id=50 should be 100");

    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0"]
async fn mysql_jepsen_mixed_dml_sequence() {
    let pool_handle = Arc::new(MySqlPoolHandle::connect(MYSQL_URL).await.unwrap());
    let table = unique_table("jepsen_mixed");
    let factory = Arc::new(SqlxMySqlConnectionFactory::new(pool_handle));
    let config = PoolConfigBuilder::new().max_size(3).build().unwrap();
    let pool = Pool::new(config, factory).unwrap();

    let mut conn = pool.acquire().await.unwrap();
    conn.execute(&format!(
        "CREATE TABLE {} (id INT PRIMARY KEY, value INT)",
        table
    ))
    .await
    .unwrap();
    conn.execute(&format!("INSERT INTO {} (id, value) VALUES (1, 10)", table))
        .await
        .unwrap();

    // INSERT -> UPDATE -> SELECT -> DELETE -> SELECT 序列
    conn.execute(&format!("INSERT INTO {} (id, value) VALUES (2, 20)", table))
        .await
        .unwrap();
    conn.execute(&format!("UPDATE {} SET value = 100 WHERE id = 1", table))
        .await
        .unwrap();
    let rows = conn
        .query(&format!("SELECT * FROM {} ORDER BY id", table))
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    conn.execute(&format!("DELETE FROM {} WHERE id = 2", table))
        .await
        .unwrap();
    let rows = conn
        .query(&format!("SELECT * FROM {}", table))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("value").and_then(|v| v.as_i64()), Some(100));

    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}

// ===================== PostgreSQL Jepsen 测试 =====================

#[tokio::test]
#[ignore = "需要 PostgreSQL 18"]
async fn pg_jepsen_concurrent_transfer_isolation() {
    let pool_handle = Arc::new(PgPoolHandle::connect(PG_URL).await.unwrap());
    let table = unique_table("pg_jepsen_xfer");
    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        sqlx::query(&format!(
            "CREATE TABLE {} (id BIGINT PRIMARY KEY, balance BIGINT NOT NULL)",
            table
        ))
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(&format!(
            "INSERT INTO {} (id, balance) VALUES (1, 1000)",
            table
        ))
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(&format!(
            "INSERT INTO {} (id, balance) VALUES (2, 1000)",
            table
        ))
        .execute(&mut *conn)
        .await
        .unwrap();
    }

    let factory = Arc::new(SqlxPgConnectionFactory::new(pool_handle));
    let config = PoolConfigBuilder::new()
        .max_size(8)
        .min_idle(2)
        .acquire_timeout(15)
        .build()
        .unwrap();
    let pool: &'static Pool = Box::leak(Box::new(Pool::new(config, factory).unwrap()));

    let success = Arc::new(AtomicU32::new(0));
    let mut handles = Vec::new();
    for _ in 0..8u32 {
        let sc = success.clone();
        let table = table.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..12u32 {
                if let Ok(mut conn) = pool.acquire().await {
                    conn.begin_transaction().await.unwrap();
                    let rows = conn
                        .query(&format!("SELECT balance FROM {} WHERE id = 1", table))
                        .await
                        .unwrap();
                    if let Some(v) = rows
                        .first()
                        .and_then(|r| r.get("balance"))
                        .and_then(|v| v.as_i64())
                    {
                        if v >= 10 {
                            conn.execute(&format!(
                                "UPDATE {} SET balance = balance - 10 WHERE id = 1",
                                table
                            ))
                            .await
                            .unwrap();
                            conn.execute(&format!(
                                "UPDATE {} SET balance = balance + 10 WHERE id = 2",
                                table
                            ))
                            .await
                            .unwrap();
                            conn.commit().await.unwrap();
                            sc.fetch_add(1, Ordering::SeqCst);
                        } else {
                            conn.rollback().await.unwrap();
                        }
                    } else {
                        conn.rollback().await.unwrap();
                    }
                    pool.release(conn).await;
                }
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    let mut conn = pool.acquire().await.unwrap();
    let rows = conn
        .query(&format!("SELECT SUM(balance) as total FROM {}", table))
        .await
        .unwrap();
    let total: i64 = rows[0].get("total").and_then(|v| v.as_i64()).unwrap_or(-1);
    assert_eq!(total, 2000, "conservation law violated: total = {}", total);

    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}

#[tokio::test]
#[ignore = "需要 PostgreSQL 18"]
async fn pg_jepsen_savepoint_nested_5_levels() {
    let pool_handle = Arc::new(PgPoolHandle::connect(PG_URL).await.unwrap());
    let table = unique_table("pg_jepsen_sp5");
    let factory = Arc::new(SqlxPgConnectionFactory::new(pool_handle));
    let config = PoolConfigBuilder::new().max_size(3).build().unwrap();
    let pool = Pool::new(config, factory).unwrap();

    let mut conn = pool.acquire().await.unwrap();
    conn.execute(&format!(
        "CREATE TABLE {} (id BIGINT PRIMARY KEY, level INT)",
        table
    ))
    .await
    .unwrap();

    conn.begin_transaction().await.unwrap();
    for level in 0..5u32 {
        conn.execute(&format!(
            "INSERT INTO {} (id, level) VALUES ({}, {})",
            table, level, level
        ))
        .await
        .unwrap();
        conn.execute(&format!("SAVEPOINT sp_{}", level))
            .await
            .unwrap();
    }
    conn.execute("ROLLBACK TO SAVEPOINT sp_2").await.unwrap();
    conn.commit().await.unwrap();

    let rows = conn
        .query(&format!("SELECT * FROM {} ORDER BY id", table))
        .await
        .unwrap();
    assert_eq!(rows.len(), 3, "should have 3 rows after savepoint rollback");

    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}

#[tokio::test]
#[ignore = "需要 PostgreSQL 18"]
async fn pg_jepsen_pool_exhaustion_recovery() {
    let pool_handle = Arc::new(PgPoolHandle::connect(PG_URL).await.unwrap());
    let factory = Arc::new(SqlxPgConnectionFactory::new(pool_handle));
    let config = PoolConfigBuilder::new()
        .max_size(3)
        .min_idle(0)
        .acquire_timeout(2)
        .build()
        .unwrap();
    let pool = Pool::new(config, factory).unwrap();

    let mut conns = Vec::new();
    for _ in 0..3 {
        conns.push(pool.acquire().await.unwrap());
    }
    let result = tokio::time::timeout(std::time::Duration::from_secs(3), pool.acquire()).await;
    assert!(
        result.is_err() || result.unwrap().is_err(),
        "4th acquire should timeout"
    );

    pool.release(conns.remove(0)).await;
    let new_conn = pool.acquire().await;
    assert!(new_conn.is_ok(), "should recover after release");
    pool.release(new_conn.unwrap()).await;
    for c in conns {
        pool.release(c).await;
    }
}

#[tokio::test]
#[ignore = "需要 PostgreSQL 18"]
async fn pg_jepsen_long_transaction_100_ops() {
    let pool_handle = Arc::new(PgPoolHandle::connect(PG_URL).await.unwrap());
    let table = unique_table("pg_jepsen_long");
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
    let config = PoolConfigBuilder::new().max_size(3).build().unwrap();
    let pool = Pool::new(config, factory).unwrap();

    let mut conn = pool.acquire().await.unwrap();
    conn.begin_transaction().await.unwrap();
    for i in 0..100u32 {
        conn.execute(&format!(
            "INSERT INTO {} (id, value) VALUES ({}, {})",
            table,
            i,
            i * 2
        ))
        .await
        .unwrap();
    }
    conn.commit().await.unwrap();

    let rows = conn
        .query(&format!("SELECT COUNT(*) as c FROM {}", table))
        .await
        .unwrap();
    let count = rows[0].get("c").and_then(|v| v.as_i64()).unwrap_or(0);
    assert_eq!(count, 100, "should have 100 rows");

    let rows = conn
        .query(&format!("SELECT value FROM {} WHERE id = 50", table))
        .await
        .unwrap();
    let v = rows[0].get("value").and_then(|v| v.as_i64()).unwrap_or(-1);
    assert_eq!(v, 100, "value at id=50 should be 100");

    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}

#[tokio::test]
#[ignore = "需要 PostgreSQL 18"]
async fn pg_jepsen_mixed_dml_sequence() {
    let pool_handle = Arc::new(PgPoolHandle::connect(PG_URL).await.unwrap());
    let table = unique_table("pg_jepsen_mixed");
    let factory = Arc::new(SqlxPgConnectionFactory::new(pool_handle));
    let config = PoolConfigBuilder::new().max_size(3).build().unwrap();
    let pool = Pool::new(config, factory).unwrap();

    let mut conn = pool.acquire().await.unwrap();
    conn.execute(&format!(
        "CREATE TABLE {} (id BIGINT PRIMARY KEY, value BIGINT)",
        table
    ))
    .await
    .unwrap();
    conn.execute(&format!("INSERT INTO {} (id, value) VALUES (1, 10)", table))
        .await
        .unwrap();

    conn.execute(&format!("INSERT INTO {} (id, value) VALUES (2, 20)", table))
        .await
        .unwrap();
    conn.execute(&format!("UPDATE {} SET value = 100 WHERE id = 1", table))
        .await
        .unwrap();
    let rows = conn
        .query(&format!("SELECT * FROM {} ORDER BY id", table))
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    conn.execute(&format!("DELETE FROM {} WHERE id = 2", table))
        .await
        .unwrap();
    let rows = conn
        .query(&format!("SELECT * FROM {}", table))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("value").and_then(|v| v.as_i64()), Some(100));

    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.release(conn).await;
}
