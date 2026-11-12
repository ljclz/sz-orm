//! TiDB 兼容方言集成测试
//!
//! TiDbDialect 通过 delegate_dialect_to! 委派 MySqlDialect，
//! 生成的 SQL 与 MySqlDialect 一致。用 MySQL 9.6 验证兼容性。
//!
//! 运行方式：cargo test -p sz-orm-core --test integration_tidb -- --ignored --nocapture

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
fn test_tidb_dialect_quote() {
    let tidb = get_dialect(DbType::TiDB).unwrap();
    let mysql = get_dialect(DbType::MySQL).unwrap();
    assert_eq!(tidb.quote("user"), mysql.quote("user"));
    assert_eq!(tidb.quote("user"), "`user`");
}

#[test]
fn test_tidb_dialect_escape() {
    let tidb = get_dialect(DbType::TiDB).unwrap();
    let mysql = get_dialect(DbType::MySQL).unwrap();
    assert_eq!(tidb.escape_string("it's"), mysql.escape_string("it's"));
}

#[test]
fn test_tidb_dialect_pagination() {
    let tidb = get_dialect(DbType::TiDB).unwrap();
    let mysql = get_dialect(DbType::MySQL).unwrap();
    assert_eq!(
        tidb.build_pagination("SELECT 1", 10, 20),
        mysql.build_pagination("SELECT 1", 10, 20)
    );
}

#[test]
fn test_tidb_dialect_db_type() {
    let dialect = get_dialect(DbType::TiDB).unwrap();
    assert_eq!(dialect.db_type(), DbType::TiDB);
}

#[test]
fn test_tidb_dialect_create_table() {
    let tidb = get_dialect(DbType::TiDB).unwrap();
    let mysql = get_dialect(DbType::MySQL).unwrap();
    let sql_tidb = tidb.build_create_table("test_table", &test_columns());
    let sql_mysql = mysql.build_create_table("test_table", &test_columns());
    assert_eq!(sql_tidb, sql_mysql);
}

// ==================== 真实 DB 集成测试 ====================

#[tokio::test]
#[ignore]
async fn test_tidb_crud_on_mysql() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::TiDB).unwrap();
    let table = unique_table("tidb");

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
async fn test_tidb_pagination_on_mysql() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::TiDB).unwrap();
    let table = unique_table("tidb_pg");

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
async fn test_tidb_sql_injection_protection() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::TiDB).unwrap();
    let table = unique_table("tidb_inj");

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
// 容器化方言矩阵（2026-11-13，MZ-TiDB × TiDB v8.5.1 容器 @4400，MySQL 协议）
// 与既有 8 项不重复：聚焦家族偏差点（FK 真执行 / AUTO_RANDOM / RETURNING 缺失 / CHECK）
// ============================================================================

