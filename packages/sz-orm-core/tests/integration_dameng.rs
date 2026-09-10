//! Dameng 兼容方言集成测试
//!
//! DamengDialect 通过 delegate_dialect_to! 委派 OracleDialect，
//! 生成的 SQL 与 OracleDialect 一致。用 Oracle 23ai Free 验证兼容性。
//!
//! 运行方式：cargo test -p sz-orm-core --test integration_dameng -- --ignored --nocapture

use oracle::Connection as OracleConn;
use std::sync::atomic::{AtomicU64, Ordering};
use sz_orm_core::dialect::{get_dialect, ColumnDef};
use sz_orm_core::DbType;

const ORACLE_USER_DEFAULT: &str = "sz_orm_test";
const ORACLE_PASSWORD_DEFAULT: &str = "SzOrmTest2026";
const ORACLE_CONNECT_STRING_DEFAULT: &str = "127.0.0.1:1521/freepdb1.FALSE";

fn oracle_user() -> String {
    std::env::var("SZ_ORM_ORACLE_USER").unwrap_or_else(|_| ORACLE_USER_DEFAULT.to_string())
}

fn oracle_password() -> String {
    std::env::var("SZ_ORM_ORACLE_PASSWORD").unwrap_or_else(|_| ORACLE_PASSWORD_DEFAULT.to_string())
}

fn oracle_connect_string() -> String {
    std::env::var("SZ_ORM_ORACLE_CONNECT_STRING")
        .unwrap_or_else(|_| ORACLE_CONNECT_STRING_DEFAULT.to_string())
}

fn open_conn() -> OracleConn {
    OracleConn::connect(oracle_user(), oracle_password(), oracle_connect_string())
        .expect("oracle connect failed - is Oracle 23ai running on 127.0.0.1:1521?")
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
        "dm_{}_{}_{}",
        prefix,
        pid % 1000,
        (nanos % 100000) as u64 * 1000 + counter
    )
}

fn drop_table_if_exists(conn: &OracleConn, table: &str) {
    let dialect = get_dialect(DbType::Dameng).unwrap();
    let sql = dialect.build_drop_table(table, true);
    let _ = conn.execute(&sql, &[]);
}

fn test_columns() -> Vec<ColumnDef> {
    vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "NUMBER".to_string(),
            nullable: false,
            default: None,
            auto_increment: true,
            primary_key: true,
        },
        ColumnDef {
            name: "name".to_string(),
            sql_type: "VARCHAR2(255)".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "value".to_string(),
            sql_type: "NUMBER".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ]
}

// ==================== 方言断言测试 ====================

#[test]
fn test_dameng_dialect_quote() {
    let dameng = get_dialect(DbType::Dameng).unwrap();
    let oracle = get_dialect(DbType::Oracle).unwrap();
    assert_eq!(dameng.quote("user"), oracle.quote("user"));
    assert_eq!(dameng.quote("user"), "\"user\"");
}

#[test]
fn test_dameng_dialect_escape() {
    let dameng = get_dialect(DbType::Dameng).unwrap();
    let oracle = get_dialect(DbType::Oracle).unwrap();
    assert_eq!(dameng.escape_string("it's"), oracle.escape_string("it's"));
}

#[test]
fn test_dameng_dialect_pagination() {
    let dameng = get_dialect(DbType::Dameng).unwrap();
    let oracle = get_dialect(DbType::Oracle).unwrap();
    assert_eq!(
        dameng.build_pagination("SELECT 1", 10, 20),
        oracle.build_pagination("SELECT 1", 10, 20)
    );
}

#[test]
fn test_dameng_dialect_db_type() {
    let dialect = get_dialect(DbType::Dameng).unwrap();
    assert_eq!(dialect.db_type(), DbType::Dameng);
}

#[test]
fn test_dameng_dialect_create_table() {
    let dameng = get_dialect(DbType::Dameng).unwrap();
    let oracle = get_dialect(DbType::Oracle).unwrap();
    let sql_dameng = dameng.build_create_table("test_table", &test_columns());
    let sql_oracle = oracle.build_create_table("test_table", &test_columns());
    assert_eq!(sql_dameng, sql_oracle);
}

#[test]
fn test_dameng_dialect_not_oracle_db_type() {
    let dialect = get_dialect(DbType::Dameng).unwrap();
    assert_ne!(dialect.db_type(), DbType::Oracle);
    assert_eq!(dialect.db_type(), DbType::Dameng);
}

// ==================== 真实 DB 集成测试（Oracle 23ai） ====================

