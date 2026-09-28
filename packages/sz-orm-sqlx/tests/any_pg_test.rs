//! T5.2 + v9.2.0 M1: sqlx/any.rs PostgreSQL #[ignore] 集成测试
//!
//! 需要本机 PostgreSQL 18: postgres://postgres:test123@127.0.0.1:5432/sz_orm_test
//! 运行: cargo test -p sz-orm-sqlx --test any_pg_test -- --ignored

use std::sync::OnceLock;

use sz_orm_core::{Connection, Value};
use sz_orm_sqlx::{AnyBackend, AnyPool};

static DSN_CELL: OnceLock<String> = OnceLock::new();

fn dsn() -> &'static str {
    DSN_CELL.get_or_init(|| {
        std::env::var("SZ_ORM_PG_URL")
            .unwrap_or_else(|_| "postgres://postgres:test123@127.0.0.1:5432/sz_orm_test".to_string())
    })
}

#[tokio::test]
#[ignore]
async fn test_any_backend_postgres_connect() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    assert_eq!(pool.backend(), AnyBackend::Postgres);
}

#[tokio::test]
#[ignore]
async fn test_any_connection_pg_create_insert_query() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS any_pg_test")
        .await
        .unwrap();
    conn.execute("CREATE TABLE any_pg_test (id SERIAL PRIMARY KEY, name TEXT NOT NULL)")
        .await
        .unwrap();
    conn.execute("INSERT INTO any_pg_test (name) VALUES ('alice')")
        .await
        .unwrap();
    let rows = conn
        .query("SELECT id, name FROM any_pg_test WHERE name = 'alice'")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    conn.execute("DROP TABLE IF EXISTS any_pg_test")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_any_connection_pg_dialect_placeholder() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let dialect = pool.dialect();
    assert_eq!(dialect.db_type(), sz_orm_core::DbType::PostgreSQL);
}

#[tokio::test]
#[ignore]
async fn test_any_connection_pg_transaction() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS any_pg_tx")
        .await
        .unwrap();
    conn.execute("CREATE TABLE any_pg_tx (id INT PRIMARY KEY)")
        .await
        .unwrap();
    conn.begin_transaction().await.unwrap();
    conn.execute("INSERT INTO any_pg_tx (id) VALUES (1)")
        .await
        .unwrap();
    conn.commit().await.unwrap();
    let rows = conn.query("SELECT id FROM any_pg_tx").await.unwrap();
    assert_eq!(rows.len(), 1);
    conn.execute("DROP TABLE IF EXISTS any_pg_tx")
        .await
        .unwrap();
}

// ===================== T4: execute/query 全分支（14 tests） =====================

#[tokio::test]
#[ignore]
async fn test_pg_execute_after_close_returns_error() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.close().await.unwrap();
    let result = conn.execute("SELECT 1").await;
    assert!(result.is_err());
    assert!(!conn.is_connected());
}

#[tokio::test]
#[ignore]
async fn test_pg_execute_error_on_nonexistent_table() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    let result = conn
        .execute("INSERT INTO nonexistent_v920 VALUES (1)")
        .await;
    assert!(result.is_err());
}

#[tokio::test]
#[ignore]
async fn test_pg_execute_rows_affected() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
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
async fn test_pg_execute_needs_raw_sql_begin_commit() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
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
    conn.commit().await.unwrap();
    let rows = conn.query("SELECT id FROM t_v920_raw").await.unwrap();
    assert_eq!(rows.len(), 1);
    conn.execute("DROP TABLE IF EXISTS t_v920_raw")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_pg_query_basic_select() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    let rows = conn.query("SELECT 1 AS one, 'hello' AS txt").await.unwrap();
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].get("one").unwrap().is_null());
    assert!(rows[0].get("txt").unwrap().is_string());
}

#[tokio::test]
#[ignore]
async fn test_pg_query_empty_result() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
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
async fn test_pg_query_with_params_dollar_placeholders() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_params")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_params (id INT PRIMARY KEY, name TEXT)")
        .await
        .unwrap();
    conn.execute_with_params(
        "INSERT INTO t_v920_params (id, name) VALUES ($1, $2)",
        &[Value::I32(1), Value::String("test".to_string())],
    )
    .await
    .unwrap();
    let rows = conn
        .query_with_params(
            "SELECT id, name FROM t_v920_params WHERE id = $1",
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

#[tokio::test]
#[ignore]
async fn test_pg_execute_with_params_various_types() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_multi")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_multi (id INT, val BIGINT, flag BOOLEAN, score DOUBLE PRECISION)")
        .await
        .unwrap();
    let affected = conn
        .execute_with_params(
            "INSERT INTO t_v920_multi (id, val, flag, score) VALUES ($1, $2, $3, $4)",
            &[
                Value::I32(1),
                Value::I64(9999999999),
                Value::Bool(true),
                Value::F64(3.14),
            ],
        )
        .await
        .unwrap();
    assert_eq!(affected, 1);
    conn.execute("DROP TABLE IF EXISTS t_v920_multi")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_pg_begin_transaction_and_in_transaction_flag() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    assert!(!conn.in_transaction());
    conn.begin_transaction().await.unwrap();
    assert!(conn.in_transaction());
    conn.rollback().await.unwrap();
    assert!(!conn.in_transaction());
}

