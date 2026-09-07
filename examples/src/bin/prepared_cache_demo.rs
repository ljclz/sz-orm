//! v6.5.0 prepared_cache demo
//!
//! 演示 PreparedStatementCache：重复查询同一 SQL 模板 100 次，打印命中率。

use sz_orm_core::prepared_cache::PreparedStatementCache;
use sz_orm_core::Value;

#[tokio::main]
async fn main() {
    println!("=== prepared_cache demo ===");

    let cache = PreparedStatementCache::new(256);
    let conn_id = 1;
    let sql = "SELECT * FROM users WHERE id = ?";

    for i in 1..=100 {
        let _ = cache.get_or_prepare(conn_id, sql, &[Value::I64(i)]).await;
    }

    let stats = cache.stats();
    println!("查询次数: 100");
    println!("命中: {}", stats.hits);
    println!("未命中: {}", stats.misses);
    println!("命中率: {:.2}%", stats.hit_rate * 100.0);
    println!("=== demo 完成 ===");
}
