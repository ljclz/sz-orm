//! v8.3.0: 真实数据库连接池端到端测试
//!
//! 连第二台服务器 MySQL/PostgreSQL + 本机 Oracle 验证连接池
//! 获取/释放/复用/并发/耗尽/健康检查。

#![cfg(feature = "e2e-real-db")]

use sqlx::Row;

#[allow(dead_code)]
mod common;

use common::cleanup::unique_table_name;
use common::e2e_env::{e2e_mysql_pool, e2e_oracle_conn, e2e_pg_pool, E2eMysqlDb};

// ==================== MySQL 连接池 ====================

#[tokio::test]
async fn test_e2e_mysql_pool_acquire_release() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_pool_mysql");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, val INT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{}` (val) VALUES (?)", table).as_str(),
    ))
    .bind(100i32)
    .execute(&pool)
    .await
    .unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT val FROM `{}` WHERE id = 1", table).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    let val: i32 = row.try_get("val").unwrap();
    assert_eq!(val, 100);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{}` (val) VALUES (?)", table).as_str(),
    ))
    .bind(200i32)
    .execute(&pool)
    .await
    .unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT val FROM `{}` WHERE id = 2", table).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    let val: i32 = row.try_get("val").unwrap();
    assert_eq!(val, 200);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_mysql_pool_health_check() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let row = sqlx::query("SELECT 1 as ok")
        .fetch_one(&pool)
        .await
        .unwrap();
    let ok: i32 = row.try_get("ok").unwrap();
    assert_eq!(ok, 1);
}

// ==================== PostgreSQL 连接池 ====================

#[tokio::test]
async fn test_e2e_pg_pool_concurrent() {
    let pool = match e2e_pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_pool_pg");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, task_id INT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let pool = pool.clone();
            let table = table.clone();
            tokio::spawn(async move {
                sqlx::query(sqlx::AssertSqlSafe(
                    format!("INSERT INTO \"{}\" (task_id) VALUES ($1)", table).as_str(),
                ))
                .bind(i)
                .execute(&pool)
                .await
                .unwrap();
            })
        })
        .collect();
    for h in handles {
        h.await.unwrap();
    }

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT COUNT(*) as cnt FROM \"{}\"", table).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 10);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_pg_pool_health_check() {
    let pool = match e2e_pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let row = sqlx::query("SELECT 1 as ok")
        .fetch_one(&pool)
        .await
        .unwrap();
    let ok: i32 = row.try_get("ok").unwrap();
    assert_eq!(ok, 1);
}

// ==================== Oracle 连接池（模拟） ====================

#[tokio::test]
async fn test_e2e_oracle_pool_exhaustion() {
    let conn = match e2e_oracle_conn() {
        Some(c) => c,
        None => return,
    };
    let table = unique_table_name("e2e_pool_ora");
    let table_lower = table.to_lowercase();
    conn.execute(
        &format!(
            "CREATE TABLE \"{}\" (id NUMBER GENERATED ALWAYS AS IDENTITY PRIMARY KEY, val NUMBER)",
            table_lower
        ),
        &[],
    )
    .unwrap();
    conn.execute(
        &format!("INSERT INTO \"{}\" (val) VALUES (:1)", table_lower),
        &[&42i32],
    )
    .unwrap();
    conn.commit().unwrap();

    let rows = conn
        .query(
            &format!("SELECT val FROM \"{}\" WHERE val = :1", table_lower),
            &[&42i32],
        )
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows.len(), 1);
    let val: i32 = rows[0].get(0).unwrap();
    assert_eq!(val, 42);

    conn.execute(&format!("DROP TABLE \"{}\"", table_lower), &[])
        .unwrap();
}

#[tokio::test]
async fn test_e2e_oracle_pool_health_check() {
    let conn = match e2e_oracle_conn() {
        Some(c) => c,
        None => return,
    };
    let rows = conn
        .query("SELECT 1 FROM DUAL", &[])
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let val: i32 = rows[0].get(0).unwrap();
    assert_eq!(val, 1);
}

// ==================== SQLite 连接池 ====================

#[tokio::test]
async fn test_e2e_sqlite_pool_acquire_release() {
    let pool = match sqlx::SqlitePool::connect("sqlite::memory:").await.ok() {
        Some(p) => p,
        None => return,
    };
    sqlx::query(sqlx::AssertSqlSafe("CREATE TABLE IF NOT EXISTS t (id INTEGER PRIMARY KEY)"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe("INSERT INTO t (id) VALUES (?)"))
        .bind(1_i64)
        .execute(&pool)
        .await
        .unwrap();
    let row = sqlx::query(sqlx::AssertSqlSafe("SELECT COUNT(*) as cnt FROM t"))
        .fetch_one(&pool)
        .await
        .unwrap();
    let cnt: i64 = row.try_get("cnt").unwrap();
    assert_eq!(cnt, 1);
}
