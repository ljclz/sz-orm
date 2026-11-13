//! GBase 兼容方言集成测试
//!
//! GBaseDialect 通过 delegate_dialect_to! 委派 SqlServerDialect，
//! 生成的 SQL 与 SqlServerDialect 一致。用 SQL Server 验证兼容性。
//!
//! 运行方式：cargo test -p sz-orm-core --test integration_gbase -- --ignored --nocapture
//!
//! 凭据不入库：真实 DB 用例需通过环境变量 `SZ_ORM_GBASE_HOST/PORT/USER/PASSWORD/DATABASE`
//! 显式配置，未配置时用例自动跳过。

use std::sync::atomic::{AtomicU64, Ordering};
use sz_orm_core::dialect::{get_dialect, ColumnDef};
use sz_orm_core::DbType;
use tiberius::AuthMethod;
use tiberius::Client;
use tiberius::Config;
use tokio_util::compat::TokioAsyncWriteCompatExt;

// 凭据一律走环境变量（SZ_ORM_GBASE_*），源码中不落任何真实主机/账号/密码。
// 以下默认值均为占位符，仅用于编译通过；未配置环境变量时真实 DB 用例自动跳过。
const MSSQL_HOST_DEFAULT: &str = "gbase.invalid";
const MSSQL_PORT_DEFAULT: u16 = 1433;
const MSSQL_USER_DEFAULT: &str = "sa";
const MSSQL_PASSWORD_DEFAULT: &str = "<configure-via-env>";
const MSSQL_DATABASE_DEFAULT: &str = "test";

fn gbase_configured() -> bool {
    std::env::var("SZ_ORM_GBASE_HOST").is_ok()
}

fn mssql_host() -> String {
    std::env::var("SZ_ORM_GBASE_HOST").unwrap_or_else(|_| MSSQL_HOST_DEFAULT.to_string())
}

