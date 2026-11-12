//! SQL Server 真实数据库集成测试
//!
//! 使用 `tiberius` crate 直接验证 sz-orm-core 的 SQL Server 方言、
//! SQL 转义、事务、分页、ALTER TABLE 等场景。
//!
//! 环境要求：
//! - SQL Server 2019+ 运行于 127.0.0.1:1433
//! - 测试数据库 `sz_orm_test`，用户 `sa`，密码经 `SZ_ORM_MSSQL_PASSWORD` 环境变量注入
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
// 凭据不入库（2026-10-28 P0-3）：默认空串，运行时必须设置对应 SZ_ORM_*_PASSWORD 环境变量
const MSSQL_PASSWORD_DEFAULT: &str = "";
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
    let pw = std::env::var("SZ_ORM_MSSQL_PASSWORD")
        .unwrap_or_else(|_| MSSQL_PASSWORD_DEFAULT.to_string());
    assert!(
        !pw.is_empty(),
        "缺少 SZ_ORM_MSSQL_PASSWORD 环境变量（本机数据库测试密码，见 AGENTS.md「本机数据库」小节；2026-10-28 P0-3 起凭据不入库）"
    );
    pw
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

    // SQL Server 要求 MERGE 语句必须以分号结尾（SQL Server 语法强制要求）
    let merge_sql = format!(
        "MERGE INTO {} t USING (SELECT @p1 AS {}, @p2 AS {}) s \
         ON (t.{} = s.{}) \
         WHEN NOT MATCHED THEN INSERT ({}, {}) VALUES (s.{}, s.{});",
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
    // 自动提交模式下 @@TRANCOUNT 为 0，无需 COMMIT（alice 已自动提交）

    // 显式开启事务，插入 bob 后 ROLLBACK，验证 bob 被回滚
    client
        .simple_query("BEGIN TRANSACTION")
        .await
        .expect("begin transaction");
    let stream = client
        .query(&insert_sql, &[&"bob", &200i32, &"d2"])
        .await
        .expect("insert 2");
    let _: Vec<tiberius::Row> = stream.into_first_result().await.expect("insert results");
    client
        .simple_query("ROLLBACK TRANSACTION")
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

// ============================================================================
// 容器化方言矩阵（2026-11-13，MZ-MSSQL × MSSQL 2022 容器 @1433）
// 与既有 13 项不重复：聚焦 sz-orm 方言生成面 × 真库行为契约
// ============================================================================

/// MZ-MSSQL-1: OUTPUT INSERTED 主键回填通道（sz-orm supports_returning()=true 的语义基础）
#[tokio::test]
#[ignore = "需要 SQL Server 运行于 127.0.0.1:1433"]
async fn matrix_mz_mssql1_output_inserted_id_backfill() {
    let mut client = open_client().await;
    let table = unique_table("mzm1");
    drop_table_if_exists(&mut client, &table).await;
    create_test_table(&mut client, &table).await;

    let dialect = get_dialect(DbType::SqlServer).unwrap();
    // sz-orm 方言生成面：supports_returning() = true（OUTPUT 等价通道）
    assert!(
        dialect.supports_returning(),
        "SqlServerDialect.supports_returning 必须为 true（OUTPUT INSERTED 通道）"
    );

    // 真库行为：INSERT ... OUTPUT INSERTED.[id] 返回自增主键
    let insert_sql = format!(
        "INSERT INTO {} ([name], [value]) OUTPUT INSERTED.[id] VALUES (@p1, @p2)",
        dialect.quote(&table)
    );
    let stream = client
        .query(&insert_sql, &[&"mz_a", &1i32])
        .await
        .expect("insert with OUTPUT");
    let rows: Vec<tiberius::Row> = stream
        .into_first_result()
        .await
        .expect("OUTPUT INSERTED results");
    assert_eq!(rows.len(), 1, "OUTPUT INSERTED 应返回 1 行");
    let backfilled: i64 = rows[0].get::<i64, _>(0).expect("id");
    assert!(
        backfilled >= 1,
        "OUTPUT INSERTED.id 应回填自增值，got {backfilled}"
    );

    // 行为断言：回填值与表内实际行一致
    let verify_sql = format!(
        "SELECT [name] FROM {} WHERE [id] = @p1",
        dialect.quote(&table)
    );
    let stream = client
        .query(&verify_sql, &[&backfilled])
        .await
        .expect("verify");
    let rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("verify results");
    let name: &str = rows[0].get::<&str, _>(0).expect("name");
    assert_eq!(name, "mz_a", "回填 id 对应行应存在");

    drop_table_if_exists(&mut client, &table).await;
}

/// MZ-MSSQL-2: [方括号] quoting × 真库（保留字列名 + 含 ] 标识符转义）
#[tokio::test]
#[ignore = "需要 SQL Server 运行于 127.0.0.1:1433"]
async fn matrix_mz_mssql2_bracket_quoting_reserved_words() {
    let mut client = open_client().await;
    let dialect = get_dialect(DbType::SqlServer).unwrap();
    // 方言生成面断言
    assert_eq!(dialect.quote("order"), "[order]");
    assert_eq!(dialect.quote("we]ird"), "[we]]ird]");

    // 真库行为：保留字列名 [order] 与含 ] 列名建表 + CRUD 往返
    let table = unique_table("mzm2");
    drop_table_if_exists(&mut client, &table).await;
    let create_sql = format!(
        "CREATE TABLE {} ([id] INT IDENTITY(1,1) PRIMARY KEY, {} NVARCHAR(50) NOT NULL, {} NVARCHAR(50) NULL)",
        dialect.quote(&table),
        dialect.quote("order"),
        dialect.quote("we]ird")
    );
    client
        .simple_query(&create_sql)
        .await
        .expect("create table with reserved-word column");

    let insert_sql = format!(
        "INSERT INTO {} ({}, {}) VALUES (@p1, @p2)",
        dialect.quote(&table),
        dialect.quote("order"),
        dialect.quote("we]ird")
    );
    client
        .query(&insert_sql, &[&"r1", &"w1"])
        .await
        .expect("insert reserved-word cols")
        .into_first_result()
        .await
        .expect("insert results");

    let select_sql = format!(
        "SELECT {}, {} FROM {} WHERE {} = @p1",
        dialect.quote("order"),
        dialect.quote("we]ird"),
        dialect.quote(&table),
        dialect.quote("order")
    );
    let stream = client
        .query(&select_sql, &[&"r1"])
        .await
        .expect("select reserved-word cols");
    let rows: Vec<tiberius::Row> = stream.into_first_result().await.expect("select results");
    assert_eq!(rows.len(), 1);
    let order_val: &str = rows[0].get::<&str, _>(0).expect("order");
    let weird_val: &str = rows[0].get::<&str, _>(1).expect("weird");
    assert_eq!(
        (order_val, weird_val),
        ("r1", "w1"),
        "保留字/含]列名往返应无损"
    );

    drop_table_if_exists(&mut client, &table).await;
}

/// MZ-MSSQL-3: TOP n vs OFFSET..FETCH 分页（无 ORDER BY 的 OFFSET 必须被真库拒绝——契约登记）
#[tokio::test]
#[ignore = "需要 SQL Server 运行于 127.0.0.1:1433"]
async fn matrix_mz_mssql3_pagination_offset_fetch_orderby_contract() {
    let mut client = open_client().await;
    let dialect = get_dialect(DbType::SqlServer).unwrap();

    let table = unique_table("mzm3");
    drop_table_if_exists(&mut client, &table).await;
    create_test_table(&mut client, &table).await;
    for i in 0..5i32 {
        let insert_sql = format!(
            "INSERT INTO {} ([name], [value]) VALUES (@p1, @p2)",
            dialect.quote(&table)
        );
        client
            .query(&insert_sql, &[&"n", &i])
            .await
            .expect("seed")
            .into_first_result()
            .await
            .expect("seed results");
    }

    // TOP n 真库可用
    let top_sql = format!("SELECT TOP 2 [id] FROM {}", dialect.quote(&table));
    let rows: Vec<tiberius::Row> = client
        .simple_query(&top_sql)
        .await
        .expect("TOP n")
        .into_first_result()
        .await
        .expect("TOP results");
    assert_eq!(rows.len(), 2, "TOP 2 应返回 2 行");

    // sz-orm build_pagination 生成 OFFSET..FETCH：真库要求前置 ORDER BY
    let paginated =
        dialect.build_pagination(&format!("SELECT [id] FROM {}", dialect.quote(&table)), 2, 2);
    assert!(paginated.contains("OFFSET 2 ROWS"), "sql = {paginated}");

    // 契约探针 A：无 ORDER BY 的 OFFSET..FETCH 被真库拒绝（Msg 102）
    let no_order = format!("SELECT [id] FROM {} OFFSET 0 ROWS", dialect.quote(&table));
    let err = match client.simple_query(&no_order).await {
        Err(e) => e,
        Ok(_) => panic!("无 ORDER BY 的 OFFSET 必须报错（MSSQL 契约）"),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("102") || msg.to_lowercase().contains("incorrect syntax"),
        "期望 Msg 102 Incorrect syntax，got: {msg}"
    );

    // 契约探针 B：前置 ORDER BY 后 sz-orm 生成面可用
    let with_order = dialect.build_pagination(
        &format!("SELECT [id] FROM {} ORDER BY [id]", dialect.quote(&table)),
        2,
        2,
    );
    let rows: Vec<tiberius::Row> = client
        .simple_query(&with_order)
        .await
        .expect("OFFSET..FETCH with ORDER BY")
        .into_first_result()
        .await
        .expect("pagination results");
    assert_eq!(rows.len(), 2, "第 2 页（page=2, limit=2）应返回 2 行");
    let first: i64 = rows[0].get::<i64, _>(0).expect("id");
    assert!(
        first >= 3,
        "第 2 页首行 id 应 >= 3（1..5 正序），got {first}"
    );

    drop_table_if_exists(&mut client, &table).await;
}

/// MZ-MSSQL-4: GETDATE() / NVARCHAR(MAX) / JSON_VALUE 类型面 × 真库
#[tokio::test]
#[ignore = "需要 SQL Server 运行于 127.0.0.1:1433"]
async fn matrix_mz_mssql4_getdate_nvarchar_json_value() {
    let mut client = open_client().await;
    let dialect = get_dialect(DbType::SqlServer).unwrap();

    // json_type 声明 NVARCHAR(MAX)——真库可用该类型建列
    assert_eq!(dialect.json_type(), "NVARCHAR(MAX)");
    let table = unique_table("mzm4");
    drop_table_if_exists(&mut client, &table).await;
    let create_sql = format!(
        "CREATE TABLE {} ([id] INT IDENTITY(1,1) PRIMARY KEY, [payload] {} NOT NULL, [created_at] DATETIME NOT NULL DEFAULT GETDATE())",
        dialect.quote(&table),
        dialect.json_type()
    );
    client
        .simple_query(&create_sql)
        .await
        .expect("create table with NVARCHAR(MAX) + GETDATE default");

    // JSON_VALUE 提取（方言生成面 × 真库）
    let extract_expr = dialect.json_extract("payload", "user.name");
    assert!(
        extract_expr.starts_with("JSON_VALUE(payload"),
        "expr = {extract_expr}"
    );
    let insert_sql = format!(
        "INSERT INTO {} ([payload]) VALUES (@p1)",
        dialect.quote(&table)
    );
    client
        .query(&insert_sql, &[&r#"{"user":{"name":"mz"}}"#])
        .await
        .expect("insert json")
        .into_first_result()
        .await
        .expect("insert results");

    let select_sql = format!(
        "SELECT {}, [created_at] FROM {} WHERE [id] = SCOPE_IDENTITY() + 0 OR [id] = (SELECT MAX([id]) FROM {})",
        extract_expr,
        dialect.quote(&table),
        dialect.quote(&table)
    );
    let rows: Vec<tiberius::Row> = client
        .simple_query(&select_sql)
        .await
        .expect("select json_value")
        .into_first_result()
        .await
        .expect("select results");
    assert_eq!(rows.len(), 1);
    let name: &str = rows[0].get::<&str, _>(0).expect("extracted name");
    assert_eq!(name, "mz", "JSON_VALUE 提取应无损");
    // GETDATE() 默认值列存在且非 NULL：用 NVARCHAR 转换探针避免跨驱动类型耦合
    let dt_sql = format!(
        "SELECT CONVERT(NVARCHAR(23), [created_at], 121) FROM {} WHERE [id] = (SELECT MAX([id]) FROM {})",
        dialect.quote(&table),
        dialect.quote(&table)
    );
    let rows: Vec<tiberius::Row> = client
        .simple_query(&dt_sql)
        .await
        .expect("select created_at")
        .into_first_result()
        .await
        .expect("created_at results");
    let dt: &str = rows[0].get::<&str, _>(0).expect("created_at string");
    assert!(
        dt.len() >= 19 && dt.as_bytes()[4] == b'-',
        "GETDATE() 默认值应填充合法 DATETIME 字符串，got {dt}"
    );

    drop_table_if_exists(&mut client, &table).await;
}
