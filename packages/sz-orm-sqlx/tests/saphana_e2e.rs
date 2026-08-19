//! SAP HANA 真实驱动 E2E 测试（v4.9.0 TASK-003）
//!
//! 需要真实 SAP HANA 数据库，通过环境变量 `SAP_HANA_URL` 指定连接字符串。
//! 格式：`hdbsql://user:pass@host:port`
//!
//! 运行：`cargo test -p sz-orm-sqlx --features dialect-saphana-driver --test saphana_e2e -- --ignored`

#![cfg(feature = "dialect-saphana-driver")]

use sz_orm_core::Connection;
use sz_orm_sqlx::saphana_adapter::SapHanaConnection;

fn hana_url() -> String {
    std::env::var("SAP_HANA_URL").unwrap_or_else(|_| {
        eprintln!("[SKIP] 未设置 SAP_HANA_URL，跳过 SAP HANA E2E 测试");
        eprintln!("       设置示例：$env:SAP_HANA_URL='hdbsql://SYSTEM:password@localhost:39015'");
        String::new()
    })
}

#[tokio::test]
#[ignore = "需 SAP HANA 数据库，设置 SAP_HANA_URL 环境变量后运行 --ignored"]
async fn saphana_connect_and_query_dummy() {
    let url = hana_url();
    if url.is_empty() {
        return;
    }
    let mut conn = SapHanaConnection::connect(&url).await.expect("连接失败");
    let rows = conn
        .query("SELECT 'hello' as MSG FROM DUMMY")
        .await
        .expect("查询失败");
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("MSG"),
        Some(&sz_orm_core::Value::String("hello".to_string()))
    );
}

#[tokio::test]
#[ignore = "需 SAP HANA 数据库，设置 SAP_HANA_URL 环境变量后运行 --ignored"]
async fn saphana_create_insert_select_transaction() {
    let url = hana_url();
    if url.is_empty() {
        return;
    }
    let mut conn = SapHanaConnection::connect(&url).await.expect("连接失败");

    let table = "SZ_ORM_TASK003_E2E";
    conn.execute(&format!("DROP TABLE {table}")).await.ok();
    conn.execute(&format!(
        "CREATE TABLE {table} (ID INT PRIMARY KEY, NAME NVARCHAR(50))"
    ))
    .await
    .expect("建表失败");

    conn.begin_transaction().await.expect("开启事务失败");
    conn.execute(&format!("INSERT INTO {table} VALUES (1, 'alice')"))
        .await
        .expect("插入失败");
    conn.execute(&format!("INSERT INTO {table} VALUES (2, 'bob')"))
        .await
        .expect("插入失败");
    conn.commit().await.expect("提交失败");

    let rows = conn
        .query(&format!("SELECT NAME FROM {table} ORDER BY ID"))
        .await
        .expect("查询失败");
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].get("NAME"),
        Some(&sz_orm_core::Value::String("alice".to_string()))
    );
    assert_eq!(
        rows[1].get("NAME"),
        Some(&sz_orm_core::Value::String("bob".to_string()))
    );

    conn.begin_transaction().await.expect("开启事务失败");
    conn.execute(&format!("INSERT INTO {table} VALUES (3, 'charlie')"))
        .await
        .expect("插入失败");
    conn.rollback().await.expect("回滚失败");

    let rows = conn
        .query(&format!("SELECT COUNT(*) as CNT FROM {table}"))
        .await
        .expect("查询失败");
    assert_eq!(
        rows[0].get("CNT"),
        Some(&sz_orm_core::Value::String("2".to_string()))
    );

    conn.execute(&format!("DROP TABLE {table}"))
        .await
        .expect("清理失败");
}

#[tokio::test]
#[ignore = "需 SAP HANA 数据库，设置 SAP_HANA_URL 环境变量后运行 --ignored"]
async fn saphana_ping_and_is_connected() {
    let url = hana_url();
    if url.is_empty() {
        return;
    }
    let mut conn = SapHanaConnection::connect(&url).await.expect("连接失败");
    assert!(conn.is_connected());
    assert!(conn.ping().await, "ping 应返回 true");
}
