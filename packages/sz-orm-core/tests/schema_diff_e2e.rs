//! v7.3.0 任务 4.4：schema diff 端到端测试（真实两库，#[ignore]）
//!
//! 需真实 MySQL/PostgreSQL 两库，运行：cargo test --features schema-diff-viz schema_diff_e2e -- --ignored
//!
//! 生产调用点证据：
//! - schema_diff_readonly 真实两库：tests/schema_diff_e2e.rs:真实两库
//! - source_write_count = 0 真实只读：tests/schema_diff_e2e.rs:只读保证

use sz_orm_core::schema_diff_viz::schema_diff_readonly;

/// 真实两库 schema diff（需 --ignored）
#[tokio::test]
#[ignore]
async fn test_schema_diff_real_two_dbs() {
    let left = "mysql://root:test123@127.0.0.1:3306/sz_orm_test";
    let right = "mysql://root:test123@127.0.0.1:3306/sz_orm_test2";
    let report = schema_diff_readonly(left, right).await.unwrap();
    assert_eq!(report.source_write_count, 0, "真实只读连接必须 0 写入");
    assert_eq!(report.left_url, left);
    assert_eq!(report.right_url, right);
}

/// 真实 PostgreSQL 两库 schema diff（需 --ignored）
#[tokio::test]
#[ignore]
async fn test_schema_diff_real_pg_two_dbs() {
    let left = "postgres://postgres:test123@127.0.0.1:5432/sz_orm_test";
    let right = "postgres://postgres:test123@127.0.0.1:5432/sz_orm_test2";
    let report = schema_diff_readonly(left, right).await.unwrap();
    assert_eq!(report.source_write_count, 0);
}
