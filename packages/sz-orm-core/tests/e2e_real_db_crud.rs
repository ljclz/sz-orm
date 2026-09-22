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

#[allow(dead_code)]
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
// ==================== v8.3.0: 第二台服务器 MySQL CRUD ====================

use common::e2e_env::{e2e_mysql_pool, e2e_oracle_conn, e2e_pg_pool, E2eMysqlDb};

#[tokio::test]
async fn test_e2e_mysql_crud_full_lifecycle_test_db() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_crud_mysql");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255), age INT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{}` (name, age) VALUES (?, ?)", table).as_str(),
    ))
    .bind("Alice")
    .bind(30i32)
    .execute(&pool)
    .await
    .unwrap();

    let row: (String, i32) = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT name, age FROM `{}` WHERE name = ?", table).as_str(),
    ))
    .bind("Alice")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "Alice");
    assert_eq!(row.1, 30);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("UPDATE `{}` SET age = ? WHERE name = ?", table).as_str(),
    ))
    .bind(31i32)
    .bind("Alice")
    .execute(&pool)
    .await
    .unwrap();

    let row: (i32,) = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT age FROM `{}` WHERE name = ?", table).as_str(),
    ))
    .bind("Alice")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, 31);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DELETE FROM `{}` WHERE name = ?", table).as_str(),
    ))
    .bind("Alice")
    .execute(&pool)
    .await
    .unwrap();

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
async fn test_e2e_mysql_crud_full_lifecycle_shop_db() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Shop).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_crud_mysql");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255), age INT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{}` (name, age) VALUES (?, ?)", table).as_str(),
    ))
    .bind("ShopUser")
    .bind(42i32)
    .execute(&pool)
    .await
    .unwrap();

    let row: (String, i32) = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT name, age FROM `{}` WHERE name = ?", table).as_str(),
    ))
    .bind("ShopUser")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "ShopUser");
    assert_eq!(row.1, 42);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_mysql_crud_full_lifecycle_njszjt_db() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Njszjt).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_crud_mysql");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255), age INT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{}` (name, age) VALUES (?, ?)", table).as_str(),
    ))
    .bind("NjszjtUser")
    .bind(18i32)
    .execute(&pool)
    .await
    .unwrap();

    let row: (String, i32) = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT name, age FROM `{}` WHERE name = ?", table).as_str(),
    ))
    .bind("NjszjtUser")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "NjszjtUser");
    assert_eq!(row.1, 18);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== v8.3.0: 第二台服务器 PostgreSQL CRUD ====================

#[tokio::test]
async fn test_e2e_pg_crud_full_lifecycle() {
    let pool = match e2e_pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_crud_pg");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT, age INT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO \"{}\" (name, age) VALUES ($1, $2)", table).as_str(),
    ))
    .bind("Alice")
    .bind(30i32)
    .execute(&pool)
    .await
    .unwrap();

    let row: (String, i32) = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT name, age FROM \"{}\" WHERE name = $1", table).as_str(),
    ))
    .bind("Alice")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "Alice");
    assert_eq!(row.1, 30);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("UPDATE \"{}\" SET age = $1 WHERE name = $2", table).as_str(),
    ))
    .bind(31i32)
    .bind("Alice")
    .execute(&pool)
    .await
    .unwrap();

    let row: (i32,) = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT age FROM \"{}\" WHERE name = $1", table).as_str(),
    ))
    .bind("Alice")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, 31);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DELETE FROM \"{}\" WHERE name = $1", table).as_str(),
    ))
    .bind("Alice")
    .execute(&pool)
    .await
    .unwrap();

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

// ==================== v8.3.0: Oracle CRUD（本机 23ai，sys/Sysdba） ====================

#[tokio::test]
async fn test_e2e_oracle_crud_full_lifecycle() {
    let conn = match e2e_oracle_conn() {
        Some(c) => c,
        None => return,
    };
    let table = unique_table_name("e2e_crud_ora");
    let table_lower = table.to_lowercase();
    conn.execute(
        &format!(
            "CREATE TABLE \"{}\" (id NUMBER GENERATED ALWAYS AS IDENTITY PRIMARY KEY, name VARCHAR2(255), age NUMBER)",
            table_lower
        ),
        &[],
    )
    .unwrap();

    conn.execute(
        &format!(
            "INSERT INTO \"{}\" (name, age) VALUES (:1, :2)",
            table_lower
        ),
        &[&"Alice", &30i32],
    )
    .unwrap();
    conn.commit().unwrap();

    let rows = conn
        .query(
            &format!("SELECT name, age FROM \"{}\" WHERE name = :1", table_lower),
            &[&"Alice"],
        )
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows.len(), 1);
    let name: String = rows[0].get(0).unwrap();
    let age: i32 = rows[0].get(1).unwrap();
    assert_eq!(name, "Alice");
    assert_eq!(age, 30);

    conn.execute(
        &format!("UPDATE \"{}\" SET age = :1 WHERE name = :2", table_lower),
        &[&31i32, &"Alice"],
    )
    .unwrap();
    conn.commit().unwrap();

    let rows = conn
        .query(
            &format!("SELECT age FROM \"{}\" WHERE name = :1", table_lower),
            &[&"Alice"],
        )
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let age: i32 = rows[0].get(0).unwrap();
    assert_eq!(age, 31);

    conn.execute(
        &format!("DELETE FROM \"{}\" WHERE name = :1", table_lower),
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
    assert_eq!(count, 0);

    conn.execute(&format!("DROP TABLE \"{}\"", table_lower), &[])
        .unwrap();
}

