//! SQL Server 真实数据库集成测试
//!
//! 使用 `tiberius` crate 直接验证 sz-orm-core 的 SQL Server 方言、
//! SQL 转义、事务、分页、ALTER TABLE 等场景。
//!
//! 环境要求：
//! - SQL Server 2019+ 运行于 127.0.0.1:1433
//! - 测试数据库 `sz_orm_test`，用户 `sa`，密码 `SzOrmTest2026`
//! - 可通过环境变量 `SZ_ORM_MSSQL_*` 覆盖默认连接参数
//!
//! v2.0.0 任务 1.2：覆盖 8 类场景（5 个方言断言 + 8 个真实 DB）。

use std::sync::atomic::{AtomicU64, Ordering};
use sz_orm_core::dialect::{get_dialect, ColumnDef, TableChange};
use sz_orm_core::DbType;
use sz_orm_core::Value;
use tiberius::AuthMethod;
use tiberius::Client;
use tiberius::Config;
use tokio_util::compat::TokioAsyncWriteCompatExt;

const MSSQL_HOST_DEFAULT: &str = "127.0.0.1";
const MSSQL_PORT_DEFAULT: u16 = 1433;
const MSSQL_USER_DEFAULT: &str = "sa";
const MSSQL_PASSWORD_DEFAULT: &str = "SzOrmTest2026";
const MSSQL_DATABASE_DEFAULT: &str = "sz_orm_test";

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
        .expect("connect TCP failed - is SQL Server running on 127.0.0.1:1433?");
    let client = Client::connect(config, tcp.compat_write())
        .await
        .expect("tiberius connect failed");
    client
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
        "t_{}_{}_{}",
        prefix,
        pid % 1000,
        (nanos % 100000) as u64 * 1000 + counter
    )
}

fn create_test_columns() -> Vec<ColumnDef> {
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
            sql_type: "INT".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "data".to_string(),
            sql_type: "NVARCHAR(255)".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ]
}

async fn create_test_table(
    client: &mut Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    table: &str,
) {
    let dialect = get_dialect(DbType::SqlServer).expect("mssql dialect");
    let sql = dialect.build_create_table(table, &create_test_columns());
    let _ = client.simple_query(&sql).await.expect("create table");
}

async fn drop_table_if_exists(
    client: &mut Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    table: &str,
) {
    let dialect = get_dialect(DbType::SqlServer).expect("mssql dialect");
    let sql = dialect.build_drop_table(table, true);
    let _ = client.simple_query(&sql).await;
}

// ============================================================================
// 方言断言测试（不需要真实 DB）
// ============================================================================

#[test]
fn test_mssql_dialect_quote_and_escape() {
    let dialect = get_dialect(DbType::SqlServer).expect("mssql dialect");
    assert_eq!(dialect.quote("user"), "[user]");
    assert_eq!(dialect.quote("with]bracket"), "[with]]bracket]");
    assert_eq!(dialect.escape_string("it's"), "it''s");
    assert!(dialect.supports_returning());
}

#[test]
fn test_mssql_dialect_pagination_syntax() {
    let dialect = get_dialect(DbType::SqlServer).expect("mssql dialect");
    let sql = dialect.build_pagination("SELECT * FROM t", 3, 10);
    assert!(sql.contains("OFFSET 20 ROWS"), "sql = {sql}");
    assert!(sql.contains("FETCH NEXT 10 ROWS ONLY"), "sql = {sql}");
}

#[test]
fn test_mssql_dialect_type_mapping() {
    let dialect = get_dialect(DbType::SqlServer).expect("mssql dialect");
    assert_eq!(dialect.auto_increment_keyword(), "IDENTITY(1,1)");
    assert!(dialect.supports_if_exists());
    assert!(dialect.supports_if_not_exists());
    assert_eq!(dialect.json_type(), "NVARCHAR(MAX)");
}

#[test]
fn test_mssql_dialect_insert_or_ignore_fallback() {
    let dialect = get_dialect(DbType::SqlServer).expect("mssql dialect");
    let sql = dialect.build_insert_or_ignore_prefix("my_table");
    assert_eq!(
        sql, "INSERT INTO [my_table]",
        "SQL Server 不支持 INSERT OR IGNORE，回退为普通 INSERT INTO；\
         幂等插入需用 MERGE 或捕获重复键冲突"
    );
}

