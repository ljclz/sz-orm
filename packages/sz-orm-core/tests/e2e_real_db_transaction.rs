//! M1-T3: 真实数据库事务端到端测试
//!
//! 连真实 MySQL/PostgreSQL/SQLite 验证事务 commit/rollback/savepoint。

#![cfg(feature = "e2e-real-db")]

use sqlx::Row;

#[allow(dead_code)]
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
// ==================== v8.3.0: 第二台服务器事务 e2e ====================

use common::e2e_env::{e2e_mysql_pool, e2e_oracle_conn, e2e_pg_pool, E2eMysqlDb};

#[tokio::test]
async fn test_e2e_mysql_tx_commit() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_tx_mysql");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255))",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    let mut tx = pool.begin().await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{}` (name) VALUES (?)", table).as_str(),
    ))
    .bind("Alice")
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT COUNT(*) as cnt FROM `{}`", table).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 1);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_mysql_tx_rollback() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_tx_mysql");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255))",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    let mut tx = pool.begin().await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{}` (name) VALUES (?)", table).as_str(),
    ))
    .bind("Alice")
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.rollback().await.unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT COUNT(*) as cnt FROM `{}`", table).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 0);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_pg_tx_commit() {
    let pool = match e2e_pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_tx_pg");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    let mut tx = pool.begin().await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO \"{}\" (name) VALUES ($1)", table).as_str(),
    ))
    .bind("Alice")
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT COUNT(*) as cnt FROM \"{}\"", table).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 1);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_pg_tx_rollback() {
    let pool = match e2e_pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_tx_pg");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    let mut tx = pool.begin().await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO \"{}\" (name) VALUES ($1)", table).as_str(),
    ))
    .bind("Alice")
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.rollback().await.unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT COUNT(*) as cnt FROM \"{}\"", table).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 0);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_oracle_tx_commit() {
    let conn = match e2e_oracle_conn() {
        Some(c) => c,
        None => return,
    };
    let table = unique_table_name("e2e_tx_ora");
    let table_lower = table.to_lowercase();
    conn.execute(
        &format!(
            "CREATE TABLE \"{}\" (id NUMBER GENERATED ALWAYS AS IDENTITY PRIMARY KEY, name VARCHAR2(255))",
            table_lower
        ),
        &[],
    )
    .unwrap();

    conn.execute(
        &format!("INSERT INTO \"{}\" (name) VALUES (:1)", table_lower),
        &[&"Alice"],
    )
    .unwrap();
    conn.commit().unwrap();

    let rows = conn
        .query(&format!("SELECT COUNT(*) FROM \"{}\"", table_lower), &[])
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let count: i64 = rows[0].get(0).unwrap();
    assert_eq!(count, 1);

    conn.execute(&format!("DROP TABLE \"{}\"", table_lower), &[])
        .unwrap();
}

#[tokio::test]
async fn test_e2e_oracle_tx_rollback() {
    let conn = match e2e_oracle_conn() {
        Some(c) => c,
        None => return,
    };
    let table = unique_table_name("e2e_tx_ora");
    let table_lower = table.to_lowercase();
    conn.execute(
        &format!(
            "CREATE TABLE \"{}\" (id NUMBER GENERATED ALWAYS AS IDENTITY PRIMARY KEY, name VARCHAR2(255))",
            table_lower
        ),
        &[],
    )
    .unwrap();

    conn.execute(
        &format!("INSERT INTO \"{}\" (name) VALUES (:1)", table_lower),
        &[&"Alice"],
    )
    .unwrap();
    conn.rollback().unwrap();

    let rows = conn
        .query(&format!("SELECT COUNT(*) FROM \"{}\"", table_lower), &[])
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let count: i64 = rows[0].get(0).unwrap();
    assert_eq!(count, 0);

    conn.execute(&format!("DROP TABLE \"{}\"", table_lower), &[])
        .unwrap();
}

#[tokio::test]
async fn test_e2e_mysql_tx_savepoint() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_sav_mysql");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255))",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    let mut tx = pool.begin().await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{}` (name) VALUES (?)", table).as_str(),
    ))
    .bind("Alice")
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("SAVEPOINT sp1")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{}` (name) VALUES (?)", table).as_str(),
    ))
    .bind("Bob")
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("ROLLBACK TO SAVEPOINT sp1")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT COUNT(*) as cnt FROM `{}`", table).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 1);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}
