//! GBase 兼容方言集成测试
//!
//! GBaseDialect 通过 delegate_dialect_to! 委派 SqlServerDialect，
//! 生成的 SQL 与 SqlServerDialect 一致。用 SQL Server 验证兼容性。
//!
//! 运行方式：cargo test -p sz-orm-core --test integration_gbase -- --ignored --nocapture

use std::sync::atomic::{AtomicU64, Ordering};
use sz_orm_core::dialect::{get_dialect, ColumnDef};
use sz_orm_core::DbType;
use tiberius::AuthMethod;
use tiberius::Client;
use tiberius::Config;
use tokio_util::compat::TokioAsyncWriteCompatExt;

const MSSQL_HOST_DEFAULT: &str = "sh-mssql-adrul9nm.sql.tencentcdb.com";
const MSSQL_PORT_DEFAULT: u16 = 22527;
const MSSQL_USER_DEFAULT: &str = "test";
const MSSQL_PASSWORD_DEFAULT: &str = "JkbC2jsaWAYDe2Gz";
const MSSQL_DATABASE_DEFAULT: &str = "test";

fn mssql_host() -> String {
    std::env::var("SZ_ORM_MSSQL_HOST").unwrap_or_else(|_| MSSQL_HOST_DEFAULT.to_string())
}

fn mssql_port() -> u16 {
    std::env::var("SZ_ORM_MSSQL_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(MSSQL_PORT_DEFAULT)
}

fn mssql_user() -> String {
    std::env::var("SZ_ORM_MSSQL_USER").unwrap_or_else(|_| MSSQL_USER_DEFAULT.to_string())
}

fn mssql_password() -> String {
    std::env::var("SZ_ORM_MSSQL_PASSWORD").unwrap_or_else(|_| MSSQL_PASSWORD_DEFAULT.to_string())
}

fn mssql_database() -> String {
    std::env::var("SZ_ORM_MSSQL_DATABASE").unwrap_or_else(|_| MSSQL_DATABASE_DEFAULT.to_string())
}

async fn open_client() -> Client<tokio_util::compat::Compat<tokio::net::TcpStream>> {
    let mut config = Config::new();
    config.host(mssql_host());
    config.port(mssql_port());
    config.authentication(AuthMethod::sql_server(mssql_user(), mssql_password()));
    config.database(mssql_database());
    config.trust_cert();

    let tcp = tokio::net::TcpStream::connect(config.get_addr())
        .await
        .expect("connect TCP failed - is SQL Server running?");
    Client::connect(config, tcp.compat_write())
        .await
        .expect("tiberius connect failed")
}

static TABLE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_table(prefix: &str) -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = TABLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "gb_{}_{}_{}",
        prefix,
        pid % 1000,
        (nanos % 100000) as u64 * 1000 + counter
    )
}