// ==================== v8.3.0: 数据类型覆盖测试 ====================

#[tokio::test]
async fn test_e2e_mysql_crud_data_types() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_dtype_mysql");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, \
             i INT, bi BIGINT, vc VARCHAR(255), tx TEXT, \
             bo BOOLEAN, dec_val DECIMAL(10,2), json_val JSON)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "INSERT INTO `{}` (i, bi, vc, tx, bo, dec_val, json_val) VALUES (?, ?, ?, ?, ?, ?, ?)",
            table
        )
        .as_str(),
    ))
    .bind(42i32)
    .bind(9_999_999_999i64)
    .bind("varchar_val")
    .bind("text_val")
    .bind(true)
    .bind("123.45")
    .bind(r#"{"key":"value"}"#)
    .execute(&pool)
    .await
    .unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "SELECT i, bi, vc, tx, bo, dec_val FROM `{}` WHERE i = ?",
            table
        )
        .as_str(),
    ))
    .bind(42i32)
    .fetch_one(&pool)
    .await
    .unwrap();
    let i: i32 = row.try_get("i").unwrap();
    let bi: i64 = row.try_get("bi").unwrap();
    let vc: String = row.try_get("vc").unwrap();
    let tx: String = row.try_get("tx").unwrap();
    let bo: bool = row.try_get("bo").unwrap();
    assert_eq!(i, 42);
    assert_eq!(bi, 9_999_999_999);
    assert_eq!(vc, "varchar_val");
    assert_eq!(tx, "text_val");
    assert!(bo);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_pg_crud_data_types() {
    let pool = match e2e_pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_dtype_pg");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, \
             i INT, bi BIGINT, vc VARCHAR(255), tx TEXT, \
             bo BOOLEAN, dec_val DECIMAL(10,2), json_val JSONB)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "INSERT INTO \"{}\" (i, bi, vc, tx, bo, dec_val, json_val) VALUES ($1, $2, $3, $4, $5, $6::numeric, $7)",
            table
        )
        .as_str(),
    ))
    .bind(42i32)
    .bind(9_999_999_999i64)
    .bind("varchar_val")
    .bind("text_val")
    .bind(true)
    .bind("123.45")
    .bind(serde_json::json!({"key": "value"}))
    .execute(&pool)
    .await
    .unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "SELECT i, bi, vc, tx, bo, dec_val FROM \"{}\" WHERE i = $1",
            table
        )
        .as_str(),
    ))
    .bind(42i32)
    .fetch_one(&pool)
    .await
    .unwrap();
    let i: i32 = row.try_get("i").unwrap();
    let bi: i64 = row.try_get("bi").unwrap();
    let vc: String = row.try_get("vc").unwrap();
    let tx: String = row.try_get("tx").unwrap();
    let bo: bool = row.try_get("bo").unwrap();
    assert_eq!(i, 42);
    assert_eq!(bi, 9_999_999_999);
    assert_eq!(vc, "varchar_val");
    assert_eq!(tx, "text_val");
    assert!(bo);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== v8.3.0: 批量 INSERT 测试 ====================

#[tokio::test]
async fn test_e2e_mysql_crud_batch_insert() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_batch_mysql");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255), val INT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    let placeholders: Vec<String> = (0..100).map(|_| "(?, ?)".to_string()).collect();
    let insert_sql = format!(
        "INSERT INTO `{}` (name, val) VALUES {}",
        table,
        placeholders.join(", ")
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()));
    for i in 0..100 {
        query = query.bind(format!("user_{}", i)).bind(i);
    }
    let result = query.execute(&pool).await.unwrap();
    assert_eq!(result.rows_affected(), 100);

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT COUNT(*) as cnt FROM `{}`", table).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 100);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_pg_crud_batch_insert() {
    let pool = match e2e_pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_batch_pg");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT, val INT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    let placeholders: Vec<String> = (1..=100)
        .map(|i| format!("(${}, ${})", i * 2 - 1, i * 2))
        .collect();
    let insert_sql = format!(
        "INSERT INTO \"{}\" (name, val) VALUES {}",
        table,
        placeholders.join(", ")
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()));
    for i in 0..100 {
        query = query.bind(format!("user_{}", i)).bind(i);
    }
    let result = query.execute(&pool).await.unwrap();
    assert_eq!(result.rows_affected(), 100);

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT COUNT(*) as cnt FROM \"{}\"", table).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 100);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== v8.3.0: 批量 UPDATE 测试 ====================

