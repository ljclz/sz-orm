//! any.rs 深度路径测试 — 覆盖 Connection trait 全方法
//!
//! 针对 sz-orm-sqlx/src/any.rs 中 SqlxSqliteConnection 的 14+ 个方法，
//! 覆盖 execute_with_params / query_with_params / query_values /
//! query_values_with_params / query_stream / execute_batch / in_transaction
//! 及错误处理路径。

use sz_orm_core::{Connection, Value};
use sz_orm_sqlx::{AnyConnection, AnyPool};

async fn setup() -> AnyConnection {
    let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, score REAL, flag INTEGER, data BLOB)",
    )
    .await
    .unwrap();
    conn
}

#[tokio::test]
async fn test_sqlite_in_transaction_after_begin() {
    let mut conn = setup().await;
    conn.begin_transaction().await.unwrap();
    assert!(conn.in_transaction());
}

#[tokio::test]
async fn test_sqlite_in_transaction_after_commit() {
    let mut conn = setup().await;
    conn.begin_transaction().await.unwrap();
    assert!(conn.in_transaction());
    conn.commit().await.unwrap();
    assert!(!conn.in_transaction());
}

#[tokio::test]
async fn test_sqlite_in_transaction_after_rollback() {
    let mut conn = setup().await;
    conn.begin_transaction().await.unwrap();
    assert!(conn.in_transaction());
    conn.rollback().await.unwrap();
    assert!(!conn.in_transaction());
}

