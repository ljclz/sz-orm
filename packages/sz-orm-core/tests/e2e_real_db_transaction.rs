//! M1-T3: 真实数据库事务端到端测试
//!
//! 连真实 MySQL/PostgreSQL/SQLite 验证事务 commit/rollback/savepoint。

#![cfg(feature = "e2e-real-db")]

use sqlx::Row;

mod common;

use common::cleanup::unique_table_name;

async fn mysql_pool() -> Option<sqlx::MySqlPool> {
    let url = std::env::var("MYSQL_URL").ok()?;
    sqlx::MySqlPool::connect(&url).await.ok()
}

async fn pg_pool() -> Option<sqlx::PgPool> {
    let url = std::env::var("POSTGRES_URL").ok()?;
    sqlx::PgPool::connect(&url).await.ok()
}

async fn sqlite_pool() -> Option<sqlx::SqlitePool> {
    sqlx::SqlitePool::connect("sqlite::memory:").await.ok()
}

// ==================== MySQL 事务 ====================

#[tokio::test]
async fn test_mysql_transaction_commit() {
    let pool = match mysql_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_txn");
    let create_sql = format!(
        "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255))",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let mut tx = pool.begin().await.unwrap();
    let insert_sql = format!("INSERT INTO `{}` (name) VALUES (?)", table);
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Bob")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let count_sql = format!("SELECT COUNT(*) as cnt FROM `{}`", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(count_sql.as_str()))
        .fetch_one(&pool)
        .await
        .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 2);

    sqlx::query(sqlx::AssertSqlSafe(
        (format!("DROP TABLE `{}`", table)).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_mysql_transaction_rollback() {
    let pool = match mysql_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_txn");
    let create_sql = format!(
        "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255))",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let mut tx = pool.begin().await.unwrap();
    let insert_sql = format!("INSERT INTO `{}` (name) VALUES (?)", table);
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();

    let count_sql = format!("SELECT COUNT(*) as cnt FROM `{}`", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(count_sql.as_str()))
        .fetch_one(&pool)
        .await
        .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 0);

    sqlx::query(sqlx::AssertSqlSafe(
        (format!("DROP TABLE `{}`", table)).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== PostgreSQL 事务 ====================

#[tokio::test]
async fn test_pg_transaction_commit() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_txn");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let mut tx = pool.begin().await.unwrap();
    let insert_sql = format!("INSERT INTO \"{}\" (name) VALUES ($1)", table);
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Bob")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let count_sql = format!("SELECT COUNT(*) as cnt FROM \"{}\"", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(count_sql.as_str()))
        .fetch_one(&pool)
        .await
        .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 2);

    sqlx::query(sqlx::AssertSqlSafe(
        (format!("DROP TABLE \"{}\"", table)).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_pg_transaction_rollback() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_txn");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let mut tx = pool.begin().await.unwrap();
    let insert_sql = format!("INSERT INTO \"{}\" (name) VALUES ($1)", table);
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();

    let count_sql = format!("SELECT COUNT(*) as cnt FROM \"{}\"", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(count_sql.as_str()))
        .fetch_one(&pool)
        .await
        .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 0);

    sqlx::query(sqlx::AssertSqlSafe(
        (format!("DROP TABLE \"{}\"", table)).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_pg_savepoint() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_txn");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let mut tx = pool.begin().await.unwrap();
    let insert_sql = format!("INSERT INTO \"{}\" (name) VALUES ($1)", table);
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .execute(&mut *tx)
        .await
        .unwrap();

    let savepoint = sqlx::query("SAVEPOINT sp1")
        .execute(&mut *tx)
        .await
        .unwrap();
    let _ = savepoint;
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Bob")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("ROLLBACK TO SAVEPOINT sp1")
        .execute(&mut *tx)
        .await
        .unwrap();

    tx.commit().await.unwrap();

    let count_sql = format!("SELECT COUNT(*) as cnt FROM \"{}\"", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(count_sql.as_str()))
        .fetch_one(&pool)
        .await
        .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 1);

    sqlx::query(sqlx::AssertSqlSafe(
        (format!("DROP TABLE \"{}\"", table)).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== SQLite 事务 ====================

#[tokio::test]
async fn test_sqlite_transaction_commit() {
    let pool = match sqlite_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_txn");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let mut tx = pool.begin().await.unwrap();
    let insert_sql = format!("INSERT INTO \"{}\" (name) VALUES (?)", table);
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Bob")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let count_sql = format!("SELECT COUNT(*) as cnt FROM \"{}\"", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(count_sql.as_str()))
        .fetch_one(&pool)
        .await
        .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn test_sqlite_transaction_rollback() {
    let pool = match sqlite_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_txn");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let mut tx = pool.begin().await.unwrap();
    let insert_sql = format!("INSERT INTO \"{}\" (name) VALUES (?)", table);
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();

    let count_sql = format!("SELECT COUNT(*) as cnt FROM \"{}\"", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(count_sql.as_str()))
        .fetch_one(&pool)
        .await
        .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 0);
}
