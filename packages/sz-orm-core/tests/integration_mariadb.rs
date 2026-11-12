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

// ============================================================================
// 容器化方言矩阵（2026-11-13，MZ-MariaDB × MariaDB 11.8 容器 @3326）
// 与既有 8 项不重复：聚焦家族偏差点（SEQUENCE / RETURNING / CHECK 执行 / sql_mode）
// ============================================================================

/// MZ-MariaDB-1: SEQUENCE（MariaDB 10.3+ 原生 SEQUENCE，MySQL 无——家族偏差）
#[tokio::test]
#[ignore]
async fn matrix_mz_mariadb1_native_sequence() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::MariaDB).unwrap();

    // 委托正确性：MariaDbDialect 仍委派 MySqlDialect（sz-orm 不生成 SEQUENCE，
    // 但 MariaDB 真库可手动使用——登记为家族扩展点，非 sz-orm 缺口）
    assert_eq!(dialect.db_type(), DbType::MariaDB);

    let create_seq = "CREATE SEQUENCE IF NOT EXISTS `mz_mariadb_seq` START WITH 10";
    sqlx::query(sqlx::AssertSqlSafe(create_seq))
        .execute(&pool)
        .await
        .expect("MariaDB 原生 CREATE SEQUENCE（MySQL 语法族扩展）");

    let row = sqlx::query(sqlx::AssertSqlSafe(
        "SELECT NEXTVAL(`mz_mariadb_seq`) AS v1, NEXTVAL(`mz_mariadb_seq`) AS v2",
    ))
    .fetch_one(&pool)
    .await
    .expect("NEXTVAL");
    let v1: i64 = row.get("v1");
    let v2: i64 = row.get("v2");
    assert_eq!((v1, v2), (10, 11), "SEQUENCE NEXTVAL 应递增 10→11");

    sqlx::query(sqlx::AssertSqlSafe(
        "DROP SEQUENCE IF EXISTS `mz_mariadb_seq`",
    ))
    .execute(&pool)
    .await
    .expect("drop sequence");
}

/// MZ-MariaDB-2: INSERT ... RETURNING（10.5+ 支持，MySQL 无——家族偏差；sz-orm 委派面
/// supports_returning()=false 裁定登记：MySQL 主线无此语法，MariaDB 属引擎扩展，不改委托）
#[tokio::test]
#[ignore]
async fn matrix_mz_mariadb2_insert_returning_engine_extension() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::MariaDB).unwrap();

    // sz-orm 委托面：MariaDbDialect → MySqlDialect，supports_returning=false（LAST_INSERT_ID() 通道）
    assert!(
        !dialect.supports_returning(),
        "MariaDbDialect 委派 MySqlDialect，supports_returning 应为 false（引擎扩展不反转委托）"
    );
    assert_eq!(
        dialect.last_insert_id_sql(),
        Some("LAST_INSERT_ID()"),
        "主键回填通道应为 LAST_INSERT_ID()"
    );

    // 真库行为：MariaDB 10.5+ 的 INSERT ... RETURNING 引擎扩展可用
    let table = unique_table("mzm2ret");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("CREATE TABLE `{table}` (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(50))")
            .as_str(),
    ))
    .execute(&pool)
    .await
    .expect("create table");

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{table}` (name) VALUES ('a') RETURNING id").as_str(),
    ))
    .fetch_one(&pool)
    .await
    .expect("INSERT RETURNING（MariaDB 10.5+ 引擎扩展）");
    let backfilled: i64 = row.get("id");
    assert!(
        backfilled >= 1,
        "RETURNING id 应回填自增值，got {backfilled}"
    );

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{table}`").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}

/// MZ-MariaDB-3: CHECK 约束真执行（10.2+；MySQL 8.0.16 前 defer——家族偏差）
#[tokio::test]
#[ignore]
async fn matrix_mz_mariadb3_check_constraint_enforced() {
    let pool = setup_pool().await;

    let table = unique_table("mzm3chk");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("CREATE TABLE `{table}` (v INT CHECK (v > 0))").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("create table with CHECK");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{table}` (v) VALUES (5)").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("valid insert");

    // 行为断言：违反 CHECK 被真库拒绝（MariaDB 10.2+ 真执行，错误码 4025）
    let err = match sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{table}` (v) VALUES (-1)").as_str(),
    ))
    .execute(&pool)
    .await
    {
        Err(e) => e,
        Ok(_) => panic!("CHECK 违反必须被 MariaDB 拒绝"),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("4025") || msg.to_lowercase().contains("constraint"),
        "期望 MariaDB CHECK 违反（4025），got: {msg}"
    );

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{table}`").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}

/// MZ-MariaDB-4: 默认 sql_mode 差异（STRICT_TRANS_TABLES + NO_AUTO_CREATE_USER，MySQL 8 已移除后者）
#[tokio::test]
#[ignore]
async fn matrix_mz_mariadb4_default_sql_mode_family_deviation() {
    let pool = setup_pool().await;

    let row = sqlx::query("SELECT @@sql_mode AS m")
        .fetch_one(&pool)
        .await
        .expect("read sql_mode");
    let mode: String = row.get("m");
    assert!(
        mode.contains("STRICT_TRANS_TABLES"),
        "MariaDB 11 默认 sql_mode 应含 STRICT_TRANS_TABLES，got: {mode}"
    );
    // NO_AUTO_CREATE_USER 为 MariaDB 默认（MySQL 8.0 已移除该模式——家族差异证据）
    assert!(
        mode.contains("NO_AUTO_CREATE_USER"),
        "MariaDB 默认 sql_mode 应含 NO_AUTO_CREATE_USER（MySQL 8 移除项），got: {mode}"
    );
}
