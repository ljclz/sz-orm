//! M1-T2: 真实数据库 CRUD 端到端测试
//!
//! 连真实 MySQL/PostgreSQL/SQLite 验证 CRUD 核心路径。
//! 通过 DATABASE_URL 环境变量配置连接串。
//!
//! # Example
//! DATABASE_URL=mysql://root:test123@127.0.0.1:3306/sz_orm_test
//! DATABASE_URL=postgres://postgres:test123@127.0.0.1:5432/sz_orm_test
//! DATABASE_URL=sqlite://:memory:

#![cfg(feature = "e2e-real-db")]

use sqlx::Row;

mod common;

use common::cleanup::unique_table_name;

/// 获取 MySQL 连接池
async fn mysql_pool() -> Option<sqlx::MySqlPool> {
    let url = std::env::var("MYSQL_URL").ok()?;
    sqlx::MySqlPool::connect(&url).await.ok()
}

/// 获取 PostgreSQL 连接池
async fn pg_pool() -> Option<sqlx::PgPool> {
    let url = std::env::var("POSTGRES_URL").ok()?;
    sqlx::PgPool::connect(&url).await.ok()
}

/// 获取 SQLite 连接池
async fn sqlite_pool() -> Option<sqlx::SqlitePool> {
    sqlx::SqlitePool::connect("sqlite::memory:").await.ok()
}

// ==================== MySQL CRUD ====================

