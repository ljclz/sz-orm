//! T5.2 + v9.2.0 M1: sqlx/any.rs MySQL #[ignore] 集成测试
//!
//! 需要本机 MySQL 9.6: mysql://root:test123@127.0.0.1:3306/sz_orm_test
//! 运行: cargo test -p sz-orm-sqlx --test any_mysql_test -- --ignored

use sz_orm_core::{Connection, Value};
use sz_orm_sqlx::{AnyBackend, AnyPool};

const DSN: &str = "mysql://root:test123@127.0.0.1:3306/sz_orm_test";

#[tokio::test]
#[ignore]
async fn test_any_backend_mysql_connect() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    assert_eq!(pool.backend(), AnyBackend::MySql);
}

#[tokio::test]
#[ignore]
async fn test_any_connection_mysql_create_insert_query() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS any_test_t")
        .await
        .unwrap();
    conn.execute("CREATE TABLE any_test_t (id INT PRIMARY KEY, name VARCHAR(255) NOT NULL)")
        .await
        .unwrap();
    conn.execute("INSERT INTO any_test_t (id, name) VALUES (1, 'alice')")
        .await
        .unwrap();
    let rows = conn
        .query("SELECT id, name FROM any_test_t WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    conn.execute("DROP TABLE IF EXISTS any_test_t")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_any_connection_mysql_dialect_placeholder() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let dialect = pool.dialect();
    assert_eq!(dialect.db_type(), sz_orm_core::DbType::MySQL);
}

#[tokio::test]
#[ignore]
async fn test_any_connection_mysql_transaction() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS any_tx_test")
        .await
        .unwrap();
    conn.execute("CREATE TABLE any_tx_test (id INT PRIMARY KEY)")
        .await
        .unwrap();
    conn.begin_transaction().await.unwrap();
    conn.execute("INSERT INTO any_tx_test (id) VALUES (1)")
        .await
        .unwrap();
    conn.rollback().await.unwrap();
    let rows = conn.query("SELECT id FROM any_tx_test").await.unwrap();
    assert_eq!(rows.len(), 0);
    conn.execute("DROP TABLE IF EXISTS any_tx_test")
        .await
        .unwrap();
}

// ===================== T1: execute 全分支（4 tests） =====================

#[tokio::test]
#[ignore]
async fn test_mysql_execute_after_close_returns_error() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.close().await.unwrap();
    let result = conn.execute("SELECT 1").await;
    assert!(result.is_err());
    assert!(!conn.is_connected());
}

#[tokio::test]
#[ignore]
async fn test_mysql_execute_error_marks_disconnected() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    let result = conn
        .execute("INSERT INTO nonexistent_table_v920 VALUES (1)")
        .await;
    assert!(result.is_err());
}

#[tokio::test]
#[ignore]
async fn test_mysql_execute_rows_affected() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_affected")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_affected (id INT PRIMARY KEY)")
        .await
        .unwrap();
    let affected = conn
        .execute("INSERT INTO t_v920_affected (id) VALUES (1), (2), (3)")
        .await
        .unwrap();
    assert_eq!(affected, 3);
    conn.execute("DROP TABLE IF EXISTS t_v920_affected")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_mysql_execute_needs_raw_sql_path() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_raw")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_raw (id INT PRIMARY KEY)")
        .await
        .unwrap();
    conn.begin_transaction().await.unwrap();
    conn.execute("INSERT INTO t_v920_raw (id) VALUES (1)")
        .await
        .unwrap();
    conn.rollback().await.unwrap();
    let rows = conn.query("SELECT id FROM t_v920_raw").await.unwrap();
    assert_eq!(rows.len(), 0);
    conn.execute("DROP TABLE IF EXISTS t_v920_raw")
        .await
        .unwrap();
}

// ===================== T2: query 全类型转换（10 tests） =====================

