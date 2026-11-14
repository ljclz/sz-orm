//! Kingbase 兼容方言集成测试
//!
//! KingbaseDialect 通过 delegate_dialect_to! 委派 PostgreSqlDialect，
//! 生成的 SQL 与 PostgreSqlDialect 一致。用 PostgreSQL 18 验证兼容性。
//!
//! 运行方式：cargo test -p sz-orm-core --test integration_kingbase -- --ignored --nocapture

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
fn test_kingbase_dialect_quote() {
    let kingbase = get_dialect(DbType::Kingbase).unwrap();
    let pg = get_dialect(DbType::PostgreSQL).unwrap();
    assert_eq!(kingbase.quote("user"), pg.quote("user"));
    assert_eq!(kingbase.quote("user"), "\"user\"");
}

#[test]
fn test_kingbase_dialect_escape() {
    let kingbase = get_dialect(DbType::Kingbase).unwrap();
    let pg = get_dialect(DbType::PostgreSQL).unwrap();
    assert_eq!(kingbase.escape_string("it's"), pg.escape_string("it's"));
}

#[test]
fn test_kingbase_dialect_pagination() {
    let kingbase = get_dialect(DbType::Kingbase).unwrap();
    let pg = get_dialect(DbType::PostgreSQL).unwrap();
    assert_eq!(
        kingbase.build_pagination("SELECT 1", 10, 20),
        pg.build_pagination("SELECT 1", 10, 20)
    );
}

#[test]
fn test_kingbase_dialect_db_type() {
    let dialect = get_dialect(DbType::Kingbase).unwrap();
    assert_eq!(dialect.db_type(), DbType::Kingbase);
}

#[test]
fn test_kingbase_dialect_create_table() {
    let kingbase = get_dialect(DbType::Kingbase).unwrap();
    let pg = get_dialect(DbType::PostgreSQL).unwrap();
    let sql_kingbase = kingbase.build_create_table("test_table", &test_columns());
    let sql_pg = pg.build_create_table("test_table", &test_columns());
    assert_eq!(sql_kingbase, sql_pg);
}

// ==================== 真实 DB 集成测试 ====================

#[tokio::test]
#[ignore]
async fn test_kingbase_crud_on_pg() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::Kingbase).unwrap();
    let table = unique_table("kingbase");

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
        format!(
            "UPDATE \"{}\" SET \"value\" = $1 WHERE \"name\" = $2",
            table
        )
        .as_str(),
    ))
    .bind(200i64)
    .bind("alice")
    .execute(&pool)
    .await
    .expect("update");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DELETE FROM \"{}\" WHERE \"name\" = $1", table).as_str(),
    ))
    .bind("alice")
    .execute(&pool)
    .await
    .expect("delete");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}

#[tokio::test]
#[ignore]
async fn test_kingbase_pagination_on_pg() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::Kingbase).unwrap();
    let table = unique_table("kingbase_pg");

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
async fn test_kingbase_sql_injection_protection() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::Kingbase).unwrap();
    let table = unique_table("kingbase_inj");

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
// Kingbase 方言矩阵（2026-11-14，KB1~KB4 × 社区镜像 KingbaseES，本批载荷容器 kb4@15435
// ——15434 为 license 已过期的 kb2 容器；经 SZ_ORM_KB_URL 注入连接，勿依赖固定端口）
// 与既有 8 项不重复：KB2/KB3 覆盖 OC 批 delegate 宏转发修复（dialect.rs:2147-2172）
// 在 Kingbase 通道的行为——既有 8 项用 PostgreSQL 18 直测，未覆盖委托方言链。
// 环境变量：SZ_ORM_KB_URL（独立变量，避免与宿主 PG 18 的 SZ_ORM_PG_URL 混淆）
// ============================================================================

use sz_orm_core::dialect::ForeignKeyDef;

/// Kingbase 专用 URL（SZ_ORM_KB_URL；未配置自动跳过）
async fn try_kb_pool() -> Option<PgPool> {
    let url = std::env::var("SZ_ORM_KB_URL").unwrap_or_default();
    if url.is_empty() {
        return None;
    }
    let res = PgPoolOptions::new()
        .max_connections(3)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&url)
        .await;
    res.ok()
}

/// KB1：引擎身份——version() 含 Kingbase + 方言委托链身份（quote 双引号）
#[tokio::test]
#[ignore = "需要 KingbaseES 运行（SZ_ORM_KB_URL 注入）"]
async fn matrix_kb1_engine_identity() {
    let Some(pool) = try_kb_pool().await else {
        eprintln!("skip: SZ_ORM_KB_URL 未配置或不可达");
        return;
    };
    let ver: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(&pool)
        .await
        .expect("version()");
    assert!(ver.contains("Kingbase"), "真库应为 KingbaseES: {ver}");
    let d = get_dialect(DbType::Kingbase).unwrap();
    assert_eq!(d.db_type(), DbType::Kingbase);
    // 委托链：KingbaseDialect→PostgreSqlDialect（quote 双引号）
    assert_eq!(d.quote("user"), "\"user\"");
}

