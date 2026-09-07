//! v6.5.0 execute_batch_parallel demo
//!
//! 演示批量 DML 并行化：执行 3 条独立 DML，打印累计影响行数。

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use sz_orm_core::connection_ext::ConnectionExt;
use sz_orm_core::{Connection, DbError, QueryRows};

struct MockConn;

impl Connection for MockConn {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        let _ = sql;
        Box::pin(async { Ok(1) })
    }
    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        let _ = sql;
        Box::pin(async { Ok(vec![HashMap::new()]) })
    }
    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    fn is_connected(&self) -> bool {
        true
    }
    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { true })
    }
    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

#[tokio::main]
async fn main() {
    println!("=== execute_batch_parallel demo ===");

    let mut conn = MockConn;
    let sqls = vec![
        "INSERT INTO t VALUES (1)".to_string(),
        "INSERT INTO t VALUES (2)".to_string(),
        "INSERT INTO t VALUES (3)".to_string(),
    ];

    let count = conn.execute_batch_parallel(&sqls, 3).await.unwrap();
    println!("累计影响行数: {}", count);
    println!("=== demo 完成 ===");
}
