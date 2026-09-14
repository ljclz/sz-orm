//! OceanBase 兼容方言集成测试
//!
//! OceanBase 直接使用 MySqlDialect（get_dialect 返回 Box::new(MySqlDialect)），
//! 生成的 SQL 与 MySqlDialect 一致。用 MySQL 9.6 验证兼容性。
//!
//! 运行方式：cargo test -p sz-orm-core --test integration_oceanbase -- --ignored --nocapture

use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use sqlx::Row;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use sz_orm_core::dialect::{get_dialect, ColumnDef};
use sz_orm_core::DbType;

const MYSQL_URL_DEFAULT: &str = "mysql://root:test123@127.0.0.1:3306/sz_orm_test";

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
fn test_oceanbase_dialect_quote() {
    let oceanbase = get_dialect(DbType::OceanBase).unwrap();
    let mysql = get_dialect(DbType::MySQL).unwrap();
    assert_eq!(oceanbase.quote("user"), mysql.quote("user"));
    assert_eq!(oceanbase.quote("user"), "`user`");
}

#[test]
fn test_oceanbase_dialect_escape() {
    let oceanbase = get_dialect(DbType::OceanBase).unwrap();
    let mysql = get_dialect(DbType::MySQL).unwrap();
    assert_eq!(oceanbase.escape_string("it's"), mysql.escape_string("it's"));
}

#[test]
fn test_oceanbase_dialect_pagination() {
    let oceanbase = get_dialect(DbType::OceanBase).unwrap();
    let mysql = get_dialect(DbType::MySQL).unwrap();
    assert_eq!(
        oceanbase.build_pagination("SELECT 1", 10, 20),
        mysql.build_pagination("SELECT 1", 10, 20)
    );
}

#[test]
fn test_oceanbase_dialect_db_type() {
    let dialect = get_dialect(DbType::OceanBase).unwrap();
    assert_eq!(dialect.db_type(), DbType::OceanBase);
}

#[test]
fn test_oceanbase_dialect_create_table() {
    let oceanbase = get_dialect(DbType::OceanBase).unwrap();
    let mysql = get_dialect(DbType::MySQL).unwrap();
    let sql_oceanbase = oceanbase.build_create_table("test_table", &test_columns());
    let sql_mysql = mysql.build_create_table("test_table", &test_columns());
    assert_eq!(sql_oceanbase, sql_mysql);
}

// ==================== 真实 DB 集成测试 ====================

#[tokio::test]
#[ignore]
async fn test_oceanbase_crud_on_mysql() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::OceanBase).unwrap();
    let table = unique_table("oceanbase");

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
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}

#[tokio::test]
#[ignore]
async fn test_oceanbase_pagination_on_mysql() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::OceanBase).unwrap();
    let table = unique_table("oceanbase_pg");

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
    let rows = sqlx::query(sqlx::AssertSqlSafe(pagination_sql.as_str()))
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
async fn test_oceanbase_sql_injection_protection() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::OceanBase).unwrap();
    let table = unique_table("oceanbase_inj");

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
