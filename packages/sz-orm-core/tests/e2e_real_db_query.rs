//! v8.3.0: 真实数据库查询构建端到端测试
//!
//! 连第二台服务器 MySQL/PostgreSQL + 本机 Oracle 验证查询构建
//! WHERE/ORDER BY/LIMIT/JOIN/聚合/子查询/参数化。

#![cfg(feature = "e2e-real-db")]

use sqlx::Row;

#[allow(dead_code)]
mod common;

use common::cleanup::unique_table_name;
use common::e2e_env::{e2e_mysql_pool, e2e_oracle_conn, e2e_pg_pool, E2eMysqlDb};

// ==================== MySQL 查询构建 ====================

#[tokio::test]
async fn test_e2e_mysql_query_where_conditions() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_qry_mysql");
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

    for (name, age) in [("Alice", 30), ("Bob", 25), ("Charlie", 35), ("Dave", 40)] {
        sqlx::query(sqlx::AssertSqlSafe(
            format!("INSERT INTO `{}` (name, age) VALUES (?, ?)", table).as_str(),
        ))
        .bind(name)
        .bind(age)
        .execute(&pool)
        .await
        .unwrap();
    }

    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(
        format!(
            "SELECT name FROM `{}` WHERE age > ? AND age < ? ORDER BY age",
            table
        )
        .as_str(),
    ))
    .bind(25i32)
    .bind(40i32)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "Alice");
    assert_eq!(rows[1].0, "Charlie");

    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(
        format!(
            "SELECT name FROM `{}` WHERE name LIKE ? ORDER BY name",
            table
        )
        .as_str(),
    ))
    .bind("A%")
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "Alice");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_mysql_query_order_limit_offset() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_qry_mysql");
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

    for i in 0..20 {
        sqlx::query(sqlx::AssertSqlSafe(
            format!("INSERT INTO `{}` (val) VALUES (?)", table).as_str(),
        ))
        .bind(i)
        .execute(&pool)
        .await
        .unwrap();
    }

    let rows: Vec<(i32,)> = sqlx::query_as(sqlx::AssertSqlSafe(
        format!(
            "SELECT val FROM `{}` ORDER BY val DESC LIMIT ? OFFSET ?",
            table
        )
        .as_str(),
    ))
    .bind(5i64)
    .bind(5i64)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0].0, 14);
    assert_eq!(rows[4].0, 10);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_mysql_query_aggregate() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_agg_mysql");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, category VARCHAR(50), val INT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    for (cat, val) in [("A", 10), ("A", 20), ("B", 30), ("B", 40), ("C", 50)] {
        sqlx::query(sqlx::AssertSqlSafe(
            format!("INSERT INTO `{}` (category, val) VALUES (?, ?)", table).as_str(),
        ))
        .bind(cat)
        .bind(val)
        .execute(&pool)
        .await
        .unwrap();
    }

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "SELECT COUNT(*) as cnt, SUM(val) as sum, AVG(val) as avg, MIN(val) as min, MAX(val) as max FROM `{}`",
            table
        )
        .as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    let cnt: i64 = row.try_get("cnt").unwrap();
    let sum: i64 = row.try_get("sum").unwrap();
    let min: i32 = row.try_get("min").unwrap();
    let max: i32 = row.try_get("max").unwrap();
    assert_eq!(cnt, 5);
    assert_eq!(sum, 150);
    assert_eq!(min, 10);
    assert_eq!(max, 50);

    let rows: Vec<(String, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(
        format!(
            "SELECT category, SUM(val) as sum FROM `{}` GROUP BY category ORDER BY category",
            table
        )
        .as_str(),
    ))
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].0, "A");
    assert_eq!(rows[0].1, 30);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_mysql_query_join() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let users = unique_table_name("e2e_join_u_mysql");
    let orders = unique_table_name("e2e_join_o_mysql");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255))",
            users
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, user_id BIGINT, amount INT)",
            orders
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{}` (name) VALUES (?), (?)", users).as_str(),
    ))
    .bind("Alice")
    .bind("Bob")
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "INSERT INTO `{}` (user_id, amount) VALUES (?, ?), (?, ?)",
            orders
        )
        .as_str(),
    ))
    .bind(1i64)
    .bind(100i32)
    .bind(2i64)
    .bind(200i32)
    .execute(&pool)
    .await
    .unwrap();

    let rows: Vec<(String, i32)> = sqlx::query_as(sqlx::AssertSqlSafe(
        format!(
            "SELECT u.name, o.amount FROM `{}` u INNER JOIN `{}` o ON u.id = o.user_id ORDER BY u.name",
            users, orders
        )
        .as_str(),
    ))
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "Alice");
    assert_eq!(rows[0].1, 100);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", orders).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", users).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== PostgreSQL 查询构建 ====================

#[tokio::test]
async fn test_e2e_pg_query_where_conditions() {
    let pool = match e2e_pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_qry_pg");
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

    for (name, age) in [("Alice", 30), ("Bob", 25), ("Charlie", 35), ("Dave", 40)] {
        sqlx::query(sqlx::AssertSqlSafe(
            format!("INSERT INTO \"{}\" (name, age) VALUES ($1, $2)", table).as_str(),
        ))
        .bind(name)
        .bind(age)
        .execute(&pool)
        .await
        .unwrap();
    }

    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(
        format!(
            "SELECT name FROM \"{}\" WHERE age > $1 AND age < $2 ORDER BY age",
            table
        )
        .as_str(),
    ))
    .bind(25i32)
    .bind(40i32)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "Alice");
    assert_eq!(rows[1].0, "Charlie");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_pg_query_aggregate() {
    let pool = match e2e_pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_agg_pg");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, category TEXT, val INT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    for (cat, val) in [("A", 10), ("A", 20), ("B", 30), ("B", 40), ("C", 50)] {
        sqlx::query(sqlx::AssertSqlSafe(
            format!("INSERT INTO \"{}\" (category, val) VALUES ($1, $2)", table).as_str(),
        ))
        .bind(cat)
        .bind(val)
        .execute(&pool)
        .await
        .unwrap();
    }

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "SELECT COUNT(*) as cnt, SUM(val) as sum, MIN(val) as min, MAX(val) as max FROM \"{}\"",
            table
        )
        .as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    let cnt: i64 = row.try_get("cnt").unwrap();
    let sum: i64 = row.try_get("sum").unwrap();
    assert_eq!(cnt, 5);
    assert_eq!(sum, 150);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_pg_query_join() {
    let pool = match e2e_pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let users = unique_table_name("e2e_join_u_pg");
    let orders = unique_table_name("e2e_join_o_pg");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
            users
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, user_id BIGINT, amount INT)",
            orders
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO \"{}\" (name) VALUES ($1), ($2)", users).as_str(),
    ))
    .bind("Alice")
    .bind("Bob")
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "INSERT INTO \"{}\" (user_id, amount) VALUES ($1, $2), ($3, $4)",
            orders
        )
        .as_str(),
    ))
    .bind(1i64)
    .bind(100i32)
    .bind(2i64)
    .bind(200i32)
    .execute(&pool)
    .await
    .unwrap();

    let rows: Vec<(String, i32)> = sqlx::query_as(sqlx::AssertSqlSafe(
        format!(
            "SELECT u.name, o.amount FROM \"{}\" u INNER JOIN \"{}\" o ON u.id = o.user_id ORDER BY u.name",
            users, orders
        )
        .as_str(),
    ))
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "Alice");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", orders).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", users).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== Oracle 查询构建 ====================

#[tokio::test]
async fn test_e2e_oracle_query_where_conditions() {
    let conn = match e2e_oracle_conn() {
        Some(c) => c,
        None => return,
    };
    let table = unique_table_name("e2e_qry_ora");
    let table_lower = table.to_lowercase();
    conn.execute(
        &format!(
            "CREATE TABLE \"{}\" (id NUMBER GENERATED ALWAYS AS IDENTITY PRIMARY KEY, name VARCHAR2(255), age NUMBER)",
            table_lower
        ),
        &[],
    )
    .unwrap();
    for (name, age) in [("Alice", 30), ("Bob", 25), ("Charlie", 35)] {
        conn.execute(
            &format!(
                "INSERT INTO \"{}\" (name, age) VALUES (:1, :2)",
                table_lower
            ),
            &[&name, &age],
        )
        .unwrap();
    }
    conn.commit().unwrap();

    let rows = conn
        .query(
            &format!(
                "SELECT name FROM \"{}\" WHERE age > :1 ORDER BY age",
                table_lower
            ),
            &[&28i32],
        )
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows.len(), 2);
    let name: String = rows[0].get(0).unwrap();
    assert_eq!(name, "Alice");

    conn.execute(&format!("DROP TABLE \"{}\"", table_lower), &[])
        .unwrap();
}

#[tokio::test]
async fn test_e2e_oracle_query_aggregate() {
    let conn = match e2e_oracle_conn() {
        Some(c) => c,
        None => return,
    };
    let table = unique_table_name("e2e_agg_ora");
    let table_lower = table.to_lowercase();
    conn.execute(
        &format!(
            "CREATE TABLE \"{}\" (id NUMBER GENERATED ALWAYS AS IDENTITY PRIMARY KEY, val NUMBER)",
            table_lower
        ),
        &[],
    )
    .unwrap();
    for val in [10, 20, 30, 40, 50] {
        conn.execute(
            &format!("INSERT INTO \"{}\" (val) VALUES (:1)", table_lower),
            &[&val],
        )
        .unwrap();
    }
    conn.commit().unwrap();

    let rows = conn
        .query(
            &format!("SELECT COUNT(*), SUM(val) FROM \"{}\"", table_lower),
            &[],
        )
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let count: i64 = rows[0].get(0).unwrap();
    let sum: i64 = rows[0].get(1).unwrap();
    assert_eq!(count, 5);
    assert_eq!(sum, 150);

    conn.execute(&format!("DROP TABLE \"{}\"", table_lower), &[])
        .unwrap();
}
