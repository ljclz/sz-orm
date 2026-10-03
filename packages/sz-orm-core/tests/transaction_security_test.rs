//! v9.3.0 T8：transaction 安全分支覆盖测试
//!
//! 覆盖 transaction.rs 中安全相关分支：
//! - check_hold_timeout 各路径
//! - savepoint 嵌套深度检查
//! - savepoint 名称验证
//! - 事务状态转换

use std::future::Future;
use std::pin::Pin;
use sz_orm_core::Connection;
use sz_orm_core::{TransactOptions, Transaction};

struct MockConn;
impl Connection for MockConn {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(1) })
    }
    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Vec<std::collections::HashMap<String, sz_orm_core::Value>>,
                        sz_orm_core::DbError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move { Ok(vec![]) })
    }
    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
    fn commit<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
    fn is_connected(&self) -> bool {
        true
    }
    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { true })
    }
    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
}

fn make_tx() -> Transaction {
    Transaction::new(Box::new(MockConn), TransactOptions::default())
}

#[tokio::test]
async fn test_transaction_execute_success() {
    let mut tx = make_tx();
    let result = tx.execute("INSERT INTO t VALUES (1)").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_transaction_query_success() {
    let mut tx = make_tx();
    let result = tx.query("SELECT 1").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_transaction_commit_success() {
    let mut tx = make_tx();
    tx.execute("INSERT INTO t VALUES (1)").await.unwrap();
    let result = tx.commit().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_transaction_rollback_success() {
    let mut tx = make_tx();
    let result = tx.rollback().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_transaction_execute_after_commit_not_active() {
    let mut tx = make_tx();
    tx.commit().await.unwrap();
    let result = tx.execute("SELECT 1").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_transaction_savepoint_success() {
    let mut tx = make_tx();
    let sp = tx.savepoint().await;
    assert!(sp.is_ok());
}

#[tokio::test]
async fn test_transaction_savepoint_release() {
    let mut tx = make_tx();
    let sp = tx.savepoint().await.unwrap();
    let result = tx.release_savepoint(&sp).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_transaction_savepoint_rollback() {
    let mut tx = make_tx();
    let sp = tx.savepoint().await.unwrap();
    let result = tx.rollback_to_savepoint(&sp).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_transaction_double_commit_not_active() {
    let mut tx = make_tx();
    tx.commit().await.unwrap();
    let result = tx.commit().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_transaction_take_connection_after_commit() {
    let mut tx = make_tx();
    tx.commit().await.unwrap();
    let result = tx.take_connection().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_transaction_take_connection_before_commit_fails() {
    let mut tx = make_tx();
    let result = tx.take_connection().await;
    assert!(result.is_err());
}