#[test]
#[ignore = "需要 Oracle 23ai 运行于 127.0.0.1:1521"]
fn test_dameng_crud_on_oracle() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Dameng).unwrap();
    let table = unique_table("crud");
    drop_table_if_exists(&conn, &table);

    let create_sql = dialect.build_create_table(&table, &test_columns());
    conn.execute(&create_sql, &[]).expect("create table");

    let insert_sql = format!(
        "INSERT INTO {} ({}, {}) VALUES (:1, :2)",
        dialect.quote(&table),
        dialect.quote("name"),
        dialect.quote("value"),
    );
    conn.execute(&insert_sql, &[&"alice", &100i64])
        .expect("insert 1");
    conn.execute(&insert_sql, &[&"bob", &200i64])
        .expect("insert 2");
    conn.execute(&insert_sql, &[&"carol", &300i64])
        .expect("insert 3");
    conn.commit().expect("commit");

    let count_sql = format!("SELECT COUNT(*) FROM {}", dialect.quote(&table));
    let count: i64 = conn.query_row_as::<i64>(&count_sql, &[]).expect("count");
    assert_eq!(count, 3);

    let select_sql = format!(
        "SELECT {} FROM {} WHERE {} > :1 ORDER BY {}",
        dialect.quote("name"),
        dialect.quote(&table),
        dialect.quote("value"),
        dialect.quote("value"),
    );
    let rows: Vec<String> = conn
        .query_as::<String>(&select_sql, &[&150i64])
        .expect("query")
        .filter_map(|r| r.ok())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], "bob");
    assert_eq!(rows[1], "carol");

    let update_sql = format!(
        "UPDATE {} SET {} = :1 WHERE {} = :2",
        dialect.quote(&table),
        dialect.quote("value"),
        dialect.quote("name"),
    );
    conn.execute(&update_sql, &[&999i64, &"alice"])
        .expect("update");
    conn.commit().expect("commit");

    let check_sql = format!(
        "SELECT {} FROM {} WHERE {} = :1",
        dialect.quote("value"),
        dialect.quote(&table),
        dialect.quote("name"),
    );
    let val: i64 = conn
        .query_row_as::<i64>(&check_sql, &[&"alice"])
        .expect("check");
    assert_eq!(val, 999);

    let delete_sql = format!(
        "DELETE FROM {} WHERE {} = :1",
        dialect.quote(&table),
        dialect.quote("name"),
    );
    conn.execute(&delete_sql, &[&"bob"]).expect("delete");
    conn.commit().expect("commit");

    let count2: i64 = conn.query_row_as::<i64>(&count_sql, &[]).expect("count2");
    assert_eq!(count2, 2);

    drop_table_if_exists(&conn, &table);
}

#[test]
#[ignore = "需要 Oracle 23ai 运行于 127.0.0.1:1521"]
fn test_dameng_pagination_on_oracle() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Dameng).unwrap();
    let table = unique_table("page");
    drop_table_if_exists(&conn, &table);

    let create_sql = dialect.build_create_table(&table, &test_columns());
    conn.execute(&create_sql, &[]).expect("create table");

    let insert_sql = format!(
        "INSERT INTO {} ({}, {}) VALUES (:1, :2)",
        dialect.quote(&table),
        dialect.quote("name"),
        dialect.quote("value"),
    );
    for i in 1..=5i64 {
        conn.execute(&insert_sql, &[&format!("user_{}", i), &(i * 10)])
            .expect("insert");
    }
    conn.commit().expect("commit");

    let base_select = format!(
        "SELECT {} FROM {} ORDER BY {}",
        dialect.quote("name"),
        dialect.quote(&table),
        dialect.quote("value"),
    );
    let page_sql = dialect.build_pagination(&base_select, 2, 2);
    let rows: Vec<String> = conn
        .query_as::<String>(&page_sql, &[])
        .expect("query")
        .filter_map(|r| r.ok())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], "user_3");
    assert_eq!(rows[1], "user_4");

    drop_table_if_exists(&conn, &table);
}

#[test]
#[ignore = "需要 Oracle 23ai 运行于 127.0.0.1:1521"]
fn test_dameng_transaction_on_oracle() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Dameng).unwrap();
    let table = unique_table("tx");
    drop_table_if_exists(&conn, &table);

    let create_sql = dialect.build_create_table(&table, &test_columns());
    conn.execute(&create_sql, &[]).expect("create table");

    let insert_sql = format!(
        "INSERT INTO {} ({}, {}) VALUES (:1, :2)",
        dialect.quote(&table),
        dialect.quote("name"),
        dialect.quote("value"),
    );
    conn.execute(&insert_sql, &[&"alice", &100i64])
        .expect("insert 1");
    conn.commit().expect("commit 1");

    conn.execute(&insert_sql, &[&"bob", &200i64])
        .expect("insert 2");
    conn.rollback().expect("rollback");

    let count_sql = format!("SELECT COUNT(*) FROM {}", dialect.quote(&table));
    let count: i64 = conn.query_row_as::<i64>(&count_sql, &[]).expect("count");
    assert_eq!(count, 1, "rollback 后应只有 alice 1 条");

    drop_table_if_exists(&conn, &table);
}
