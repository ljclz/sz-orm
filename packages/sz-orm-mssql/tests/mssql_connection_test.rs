//! T26: MssqlConnection impl Connection 全方法集成测试（20 tests，#[ignore]）
//!
//! 需要本机 SQL Server 2019+: 从环境变量获取凭据
//! SZ_ORM_MSSQL_HOST/PORT/USER/PASSWORD/DATABASE
//! 运行: cargo test -p sz-orm-mssql --test mssql_connection_test -- --ignored

use std::sync::Arc;
use sz_orm_core::{Connection, ConnectionFactory, Value};
use sz_orm_mssql::{MssqlConnectionFactory, MssqlPoolHandle};

fn dsn() -> String {
    let host = std::env::var("SZ_ORM_MSSQL_HOST").expect("SZ_ORM_MSSQL_HOST not set");
    let port = std::env::var("SZ_ORM_MSSQL_PORT").expect("SZ_ORM_MSSQL_PORT not set");
    let user = std::env::var("SZ_ORM_MSSQL_USER").expect("SZ_ORM_MSSQL_USER not set");
    let password = std::env::var("SZ_ORM_MSSQL_PASSWORD").expect("SZ_ORM_MSSQL_PASSWORD not set");
    let database = std::env::var("SZ_ORM_MSSQL_DATABASE").expect("SZ_ORM_MSSQL_DATABASE not set");
    format!(
        "server={},{};user={};password={};database={};TrustServerCertificate=true",
        host, port, user, password, database
    )
}

async fn make_conn() -> Box<dyn Connection> {
    let handle = Arc::new(
        MssqlPoolHandle::connect(&dsn())
            .await
            .expect("connect mssql"),
    );
    let factory = MssqlConnectionFactory::new(handle);
    factory.create().await.expect("create connection")
}

async fn create_table(conn: &mut dyn Connection, name: &str) {
    conn.execute(&format!(
        "IF OBJECT_ID('{}', 'U') IS NOT NULL DROP TABLE {}",
        name, name
    ))
    .await
    .ok();
    conn.execute(&format!(
        "CREATE TABLE {} (id INT PRIMARY KEY, name NVARCHAR(255) NOT NULL)",
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
async fn test_mssql_execute_create_table() {
    let mut conn = make_conn().await;
    create_table(&mut *conn, "t_v920_exec_create").await;
    drop_table(&mut *conn, "t_v920_exec_create").await;
}

#[tokio::test]
#[ignore]
async fn test_mssql_execute_insert_returns_rows() {
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
async fn test_mssql_execute_update_modifies_row() {
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
async fn test_mssql_execute_delete_removes_row() {
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
async fn test_mssql_execute_invalid_sql_error() {
    let mut conn = make_conn().await;
    let result = conn
        .execute("CREATE TABLE t_v920_bad_syntax (id INT PRIMARY KEY,)")
        .await;
    assert!(result.is_err());
}

#[tokio::test]
#[ignore]
async fn test_mssql_query_single_row() {
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
    assert_eq!(rows[0].get("id").and_then(|v| v.as_i64()), Some(1));
    drop_table(&mut *conn, t).await;
}

#[tokio::test]
#[ignore]
async fn test_mssql_query_empty_result() {
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
async fn test_mssql_query_multiple_rows() {
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
async fn test_mssql_query_stream_rows() {
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
        assert!(row.contains_key("id"));
        count += 1;
    }
    assert_eq!(count, 2);
    drop(stream);
    drop_table(&mut *conn, t).await;
}

#[tokio::test]
#[ignore]
async fn test_mssql_execute_with_params_insert() {
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
async fn test_mssql_query_with_params_select() {
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
async fn test_mssql_query_values_select() {
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
async fn test_mssql_query_values_with_params_select() {
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
async fn test_mssql_begin_transaction_and_rollback() {
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
    let rows = conn.query(&format!("SELECT id FROM {}", t)).await.unwrap();
    assert_eq!(rows.len(), 0);
    drop_table(&mut *conn, t).await;
}

#[tokio::test]
#[ignore]
async fn test_mssql_begin_transaction_and_commit() {
    let mut conn = make_conn().await;
    let t = "t_v920_tx_commit";
    create_table(&mut *conn, t).await;
    conn.begin_transaction().await.unwrap();
    conn.execute(&format!(
        "INSERT INTO {} (id, name) VALUES (1, 'persist')",
        t
    ))
    .await
    .unwrap();
    conn.commit().await.unwrap();
    assert!(!conn.in_transaction());
    let rows = conn.query(&format!("SELECT id FROM {}", t)).await.unwrap();
    assert_eq!(rows.len(), 1);
    drop_table(&mut *conn, t).await;
}

#[tokio::test]
#[ignore]
async fn test_mssql_is_connected_true() {
    let mut conn = make_conn().await;
    assert!(conn.is_connected());
    conn.close().await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_mssql_in_transaction_flag_lifecycle() {
    let mut conn = make_conn().await;
    assert!(!conn.in_transaction());
    conn.begin_transaction().await.unwrap();
    assert!(conn.in_transaction());
    conn.rollback().await.unwrap();
    assert!(!conn.in_transaction());
}

#[tokio::test]
#[ignore]
async fn test_mssql_ping_alive_returns_true() {
    let mut conn = make_conn().await;
    assert!(conn.ping().await);
    conn.close().await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_mssql_close_then_is_connected_false() {
    let mut conn = make_conn().await;
    assert!(conn.is_connected());
    conn.close().await.unwrap();
    assert!(!conn.is_connected());
}

#[tokio::test]
#[ignore]
async fn test_mssql_execute_after_close_error() {
    let mut conn = make_conn().await;
    conn.close().await.unwrap();
    let result = conn.execute("SELECT 1").await;
    assert!(result.is_err());
}