async fn drop_table_if_exists(
    client: &mut Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    table: &str,
) {
    let dialect = get_dialect(DbType::GBase).unwrap();
    let sql = dialect.build_drop_table(table, true);
    let _ = client.simple_query(&sql).await;
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
            sql_type: "NVARCHAR(255)".to_string(),
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
fn test_gbase_dialect_quote() {
    let gbase = get_dialect(DbType::GBase).unwrap();
    let mssql = get_dialect(DbType::SqlServer).unwrap();
    assert_eq!(gbase.quote("user"), mssql.quote("user"));
    assert_eq!(gbase.quote("user"), "[user]");
}

#[test]
fn test_gbase_dialect_escape() {
    let gbase = get_dialect(DbType::GBase).unwrap();
    let mssql = get_dialect(DbType::SqlServer).unwrap();
    assert_eq!(gbase.escape_string("it's"), mssql.escape_string("it's"));
}

#[test]
fn test_gbase_dialect_pagination() {
    let gbase = get_dialect(DbType::GBase).unwrap();
    let mssql = get_dialect(DbType::SqlServer).unwrap();
    assert_eq!(
        gbase.build_pagination("SELECT 1", 10, 20),
        mssql.build_pagination("SELECT 1", 10, 20)
    );
}

#[test]
fn test_gbase_dialect_db_type() {
    let dialect = get_dialect(DbType::GBase).unwrap();
    assert_eq!(dialect.db_type(), DbType::GBase);
}

#[test]
fn test_gbase_dialect_create_table() {
    let gbase = get_dialect(DbType::GBase).unwrap();
    let mssql = get_dialect(DbType::SqlServer).unwrap();
    let sql_gbase = gbase.build_create_table("test_table", &test_columns());
    let sql_mssql = mssql.build_create_table("test_table", &test_columns());
    assert_eq!(sql_gbase, sql_mssql);
}

#[test]
fn test_gbase_dialect_not_sqlserver_db_type() {
    let dialect = get_dialect(DbType::GBase).unwrap();
    assert_ne!(dialect.db_type(), DbType::SqlServer);
    assert_eq!(dialect.db_type(), DbType::GBase);
}

// ==================== 真实 DB 集成测试（SQL Server） ====================

#[tokio::test]
#[ignore = "需要 SQL Server 运行（设置 SZ_ORM_MSSQL_* 环境变量覆盖）"]
async fn test_gbase_crud_on_mssql() {
    let mut client = open_client().await;
    let dialect = get_dialect(DbType::GBase).unwrap();
    let table = unique_table("crud");
    drop_table_if_exists(&mut client, &table).await;

    let create_sql = dialect.build_create_table(&table, &test_columns());
    let _ = client
        .simple_query(&create_sql)
        .await
        .expect("create table");

    let insert_sql = format!(
        "INSERT INTO {} ({}, {}) VALUES (@p1, @p2)",
        dialect.quote(&table),
        dialect.quote("name"),
        dialect.quote("value"),
    );
    for (name, value) in [("alice", 100i64), ("bob", 200i64), ("carol", 300i64)] {
        let stream = client
            .query(&insert_sql, &[&name, &value])
            .await
            .expect("insert");
        let _: Vec<tiberius::Row> = stream.into_first_result().await.expect("insert results");
    }

    let count_sql = format!("SELECT COUNT(*) FROM {}", dialect.quote(&table));
    let stream = client.simple_query(&count_sql).await.expect("count");
    let rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("count results");
    let count: i32 = rows[0].get::<i32, _>(0).expect("count value");
    assert_eq!(count, 3);

    let select_sql = format!(
        "SELECT {} FROM {} WHERE {} > @p1 ORDER BY {}",
        dialect.quote("name"),
        dialect.quote(&table),
        dialect.quote("value"),
        dialect.quote("value"),
    );
    let stream = client.query(&select_sql, &[&150i64]).await.expect("select");
    let rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("select results");
    assert_eq!(rows.len(), 2);
    let n0: &str = rows[0].get::<&str, _>(0).expect("name 0");
    assert_eq!(n0, "bob");

    let delete_sql = format!(
        "DELETE FROM {} WHERE {} = @p1",
        dialect.quote(&table),
        dialect.quote("name"),
    );
    let stream = client.query(&delete_sql, &[&"bob"]).await.expect("delete");
    let _: Vec<tiberius::Row> = stream.into_first_result().await.expect("delete results");

    let stream = client.simple_query(&count_sql).await.expect("count2");
    let rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("count2 results");
    let count2: i32 = rows[0].get::<i32, _>(0).expect("count2 value");
    assert_eq!(count2, 2);

    drop_table_if_exists(&mut client, &table).await;
}

#[tokio::test]
#[ignore = "需要 SQL Server 运行（设置 SZ_ORM_MSSQL_* 环境变量覆盖）"]
async fn test_gbase_pagination_on_mssql() {
    let mut client = open_client().await;
    let dialect = get_dialect(DbType::GBase).unwrap();
    let table = unique_table("page");
    drop_table_if_exists(&mut client, &table).await;

    let create_sql = dialect.build_create_table(&table, &test_columns());
    let _ = client
        .simple_query(&create_sql)
        .await
        .expect("create table");

    let insert_sql = format!(
        "INSERT INTO {} ({}, {}) VALUES (@p1, @p2)",
        dialect.quote(&table),
        dialect.quote("name"),
        dialect.quote("value"),
    );
    for i in 0..5i64 {
        let stream = client
            .query(&insert_sql, &[&format!("user_{}", i), &i])
            .await
            .expect("insert");
        let _: Vec<tiberius::Row> = stream.into_first_result().await.expect("insert results");
    }

    let base_select = format!(
        "SELECT {} FROM {} ORDER BY {}",
        dialect.quote("name"),
        dialect.quote(&table),
        dialect.quote("value"),
    );
    let page_sql = dialect.build_pagination(&base_select, 2, 2);
    let stream = client.simple_query(&page_sql).await.expect("page");
    let rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("page results");
    assert_eq!(rows.len(), 2);
    let n0: &str = rows[0].get::<&str, _>(0).expect("name 0");
    assert_eq!(n0, "user_2");

    drop_table_if_exists(&mut client, &table).await;
}