#[tokio::test]
#[ignore]
async fn test_pg_commit_clears_transaction_flag() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.begin_transaction().await.unwrap();
    assert!(conn.in_transaction());
    conn.commit().await.unwrap();
    assert!(!conn.in_transaction());
}

#[tokio::test]
#[ignore]
async fn test_pg_rollback_clears_transaction_flag() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.begin_transaction().await.unwrap();
    conn.rollback().await.unwrap();
    assert!(!conn.in_transaction());
}

#[tokio::test]
#[ignore]
async fn test_pg_double_begin_returns_error() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.begin_transaction().await.unwrap();
    let result = conn.begin_transaction().await;
    assert!(result.is_err());
    conn.rollback().await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_pg_is_connected_after_connect() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    assert!(conn.is_connected());
    conn.close().await.unwrap();
    assert!(!conn.is_connected());
}

#[tokio::test]
#[ignore]
async fn test_pg_ping_returns_true() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    assert!(conn.ping().await);
}

// ===================== T5: PG 特有类型转换（15 tests） =====================

#[tokio::test]
#[ignore]
async fn test_pg_query_bool_type() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_bool")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_bool (a BOOLEAN)")
        .await
        .unwrap();
    conn.execute("INSERT INTO t_v920_bool VALUES (TRUE), (FALSE)")
        .await
        .unwrap();
    let rows = conn.query("SELECT a FROM t_v920_bool").await.unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].get("a").unwrap().is_bool());
    conn.execute("DROP TABLE IF EXISTS t_v920_bool")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_pg_query_int2_type() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    let rows = conn.query("SELECT 32767::INT2 AS val").await.unwrap();
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].get("val").unwrap().is_null());
}

#[tokio::test]
#[ignore]
async fn test_pg_query_int4_type() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    let rows = conn.query("SELECT 2147483647::INT4 AS val").await.unwrap();
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].get("val").unwrap().is_null());
}

#[tokio::test]
#[ignore]
async fn test_pg_query_int8_type() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    let rows = conn
        .query("SELECT 9223372036854775807::INT8 AS val")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].get("val").unwrap().is_i64());
}

#[tokio::test]
#[ignore]
async fn test_pg_query_float4_type() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    let rows = conn.query("SELECT 3.14::FLOAT4 AS val").await.unwrap();
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].get("val").unwrap().is_null());
}

#[tokio::test]
#[ignore]
async fn test_pg_query_float8_type() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    let rows = conn
        .query("SELECT 2.718281828459045::FLOAT8 AS val")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].get("val").unwrap().is_f64());
}

#[tokio::test]
#[ignore]
async fn test_pg_query_text_varchar_types() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    let rows = conn
        .query("SELECT 'hello'::TEXT AS a, 'world'::VARCHAR(20) AS b")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].get("a").unwrap().is_string());
    assert!(rows[0].get("b").unwrap().is_string());
}

#[tokio::test]
#[ignore]
async fn test_pg_query_numeric_type() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    let rows = conn
        .query("SELECT 123456789.99::NUMERIC(20,2) AS val")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let val = rows[0].get("val").unwrap();
    assert!(!val.is_null());
}

#[tokio::test]
#[ignore]
async fn test_pg_query_uuid_type() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    let rows = conn
        .query("SELECT '550e8400-e29b-41d4-a716-446655440000'::UUID AS val")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].get("val").unwrap().is_string());
}

#[tokio::test]
#[ignore]
async fn test_pg_query_jsonb_type() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    let rows = conn
        .query("SELECT '{\"key\": \"value\"}'::JSONB AS val")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let val = rows[0].get("val").unwrap();
    assert!(val.is_string() || val.is_null());
}

#[tokio::test]
#[ignore]
async fn test_pg_query_bytea_type() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    let rows = conn
        .query("SELECT E'\\\\xdeadbeef'::BYTEA AS val")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let val = rows[0].get("val").unwrap();
    assert!(val.is_bytes() || val.is_null());
}

#[tokio::test]
#[ignore]
async fn test_pg_query_date_type() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    let rows = conn
        .query("SELECT '2026-09-27'::DATE AS val")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let val = rows[0].get("val").unwrap();
    assert!(val.is_string() || val.is_null());
}

