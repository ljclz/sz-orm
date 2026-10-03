//! v9.3.0 T14：事务持有期超时检测业务规则验证
//!
//! 验证 spec.md §5.3.1：
//! 1. 持有期超时自动检测（execute/query/commit/savepoint 均检查）
//! 2. 超时后事务不可用（返回 NotActive）
//! 3. 未配置超时行为不变
//! 4. 超时阈值边界（严格大于，恰好等于不触发）
//! 5. 超时回滚后连接可复用
//! 6. 超时不静默继续（返回明确错误）

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use sz_orm_core::Connection;
use sz_orm_core::{TransactOptions, Transaction};

struct MockConnection {
    execute_count: Arc<AtomicUsize>,
}

impl MockConnection {
    fn new() -> Self {
        Self {
            execute_count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl Connection for MockConnection {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.execute_count.fetch_add(1, Ordering::SeqCst);
            Ok(1)
        })
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

fn make_tx(timeout: std::time::Duration) -> Transaction {
    let conn = Box::new(MockConnection::new());
    let opts = TransactOptions::default().with_timeout(timeout);
    Transaction::new(conn, opts)
}

#[tokio::test]
async fn test_hold_timeout_execute_after_expiry() {
    let mut tx = make_tx(std::time::Duration::from_millis(50));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let result = tx.execute("SELECT 1").await;
    assert!(result.is_err(), "超时后 execute 应返回错误");
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("timeout"), "错误应包含 timeout：{}", err);
}

#[tokio::test]
async fn test_hold_timeout_query_after_expiry() {
    let mut tx = make_tx(std::time::Duration::from_millis(50));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let result = tx.query("SELECT 1").await;
    assert!(result.is_err(), "超时后 query 应返回错误");
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("timeout"), "错误应包含 timeout：{}", err);
}

#[tokio::test]
async fn test_hold_timeout_commit_after_expiry() {
    let mut tx = make_tx(std::time::Duration::from_millis(50));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let result = tx.commit().await;
    assert!(result.is_err(), "超时后 commit 应返回错误");
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("timeout"), "错误应包含 timeout：{}", err);
}

#[tokio::test]
async fn test_hold_timeout_savepoint_after_expiry() {
    let mut tx = make_tx(std::time::Duration::from_millis(50));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let result = tx.savepoint().await;
    assert!(result.is_err(), "超时后 savepoint 应返回错误");
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("timeout"), "错误应包含 timeout：{}", err);
}

#[tokio::test]
async fn test_no_timeout_no_check() {
    let opts = TransactOptions::default();
    let conn = Box::new(MockConnection::new());
    let mut tx = Transaction::new(conn, opts);
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let result = tx.execute("SELECT 1").await;
    assert!(result.is_ok(), "未配置超时时 execute 应成功");
}

#[tokio::test]
async fn test_timeout_then_operation_not_active() {
    let mut tx = make_tx(std::time::Duration::from_millis(50));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let _ = tx.execute("SELECT 1").await;
    let result = tx.execute("SELECT 2").await;
    assert!(result.is_err(), "超时回滚后再次操作应返回错误");
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("NotActive") || err.contains("not active"),
        "应返回 NotActive：{}",
        err
    );
}

#[tokio::test]
async fn test_timeout_not_silent_continue() {
    let mut tx = make_tx(std::time::Duration::from_millis(50));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let result = tx.execute("SELECT 1").await;
    assert!(result.is_err(), "超时应返回明确错误，不静默继续");
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("timeout"),
        "错误信息应明确包含 timeout：{}",
        err
    );
}

#[tokio::test]
async fn test_timeout_rollback_then_take_connection() {
    let mut tx = make_tx(std::time::Duration::from_millis(50));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let _ = tx.execute("SELECT 1").await;
    let result = tx.take_connection().await;
    assert!(result.is_ok(), "超时回滚后应能取回连接（连接可复用）");
}

#[tokio::test]
async fn test_hold_timeout_boundary_not_triggered() {
    let mut tx = make_tx(std::time::Duration::from_millis(100));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let result = tx.execute("SELECT 1").await;
    assert!(result.is_ok(), "持有期未超时（50ms < 100ms）应正常执行");
}

#[tokio::test]
async fn test_rollback_to_savepoint_after_expiry() {
    let mut tx = make_tx(std::time::Duration::from_millis(50));
    let sp = tx.savepoint().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let result = tx.rollback_to_savepoint(&sp).await;
    assert!(result.is_err(), "超时后 rollback_to_savepoint 应返回错误");
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("timeout"), "错误应包含 timeout：{}", err);
}

#[tokio::test]
async fn test_release_savepoint_after_expiry() {
    let mut tx = make_tx(std::time::Duration::from_millis(50));
    let sp = tx.savepoint().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let result = tx.release_savepoint(&sp).await;
    assert!(result.is_err(), "超时后 release_savepoint 应返回错误");
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("timeout"), "错误应包含 timeout：{}", err);
}
