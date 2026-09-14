//! GaussDB 兼容方言集成测试
//!
//! GaussDbDialect 通过 delegate_dialect_to! 委派 PostgreSqlDialect，
//! 生成的 SQL 与 PostgreSqlDialect 一致。用 PostgreSQL 18 验证兼容性。
//!
//! 运行方式：cargo test -p sz-orm-core --test integration_gaussdb -- --ignored --nocapture

use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use sz_orm_core::dialect::{get_dialect, ColumnDef};
use sz_orm_core::DbType;

const PG_URL_DEFAULT: &str = "postgres://postgres:test123@127.0.0.1:5432/sz_orm_test";

fn pg_url() -> String {
    std::env::var("SZ_ORM_PG_URL").unwrap_or_else(|_| PG_URL_DEFAULT.to_string())
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

async fn setup_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(30))
        .connect(&pg_url())
        .await
        .expect("pg connect failed - is PostgreSQL running?")
}

fn test_columns() -> Vec<ColumnDef> {
    vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGSERIAL".to_string(),
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
fn test_gaussdb_dialect_quote() {
    let gaussdb = get_dialect(DbType::GaussDB).unwrap();
    let pg = get_dialect(DbType::PostgreSQL).unwrap();
    assert_eq!(gaussdb.quote("user"), pg.quote("user"));
    assert_eq!(gaussdb.quote("user"), "\"user\"");
}

#[test]
fn test_gaussdb_dialect_escape() {
    let gaussdb = get_dialect(DbType::GaussDB).unwrap();
    let pg = get_dialect(DbType::PostgreSQL).unwrap();
    assert_eq!(gaussdb.escape_string("it's"), pg.escape_string("it's"));
}

#[test]
fn test_gaussdb_dialect_pagination() {
    let gaussdb = get_dialect(DbType::GaussDB).unwrap();
    let pg = get_dialect(DbType::PostgreSQL).unwrap();
    assert_eq!(
        gaussdb.build_pagination("SELECT 1", 10, 20),
        pg.build_pagination("SELECT 1", 10, 20)
    );
}

#[test]
fn test_gaussdb_dialect_db_type() {
    let dialect = get_dialect(DbType::GaussDB).unwrap();
    assert_eq!(dialect.db_type(), DbType::GaussDB);
}

#[test]
fn test_gaussdb_dialect_create_table() {
    let gaussdb = get_dialect(DbType::GaussDB).unwrap();
    let pg = get_dialect(DbType::PostgreSQL).unwrap();
    let sql_gaussdb = gaussdb.build_create_table("test_table", &test_columns());
    let sql_pg = pg.build_create_table("test_table", &test_columns());
    assert_eq!(sql_gaussdb, sql_pg);
}

// ==================== 真实 DB 集成测试 ====================

#[tokio::test]
#[ignore]
async fn test_gaussdb_crud_on_pg() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::GaussDB).unwrap();
    let table = unique_table("gaussdb");

    let create_sql = dialect.build_create_table(&table, &test_columns());
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "INSERT INTO \"{}\" (\"name\", \"value\") VALUES ($1, $2)",
            table
        )
        .as_str(),
    ))
    .bind("alice")
    .bind(100i64)
    .execute(&pool)
    .await
    .expect("insert");

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "SELECT \"name\", \"value\" FROM \"{}\" WHERE \"name\" = $1",
            table
        )
        .as_str(),
    ))
    .bind("alice")
    .fetch_one(&pool)
    .await
    .expect("select");

    assert_eq!(row.get::<String, _>("name"), "alice");
    assert_eq!(row.get::<i64, _>("value"), 100);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}

#[tokio::test]
#[ignore]
async fn test_gaussdb_pagination_on_pg() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::GaussDB).unwrap();
    let table = unique_table("gaussdb_pg");

    let create_sql = dialect.build_create_table(&table, &test_columns());
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    for i in 0..5i64 {
        sqlx::query(sqlx::AssertSqlSafe(
            format!(
                "INSERT INTO \"{}\" (\"name\", \"value\") VALUES ($1, $2)",
                table
            )
            .as_str(),
        ))
        .bind(format!("user_{}", i))
        .bind(i)
        .execute(&pool)
        .await
        .expect("insert");
    }

    let pagination_sql =
        dialect.build_pagination(&format!("SELECT \"name\" FROM \"{}\"", table), 2, 2);
    let rows = sqlx::query(sqlx::AssertSqlSafe(pagination_sql.as_str()))
        .fetch_all(&pool)
        .await
        .expect("select pagination");
    assert_eq!(rows.len(), 2);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}

#[tokio::test]
#[ignore]
async fn test_gaussdb_sql_injection_protection() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::GaussDB).unwrap();
    let table = unique_table("gaussdb_inj");

    let create_sql = dialect.build_create_table(&table, &test_columns());
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "INSERT INTO \"{}\" (\"name\", \"value\") VALUES ($1, $2)",
            table
        )
        .as_str(),
    ))
    .bind("alice")
    .bind(1i64)
    .execute(&pool)
    .await
    .expect("insert");

    let malicious = "' OR '1'='1";
    let rows = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT * FROM \"{}\" WHERE \"name\" = $1", table).as_str(),
    ))
    .bind(malicious)
    .fetch_all(&pool)
    .await
    .expect("select");
    assert_eq!(rows.len(), 0, "SQL injection should return 0 rows");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}