#[tokio::test]
async fn test_sqlite_execute_with_params_insert() {
    let mut conn = setup().await;
    let n = conn
        .execute_with_params(
            "INSERT INTO t (id, name, score) VALUES (?, ?, ?)",
            &[
                Value::I64(1),
                Value::String("alice".into()),
                Value::F64(95.5),
            ],
        )
        .await
        .unwrap();
    assert_eq!(n, 1);
    let rows = conn
        .query("SELECT id, name FROM t WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn test_sqlite_execute_with_params_update() {
    let mut conn = setup().await;
    conn.execute("INSERT INTO t (id, name) VALUES (1, 'old')")
        .await
        .unwrap();
    let n = conn
        .execute_with_params(
            "UPDATE t SET name = ? WHERE id = ?",
            &[Value::String("new".into()), Value::I64(1)],
        )
        .await
        .unwrap();
    assert_eq!(n, 1);
    let rows = conn.query("SELECT name FROM t WHERE id = 1").await.unwrap();
    assert_eq!(rows[0].get("name").and_then(|v| v.as_str()), Some("new"));
}

#[tokio::test]
async fn test_sqlite_execute_with_params_delete() {
    let mut conn = setup().await;
    conn.execute("INSERT INTO t (id, name) VALUES (1, 'a')")
        .await
        .unwrap();
    conn.execute("INSERT INTO t (id, name) VALUES (2, 'b')")
        .await
        .unwrap();
    let n = conn
        .execute_with_params("DELETE FROM t WHERE id = ?", &[Value::I64(1)])
        .await
        .unwrap();
    assert_eq!(n, 1);
    let rows = conn.query("SELECT id FROM t").await.unwrap();
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn test_sqlite_execute_with_params_all_value_types() {
    let mut conn = setup().await;
    let n = conn
        .execute_with_params(
            "INSERT INTO t (id, name, score, flag, data) VALUES (?, ?, ?, ?, ?)",
            &[
                Value::I32(10),
                Value::String("test".into()),
                Value::F32(3.15),
                Value::Bool(true),
                Value::Bytes(vec![0x41, 0x42, 0x43]),
            ],
        )
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn test_sqlite_execute_with_params_null_and_decimal() {
    let mut conn = setup().await;
    let n = conn
        .execute_with_params(
            "INSERT INTO t (id, name, score) VALUES (?, ?, ?)",
            &[Value::I64(5), Value::Null, Value::Decimal("99.99".into())],
        )
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn test_sqlite_execute_with_params_empty_falls_back_to_execute() {
    let mut conn = setup().await;
    let n = conn
        .execute_with_params("INSERT INTO t (id, name) VALUES (1, 'fb')", &[])
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn test_sqlite_query_with_params_select() {
    let mut conn = setup().await;
    conn.execute("INSERT INTO t (id, name) VALUES (1, 'alice')")
        .await
        .unwrap();
    conn.execute("INSERT INTO t (id, name) VALUES (2, 'bob')")
        .await
        .unwrap();
    let rows = conn
        .query_with_params("SELECT id, name FROM t WHERE id = ?", &[Value::I64(1)])
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("name").and_then(|v| v.as_str()), Some("alice"));
}

#[tokio::test]
async fn test_sqlite_query_with_params_no_result() {
    let mut conn = setup().await;
    conn.execute("INSERT INTO t (id, name) VALUES (1, 'a')")
        .await
        .unwrap();
    let rows = conn
        .query_with_params("SELECT id FROM t WHERE id = ?", &[Value::I64(999)])
        .await
        .unwrap();
    assert_eq!(rows.len(), 0);
}

#[tokio::test]
async fn test_sqlite_query_values() {
    let mut conn = setup().await;
    conn.execute("INSERT INTO t (id, name) VALUES (1, 'a')")
        .await
        .unwrap();
    conn.execute("INSERT INTO t (id, name) VALUES (2, 'b')")
        .await
        .unwrap();
    let result = conn.query_values("SELECT id, name FROM t").await;
    assert!(result.is_ok());
    let qv = result.unwrap();
    assert!(qv.0.len() >= 2);
    assert_eq!(qv.1.len(), 2);
}

#[tokio::test]
async fn test_sqlite_query_values_with_params() {
    let mut conn = setup().await;
    conn.execute("INSERT INTO t (id, name) VALUES (1, 'a')")
        .await
        .unwrap();
    conn.execute("INSERT INTO t (id, name) VALUES (2, 'b')")
        .await
        .unwrap();
    let result = conn
        .query_values_with_params("SELECT id, name FROM t WHERE id >= ?", &[Value::I64(1)])
        .await;
    assert!(result.is_ok());
    let qv = result.unwrap();
    assert_eq!(qv.1.len(), 2);
}

#[tokio::test]
async fn test_sqlite_query_stream() {
    use futures::StreamExt;
    let mut conn = setup().await;
    for i in 1..=5 {
        conn.execute(&format!(
            "INSERT INTO t (id, name) VALUES ({}, 'n{}')",
            i, i
        ))
        .await
        .unwrap();
    }
    let mut stream = conn.query_stream("SELECT id, name FROM t");
    let mut count = 0;
    while let Some(item) = stream.next().await {
        assert!(item.is_ok());
        count += 1;
    }
    assert_eq!(count, 5);
}

#[tokio::test]
async fn test_sqlite_query_stream_empty() {
    use futures::StreamExt;
    let mut conn = setup().await;
    let mut stream = conn.query_stream("SELECT id FROM t");
    let mut count = 0;
    while stream.next().await.is_some() {
        count += 1;
    }
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_sqlite_execute_batch() {
    let mut conn = setup().await;
    let sqls: Vec<String> = vec![
        "INSERT INTO t (id, name) VALUES (1, 'a')".into(),
        "INSERT INTO t (id, name) VALUES (2, 'b')".into(),
        "INSERT INTO t (id, name) VALUES (3, 'c')".into(),
    ];
    let total = conn.execute_batch(&sqls).await.unwrap();
    assert_eq!(total, 3);
    let rows = conn.query("SELECT id FROM t").await.unwrap();
    assert_eq!(rows.len(), 3);
}

#[tokio::test]
async fn test_sqlite_invalid_sql_returns_err() {
    let mut conn = setup().await;
    let result = conn.execute("INVALID SQL STATEMENT").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_sqlite_query_nonexistent_table_returns_err() {
    let mut conn = setup().await;
    let result = conn.query("SELECT * FROM nonexistent_table").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_sqlite_execute_with_params_invalid_sql_returns_err() {
    let mut conn = setup().await;
    let result = conn
        .execute_with_params("INSERT INTO nonexistent (id) VALUES (?)", &[Value::I64(1)])
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_sqlite_is_connected_after_close() {
    let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
    let mut conn = pool.create().await.unwrap();
    assert!(conn.is_connected());
    conn.close().await.unwrap();
    assert!(!conn.is_connected());
}

#[tokio::test]
async fn test_sqlite_ping_after_close() {
    let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.close().await.unwrap();
    assert!(!conn.ping().await);
}

#[tokio::test]
async fn test_sqlite_query_stream_cursor() {
    use futures::StreamExt;
    let mut conn = setup().await;
    for i in 1..=10 {
        conn.execute(&format!(
            "INSERT INTO t (id, name) VALUES ({}, 'n{}')",
            i, i
        ))
        .await
        .unwrap();
    }
    let mut stream = conn.query_stream_cursor("SELECT id FROM t", 3);
    let mut count = 0;
    while let Some(item) = stream.next().await {
        assert!(item.is_ok());
        count += 1;
    }
    assert_eq!(count, 10);
}

#[tokio::test]
async fn test_sqlite_transaction_with_params() {
    let mut conn = setup().await;
    conn.begin_transaction().await.unwrap();
    conn.execute_with_params(
        "INSERT INTO t (id, name) VALUES (?, ?)",
        &[Value::I64(1), Value::String("tx".into())],
    )
    .await
    .unwrap();
    conn.execute_with_params(
        "INSERT INTO t (id, name) VALUES (?, ?)",
        &[Value::I64(2), Value::String("tx2".into())],
    )
    .await
    .unwrap();
    conn.commit().await.unwrap();
    let rows = conn.query("SELECT id FROM t").await.unwrap();
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn test_sqlite_various_column_types() {
    let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("CREATE TABLE types_t (i INTEGER, t TEXT, r REAL, b BLOB, n INTEGER)")
        .await
        .unwrap();
    conn.execute("INSERT INTO types_t VALUES (42, 'hello', 3.14, x'DEADBEEF', NULL)")
        .await
        .unwrap();
    let rows = conn.query("SELECT * FROM types_t").await.unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.get("i").and_then(|v| v.as_i64()), Some(42));
    assert_eq!(row.get("t").and_then(|v| v.as_str()), Some("hello"));
}

#[tokio::test]
async fn test_sqlite_execute_with_u_types() {
    let mut conn = setup().await;
    let n = conn
        .execute_with_params(
            "INSERT INTO t (id, name, score) VALUES (?, ?, ?)",
            &[
                Value::U8(1),
                Value::String("u8test".into()),
                Value::U16(100),
            ],
        )
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn test_sqlite_query_with_params_multiple_conditions() {
    let mut conn = setup().await;
    conn.execute("INSERT INTO t (id, name, score) VALUES (1, 'a', 10.0)")
        .await
        .unwrap();
    conn.execute("INSERT INTO t (id, name, score) VALUES (2, 'b', 20.0)")
        .await
        .unwrap();
    conn.execute("INSERT INTO t (id, name, score) VALUES (3, 'c', 30.0)")
        .await
        .unwrap();
    let rows = conn
        .query_with_params(
            "SELECT id FROM t WHERE id >= ? AND score <= ?",
            &[Value::I64(2), Value::F64(25.0)],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("id").and_then(|v| v.as_i64()), Some(2));
}
