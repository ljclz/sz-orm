//! ClickHouse 独立方言集成测试
//!
//! ClickHouseDialect 是独立方言实现（列式 OLAP 数据库），使用反引号 quote、
//! LIMIT offset, limit 分页、MergeTree 引擎建表、Int64/Int32 类型映射。
//!
//! 真实 DB 测试通过 ClickHouse MySQL 兼容协议（端口 9004）连接，
//! 使用 sqlx MySQL 驱动执行 ClickHouse SQL。
//!
//! 运行方式：cargo test -p sz-orm-core --test integration_clickhouse -- --ignored --nocapture

use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use sqlx::Row;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use sz_orm_core::dialect::{get_dialect, ColumnDef};
use sz_orm_core::DbType;

const CLICKHOUSE_URL_DEFAULT: &str = "mysql://default:@127.0.0.1:9004/default";

fn clickhouse_url() -> String {
    std::env::var("SZ_ORM_CLICKHOUSE_URL").unwrap_or_else(|_| CLICKHOUSE_URL_DEFAULT.to_string())
}

static TABLE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_table(prefix: &str) -> String {
    let pid = std::process::id();

    let counter = TABLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("ch_{}_{}_{}", prefix, pid, counter)
}

async fn setup_pool() -> MySqlPool {
    MySqlPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(30))
        .connect(&clickhouse_url())
        .await
        .expect("clickhouse connect failed - is ClickHouse MySQL port 9004 running?")
}

fn test_columns() -> Vec<ColumnDef> {
    vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
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
fn test_clickhouse_dialect_quote() {
    let dialect = get_dialect(DbType::ClickHouse).unwrap();
    assert_eq!(dialect.quote("user"), "`user`");
}

#[test]
fn test_clickhouse_dialect_escape() {
    let dialect = get_dialect(DbType::ClickHouse).unwrap();
    assert_eq!(dialect.escape_string("it's"), "it\\'s");
}

#[test]
fn test_clickhouse_dialect_pagination() {
    let dialect = get_dialect(DbType::ClickHouse).unwrap();
    let sql = dialect.build_pagination("SELECT 1", 10, 20);
    assert!(
        sql.contains("LIMIT"),
        "ClickHouse pagination must contain LIMIT"
    );
}

#[test]
fn test_clickhouse_dialect_db_type() {
    let dialect = get_dialect(DbType::ClickHouse).unwrap();
    assert_eq!(dialect.db_type(), DbType::ClickHouse);
}

#[test]
fn test_clickhouse_dialect_no_returning() {
    let dialect = get_dialect(DbType::ClickHouse).unwrap();
    assert!(!dialect.supports_returning());
}

#[test]
fn test_clickhouse_dialect_no_lock() {
    let dialect = get_dialect(DbType::ClickHouse).unwrap();
    assert!(!dialect.supports_lock_for_update());
    assert!(!dialect.supports_lock_shared());
}

#[test]
fn test_clickhouse_dialect_create_table() {
    let dialect = get_dialect(DbType::ClickHouse).unwrap();
    let sql = dialect.build_create_table("test_table", &test_columns());
    assert!(
        sql.contains("MergeTree"),
        "ClickHouse CREATE TABLE must specify engine"
    );
}

#[test]
fn test_clickhouse_type_mapping() {
    let dialect = get_dialect(DbType::ClickHouse).unwrap();
    let sql = dialect.build_create_table("test_table", &test_columns());
    assert!(sql.contains("Int64"), "BIGINT should map to Int64");
}

// ==================== 真实 DB 集成测试 ====================

#[tokio::test]
#[ignore]
async fn test_clickhouse_crud() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::ClickHouse).unwrap();
    let table = unique_table("crud");

    let create_sql = dialect.build_create_table(&table, &test_columns());
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "INSERT INTO `{}` (`id`, `name`, `value`) VALUES (1, 'alice', 100)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .expect("insert");

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "SELECT `name`, `value` FROM `{}` WHERE `name` = 'alice'",
            table
        )
        .as_str(),
    ))
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
async fn test_clickhouse_sql_injection_protection() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::ClickHouse).unwrap();
    let table = unique_table("inject");

    let create_sql = dialect.build_create_table(&table, &test_columns());
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "INSERT INTO `{}` (`id`, `name`, `value`) VALUES (1, 'alice', 100)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .expect("insert");

    let malicious = "' OR '1'='1";
    let escaped = dialect.escape_string(malicious);
    let select_sql = format!(
        "SELECT CAST(COUNT(*) AS Int64) AS cnt FROM `{}` WHERE `name` = '{}'",
        table, escaped
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(select_sql.as_str()))
        .fetch_one(&pool)
        .await
        .expect("select");
    let count: i64 = row.get("cnt");
    assert_eq!(count, 0, "malicious input should match 0 rows");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}

#[tokio::test]
#[ignore]
async fn test_clickhouse_pagination() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::ClickHouse).unwrap();
    let table = unique_table("pg");

    let create_sql = dialect.build_create_table(&table, &test_columns());
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    for i in 1..=5i64 {
        sqlx::query(sqlx::AssertSqlSafe(
            format!(
                "INSERT INTO `{}` (`id`, `name`, `value`) VALUES ({}, 'user_{}', {})",
                table,
                i,
                i,
                i * 10
            )
            .as_str(),
        ))
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