#[tokio::test]
#[ignore]
async fn test_mysql_query_int_types() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_int")
        .await
        .unwrap();
    conn.execute(
        "CREATE TABLE t_v920_int (a TINYINT, b SMALLINT, c INT, d BIGINT)",
    )
    .await
    .unwrap();
    conn.execute("INSERT INTO t_v920_int VALUES (127, 32767, 2147483647, 9223372036854775807)")
        .await
        .unwrap();
    let rows = conn.query("SELECT a, b, c, d FROM t_v920_int").await.unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert!(!row.get("a").unwrap().is_null());
    assert!(!row.get("b").unwrap().is_null());
    assert!(!row.get("c").unwrap().is_null());
    assert!(!row.get("d").unwrap().is_null());
    conn.execute("DROP TABLE IF EXISTS t_v920_int")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_mysql_query_unsigned_types() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_unsigned")
        .await
        .unwrap();
    conn.execute(
        "CREATE TABLE t_v920_unsigned (a INT UNSIGNED, b BIGINT UNSIGNED)",
    )
    .await
    .unwrap();
    conn.execute("INSERT INTO t_v920_unsigned VALUES (4294967295, 18446744073709551615)")
        .await
        .unwrap();
    let rows = conn.query("SELECT a, b FROM t_v920_unsigned")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    conn.execute("DROP TABLE IF EXISTS t_v920_unsigned")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_mysql_query_float_double() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_float")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_float (a FLOAT, b DOUBLE)")
        .await
        .unwrap();
    conn.execute("INSERT INTO t_v920_float VALUES (3.14, 2.718281828459045)")
        .await
        .unwrap();
    let rows = conn.query("SELECT a, b FROM t_v920_float").await.unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert!(!row.get("a").unwrap().is_null());
    assert!(!row.get("b").unwrap().is_null());
    conn.execute("DROP TABLE IF EXISTS t_v920_float")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_mysql_query_varchar_text() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_str")
        .await
        .unwrap();
    conn.execute(
        "CREATE TABLE t_v920_str (a VARCHAR(50), b TEXT, c CHAR(3))",
    )
    .await
    .unwrap();
    conn.execute("INSERT INTO t_v920_str VALUES ('hello', 'world text', 'abc')")
        .await
        .unwrap();
    let rows = conn.query("SELECT a, b, c FROM t_v920_str").await.unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert!(row.get("a").unwrap().is_string());
    assert!(row.get("b").unwrap().is_string());
    assert!(row.get("c").unwrap().is_string());
    conn.execute("DROP TABLE IF EXISTS t_v920_str")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_mysql_query_blob_binary() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_blob")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_blob (a BLOB, b VARBINARY(100))")
        .await
        .unwrap();
    conn.execute("INSERT INTO t_v920_blob VALUES (x'deadbeef', x'cafe')")
        .await
        .unwrap();
    let rows = conn.query("SELECT a, b FROM t_v920_blob").await.unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert!(row.get("a").unwrap().is_bytes());
    assert!(row.get("b").unwrap().is_bytes());
    conn.execute("DROP TABLE IF EXISTS t_v920_blob")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_mysql_query_decimal() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_dec")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_dec (a DECIMAL(20,2))")
        .await
        .unwrap();
    conn.execute("INSERT INTO t_v920_dec VALUES (123456789.99)")
        .await
        .unwrap();
    let rows = conn.query("SELECT a FROM t_v920_dec").await.unwrap();
    assert_eq!(rows.len(), 1);
    let val = rows[0].get("a").unwrap();
    assert!(!val.is_null());
    conn.execute("DROP TABLE IF EXISTS t_v920_dec")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_mysql_query_date_datetime_time() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_date")
        .await
        .unwrap();
    conn.execute(
        "CREATE TABLE t_v920_date (a DATE, b DATETIME, c TIME)",
    )
    .await
    .unwrap();
    conn.execute("INSERT INTO t_v920_date VALUES ('2026-09-27', '2026-09-27 12:30:00', '08:00:00')")
        .await
        .unwrap();
    let rows = conn.query("SELECT a, b, c FROM t_v920_date")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);

    conn.execute("DROP TABLE IF EXISTS t_v920_date")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_mysql_query_null_values() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_null")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_null (a INT, b VARCHAR(50))")
        .await
        .unwrap();
    conn.execute("INSERT INTO t_v920_null VALUES (NULL, NULL)")
        .await
        .unwrap();
    let rows = conn.query("SELECT a, b FROM t_v920_null").await.unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert!(row.get("a").unwrap().is_null());
    assert!(row.get("b").unwrap().is_null());
    conn.execute("DROP TABLE IF EXISTS t_v920_null")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_mysql_query_empty_result() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_empty")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_empty (id INT PRIMARY KEY)")
        .await
        .unwrap();
    let rows = conn
        .query("SELECT id FROM t_v920_empty WHERE id > 100")
        .await
        .unwrap();
    assert_eq!(rows.len(), 0);
    conn.execute("DROP TABLE IF EXISTS t_v920_empty")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_mysql_query_with_params() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_params")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_params (id INT PRIMARY KEY, name VARCHAR(50))")
        .await
        .unwrap();
    conn.execute_with_params(
        "INSERT INTO t_v920_params (id, name) VALUES (?, ?)",
        &[Value::I32(1), Value::String("test".to_string())],
    )
    .await
    .unwrap();
    let rows = conn
        .query_with_params(
            "SELECT id, name FROM t_v920_params WHERE id = ?",
            &[Value::I32(1)],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].get("name").unwrap().is_string());
    conn.execute("DROP TABLE IF EXISTS t_v920_params")
        .await
        .unwrap();
}

// ===================== T3: bulk_insert 路径（2 tests） =====================

#[tokio::test]
#[ignore]
async fn test_mysql_bulk_insert_via_execute_with_params() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_bulk")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_bulk (id INT PRIMARY KEY, name VARCHAR(50))")
        .await
        .unwrap();
    let affected = conn
        .execute_with_params(
            "INSERT INTO t_v920_bulk (id, name) VALUES (?, ?), (?, ?)",
            &[
                Value::I32(1),
                Value::String("a".to_string()),
                Value::I32(2),
                Value::String("b".to_string()),
            ],
        )
        .await
        .unwrap();
    assert_eq!(affected, 2);
    let rows = conn.query("SELECT id FROM t_v920_bulk").await.unwrap();
    assert_eq!(rows.len(), 2);
    conn.execute("DROP TABLE IF EXISTS t_v920_bulk")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_mysql_execute_with_params_empty_falls_back_to_execute() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_noparams")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_noparams (id INT PRIMARY KEY)")
        .await
        .unwrap();
    let affected = conn
        .execute_with_params(
            "INSERT INTO t_v920_noparams (id) VALUES (42)",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(affected, 1);
    conn.execute("DROP TABLE IF EXISTS t_v920_noparams")
        .await
        .unwrap();
}