#[test]
fn test_mssql_value_string_escape() {
    let v_str = Value::String("O'Brien".to_string());
    let dialect = get_dialect(DbType::SqlServer).expect("mssql dialect");
    let escaped = dialect.escape_string(v_str.as_str().unwrap());
    assert_eq!(escaped, "O''Brien");
}

// ============================================================================
// 真实 DB 集成测试（需要 SQL Server 运行于 127.0.0.1:1433）
// ============================================================================

#[tokio::test]
#[ignore = "需要 SQL Server 运行于 127.0.0.1:1433（设置 SZ_ORM_MSSQL_* 环境变量覆盖）"]
async fn test_mssql_parameterized_insert_query() {
    let mut client = open_client().await;
    let table = unique_table("tparam");
    drop_table_if_exists(&mut client, &table).await;
    create_test_table(&mut client, &table).await;

    let dialect = get_dialect(DbType::SqlServer).unwrap();
    let insert_sql = format!(
        "INSERT INTO {} ({}, {}, {}) VALUES (@p1, @p2, @p3)",
        dialect.quote(&table),
        dialect.quote("name"),
        dialect.quote("value"),
        dialect.quote("data"),
    );
    for (name, value, data) in [
        ("alice", 100i32, "d1"),
        ("bob", 200i32, "d2"),
        ("carol", 300i32, "d3"),
    ] {
        let stream = client
            .query(&insert_sql, &[&name, &value, &data])
            .await
            .expect("insert");
        let _rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("insert results");
    }

    let select_sql = format!(
        "SELECT {}, {} FROM {} WHERE {} > @p1 ORDER BY {}",
        dialect.quote("name"),
        dialect.quote("value"),
        dialect.quote(&table),
        dialect.quote("value"),
        dialect.quote("value"),
    );
    let stream = client.query(&select_sql, &[&150i32]).await.expect("select");
    let rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("select results");
    assert_eq!(rows.len(), 2);
    let n0: &str = rows[0].get::<&str, _>(0).expect("name 0");
    let v0: i32 = rows[0].get::<i32, _>(1).expect("value 0");
    assert_eq!(n0, "bob");
    assert_eq!(v0, 200);
    let n1: &str = rows[1].get::<&str, _>(0).expect("name 1");
    assert_eq!(n1, "carol");

    drop_table_if_exists(&mut client, &table).await;
}

#[tokio::test]
#[ignore = "需要 SQL Server 运行于 127.0.0.1:1433（设置 SZ_ORM_MSSQL_* 环境变量覆盖）"]
async fn test_mssql_merge_into_ignore_semantics() {
    let mut client = open_client().await;
    let table = unique_table("tmerge");
    drop_table_if_exists(&mut client, &table).await;

    let dialect = get_dialect(DbType::SqlServer).unwrap();
    let create_sql = format!(
        "CREATE TABLE {} ({} INT PRIMARY KEY, {} NVARCHAR(100))",
        dialect.quote(&table),
        dialect.quote("id"),
        dialect.quote("name"),
    );
    let _ = client
        .simple_query(&create_sql)
        .await
        .expect("create table");

    let insert_sql = format!(
        "INSERT INTO {} ({}, {}) VALUES (@p1, @p2)",
        dialect.quote(&table),
        dialect.quote("id"),
        dialect.quote("name"),
    );
    let stream = client
        .query(&insert_sql, &[&1i32, &"original"])
        .await
        .expect("insert original");
    let _: Vec<tiberius::Row> = stream.into_first_result().await.expect("insert results");

    let merge_sql = format!(
        "MERGE INTO {} t USING (SELECT @p1 AS {}, @p2 AS {}) s \
         ON (t.{} = s.{}) \
         WHEN NOT MATCHED THEN INSERT ({}, {}) VALUES (s.{}, s.{})",
        dialect.quote(&table),
        dialect.quote("id"),
        dialect.quote("name"),
        dialect.quote("id"),
        dialect.quote("id"),
        dialect.quote("id"),
        dialect.quote("name"),
        dialect.quote("id"),
        dialect.quote("name"),
    );
    let stream = client
        .query(&merge_sql, &[&1i32, &"ignored"])
        .await
        .expect("merge existing");
    let _: Vec<tiberius::Row> = stream.into_first_result().await.expect("merge results");
    let stream = client
        .query(&merge_sql, &[&2i32, &"new"])
        .await
        .expect("merge new");
    let _: Vec<tiberius::Row> = stream.into_first_result().await.expect("merge results");

    let count_sql = format!("SELECT COUNT(*) FROM {}", dialect.quote(&table));
    let stream = client.simple_query(&count_sql).await.expect("count");
    let rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("count results");
    let count: i32 = rows[0].get::<i32, _>(0).expect("count value");
    assert_eq!(count, 2, "应有 2 行：original(id=1) + new(id=2)");

    let name_sql = format!(
        "SELECT {} FROM {} WHERE {} = @p1",
        dialect.quote("name"),
        dialect.quote(&table),
        dialect.quote("id"),
    );
    let stream = client
        .query(&name_sql, &[&1i32])
        .await
        .expect("select name 1");
    let rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("name results");
    let name1: &str = rows[0].get::<&str, _>(0).expect("name value");
    assert_eq!(name1, "original", "id=1 应保持 original，未被覆盖");

    drop_table_if_exists(&mut client, &table).await;
}