#[tokio::test]
async fn test_mysql_crud_insert_select() {
    let pool = match mysql_pool().await {
        Some(p) => p,
        None => {
            eprintln!("MySQL 未配置，跳过");
            return;
        }
    };
    let table = unique_table_name("e2e_crud");
    let create_sql = format!(
        "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255), age INT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!("INSERT INTO `{}` (name, age) VALUES (?, ?)", table);
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .bind(30i32)
        .execute(&pool)
        .await
        .unwrap();

    let select_sql = format!("SELECT name, age FROM `{}` WHERE name = ?", table);
    let row: (String, i32) = sqlx::query_as(sqlx::AssertSqlSafe(select_sql.as_str()))
        .bind("Alice")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.0, "Alice");
    assert_eq!(row.1, 30);

    let drop_sql = format!("DROP TABLE `{}`", table);
    sqlx::query(sqlx::AssertSqlSafe(drop_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_mysql_crud_update_delete() {
    let pool = match mysql_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_crud");
    let create_sql = format!(
        "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255), age INT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!("INSERT INTO `{}` (name, age) VALUES (?, ?), (?, ?)", table);
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Bob")
        .bind(25i32)
        .bind("Charlie")
        .bind(35i32)
        .execute(&pool)
        .await
        .unwrap();

    let update_sql = format!("UPDATE `{}` SET age = ? WHERE name = ?", table);
    let result = sqlx::query(sqlx::AssertSqlSafe(update_sql.as_str()))
        .bind(26i32)
        .bind("Bob")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(result.rows_affected(), 1);

    let delete_sql = format!("DELETE FROM `{}` WHERE name = ?", table);
    let result = sqlx::query(sqlx::AssertSqlSafe(delete_sql.as_str()))
        .bind("Charlie")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(result.rows_affected(), 1);

    let count_sql = format!("SELECT COUNT(*) as cnt FROM `{}`", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(count_sql.as_str()))
        .fetch_one(&pool)
        .await
        .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 1);

    let drop_sql = format!("DROP TABLE `{}`", table);
    sqlx::query(sqlx::AssertSqlSafe(drop_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_mysql_crud_batch_insert() {
    let pool = match mysql_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_crud");
    let create_sql = format!(
        "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255))",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!("INSERT INTO `{}` (name) VALUES (?), (?), (?)", table);
    let result = sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("User1")
        .bind("User2")
        .bind("User3")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(result.rows_affected(), 3);

    let count_sql = format!("SELECT COUNT(*) as cnt FROM `{}`", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(count_sql.as_str()))
        .fetch_one(&pool)
        .await
        .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 3);

    let drop_sql = format!("DROP TABLE `{}`", table);
    sqlx::query(sqlx::AssertSqlSafe(drop_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
}

// ==================== PostgreSQL CRUD ====================

#[tokio::test]
async fn test_pg_crud_insert_select() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => {
            eprintln!("PostgreSQL 未配置，跳过");
            return;
        }
    };
    let table = unique_table_name("e2e_crud");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT, age INT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!("INSERT INTO \"{}\" (name, age) VALUES ($1, $2)", table);
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .bind(30i32)
        .execute(&pool)
        .await
        .unwrap();

    let select_sql = format!("SELECT name, age FROM \"{}\" WHERE name = $1", table);
    let row: (String, i32) = sqlx::query_as(sqlx::AssertSqlSafe(select_sql.as_str()))
        .bind("Alice")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.0, "Alice");
    assert_eq!(row.1, 30);

    let drop_sql = format!("DROP TABLE \"{}\"", table);
    sqlx::query(sqlx::AssertSqlSafe(drop_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_pg_crud_update_delete() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_crud");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT, age INT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!(
        "INSERT INTO \"{}\" (name, age) VALUES ($1, $2), ($3, $4)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Bob")
        .bind(25i32)
        .bind("Charlie")
        .bind(35i32)
        .execute(&pool)
        .await
        .unwrap();

    let update_sql = format!("UPDATE \"{}\" SET age = $1 WHERE name = $2", table);
    let result = sqlx::query(sqlx::AssertSqlSafe(update_sql.as_str()))
        .bind(26i32)
        .bind("Bob")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(result.rows_affected(), 1);

    let delete_sql = format!("DELETE FROM \"{}\" WHERE name = $1", table);
    let result = sqlx::query(sqlx::AssertSqlSafe(delete_sql.as_str()))
        .bind("Charlie")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(result.rows_affected(), 1);

    let count_sql = format!("SELECT COUNT(*) as cnt FROM \"{}\"", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(count_sql.as_str()))
        .fetch_one(&pool)
        .await
        .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 1);

    let drop_sql = format!("DROP TABLE \"{}\"", table);
    sqlx::query(sqlx::AssertSqlSafe(drop_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_pg_crud_returning() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_crud");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!(
        "INSERT INTO \"{}\" (name) VALUES ($1) RETURNING id, name",
        table
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .fetch_one(&pool)
        .await
        .unwrap();
    let id: i64 = row.try_get("id").unwrap();
    let name: String = row.try_get("name").unwrap();
    assert!(id > 0);
    assert_eq!(name, "Alice");

    let drop_sql = format!("DROP TABLE \"{}\"", table);
    sqlx::query(sqlx::AssertSqlSafe(drop_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
}

// ==================== SQLite CRUD ====================

#[tokio::test]
async fn test_sqlite_crud_insert_select() {
    let pool = match sqlite_pool().await {
        Some(p) => p,
        None => {
            eprintln!("SQLite 未配置，跳过");
            return;
        }
    };
    let table = unique_table_name("e2e_crud");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, age INTEGER)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!("INSERT INTO \"{}\" (name, age) VALUES (?, ?)", table);
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .bind(30i32)
        .execute(&pool)
        .await
        .unwrap();

    let select_sql = format!("SELECT name, age FROM \"{}\" WHERE name = ?", table);
    let row: (String, i32) = sqlx::query_as(sqlx::AssertSqlSafe(select_sql.as_str()))
        .bind("Alice")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.0, "Alice");
    assert_eq!(row.1, 30);
}

#[tokio::test]
async fn test_sqlite_crud_update_delete() {
    let pool = match sqlite_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_crud");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, age INTEGER)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!(
        "INSERT INTO \"{}\" (name, age) VALUES (?, ?), (?, ?)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Bob")
        .bind(25i32)
        .bind("Charlie")
        .bind(35i32)
        .execute(&pool)
        .await
        .unwrap();

    let update_sql = format!("UPDATE \"{}\" SET age = ? WHERE name = ?", table);
    let result = sqlx::query(sqlx::AssertSqlSafe(update_sql.as_str()))
        .bind(26i32)
        .bind("Bob")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(result.rows_affected(), 1);

    let delete_sql = format!("DELETE FROM \"{}\" WHERE name = ?", table);
    let result = sqlx::query(sqlx::AssertSqlSafe(delete_sql.as_str()))
        .bind("Charlie")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(result.rows_affected(), 1);

    let count_sql = format!("SELECT COUNT(*) as cnt FROM \"{}\"", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(count_sql.as_str()))
        .fetch_one(&pool)
        .await
        .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_sqlite_crud_where_clause() {
    let pool = match sqlite_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_crud");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, age INTEGER)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    for (name, age) in [("Alice", 30), ("Bob", 25), ("Charlie", 35), ("Dave", 40)] {
        let insert_sql = format!("INSERT INTO \"{}\" (name, age) VALUES (?, ?)", table);
        sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
            .bind(name)
            .bind(age)
            .execute(&pool)
            .await
            .unwrap();
    }

    let select_sql = format!("SELECT name FROM \"{}\" WHERE age > ? ORDER BY age", table);
    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(select_sql.as_str()))
        .bind(28i32)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].0, "Alice");
    assert_eq!(rows[1].0, "Charlie");
    assert_eq!(rows[2].0, "Dave");
}
