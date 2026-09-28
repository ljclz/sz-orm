//! T22: OracleConnection impl Connection 全方法集成测试（22 tests，#[ignore]）
//!
//! 需要本机 Oracle 23ai Free: 127.0.0.1:1521/freepdb1.FALSE (sz_orm_test/SzOrmTest2026)
//! 运行: cargo test -p sz-orm-oracle --test oracle_connection_test -- --ignored

use std::sync::Arc;
use sz_orm_core::{ConnectionFactory, Connection, Value};
use sz_orm_oracle::{OracleConnectionFactory, OraclePoolHandle};

async fn make_conn() -> Box<dyn Connection> {
    let handle = Arc::new(
        OraclePoolHandle::connect(
            "sz_orm_test",
            "SzOrmTest2026",
            "127.0.0.1:1521/freepdb1.FALSE",
        )
        .expect("connect oracle"),
    );
    let factory = OracleConnectionFactory::new(handle);
    factory.create().await.expect("create connection")
}

async fn create_table(conn: &mut dyn Connection, name: &str) {
    conn.execute(&format!("DROP TABLE {}", name)).await.ok();
    conn.execute(&format!(
        "CREATE TABLE {} (id NUMBER PRIMARY KEY, name VARCHAR2(255) NOT NULL)",
        name
    ))
    .await
    .unwrap();
}

async fn drop_table(conn: &mut dyn Connection, name: &str) {
    conn.execute(&format!("DROP TABLE {}", name)).await.ok();
}

#[tokio::test]
#[ignore]
async fn test_oracle_execute_create_table() {
    let mut conn = make_conn().await;
    create_table(&mut *conn, "t_v920_exec_create").await;
    drop_table(&mut *conn, "t_v920_exec_create").await;
}

#[tokio::test]
#[ignore]
async fn test_oracle_execute_insert_returns_rows() {
    let mut conn = make_conn().await;
    let t = "t_v920_exec_insert";
    create_table(&mut *conn, t).await;
    let n = conn
        .execute(&format!("INSERT INTO {} (id, name) VALUES (1, 'alice')", t))
        .await
        .unwrap();
    assert_eq!(n, 1);
    drop_table(&mut *conn, t).await;
}

#[tokio::test]
#[ignore]
async fn test_oracle_execute_update_modifies_row() {
    let mut conn = make_conn().await;
    let t = "t_v920_exec_update";
    create_table(&mut *conn, t).await;
    conn.execute(&format!("INSERT INTO {} (id, name) VALUES (1, 'alice')", t))
        .await
        .unwrap();
    let n = conn
        .execute(&format!("UPDATE {} SET name = 'bob' WHERE id = 1", t))
        .await
        .unwrap();
    assert_eq!(n, 1);
    drop_table(&mut *conn, t).await;
}

#[tokio::test]
#[ignore]
async fn test_oracle_execute_delete_removes_row() {
    let mut conn = make_conn().await;
    let t = "t_v920_exec_delete";
    create_table(&mut *conn, t).await;
    conn.execute(&format!("INSERT INTO {} (id, name) VALUES (1, 'alice')", t))
        .await
        .unwrap();
    let n = conn
        .execute(&format!("DELETE FROM {} WHERE id = 1", t))
        .await
        .unwrap();
    assert_eq!(n, 1);
    drop_table(&mut *conn, t).await;
}

#[tokio::test]
#[ignore]
async fn test_oracle_execute_invalid_sql_error() {
    let mut conn = make_conn().await;
    let result = conn
        .execute("CREATE TABLE t_v920_bad_syntax (id NUMBER PRIMARY KEY,)")
        .await;
    assert!(result.is_err());
}

#[tokio::test]
#[ignore]
async fn test_oracle_query_single_row() {
    let mut conn = make_conn().await;
    let t = "t_v920_query_single";
    create_table(&mut *conn, t).await;
    conn.execute(&format!("INSERT INTO {} (id, name) VALUES (1, 'alice')", t))
        .await
        .unwrap();
    let rows = conn
        .query(&format!("SELECT id, name FROM {} WHERE id = 1", t))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("ID").and_then(|v| v.as_i64()), Some(1));
    drop_table(&mut *conn, t).await;
}

#[tokio::test]
#[ignore]
async fn test_oracle_query_empty_result() {
    let mut conn = make_conn().await;
    let t = "t_v920_query_empty";
    create_table(&mut *conn, t).await;
    let rows = conn
        .query(&format!("SELECT id FROM {} WHERE id = 999", t))
        .await
        .unwrap();
    assert_eq!(rows.len(), 0);
    drop_table(&mut *conn, t).await;
}

#[tokio::test]
#[ignore]
async fn test_oracle_query_multiple_rows() {
    let mut conn = make_conn().await;
    let t = "t_v920_query_multi";
    create_table(&mut *conn, t).await;
    for i in 1..=3 {
        conn.execute(&format!(
            "INSERT INTO {} (id, name) VALUES ({}, 'user{}')",
            t, i, i
        ))
        .await
        .unwrap();
    }
    let rows = conn
        .query(&format!("SELECT id FROM {} ORDER BY id", t))
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
    drop_table(&mut *conn, t).await;
}

#[tokio::test]
#[ignore]
async fn test_oracle_query_stream_rows() {
    use futures::StreamExt;
    let mut conn = make_conn().await;
    let t = "t_v920_query_stream";
    create_table(&mut *conn, t).await;
    for i in 1..=2 {
        conn.execute(&format!(
            "INSERT INTO {} (id, name) VALUES ({}, 's{}')",
            t, i, i
        ))
        .await
        .unwrap();
    }
    let sql = format!("SELECT id FROM {}", t);
    let mut stream = conn.query_stream(&sql);
    let mut count = 0;
    while let Some(item) = stream.next().await {
        let row = item.unwrap();
        assert!(row.contains_key("ID"));
        count += 1;
    }
    assert_eq!(count, 2);
    drop(stream);
    drop_table(&mut *conn, t).await;
}

