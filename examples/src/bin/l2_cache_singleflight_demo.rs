//! v9.3.0 生产接线 — L2Cache SingleFlight 缓存击穿防护演示
//!
//! 本示例演示 v9.3.0 新增的 L2Cache SingleFlight 协调层：
//!   - 并发相同查询 → 仅 1 个 Leader 回源，其余 Follower 共享结果
//!   - 不同查询 → 各自独立加载，互不阻塞
//!
//! 业务场景：高并发查询热门商品（缓存击穿防护）
//!
//! 运行：`cargo run -p sz-orm-examples --bin l2_cache_singleflight_demo`

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use sz_orm_core::l2_cache::L2Cache;
use sz_orm_core::Value;

#[tokio::main]
async fn main() {
    println!("=== v9.3.0 L2Cache SingleFlight 缓存击穿防护演示 ===\n");

    let cache = Arc::new(
        L2Cache::new()
            .with_default_ttl(Duration::from_secs(300))
            .with_max_size(1000),
    );

    let load_count = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for i in 0..5 {
        let cache = cache.clone();
        let lc = load_count.clone();
        handles.push(tokio::spawn(async move {
            let result = cache
                .get_or_load_query(
                    "products",
                    "SELECT * FROM products WHERE id = ?",
                    &[Value::I64(1)],
                    Duration::from_secs(300),
                    move || {
                        let lc = lc.clone();
                        async move {
                            lc.fetch_add(1, Ordering::SeqCst);
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            let mut row = HashMap::new();
                            row.insert("id".to_string(), Value::I64(1));
                            row.insert("name".to_string(), Value::String("商品A".to_string()));
                            row.insert("price".to_string(), Value::F64(99.9));
                            Ok(vec![row])
                        }
                    },
                )
                .await;
            (i, result.is_ok())
        }));
    }

    let mut all_ok = true;
    for handle in handles {
        let (id, ok) = handle.await.unwrap();
        println!("  [Follower {}] 查询结果: ok={}", id, ok);
        if !ok {
            all_ok = false;
        }
    }

    println!(
        "\n回源加载次数: {} (期望 1，SingleFlight 合并)",
        load_count.load(Ordering::SeqCst)
    );
    println!("所有查询成功: {}", all_ok);

    let cache2 = Arc::new(L2Cache::new().with_default_ttl(Duration::from_secs(300)));
    let load_count_2 = Arc::new(AtomicUsize::new(0));

    let mut handles2 = Vec::new();
    for i in 0..3 {
        let cache = cache2.clone();
        let lc = load_count_2.clone();
        let sql = if i % 2 == 0 {
            "SELECT * FROM products WHERE id = ?"
        } else {
            "SELECT * FROM products WHERE category = ?"
        };
        let param = if i % 2 == 0 {
            Value::I64(1)
        } else {
            Value::String("electronics".to_string())
        };
        handles2.push(tokio::spawn(async move {
            let lc = lc.clone();
            let result = cache
                .get_or_load_query(
                    "products",
                    sql,
                    &[param],
                    Duration::from_secs(300),
                    move || {
                        let lc = lc.clone();
                        async move {
                            lc.fetch_add(1, Ordering::SeqCst);
                            Ok(vec![HashMap::new()])
                        }
                    },
                )
                .await;
            (i, sql, result.is_ok())
        }));
    }

    for handle in handles2 {
        let (id, sql, ok) = handle.await.unwrap();
        println!("  [不同查询 {}] sql={} ok={}", id, sql, ok);
    }
    println!(
        "不同查询回源次数: {} (期望 2，不同键不合并)",
        load_count_2.load(Ordering::SeqCst)
    );

    println!("\n=== 演示完成 ===");
}