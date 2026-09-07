//! v6.5.0 parallel_queries demo
//!
//! 演示多表并行查询 API：对 3 张表并行查询，打印结果对齐验证。

use std::future::Future;
use std::pin::Pin;

use sz_orm_parallel::{config::ParallelQueryConfig, outcome::QueryOutcome, parallel_queries};

#[tokio::main]
async fn main() {
    println!("=== parallel_queries demo ===");

    let queries: Vec<
        Box<
            dyn FnOnce() -> Pin<
                    Box<dyn Future<Output = Result<QueryOutcome<Vec<String>>, String>> + Send>,
                > + Send,
        >,
    > = vec![
        Box::new(|| {
            Box::pin(async {
                Ok(QueryOutcome::new(
                    vec!["user1".into(), "user2".into()],
                    2,
                    10,
                ))
            })
        }),
        Box::new(|| Box::pin(async { Ok(QueryOutcome::new(vec!["order1".into()], 1, 15)) })),
        Box::new(|| {
            Box::pin(async {
                Ok(QueryOutcome::new(
                    vec!["log1".into(), "log2".into(), "log3".into()],
                    3,
                    20,
                ))
            })
        }),
    ];

    let results = parallel_queries(queries, ParallelQueryConfig::default())
        .await
        .unwrap();

    println!("结果数量: {}", results.len());
    for (i, result) in results.iter().enumerate() {
        match result {
            Ok(rows) => println!("  query[{}]: {} rows", i, rows.len()),
            Err(e) => println!("  query[{}]: error = {}", i, e),
        }
    }

    println!("=== demo 完成 ===");
}