/// KB2：upsert 委托链（OC 批修复的 KB 通道）——ON CONFLICT..EXCLUDED 行为级
#[tokio::test]
#[ignore = "需要 KingbaseES 运行"]
async fn matrix_kb2_delegate_upsert_forwarded() {
    let Some(pool) = try_kb_pool().await else {
        eprintln!("skip: 不可达");
        return;
    };
    let d = get_dialect(DbType::Kingbase).unwrap();
    let table = unique_table("kb2");
    let clause = d
        .build_upsert_on_conflict(&["id"], &["v"], &["id".to_string(), "v".to_string()])
        .expect("委托修复后 Kingbase 支持 upsert 渲染");
    assert!(
        clause.contains("ON CONFLICT") && clause.contains("EXCLUDED"),
        "PG 形态渲染: {clause}"
    );
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE IF EXISTS \"{table}\"").as_str(),
    ))
    .execute(&pool)
    .await
    .ok();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("CREATE TABLE \"{table}\" (id INT PRIMARY KEY, v INT)").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("create");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO \"{table}\" (id, v) VALUES (1, 10) {clause}").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("upsert insert");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO \"{table}\" (id, v) VALUES (1, 99) {clause}").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("upsert update");
    let v: i32 = sqlx::query_scalar(sqlx::AssertSqlSafe(
        format!("SELECT v FROM \"{table}\" WHERE id = 1").as_str(),
    ))
    .fetch_one(&pool)
    .await
    .expect("read back");
    assert_eq!(v, 99, "EXCLUDED 冲突更新生效");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{table}\"").as_str(),
    ))
    .execute(&pool)
    .await
    .ok();
}

/// KB3：FK 动作委托链（OC 批修复的 KB 通道）——ON DELETE CASCADE 行为级
#[tokio::test]
#[ignore = "需要 KingbaseES 运行"]
async fn matrix_kb3_delegate_fk_forwarded() {
    let Some(pool) = try_kb_pool().await else {
        eprintln!("skip: 不可达");
        return;
    };
    let d = get_dialect(DbType::Kingbase).unwrap();
    let parent = unique_table("kb3_p");
    let child = unique_table("kb3_c");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE IF EXISTS \"{child}\"").as_str(),
    ))
    .execute(&pool)
    .await
    .ok();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE IF EXISTS \"{parent}\"").as_str(),
    ))
    .execute(&pool)
    .await
    .ok();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("CREATE TABLE \"{parent}\" (id INT PRIMARY KEY)").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("parent");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("CREATE TABLE \"{child}\" (id INT PRIMARY KEY, pid INT)").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("child");
    let fk = ForeignKeyDef {
        columns: vec!["pid".to_string()],
        reference_table: parent.clone(),
        reference_columns: vec!["id".to_string()],
        on_delete: Some("CASCADE".to_string()),
        on_update: None,
    };
    let ddl = d.build_add_foreign_key(&child, &fk).expect("FK DDL");
    assert!(
        ddl.contains("\"pid\"") && ddl.contains("CASCADE"),
        "形态: {ddl}"
    );
    sqlx::query(sqlx::AssertSqlSafe(ddl.as_str()))
        .execute(&pool)
        .await
        .expect("ADD CONSTRAINT FK 接受");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO \"{parent}\" (id) VALUES (1)").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("parent row");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO \"{child}\" (id, pid) VALUES (10, 1)").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("child row");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DELETE FROM \"{parent}\" WHERE id = 1").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("delete parent");
    let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(
        format!("SELECT COUNT(*)::BIGINT FROM \"{child}\" WHERE pid = 1").as_str(),
    ))
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(n, 0, "CASCADE 级联生效");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{child}\"").as_str(),
    ))
    .execute(&pool)
    .await
    .ok();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{parent}\"").as_str(),
    ))
    .execute(&pool)
    .await
    .ok();
}

/// KB4：类型面——NUMERIC 38 位大数往返（PG 族 NUMERIC 通道，对照 PG 批 R2 结论）
#[tokio::test]
#[ignore = "需要 KingbaseES 运行"]
async fn matrix_kb4_numeric_and_types() {
    let Some(pool) = try_kb_pool().await else {
        eprintln!("skip: 不可达");
        return;
    };
    let table = unique_table("kb4");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE IF EXISTS \"{table}\"").as_str(),
    ))
    .execute(&pool)
    .await
    .ok();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("CREATE TABLE \"{table}\" (id INT PRIMARY KEY, dec_col NUMERIC(38,6))").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("create");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO \"{table}\" VALUES (1, 99999999999999999999.123456)").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("insert big numeric");
    let s: String = sqlx::query_scalar(sqlx::AssertSqlSafe(
        format!("SELECT CAST(dec_col AS TEXT) FROM \"{table}\" WHERE id = 1").as_str(),
    ))
    .fetch_one(&pool)
    .await
    .expect("read");
    assert!(
        s.starts_with("99999999999999999999.123456"),
        "NUMERIC(38,6) 大数无损: {s}"
    );
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{table}\"").as_str(),
    ))
    .execute(&pool)
    .await
    .ok();
}