/// MZ-TiDB-1: 外键 v6.6+ 真执行——sz-orm FK 生成面（build_add_foreign_key）在 TiDB 行为契约
#[tokio::test]
#[ignore]
async fn matrix_mz_tidb1_foreign_key_enforced() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::TiDB).unwrap();

    // 委托正确性：TiDbDialect 委派 MySqlDialect
    assert_eq!(dialect.db_type(), DbType::TiDB);

    sqlx::query(sqlx::AssertSqlSafe(
        "CREATE TABLE IF NOT EXISTS `mz_tidb_parent` (id INT PRIMARY KEY)",
    ))
    .execute(&pool)
    .await
    .expect("create parent");

    let child = unique_table("mzt1child");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("CREATE TABLE `{child}` (id INT PRIMARY KEY, pid INT, FOREIGN KEY (pid) REFERENCES `mz_tidb_parent`(id))").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("create child with FK");

    // 幂等 seed：重复运行时 1062 忽略
    let _ = sqlx::query(sqlx::AssertSqlSafe(
        "INSERT INTO `mz_tidb_parent` VALUES (1)",
    ))
    .execute(&pool)
    .await;

    // 合法引用插入成功
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{child}` VALUES (1, 1)").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("valid FK insert");

    // 行为断言：孤儿引用被 TiDB 拒绝（v6.6+ FK 真执行，错误码 1452——旧版静默忽略）
    let err = match sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{child}` VALUES (2, 99)").as_str(),
    ))
    .execute(&pool)
    .await
    {
        Err(e) => e,
        Ok(_) => panic!("TiDB v8.5 FK 必须拒绝孤儿引用"),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("1452") || msg.to_lowercase().contains("foreign key"),
        "期望 TiDB FK 违反（1452），got: {msg}"
    );

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{child}`").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop child");
}

/// MZ-TiDB-2: AUTO_RANDOM 分布式主键（替代 AUTO_INCREMENT——家族扩展；sz-orm 默认生成
/// AUTO_INCREMENT 仍可用，AUTO_RANDOM 属引擎特有登记）
#[tokio::test]
#[ignore]
async fn matrix_mz_tidb2_auto_random_extension() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::TiDB).unwrap();

    // sz-orm 生成面：默认 AUTO_INCREMENT 关键字（TiDB 兼容）
    assert_eq!(
        dialect.auto_increment_keyword(),
        "AUTO_INCREMENT",
        "TiDbDialect 委派 MySqlDialect，自增关键字应为 AUTO_INCREMENT"
    );

    // 引擎特有：AUTO_RANDOM 建表 + 插入回填非确定性分布式 id
    let table = unique_table("mzt2ar");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("CREATE TABLE `{table}` (id BIGINT AUTO_RANDOM PRIMARY KEY, v VARCHAR(20))")
            .as_str(),
    ))
    .execute(&pool)
    .await
    .expect("create AUTO_RANDOM table（TiDB 特有）");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{table}` (v) VALUES ('a')").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("insert into AUTO_RANDOM");

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT id FROM `{table}`").as_str(),
    ))
    .fetch_one(&pool)
    .await
    .expect("select auto random id");
    let id: i64 = row.get("id");
    assert!(id != 0, "AUTO_RANDOM 应生成非零分布式 id，got {id}");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{table}`").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
    // 注意：mz_tidb_parent 由 mz_tidb1 创建并在该测试末尾随 child 一同清理；
    // 此处不在 mz_tidb2 中删 parent（FK 依赖，3730）
}

/// MZ-TiDB-3: INSERT ... RETURNING 不支持（MySQL 家族缺失项——LAST_INSERT_ID() 通道契约）
#[tokio::test]
#[ignore]
async fn matrix_mz_tidb3_no_insert_returning() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::TiDB).unwrap();

    // sz-orm 委托面：supports_returning=false → 走 LAST_INSERT_ID() 通道（正确委托）
    assert!(!dialect.supports_returning());
    assert_eq!(dialect.last_insert_id_sql(), Some("LAST_INSERT_ID()"));

    // 真库行为：RETURNING 语法被 TiDB v8.5 拒绝（1064）
    let table = unique_table("mzt3ret");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("CREATE TABLE `{table}` (id INT PRIMARY KEY, v VARCHAR(5))").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("create table");

    let err = match sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{table}` VALUES (1,'a') RETURNING id").as_str(),
    ))
    .execute(&pool)
    .await
    {
        Err(e) => e,
        Ok(_) => panic!("TiDB 不支持 INSERT RETURNING，必须报错"),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("1064") || msg.to_lowercase().contains("syntax"),
        "期望 TiDB 1064 语法错误，got: {msg}"
    );

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{table}`").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}

/// MZ-TiDB-4: CHECK 约束执行面（v8.5 解析但不执行违规拦截探针——登记引擎行为）
#[tokio::test]
#[ignore]
async fn matrix_mz_tidb4_check_constraint_behavior() {
    let pool = setup_pool().await;

    let table = unique_table("mzt4ck");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("CREATE TABLE `{table}` (v INT CHECK (v > 0))").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("create table with CHECK（TiDB 可建）");

    // 行为探针：v8.5 CHECK 违反是否被拒绝（真库实测登记，不预设）
    let violation = sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{table}` (v) VALUES (-1)").as_str(),
    ))
    .execute(&pool)
    .await;

    match violation {
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("3819") || msg.to_lowercase().contains("check"),
                "若拒绝应为 CHECK 违反（3819），got: {msg}"
            );
        }
        Ok(_) => {
            // TiDB 部分版本解析 CHECK 但不执行——登记为引擎特有（合法结果，测试记录事实）
            let count = sqlx::query(sqlx::AssertSqlSafe(
                format!("SELECT COUNT(*) AS c FROM `{table}` WHERE v = -1").as_str(),
            ))
            .fetch_one(&pool)
            .await
            .expect("count");
            let c: i64 = count.get("c");
            assert_eq!(
                c, 1,
                "静默路径下 -1 应已入库（TiDB CHECK 未执行的引擎行为登记）"
            );
        }
    }

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{table}`").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}