#[tokio::test]
#[ignore = "需要 SQL Server 运行于 127.0.0.1:1433（设置 SZ_ORM_MSSQL_* 环境变量覆盖）"]
async fn test_mssql_transaction_commit() {
    let mut client = open_client().await;
    let table = unique_table("tcommit");
    drop_table_if_exists(&mut client, &table).await;
    create_test_table(&mut client, &table).await;

    let dialect = get_dialect(DbType::SqlServer).unwrap();
    let insert_sql = format!(
        "INSERT INTO {} ({}, {}, {}) VALUES (@p1, @p2, @p3)",
        dialect.quote(&table),
        dialect.quote("name"),
        dialect.quote("value"),
        dialect.quote("data"),
    );
    for (name, value, data) in [("alice", 100i32, "d1"), ("bob", 200i32, "d2")] {
        let stream = client
            .query(&insert_sql, &[&name, &value, &data])
            .await
            .expect("insert");
        let _: Vec<tiberius::Row> = stream.into_first_result().await.expect("insert results");
    }
    client
        .simple_query("IF @@TRANCOUNT > 0 COMMIT")
        .await
        .expect("commit");

    let count_sql = format!("SELECT COUNT(*) FROM {}", dialect.quote(&table));
    let stream = client.simple_query(&count_sql).await.expect("count");
    let rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("count results");
    let count: i32 = rows[0].get::<i32, _>(0).expect("count");
    assert_eq!(count, 2, "commit 后应可见 2 行");

    drop_table_if_exists(&mut client, &table).await;
}

#[tokio::test]
#[ignore = "需要 SQL Server 运行于 127.0.0.1:1433（设置 SZ_ORM_MSSQL_* 环境变量覆盖）"]
async fn test_mssql_transaction_rollback() {
    let mut client = open_client().await;
    let table = unique_table("trollback");
    drop_table_if_exists(&mut client, &table).await;
    create_test_table(&mut client, &table).await;

    let dialect = get_dialect(DbType::SqlServer).unwrap();
    let insert_sql = format!(
        "INSERT INTO {} ({}, {}, {}) VALUES (@p1, @p2, @p3)",
        dialect.quote(&table),
        dialect.quote("name"),
        dialect.quote("value"),
        dialect.quote("data"),
    );
    let stream = client
        .query(&insert_sql, &[&"alice", &100i32, &"d1"])
        .await
        .expect("insert");
    let _: Vec<tiberius::Row> = stream.into_first_result().await.expect("insert results");
    client
        .simple_query("IF @@TRANCOUNT > 0 COMMIT")
        .await
        .expect("commit");

    let stream = client
        .query(&insert_sql, &[&"bob", &200i32, &"d2"])
        .await
        .expect("insert 2");
    let _: Vec<tiberius::Row> = stream.into_first_result().await.expect("insert results");
    client
        .simple_query("IF @@TRANCOUNT > 0 ROLLBACK")
        .await
        .expect("rollback");

    let count_sql = format!("SELECT COUNT(*) FROM {}", dialect.quote(&table));
    let stream = client.simple_query(&count_sql).await.expect("count");
    let rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("count results");
    let count: i32 = rows[0].get::<i32, _>(0).expect("count");
    assert_eq!(count, 1, "rollback 后应仍为 1 行");

    drop_table_if_exists(&mut client, &table).await;
}

