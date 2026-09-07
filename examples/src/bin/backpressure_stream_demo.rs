//! v6.5.0 backpressure_stream demo
//!
//! 演示 BackpressureRowStream：消费流式结果集，验证背压生效。

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use sz_orm_core::connection_ext::ConnectionExt;
use sz_orm_core::row_stream::AsyncRowStream;
use sz_orm_core::{Connection, DbError, QueryRows};
use sz_orm_stream::backpressure_stream::BackpressureRowStream;

struct MockConn;

impl Connection for MockConn {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        let _ = sql;
        Box::pin(async { Ok(0) })
    }
    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        let _ = sql;
        Box::pin(async {
            let mut rows = Vec::new();
            for i in 0..100 {
                let mut row = HashMap::new();
                row.insert("id".to_string(), sz_orm_core::Value::I64(i));
                rows.push(row);
            }
            Ok(rows)
        })
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
    println!("=== backpressure_stream demo ===");

    let mut conn = MockConn;
    let stream = conn
        .query_stream_unified("SELECT * FROM large_table", 10)
        .unwrap();
    let mut bp_stream = BackpressureRowStream::new(stream, 10);

    let mut count = 0;
    let mut backpressure_triggered = 0;
    while let Some(row) = bp_stream.next_row().await {
        if row.is_ok() {
            count += 1;
            if bp_stream.is_over_threshold() {
                backpressure_triggered += 1;
            }
            bp_stream.ack();
        }
    }

    println!("总行数: {}", count);
    println!("背压触发次数: {}", backpressure_triggered);
    println!("=== demo 完成 ===");
}