#[tokio::test]
async fn test_e2e_mysql_crud_batch_update() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_bupd_mysql");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255), val INT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    let placeholders: Vec<String> = (0..10).map(|_| "(?, ?)".to_string()).collect();
    let insert_sql = format!(
        "INSERT INTO `{}` (name, val) VALUES {}",
        table,
        placeholders.join(", ")
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()));
    for i in 0..10 {
        query = query.bind(format!("user_{}", i)).bind(i);
    }
    query.execute(&pool).await.unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!("UPDATE `{}` SET val = val + 100 WHERE val >= ?", table).as_str(),
    ))
    .bind(5i32)
    .execute(&pool)
    .await
    .unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT COUNT(*) as cnt FROM `{}` WHERE val >= ?", table).as_str(),
    ))
    .bind(105i32)
    .fetch_one(&pool)
    .await
    .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 5);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== v8.3.0: RETURNING 子句测试 ====================

#[tokio::test]
async fn test_e2e_pg_crud_returning() {
    let pool = match e2e_pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_ret_pg");
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

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "INSERT INTO \"{}\" (name) VALUES ($1) RETURNING id, name",
            table
        )
        .as_str(),
    ))
    .bind("Alice")
    .fetch_one(&pool)
    .await
    .unwrap();
    let id: i64 = row.try_get("id").unwrap();
    let name: String = row.try_get("name").unwrap();
    assert!(id > 0);
    assert_eq!(name, "Alice");

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "UPDATE \"{}\" SET name = $1 WHERE id = $2 RETURNING id, name",
            table
        )
        .as_str(),
    ))
    .bind("Bob")
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let name: String = row.try_get("name").unwrap();
    assert_eq!(name, "Bob");

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("DELETE FROM \"{}\" WHERE id = $1 RETURNING id, name", table).as_str(),
    ))
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let name: String = row.try_get("name").unwrap();
    assert_eq!(name, "Bob");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_mysql_crud_returning_skipped() {
    eprintln!("MySQL 不支持 RETURNING 子句（ADR-007），覆盖矩阵 MySQL RETURNING 格标 N/A");
}
// ==================== v8.4.0: Oracle 批量操作 ====================

#[tokio::test]
async fn test_e2e_oracle_crud_batch_insert() {
    let conn = match e2e_oracle_conn() {
        Some(c) => c,
        None => return,
    };
    let table = unique_table_name("e2e_batch_ora");
    let table_lower = table.to_lowercase();

    conn.execute(
        &format!(
            "CREATE TABLE \"{}\" (id NUMBER GENERATED ALWAYS AS IDENTITY PRIMARY KEY, name VARCHAR2(255), age NUMBER)",
            table_lower
        ),
        &[],
    )
    .unwrap();

    for i in 1..=10i32 {
        conn.execute(
            &format!(
                "INSERT INTO \"{}\" (name, age) VALUES (:1, :2)",
                table_lower
            ),
            &[&format!("user{}", i), &i],
        )
        .unwrap();
    }
    conn.commit().unwrap();

    let rows = conn
        .query(&format!("SELECT COUNT(*) FROM \"{}\"", table_lower), &[])
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let count: i64 = rows[0].get(0).unwrap();
    assert_eq!(count, 10);

    conn.execute(&format!("DROP TABLE \"{}\"", table_lower), &[])
        .unwrap();
}

// ==================== v8.4.0: Oracle RETURNING（通过 SEQUENCE 模拟） ====================

#[tokio::test]
async fn test_e2e_oracle_crud_returning() {
    let conn = match e2e_oracle_conn() {
        Some(c) => c,
        None => return,
    };
    let table = unique_table_name("e2e_ret_ora");
    let table_lower = table.to_lowercase();
    let seq_name = format!("{}_seq", table_lower);

    conn.execute(
        &format!(
            "CREATE TABLE \"{}\" (id NUMBER PRIMARY KEY, name VARCHAR2(255))",
            table_lower
        ),
        &[],
    )
    .unwrap();
    conn.execute(
        &format!(
            "CREATE SEQUENCE \"{}\" START WITH 1 INCREMENT BY 1",
            seq_name
        ),
        &[],
    )
    .unwrap();

    conn.execute(
        &format!(
            "INSERT INTO \"{}\" (id, name) VALUES (\"{}\".NEXTVAL, :1)",
            table_lower, seq_name
        ),
        &[&"Alice"],
    )
    .unwrap();
    conn.commit().unwrap();

    let rows = conn
        .query(
            &format!("SELECT id, name FROM \"{}\" WHERE name = :1", table_lower),
            &[&"Alice"],
        )
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows.len(), 1);
    let id: i64 = rows[0].get(0).unwrap();
    let name: String = rows[0].get(1).unwrap();
    assert_eq!(id, 1);
    assert_eq!(name, "Alice");

    conn.execute(&format!("DROP SEQUENCE \"{}\"", seq_name), &[])
        .unwrap();
    conn.execute(&format!("DROP TABLE \"{}\"", table_lower), &[])
        .unwrap();
}