fn mssql_port() -> u16 {
    std::env::var("SZ_ORM_GBASE_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(MSSQL_PORT_DEFAULT)
}

fn mssql_user() -> String {
    std::env::var("SZ_ORM_GBASE_USER").unwrap_or_else(|_| MSSQL_USER_DEFAULT.to_string())
}

fn mssql_password() -> String {
    std::env::var("SZ_ORM_GBASE_PASSWORD").unwrap_or_else(|_| MSSQL_PASSWORD_DEFAULT.to_string())
}

fn mssql_database() -> String {
    std::env::var("SZ_ORM_GBASE_DATABASE").unwrap_or_else(|_| MSSQL_DATABASE_DEFAULT.to_string())
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
#[ignore = "需要 GBase/SQL Server 运行（设置 SZ_ORM_GBASE_* 环境变量，未配置自动跳过）"]
async fn test_gbase_crud_on_mssql() {
    if !gbase_configured() {
        eprintln!("跳过：GBase 云实例未配置（设置 SZ_ORM_GBASE_HOST 等启用）");
        return;
    }
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
#[ignore = "需要 GBase/SQL Server 运行（设置 SZ_ORM_GBASE_* 环境变量，未配置自动跳过）"]
async fn test_gbase_pagination_on_mssql() {
    if !gbase_configured() {
        eprintln!("跳过：GBase 云实例未配置（设置 SZ_ORM_GBASE_HOST 等启用）");
        return;
    }
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

// ============================================================================
// GBase 8s 方言矩阵（2026-11-14，GB1~GB3 × liaosnet/gbase8s V8.8 AEE 3.5.0 @19088）
// 与既有 TDS 用例不重复：本批实例为 Informix 协议社区镜像（无 TDS 网关、无 Rust
// 驱动——真库断言经容器内 dbaccess CLI，MZ 批 DB2 CLP 同型）。核心载荷是
// **GBaseDialect 委托 SqlServerDialect 的 T-SQL 形态在纯 8s 引擎上的行为边界**。
// ============================================================================

/// GB1：引擎身份——sysmaster 直连裸 SQL（无方言 quote）可执行
#[test]
#[ignore = "需要 GBase 8s 容器运行（docker exec gb8s）"]
fn matrix_gb1_engine_identity_bare_sql() {
    let out = run_gb8s_sql("select first 1 tabname from systables;", "sysmaster");
    assert!(
        out.contains("SYSTABLES") || out.contains("row(s) retrieved"),
        "8s 裸 SQL 可执行且引擎在位: {out}"
    );
}

/// GB2：方言 quote 形态行为级——SqlServerDialect 的 [bracket] 在纯 8s 被拒
/// （201 syntax error），双引号 delimited identifier 被接受——**引擎事实登记：
/// GBaseDialect→SqlServerDialect 委托仅在带 TDS 网关的企业实例上可执行，
/// 纯 Informix 协议社区版不可执行**（文档措辞登记项）
#[test]
#[ignore = "需要 GBase 8s 容器运行"]
fn matrix_gb2_quote_forms_on_pure_8s() {
    // 自建依赖表（红队 R-P2-2：不依赖手工预建——容器重建即自足）
    let _ = run_gb8s_sql("create table t_probe(id integer, nameId varchar(50));", "szorm_test");
    let _ = run_gb8s_sql("insert into t_probe values (1);", "szorm_test");
    // 方言形态：[id]（SqlServerDialect quote）→ 8s 拒绝
    let bracket = run_gb8s_sql("select [id] from t_probe;", "szorm_test");
    assert!(
        bracket.contains("201") || bracket.contains("syntax"),
        "[bracket] 在纯 8s 应被拒: {bracket}"
    );
    // 双引号 delimited identifier：8s 接受（对照证据）
    let quoted = run_gb8s_sql("select \"id\" from t_probe;", "szorm_test");
    assert!(
        quoted.contains("row(s) retrieved"),
        "双引号 delimited identifier 被接受: {quoted}"
    );
    let _ = run_gb8s_sql("drop table t_probe;", "szorm_test");
}

/// GB3：委托选择正确性边界——GBaseDialect 生成面（T-SQL 形态）与 8s Informix
/// 语法的差异清单（产物对比登记，非行为断言）
/// 红队 R-P1：CLI 断言臂依赖 wsl+docker 容器——与 GB1/GB2 同标 #[ignore]，
/// 防容器依赖泄漏进默认套件（Linux CI Command::new("wsl") 必 panic）
#[test]
#[ignore = "需要 GBase 8s 容器运行"]
fn matrix_gb3_dialect_shape_vs_8s_syntax() {
    let gbase = get_dialect(DbType::GBase).unwrap();
    // SqlServerDialect 形态（委托产物）——纯生成面断言（无容器依赖）
    assert_eq!(gbase.quote("t"), "[t]");
    assert!(gbase.build_pagination("SELECT 1", 1, 1).contains("OFFSET"));
    // 纯 8s（Informix 语法）对照：SKIP..FIRST 分页语法（SKIP 在前 FIRST 在后，
    let _ = run_gb8s_sql("create table t_probe(id integer, nameId varchar(50));", "szorm_test");
    let _ = run_gb8s_sql("insert into t_probe values (1);", "szorm_test");
    let out = run_gb8s_sql("select skip 0 first 1 id from t_probe;", "szorm_test");
    assert!(
        out.contains("row") || out.contains("retrieved"),
        "8s 原生 SKIP/FIRST 分页语法可执行: {out}"
    );
    let _ = run_gb8s_sql("drop table t_probe;", "szorm_test");
}

/// GBase 8s 容器内 dbaccess 执行 helper（MZ 批 DB2 CLP 同型）
///
/// sql 内不得含 "database x;" 前缀（库名经 dbaccess 参数传入，避免双重选择）；
/// sql 不得含单引号（su -c 单引号层）；双引号按四层嵌套手工构造（一次性成形，
/// 禁止二次 replace——GB2 初版教训）。
fn run_gb8s_sql(sql: &str, db: &str) -> String {
    let escaped = sql.replace('"', "\\\""); // Rust → 实参为 \" ，docker bash -c 解析后为 "
    let full = format!(
        "docker exec gb8s bash -c \"su - gbasedbt -c 'echo \\\"{}\\\" | dbaccess {}'\"",
        escaped, db
    );
    let script = std::process::Command::new("wsl")
        .args(["bash", "-c", &full])
        .output()
        .expect("docker exec gb8s");
    String::from_utf8_lossy(&script.stdout).to_string()
        + String::from_utf8_lossy(&script.stderr).trim()
}