#[tokio::test]
#[ignore = "需要 SQL Server 运行于 127.0.0.1:1433（设置 SZ_ORM_MSSQL_* 环境变量覆盖）"]
async fn test_mssql_pagination_executes() {
    let mut client = open_client().await;
    let table = unique_table("tpage");
    drop_table_if_exists(&mut client, &table).await;
    create_test_table(&mut client, &table).await;

    let dialect = get_dialect(DbType::SqlServer).unwrap();
    let insert_sql = format!(
        "INSERT INTO {} ({}, {}, {}) VALUES (@p1, @p2, @p3)",
        dialect.quote(&table),
        dialect.quote("name"),
        dialect.quote("value"),
        dialect.quote("data"),
    );
    for i in 0..25i32 {
        let stream = client
            .query(
                &insert_sql,
                &[&format!("user_{}", i), &i, &format!("d{}", i % 5)],
            )
            .await
            .expect("insert");
        let _: Vec<tiberius::Row> = stream.into_first_result().await.expect("insert results");
    }

    let base_select = format!(
        "SELECT {}, {} FROM {} ORDER BY {}",
        dialect.quote("name"),
        dialect.quote("value"),
        dialect.quote(&table),
        dialect.quote("value"),
    );
    let page3_sql = dialect.build_pagination(&base_select, 3, 10);
    let stream = client.simple_query(&page3_sql).await.expect("page 3");
    let rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("page 3 results");
    assert_eq!(rows.len(), 5, "25 条分页 10/页，第 3 页应 5 条");
    let n0: &str = rows[0].get::<&str, _>(0).expect("name 0");
    let v0: i32 = rows[0].get::<i32, _>(1).expect("value 0");
    assert_eq!(n0, "user_20");
    assert_eq!(v0, 20);

    let page1_sql = dialect.build_pagination(&base_select, 1, 10);
    let stream = client.simple_query(&page1_sql).await.expect("page 1");
    let rows1: Vec<tiberius::Row> = stream.into_first_result().await.expect("page 1 results");
    assert_eq!(rows1.len(), 10);

    drop_table_if_exists(&mut client, &table).await;
}

#[tokio::test]
#[ignore = "需要 SQL Server 运行于 127.0.0.1:1433（设置 SZ_ORM_MSSQL_* 环境变量覆盖）"]
async fn test_mssql_alter_table_executes() {
    let mut client = open_client().await;
    let table = unique_table("talter");
    drop_table_if_exists(&mut client, &table).await;
    create_test_table(&mut client, &table).await;

    let dialect = get_dialect(DbType::SqlServer).unwrap();
    let insert_sql = format!(
        "INSERT INTO {} ({}, {}, {}) VALUES (@p1, @p2, @p3)",
        dialect.quote(&table),
        dialect.quote("name"),
        dialect.quote("value"),
        dialect.quote("data"),
    );
    let stream = client
        .query(&insert_sql, &[&"alice", &100i32, &"d1"])
        .await
        .expect("insert");
    let _: Vec<tiberius::Row> = stream.into_first_result().await.expect("insert results");

    let add_col = TableChange::AddColumn(ColumnDef {
        name: "email".to_string(),
        sql_type: "NVARCHAR(255)".to_string(),
        nullable: true,
        default: None,
        auto_increment: false,
        primary_key: false,
    });
    let alter_sql = dialect.build_alter_table(&table, &[add_col]);
    let _ = client
        .simple_query(&alter_sql)
        .await
        .expect("alter add column");

    let update_sql = format!(
        "UPDATE {} SET {} = @p1 WHERE {} = @p2",
        dialect.quote(&table),
        dialect.quote("email"),
        dialect.quote("name"),
    );
    let stream = client
        .query(&update_sql, &[&"alice@example.com", &"alice"])
        .await
        .expect("update email");
    let _: Vec<tiberius::Row> = stream.into_first_result().await.expect("update results");

    let select_sql = format!(
        "SELECT {} FROM {} WHERE {} = @p1",
        dialect.quote("email"),
        dialect.quote(&table),
        dialect.quote("name"),
    );
    let stream = client
        .query(&select_sql, &[&"alice"])
        .await
        .expect("select email");
    let rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("select results");
    let email: &str = rows[0].get::<&str, _>(0).expect("email value");
    assert_eq!(email, "alice@example.com");

    let drop_col = TableChange::DropColumn("email".to_string());
    let drop_sql = dialect.build_alter_table(&table, &[drop_col]);
    let _ = client
        .simple_query(&drop_sql)
        .await
        .expect("alter drop column");

    drop_table_if_exists(&mut client, &table).await;
}

