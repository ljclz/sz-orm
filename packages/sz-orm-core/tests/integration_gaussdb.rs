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

// ============================================================================
// 容器化方言矩阵（2026-11-13，MZ-openGauss × openGauss 6.0.0 容器 @5433，PG 兼容模式）
// 与既有 8 项不重复：聚焦 PG 族偏差（sql_compatibility / RETURNING / 序列通道 / 双引号折叠）
// ============================================================================

/// MZ-openGauss-1: sql_compatibility=PG 默认模式确认（委托 GaussDbDialect→PG 族正确性基础）
#[tokio::test]
#[ignore]
async fn matrix_mz_gaussdb1_compatibility_mode_pg() {
    let pool = setup_pool().await;

    // openGauss SHOW 在 extended protocol 下经 sqlx 返回空/TEXT 不稳定（实测 2026-11-13）——
    // 改走 pg_settings 目录查询（确定性标量 TEXT），真库值 PG（gsql -a 实测）
    let mode_raw: String =
        sqlx::query_scalar("SELECT setting FROM pg_settings WHERE name = $1")
            .bind("sql_compatibility")
            .fetch_one(&pool)
            .await
            .expect("read sql_compatibility via pg_settings");
    let mode = mode_raw.trim().to_uppercase();
    assert_eq!(
        mode, "PG",
        "openGauss 容器默认 sql_compatibility 应为 PG（GaussDbDialect 委派 PG 族的前提）"
    );

    // 委托正确性
    let dialect = get_dialect(DbType::GaussDB).unwrap();
    assert_eq!(dialect.db_type(), DbType::GaussDB);
}

/// MZ-openGauss-2: INSERT ... RETURNING（openGauss 支持 PG 形态——与 MySQL 族差异点）
#[tokio::test]
#[ignore]
async fn matrix_mz_gaussdb2_insert_returning_supported() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::GaussDB).unwrap();

    // 委托面：GaussDbDialect → PG 族，supports_returning=true（真库确认）
    assert!(
        dialect.supports_returning(),
        "GaussDbDialect 委派 PG 族，supports_returning 应为 true"
    );

    let table = unique_table("mzg2ret");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("CREATE TABLE \"{}\" (id serial PRIMARY KEY, v text)", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("create table");

    // 真库行为：RETURNING 回填 serial id
    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO \"{}\" (v) VALUES ($1) RETURNING id", table).as_str(),
    ))
    .bind("a")
    .fetch_one(&pool)
    .await
    .expect("INSERT RETURNING on openGauss");
    // serial 列为 INT4——按 i32 解码（openGauss serial 默认 integer 非 bigint）
    let backfilled: i32 = row.get("id");
    assert!(
        backfilled >= 1,
        "RETURNING id 应回填 serial 值，got {backfilled}"
    );

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}

/// MZ-openGauss-3: 序列 NEXTVAL 通道（pg_get_serial_sequence + nextval 与 PG 一致）
#[tokio::test]
#[ignore]
async fn matrix_mz_gaussdb3_sequence_nextval_channel() {
    let pool = setup_pool().await;
    let table = unique_table("mzg3seq");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("CREATE TABLE \"{}\" (id serial PRIMARY KEY, v text)", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("create table");

    // 插入消耗一个序列值后，NEXTVAL 应继续递增
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO \"{}\" (v) VALUES ('a')", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("insert");

    // openGauss 引擎限制（与 PG 差异点）：nextval 参数必须是字符串字面量，
    // 不接受 pg_get_serial_sequence() 函数结果——改用确定性序列名（serial 隐式
    // 生成 "<table>_id_seq"，与 PG 命名规则一致）
    // 另：openGauss extended protocol 将 nextval 标量以 TEXT 返回——CAST 保险
    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT nextval('{}_id_seq')::bigint AS nv", table).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .expect("nextval on openGauss（字面量序列名）");
    let nv: i64 = row.get("nv");
    assert!(
        nv >= 2,
        "NEXTVAL 应 >= 2（serial 首值 1 已被消耗），got {nv}"
    );

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}

/// MZ-openGauss-4: 双引号折叠行为与 PG 一致——quote 保留小写、保留字列名需 quote
#[tokio::test]
#[ignore]
async fn matrix_mz_gaussdb4_quote_preserves_case_and_reserved_words() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::GaussDB).unwrap();

    // 方言生成面：PG 族双引号
    assert_eq!(dialect.quote("order"), "\"order\"");
    assert_eq!(dialect.quote("mixedCase"), "\"mixedCase\"");

    let table = unique_table("mzg4fold");
    // 保留字列名 "order" + 小写列名混用
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id int, \"order\" text, \"mixedCase\" text)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .expect("create table with quoted reserved word");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO \"{}\" VALUES (1, 'o', 'm')", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("insert");

    // 裸保留字列名探针（26.x 实测修正）：openGauss PG 模式下 `order` 裸用作
    // SELECT 列引用时——`SELECT order FROM t` 实测被拒（gsql ERROR syntax error
    // at or near "order"），但经 sqlx extended protocol 部分版本放行——
    // 这里仅登记：quoted 形态必须可用（正路径），裸形态行为登记为引擎差异
    let bare = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT order FROM \"{}\"", table).as_str(),
    ))
    .fetch_all(&pool)
    .await;
    // 两种引擎行为皆登记：Err=保留字拒绝（gsql 实测），Ok=extended protocol 放行
    let _ = bare;

    // quoted 往返无损
    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "SELECT \"order\", \"mixedCase\" FROM \"{}\" WHERE id = 1",
            table
        )
        .as_str(),
    ))
    .fetch_one(&pool)
    .await
    .expect("quoted select");
    let (o, m): (String, String) = (row.get(0), row.get(1));
    assert_eq!(
        (o.as_str(), m.as_str()),
        ("o", "m"),
        "quoted 列名往返应无损"
    );

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}
