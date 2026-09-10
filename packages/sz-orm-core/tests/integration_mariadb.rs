//! MariaDB 兼容方言集成测试
//!
//! MariaDbDialect 通过 delegate_dialect_to! 委派 MySqlDialect，
//! 生成的 SQL 与 MySqlDialect 一致。用 MySQL 9.6 验证兼容性。
//!
//! 运行方式：cargo test -p sz-orm-core --test integration_mariadb -- --ignored --nocapture

use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use sqlx::Row;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use sz_orm_core::dialect::{get_dialect, ColumnDef};
use sz_orm_core::DbType;

const MYSQL_URL_DEFAULT: &str = "mysql://root:szormtestpwd@127.0.0.1:3306/sz_orm_test";

fn mysql_url() -> String {
    std::env::var("SZ_ORM_MYSQL_URL").unwrap_or_else(|_| MYSQL_URL_DEFAULT.to_string())
}

static TABLE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_table(prefix: &str) -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = TABLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}_{}_{}_{}", prefix, pid, nanos % 1_000_000, counter)
}

async fn setup_pool() -> MySqlPool {
    MySqlPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(30))
        .connect(&mysql_url())
        .await
        .expect("mysql connect failed - is MySQL running?")
}

fn test_columns() -> Vec<ColumnDef> {
    vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: false,
            default: None,
            auto_increment: true,
            primary_key: true,
        },
        ColumnDef {
            name: "name".to_string(),
            sql_type: "VARCHAR(255)".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "value".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ]
}

// ==================== 方言断言测试 ====================

#[test]
fn test_mariadb_dialect_quote() {
    let mariadb = get_dialect(DbType::MariaDB).unwrap();
    let mysql = get_dialect(DbType::MySQL).unwrap();
    assert_eq!(mariadb.quote("user"), mysql.quote("user"));
    assert_eq!(mariadb.quote("user"), "`user`");
}

#[test]
fn test_mariadb_dialect_escape() {
    let mariadb = get_dialect(DbType::MariaDB).unwrap();
    let mysql = get_dialect(DbType::MySQL).unwrap();
    assert_eq!(mariadb.escape_string("it's"), mysql.escape_string("it's"));
}

#[test]
fn test_mariadb_dialect_pagination() {
    let mariadb = get_dialect(DbType::MariaDB).unwrap();
    let mysql = get_dialect(DbType::MySQL).unwrap();
    assert_eq!(
        mariadb.build_pagination("SELECT 1", 10, 20),
        mysql.build_pagination("SELECT 1", 10, 20)
    );
}

#[test]
fn test_mariadb_dialect_db_type() {
    let dialect = get_dialect(DbType::MariaDB).unwrap();
    assert_eq!(dialect.db_type(), DbType::MariaDB);
}

#[test]
fn test_mariadb_dialect_create_table() {
    let mariadb = get_dialect(DbType::MariaDB).unwrap();
    let mysql = get_dialect(DbType::MySQL).unwrap();
    let sql_mariadb = mariadb.build_create_table("test_table", &test_columns());
    let sql_mysql = mysql.build_create_table("test_table", &test_columns());
    assert_eq!(sql_mariadb, sql_mysql);
}

// ==================== 真实 DB 集成测试 ====================

#[tokio::test]
#[ignore]
async fn test_mariadb_crud_on_mysql() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::MariaDB).unwrap();
    let table = unique_table("mariadb");

    let create_sql = dialect.build_create_table(&table, &test_columns());
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{}` (`name`, `value`) VALUES (?, ?)", table).as_str(),
    ))
    .bind("alice")
    .bind(100i64)
    .execute(&pool)
    .await
    .expect("insert");

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT `name`, `value` FROM `{}` WHERE `name` = ?", table).as_str(),
    ))
    .bind("alice")
    .fetch_one(&pool)
    .await
    .expect("select");

    assert_eq!(row.get::<String, _>("name"), "alice");
    assert_eq!(row.get::<i64, _>("value"), 100);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("UPDATE `{}` SET `value` = ? WHERE `name` = ?", table).as_str(),
    ))
    .bind(200i64)
    .bind("alice")
    .execute(&pool)
    .await
    .expect("update");

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT `value` FROM `{}` WHERE `name` = ?", table).as_str(),
    ))
    .bind("alice")
    .fetch_one(&pool)
    .await
    .expect("select after update");
    assert_eq!(row.get::<i64, _>("value"), 200);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DELETE FROM `{}` WHERE `name` = ?", table).as_str(),
    ))
    .bind("alice")
    .execute(&pool)
    .await
    .expect("delete");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}

#[tokio::test]
#[ignore]
async fn test_mariadb_pagination_on_mysql() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::MariaDB).unwrap();
    let table = unique_table("mariadb_pg");

    let create_sql = dialect.build_create_table(&table, &test_columns());
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    for i in 0..5i64 {
        sqlx::query(sqlx::AssertSqlSafe(
            format!("INSERT INTO `{}` (`name`, `value`) VALUES (?, ?)", table).as_str(),
        ))
        .bind(format!("user_{}", i))
        .bind(i)
        .execute(&pool)
        .await
        .expect("insert");
    }

    let pagination_sql = dialect.build_pagination(&format!("SELECT `name` FROM `{}`", table), 2, 2);
    let full_sql = pagination_sql;
    let rows = sqlx::query(sqlx::AssertSqlSafe(full_sql.as_str()))
        .fetch_all(&pool)
        .await
        .expect("select pagination");
    assert_eq!(rows.len(), 2);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}

#[tokio::test]
#[ignore]
async fn test_mariadb_sql_injection_protection() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::MariaDB).unwrap();
    let table = unique_table("mariadb_inj");

    let create_sql = dialect.build_create_table(&table, &test_columns());
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{}` (`name`, `value`) VALUES (?, ?)", table).as_str(),
    ))
    .bind("alice")
    .bind(1i64)
    .execute(&pool)
    .await
    .expect("insert");

    let malicious = "' OR '1'='1";
    let rows = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT * FROM `{}` WHERE `name` = ?", table).as_str(),
    ))
    .bind(malicious)
    .fetch_all(&pool)
    .await
    .expect("select");
    assert_eq!(rows.len(), 0, "SQL injection should return 0 rows");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}
