//! M1-T9: 真实数据库方言行为一致性端到端测试
//!
//! 连真实 MySQL/PostgreSQL/SQLite 验证 UPSERT/行锁/RETURNING/标识符引用等方言行为。

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

// ==================== UPSERT ====================

#[tokio::test]
async fn test_mysql_upsert_on_duplicate_key() {
    let pool = match mysql_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_dialect");
    let create_sql = format!(
        "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255) UNIQUE, age INT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!(
        "INSERT INTO `{}` (name, age) VALUES (?, ?) ON DUPLICATE KEY UPDATE age = VALUES(age)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .bind(30i32)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .bind(31i32)
        .execute(&pool)
        .await
        .unwrap();

    let select_sql = format!("SELECT age FROM `{}` WHERE name = ?", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(select_sql.as_str()))
        .bind("Alice")
        .fetch_one(&pool)
        .await
        .unwrap();
    let age: i32 = row.try_get("age").unwrap();
    assert_eq!(age, 31);

    sqlx::query(sqlx::AssertSqlSafe(
        (format!("DROP TABLE `{}`", table)).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_pg_upsert_on_conflict() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_dialect");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT UNIQUE, age INT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!(
        "INSERT INTO \"{}\" (name, age) VALUES ($1, $2) ON CONFLICT (name) DO UPDATE SET age = EXCLUDED.age",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .bind(30i32)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .bind(31i32)
        .execute(&pool)
        .await
        .unwrap();

    let select_sql = format!("SELECT age FROM \"{}\" WHERE name = $1", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(select_sql.as_str()))
        .bind("Alice")
        .fetch_one(&pool)
        .await
        .unwrap();
    let age: i32 = row.try_get("age").unwrap();
    assert_eq!(age, 31);

    sqlx::query(sqlx::AssertSqlSafe(
        (format!("DROP TABLE \"{}\"", table)).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_sqlite_upsert_on_conflict() {
    let pool = match sqlite_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_dialect");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT UNIQUE, age INTEGER)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!(
        "INSERT INTO \"{}\" (name, age) VALUES (?, ?) ON CONFLICT(name) DO UPDATE SET age = excluded.age",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .bind(30i32)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .bind(31i32)
        .execute(&pool)
        .await
        .unwrap();

    let select_sql = format!("SELECT age FROM \"{}\" WHERE name = ?", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(select_sql.as_str()))
        .bind("Alice")
        .fetch_one(&pool)
        .await
        .unwrap();
    let age: i32 = row.try_get("age").unwrap();
    assert_eq!(age, 31);
}

// ==================== 行锁 ====================

#[tokio::test]
async fn test_mysql_for_update() {
    let pool = match mysql_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_dialect");
    let create_sql = format!(
        "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255))",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!("INSERT INTO `{}` (name) VALUES (?)", table);
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .execute(&pool)
        .await
        .unwrap();

    let mut tx = pool.begin().await.unwrap();
    let select_sql = format!("SELECT name FROM `{}` WHERE id = 1 FOR UPDATE", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(select_sql.as_str()))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let name: String = row.try_get("name").unwrap();
    assert_eq!(name, "Alice");
    tx.commit().await.unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        (format!("DROP TABLE `{}`", table)).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_pg_for_update() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_dialect");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!("INSERT INTO \"{}\" (name) VALUES ($1)", table);
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .execute(&pool)
        .await
        .unwrap();

    let mut tx = pool.begin().await.unwrap();
    let select_sql = format!("SELECT name FROM \"{}\" WHERE id = 1 FOR UPDATE", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(select_sql.as_str()))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let name: String = row.try_get("name").unwrap();
    assert_eq!(name, "Alice");
    tx.commit().await.unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        (format!("DROP TABLE \"{}\"", table)).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== RETURNING ====================

#[tokio::test]
async fn test_pg_returning() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_dialect");
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

    sqlx::query(sqlx::AssertSqlSafe(
        (format!("DROP TABLE \"{}\"", table)).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== 标识符引用 ====================

#[tokio::test]
async fn test_mysql_identifier_quote() {
    let pool = match mysql_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_dialect");
    let create_sql = format!(
        "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, `name` VARCHAR(255))",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!("INSERT INTO `{}` (`name`) VALUES (?)", table);
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .execute(&pool)
        .await
        .unwrap();

    let select_sql = format!("SELECT `name` FROM `{}` WHERE `name` = ?", table);
    let row: (String,) = sqlx::query_as(sqlx::AssertSqlSafe(select_sql.as_str()))
        .bind("Alice")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.0, "Alice");

    sqlx::query(sqlx::AssertSqlSafe(
        (format!("DROP TABLE `{}`", table)).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_pg_identifier_quote() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_dialect");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, \"name\" TEXT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!("INSERT INTO \"{}\" (\"name\") VALUES ($1)", table);
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .execute(&pool)
        .await
        .unwrap();

    let select_sql = format!("SELECT \"name\" FROM \"{}\" WHERE \"name\" = $1", table);
    let row: (String,) = sqlx::query_as(sqlx::AssertSqlSafe(select_sql.as_str()))
        .bind("Alice")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.0, "Alice");

    sqlx::query(sqlx::AssertSqlSafe(
        (format!("DROP TABLE \"{}\"", table)).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}
// ==================== v8.3.0: 方言行为 e2e（Oracle + UPSERT） ====================

use common::e2e_env::{e2e_mysql_pool, e2e_oracle_conn, e2e_pg_pool, E2eMysqlDb};

#[tokio::test]
async fn test_e2e_oracle_dialect_identifier() {
    let conn = match e2e_oracle_conn() {
        Some(c) => c,
        None => return,
    };
    let table = unique_table_name("e2e_dia_ora");
    let table_lower = table.to_lowercase();
    conn.execute(
        &format!(
            "CREATE TABLE \"{}\" (id NUMBER GENERATED ALWAYS AS IDENTITY PRIMARY KEY, \"userName\" VARCHAR2(255))",
            table_lower
        ),
        &[],
    )
    .unwrap();
    conn.execute(
        &format!("INSERT INTO \"{}\" (\"userName\") VALUES (:1)", table_lower),
        &[&"Alice"],
    )
    .unwrap();
    conn.commit().unwrap();

    let rows = conn
        .query(
            &format!("SELECT \"userName\" FROM \"{}\"", table_lower),
            &[],
        )
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let name: String = rows[0].get(0).unwrap();
    assert_eq!(name, "Alice");

    conn.execute(&format!("DROP TABLE \"{}\"", table_lower), &[])
        .unwrap();
}

#[tokio::test]
async fn test_e2e_mysql_dialect_upsert() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_upsert_mysql");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id INT PRIMARY KEY, name VARCHAR(255))",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "INSERT INTO `{}` (id, name) VALUES (?, ?) ON DUPLICATE KEY UPDATE name = VALUES(name)",
            table
        )
        .as_str(),
    ))
    .bind(1i32)
    .bind("Alice")
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "INSERT INTO `{}` (id, name) VALUES (?, ?) ON DUPLICATE KEY UPDATE name = VALUES(name)",
            table
        )
        .as_str(),
    ))
    .bind(1i32)
    .bind("Bob")
    .execute(&pool)
    .await
    .unwrap();

    let row: (String,) = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT name FROM `{}` WHERE id = ?", table).as_str(),
    ))
    .bind(1i32)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "Bob");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_pg_dialect_upsert() {
    let pool = match e2e_pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_upsert_pg");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("CREATE TABLE \"{}\" (id INT PRIMARY KEY, name TEXT)", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "INSERT INTO \"{}\" (id, name) VALUES ($1, $2) ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name",
            table
        )
        .as_str(),
    ))
    .bind(1i32)
    .bind("Alice")
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "INSERT INTO \"{}\" (id, name) VALUES ($1, $2) ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name",
            table
        )
        .as_str(),
    ))
    .bind(1i32)
    .bind("Bob")
    .execute(&pool)
    .await
    .unwrap();

    let row: (String,) = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT name FROM \"{}\" WHERE id = $1", table).as_str(),
    ))
    .bind(1i32)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "Bob");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}