#[tokio::test]
#[ignore]
async fn test_pg_query_timestamp_type() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    let rows = conn
        .query("SELECT '2026-09-27 12:30:00'::TIMESTAMP AS val")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let val = rows[0].get("val").unwrap();
    assert!(val.is_string() || val.is_null());
}

#[tokio::test]
#[ignore]
async fn test_pg_query_timestamptz_type() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    let rows = conn
        .query("SELECT '2026-09-27 12:30:00+00'::TIMESTAMPTZ AS val")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let val = rows[0].get("val").unwrap();
    assert!(val.is_string() || val.is_null());
}

#[tokio::test]
#[ignore]
async fn test_pg_query_null_values() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    let rows = conn
        .query("SELECT NULL::INT AS a, NULL::TEXT AS b")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].get("a").unwrap().is_null());
    assert!(rows[0].get("b").unwrap().is_null());
}

// ===================== T6: bulk_insert + PG 扩展（8 tests） =====================

#[tokio::test]
#[ignore]
async fn test_pg_bulk_insert_via_execute_with_params() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_bulk")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_bulk (id INT PRIMARY KEY, name TEXT)")
        .await
        .unwrap();
    let affected = conn
        .execute_with_params(
            "INSERT INTO t_v920_bulk (id, name) VALUES ($1, $2), ($3, $4)",
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
async fn test_pg_execute_with_params_empty_falls_back() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_noparams")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_noparams (id INT PRIMARY KEY)")
        .await
        .unwrap();
    let affected = conn
        .execute_with_params("INSERT INTO t_v920_noparams (id) VALUES (42)", &[])
        .await
        .unwrap();
    assert_eq!(affected, 1);
    conn.execute("DROP TABLE IF EXISTS t_v920_noparams")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_pg_execute_with_params_null_value() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_null_param")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_null_param (id INT, val TEXT)")
        .await
        .unwrap();
    conn.execute_with_params(
        "INSERT INTO t_v920_null_param (id, val) VALUES ($1, $2)",
        &[Value::I32(1), Value::Null],
    )
    .await
    .unwrap();
    let rows = conn
        .query("SELECT val FROM t_v920_null_param WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].get("val").unwrap().is_null());
    conn.execute("DROP TABLE IF EXISTS t_v920_null_param")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_pg_execute_with_params_bool_value() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_bool_param")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_bool_param (id INT, flag BOOLEAN)")
        .await
        .unwrap();
    conn.execute_with_params(
        "INSERT INTO t_v920_bool_param (id, flag) VALUES ($1, $2)",
        &[Value::I32(1), Value::Bool(true)],
    )
    .await
    .unwrap();
    let rows = conn
        .query("SELECT flag FROM t_v920_bool_param WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].get("flag").unwrap().is_bool());
    conn.execute("DROP TABLE IF EXISTS t_v920_bool_param")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_pg_execute_with_params_i64_value() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_i64_param")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_i64_param (id INT, big_val BIGINT)")
        .await
        .unwrap();
    conn.execute_with_params(
        "INSERT INTO t_v920_i64_param (id, big_val) VALUES ($1, $2)",
        &[Value::I32(1), Value::I64(9223372036854775807)],
    )
    .await
    .unwrap();
    let rows = conn
        .query("SELECT big_val FROM t_v920_i64_param WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].get("big_val").unwrap().is_i64());
    conn.execute("DROP TABLE IF EXISTS t_v920_i64_param")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_pg_execute_with_params_bytes_value() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_bytes_param")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_bytes_param (id INT, data BYTEA)")
        .await
        .unwrap();
    conn.execute_with_params(
        "INSERT INTO t_v920_bytes_param (id, data) VALUES ($1, $2)",
        &[Value::I32(1), Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef])],
    )
    .await
    .unwrap();
    let rows = conn
        .query("SELECT data FROM t_v920_bytes_param WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].get("data").unwrap().is_bytes());
    conn.execute("DROP TABLE IF EXISTS t_v920_bytes_param")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_pg_query_with_params_multiple_results() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS t_v920_multi_result")
        .await
        .unwrap();
    conn.execute("CREATE TABLE t_v920_multi_result (id INT, name TEXT)")
        .await
        .unwrap();
    conn.execute("INSERT INTO t_v920_multi_result VALUES (1, 'a'), (2, 'b'), (3, 'c')")
        .await
        .unwrap();
    let rows = conn
        .query_with_params(
            "SELECT id, name FROM t_v920_multi_result WHERE id >= $1 ORDER BY id",
            &[Value::I32(2)],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    conn.execute("DROP TABLE IF EXISTS t_v920_multi_result")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_pg_rollback_without_begin_is_noop() {
    let pool = AnyPool::connect(dsn()).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.rollback().await.unwrap();
    assert!(!conn.in_transaction());
}
