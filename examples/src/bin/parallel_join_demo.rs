//! v6.5.0 parallel_join! demo
//!
//! 演示 parallel_join! 宏：并发 3 个查询，验证语义等价 tokio::join!。

use sz_orm_parallel::parallel_join;

#[tokio::main]
async fn main() {
    println!("=== parallel_join! demo ===");

    let results: Vec<Result<i32, &str>> =
        parallel_join!(async { Ok(100) }, async { Ok(200) }, async { Ok(300) },).await;

    println!("结果数量: {}", results.len());
    for (i, r) in results.iter().enumerate() {
        println!("  query[{}]: {:?}", i, r);
    }

    println!("=== demo 完成 ===");
}
