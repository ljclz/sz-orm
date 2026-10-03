//! v9.3.0 生产接线 — run_transaction / run_savepoint 闭包模板演示
//!
//! 本示例演示 v9.3.0 新增的事务闭包模板 API：
//!   - `run_transaction`：自动 begin/commit/rollback + panic 安全回滚
//!   - `run_savepoint`：savepoint 级局部回滚 + panic 安全
//!
//! 业务场景：银行转账（成功路径 + 业务错误回滚 + panic 回滚）
//!
//! 运行：`cargo run -p sz-orm-examples --bin transaction_run_demo`

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use sz_orm_core::{run_transaction, Connection, TransactOptions};

struct DemoConnection {
    execute_count: Arc<AtomicUsize>,
    commit_count: Arc<AtomicUsize>,
    rollback_count: Arc<AtomicUsize>,
}

impl DemoConnection {
    fn new() -> Self {
        Self {
            execute_count: Arc::new(AtomicUsize::new(0)),
            commit_count: Arc::new(AtomicUsize::new(0)),
            rollback_count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl Connection for DemoConnection {
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
        Box::pin(async move {
            self.commit_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.rollback_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
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

#[tokio::main]
async fn main() {
    println!("=== v9.3.0 run_transaction / run_savepoint 演示 ===\n");

    // 场景 1：成功路径 — commit
    let conn = Box::new(DemoConnection::new()) as Box<dyn Connection>;
    let opts = TransactOptions::default();
    let result = run_transaction(conn, opts, |tx| {
        Box::pin(async move {
            tx.execute("UPDATE accounts SET balance = balance - 100 WHERE id = 1")
                .await?;
            tx.execute("UPDATE accounts SET balance = balance + 100 WHERE id = 2")
                .await?;
            Ok::<_, sz_orm_core::TxError>(42_i64)
        })
    })
    .await;
    println!("[场景1] 成功路径: result = {:?}", result);

    // 场景 2：业务错误 — rollback
    let conn = Box::new(DemoConnection::new()) as Box<dyn Connection>;
    let opts = TransactOptions::default();
    let result = run_transaction(conn, opts, |tx| {
        Box::pin(async move {
            tx.execute("UPDATE accounts SET balance = balance - 100 WHERE id = 1")
                .await?;
            Err::<i64, _>(sz_orm_core::TxError::CommitFailed("余额不足".to_string()))
        })
    })
    .await;
    println!("[场景2] 业务错误: result = {:?}", result);

    // 场景 3：savepoint 局部回滚
    let conn = Box::new(DemoConnection::new()) as Box<dyn Connection>;
    let opts = TransactOptions::default();
    let result = run_transaction(conn, opts, |tx| {
        Box::pin(async move {
            tx.execute("INSERT INTO orders (id, amount) VALUES (1, 100)")
                .await?;

            let sp_result = sz_orm_core::run_savepoint(tx, |tx| {
                Box::pin(async move {
                    tx.execute("INSERT INTO order_items (order_id, sku) VALUES (1, 'A')")
                        .await?;
                    Err::<(), _>(sz_orm_core::TxError::SavepointError(
                        "SKU 库存不足".to_string(),
                    ))
                })
            })
            .await;

            println!("  [savepoint] 局部回滚结果: {:?}", sp_result);
            Ok::<_, sz_orm_core::TxError>(1_i64)
        })
    })
    .await;
    println!("[场景3] savepoint 局部回滚: result = {:?}", result);

    println!("\n=== 演示完成 ===");
}