#[tokio::test]
#[ignore = "需要 SQL Server 运行于 127.0.0.1:1433（设置 SZ_ORM_MSSQL_* 环境变量覆盖）"]
async fn test_mssql_escape_executes() {
    let mut client = open_client().await;
    let table = unique_table("tescape");
    drop_table_if_exists(&mut client, &table).await;
    create_test_table(&mut client, &table).await;

    let dialect = get_dialect(DbType::SqlServer).unwrap();
    let raw = "O'Brien";
    let escaped = dialect.escape_string(raw);
    assert_eq!(escaped, "O''Brien");

    let insert_sql = format!(
        "INSERT INTO {} ({}, {}, {}) VALUES (@p1, @p2, @p3)",
        dialect.quote(&table),
        dialect.quote("name"),
        dialect.quote("value"),
        dialect.quote("data"),
    );
    let stream = client
        .query(&insert_sql, &[&raw, &1i32, &"single quote"])
        .await
        .expect("insert with quote");
    let _: Vec<tiberius::Row> = stream.into_first_result().await.expect("insert results");

    let select_sql = format!(
        "SELECT {} FROM {} WHERE {} = @p1",
        dialect.quote("name"),
        dialect.quote(&table),
        dialect.quote("value"),
    );
    let stream = client
        .query(&select_sql, &[&1i32])
        .await
        .expect("select name");
    let rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("select results");
    let name: &str = rows[0].get::<&str, _>(0).expect("name value");
    assert_eq!(name, "O'Brien", "参数化绑定应原样存取，无需手动转义");

    let count_sql = format!(
        "SELECT COUNT(*) FROM {} WHERE {} = '{}'",
        dialect.quote(&table),
        dialect.quote("name"),
        escaped,
    );
    let stream = client.simple_query(&count_sql).await.expect("count");
    let rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("count results");
    let count: i32 = rows[0].get::<i32, _>(0).expect("count value");
    assert_eq!(count, 1, "escape_string 生成的字面量应可正确查询");

    drop_table_if_exists(&mut client, &table).await;
}

#[tokio::test]
#[ignore = "需要 SQL Server 运行于 127.0.0.1:1433（设置 SZ_ORM_MSSQL_* 环境变量覆盖）"]
async fn test_mssql_drop_table_executes() {
    let mut client = open_client().await;
    let table = unique_table("tdrop");
    drop_table_if_exists(&mut client, &table).await;
    create_test_table(&mut client, &table).await;

    let dialect = get_dialect(DbType::SqlServer).unwrap();
    let insert_sql = format!(
        "INSERT INTO {} ({}, {}, {}) VALUES (@p1, @p2, @p3)",
        dialect.quote(&table),
        dialect.quote("name"),
        dialect.quote("value"),
        dialect.quote("data"),
    );
    let stream = client
        .query(&insert_sql, &[&"alice", &1i32, &"d1"])
        .await
        .expect("insert");
    let _: Vec<tiberius::Row> = stream.into_first_result().await.expect("insert results");

    let count_sql = format!("SELECT COUNT(*) FROM {}", dialect.quote(&table));
    let stream = client.simple_query(&count_sql).await.expect("count");
    let rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("count results");
    let count: i32 = rows[0].get::<i32, _>(0).expect("count");
    assert_eq!(count, 1);

    drop_table_if_exists(&mut client, &table).await;

    let check_sql = "SELECT COUNT(*) FROM sys.tables WHERE name = @p1";
    let stream = client
        .query(check_sql, &[&table])
        .await
        .expect("check exist");
    let rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("check results");
    let remain: i32 = rows[0].get::<i32, _>(0).expect("remain");
    assert_eq!(remain, 0, "DROP TABLE 后表应不存在");

    let drop_again_sql = dialect.build_drop_table(&table, true);
    let result = client.simple_query(&drop_again_sql).await;
    assert!(result.is_ok(), "DROP TABLE IF EXISTS 对不存在的表应不报错");
}