#[tokio::test]
#[ignore]
async fn test_oracle_execute_with_params_insert() {
    let mut conn = make_conn().await;
    let t = "t_v920_exec_params";
    create_table(&mut *conn, t).await;
    let n = conn
        .execute_with_params(
            &format!("INSERT INTO {} (id, name) VALUES (?, ?)", t),
            &[Value::I64(42), Value::String("param_user".into())],
        )
        .await
        .unwrap();
    assert_eq!(n, 1);
    drop_table(&mut *conn, t).await;
}

#[tokio::test]
#[ignore]
async fn test_oracle_query_with_params_select() {
    let mut conn = make_conn().await;
    let t = "t_v920_query_params";
    create_table(&mut *conn, t).await;
    conn.execute(&format!("INSERT INTO {} (id, name) VALUES (1, 'alice')", t))
        .await
        .unwrap();
    conn.execute(&format!("INSERT INTO {} (id, name) VALUES (2, 'bob')", t))
        .await
        .unwrap();
    let rows = conn
        .query_with_params(
            &format!("SELECT id, name FROM {} WHERE id = ?", t),
            &[Value::I64(2)],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    drop_table(&mut *conn, t).await;
}

#[tokio::test]
#[ignore]
async fn test_oracle_query_values_select() {
    let mut conn = make_conn().await;
    let t = "t_v920_query_values";
    create_table(&mut *conn, t).await;
    conn.execute(&format!("INSERT INTO {} (id, name) VALUES (1, 'alice')", t))
        .await
        .unwrap();
    let (cols, matrix) = conn
        .query_values(&format!("SELECT id, name FROM {}", t))
        .await
        .unwrap();
    assert_eq!(cols.len(), 2);
    assert_eq!(matrix.len(), 1);
    drop_table(&mut *conn, t).await;
}

#[tokio::test]
#[ignore]
async fn test_oracle_query_values_with_params_select() {
    let mut conn = make_conn().await;
    let t = "t_v920_query_values_params";
    create_table(&mut *conn, t).await;
    conn.execute(&format!("INSERT INTO {} (id, name) VALUES (1, 'alice')", t))
        .await
        .unwrap();
    let (cols, matrix) = conn
        .query_values_with_params(
            &format!("SELECT id, name FROM {} WHERE id = ?", t),
            &[Value::I64(1)],
        )
        .await
        .unwrap();
    assert_eq!(cols.len(), 2);
    assert_eq!(matrix.len(), 1);
    drop_table(&mut *conn, t).await;
}

#[tokio::test]
#[ignore]
async fn test_oracle_begin_transaction_and_rollback() {
    let mut conn = make_conn().await;
    let t = "t_v920_tx_rollback";
    create_table(&mut *conn, t).await;
    conn.begin_transaction().await.unwrap();
    assert!(conn.in_transaction());
    conn.execute(&format!("INSERT INTO {} (id, name) VALUES (1, 'temp')", t))
        .await
        .unwrap();
    conn.rollback().await.unwrap();
    assert!(!conn.in_transaction());
    let rows = conn
        .query(&format!("SELECT id FROM {}", t))
        .await
        .unwrap();
    assert_eq!(rows.len(), 0);
    drop_table(&mut *conn, t).await;
}

#[tokio::test]
#[ignore]
async fn test_oracle_begin_transaction_and_commit() {
    let mut conn = make_conn().await;
    let t = "t_v920_tx_commit";
    create_table(&mut *conn, t).await;
    conn.begin_transaction().await.unwrap();
    conn.execute(&format!("INSERT INTO {} (id, name) VALUES (1, 'persist')", t))
        .await
        .unwrap();
    conn.commit().await.unwrap();
    assert!(!conn.in_transaction());
    let rows = conn
        .query(&format!("SELECT id FROM {}", t))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    drop_table(&mut *conn, t).await;
}

#[tokio::test]
#[ignore]
async fn test_oracle_is_connected_true() {
    let mut conn = make_conn().await;
    assert!(conn.is_connected());
    conn.close().await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_oracle_in_transaction_flag_lifecycle() {
    let mut conn = make_conn().await;
    assert!(!conn.in_transaction());
    conn.begin_transaction().await.unwrap();
    assert!(conn.in_transaction());
    conn.rollback().await.unwrap();
    assert!(!conn.in_transaction());
}

#[tokio::test]
#[ignore]
async fn test_oracle_ping_on_live_connection() {
    let mut conn = make_conn().await;
    assert!(conn.is_connected());
    let _alive = conn.ping().await;
    conn.close().await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_oracle_close_then_is_connected_false() {
    let mut conn = make_conn().await;
    assert!(conn.is_connected());
    conn.close().await.unwrap();
    assert!(!conn.is_connected());
}

#[tokio::test]
#[ignore]
async fn test_oracle_execute_after_close_error() {
    let mut conn = make_conn().await;
    conn.close().await.unwrap();
    let result = conn.execute("SELECT 1 FROM dual").await;
    assert!(result.is_err());
}

#[tokio::test]
#[ignore]
async fn test_oracle_query_after_close_error() {
    let mut conn = make_conn().await;
    conn.close().await.unwrap();
    let result = conn.query("SELECT 1 FROM dual").await;
    assert!(result.is_err());
}

#[tokio::test]
#[ignore]
async fn test_oracle_ping_after_close_returns_false() {
    let mut conn = make_conn().await;
    conn.close().await.unwrap();
    assert!(!conn.ping().await);
}