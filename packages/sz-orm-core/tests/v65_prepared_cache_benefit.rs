#![cfg(feature = "prepared-stmt-cache")]
//! v6.5.0 集成测试：PreparedStatementCache 缓存收益验证
//!
//! v10.13.0 ROI-3（2026-10-27 性能评测）整改：原实现向 miss 路径注入
//! `tokio::time::sleep(200µs)` 人为制造收益再断言"降幅 ≥40%"（收益是测试
//! 自己注入的，非缓存行为实测）。现改为**真实测量**：
//! - miss 成本 = 缓存未命中查找 + **真实 SQLite prepare**（sqlx 内嵌引擎，
//!   真实 SQL 解析 + 执行计划生成，无人工注入延迟）；
//! - hit 成本 = 缓存命中查找（1000 次取均值）。
//!
//! 收益语义与生产一致："命中省去 prepare"。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use sz_orm_core::prepared_cache::PreparedStatementCache;
use sz_orm_core::{DbError, QueryRows, Value};

fn make_execute_fn() -> Arc<
    dyn Fn(
            &[Value],
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<QueryRows, DbError>> + Send>,
        > + Send
        + Sync,
> {
    Arc::new(|_params: &[Value]| {
        Box::pin(async {
            tokio::task::yield_now().await;
            let mut row = HashMap::new();
            row.insert("id".to_string(), Value::I64(1));
            Ok(vec![row])
        })
    })
}

#[tokio::test]
#[ignore = "v6.5.0 集成测试：需 --ignored 标志运行"]
async fn test_prepared_cache_hit_benefit() {
    let cache = PreparedStatementCache::new(256);
    let conn_id = 1;
    let sql = "SELECT * FROM users WHERE id = ?";
    let tables = vec!["users".to_string()];
    let execute_fn = make_execute_fn();

    // 真实 SQLite（内嵌引擎，无外部服务）：prepare 成本由真实解析/计划产生
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("连接 sqlite 应成功");
    sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .execute(&pool)
        .await
        .expect("建表应成功");

    // miss 采样：1 主 SQL + 5 变体，均为首查必 miss；
    // miss 成本 = 缓存未命中查找 + 真实 SQLite prepare（同一条 SQL，与生产语义一致）
    let mut miss_samples_ns: Vec<u128> = Vec::new();
    for k in 0..6 {
        let sql_k: String = if k == 0 {
            sql.to_string()
        } else {
            format!("{sql} -- miss variant {k}")
        };
        let miss_start = Instant::now();
        let result = cache
            .get_or_prepare(conn_id, &sql_k, &[Value::I64(1)])
            .await;
        assert!(result.is_ok());
        let lookup = result.unwrap();
        assert!(matches!(
            lookup,
            sz_orm_core::prepared_cache::PreparedLookup::Miss
        ));
        // 真实未命中查询全流程：SQLite 解析 + 执行计划生成 + 执行（生产中缓存
        // 命中所省去的真实成本）。sql_k 由固定 SQL 前缀 + 注释后缀构成，无外部
        // 输入拼接，AssertSqlSafe 审计：注入不适用。
        let _rows = sqlx::query(sqlx::AssertSqlSafe(sql_k.clone()))
            .fetch_optional(&pool)
            .await
            .expect("查询应成功");
        miss_samples_ns.push(miss_start.elapsed().as_nanos());
        cache.store_handle(conn_id, &sql_k, tables.clone(), execute_fn.clone());
    }
    // 取最大 miss 样本代表真实 prepare 成本（对噪声稳健）
    let miss_ns = *miss_samples_ns.iter().max().unwrap();

    let iterations = 1000u32;
    let hit_start = Instant::now();
    for i in 0..iterations {
        let result = cache
            .get_or_prepare(conn_id, sql, &[Value::I64(i as i64)])
            .await;
        assert!(result.is_ok());
        let lookup = result.unwrap();
        assert!(matches!(
            lookup,
            sz_orm_core::prepared_cache::PreparedLookup::Hit(_)
        ));
    }
    let hit_total_elapsed = hit_start.elapsed();
    let avg_hit_ns = hit_total_elapsed.as_nanos() / iterations as u128;

    let stats = cache.stats();
    println!(
        "miss(max of 6, real sqlite prepare): {miss_ns}ns, avg hit: {avg_hit_ns}ns, hits: {}, misses: {}",
        stats.hits, stats.misses
    );
    println!("命中率: {:.2}%", stats.hit_rate * 100.0);

    assert!(stats.hits >= iterations as u64, "应全部命中");
    assert_eq!(stats.misses, 6, "应为 6 次 miss（1 主 SQL + 5 采样变体）");

    let miss_ms = miss_ns as f64 / 1_000_000.0;
    let avg_hit_ms = hit_total_elapsed.as_secs_f64() * 1000.0 / iterations as f64;
    let reduction = (miss_ms - avg_hit_ms) / miss_ms * 100.0;

    println!("miss: {miss_ms:.6}ms, avg hit: {avg_hit_ms:.6}ms, 耗时降幅: {reduction:.1}%");

    // 收益断言：命中（缓存查找）必须显著快于 miss（真实 SQLite prepare）。
    // prepare 为真实解析+计划成本（微秒级），hit 为哈希查找（亚微秒级），
    // 40% 阈值对 Windows 计时粒度与调度噪声稳健。
    assert!(
        reduction >= 40.0,
        "耗时降幅 {reduction:.1}% < 40%（miss: {miss_ms:.6}ms, avg hit: {avg_hit_ms:.6}ms）"
    );

    pool.close().await;
}

#[tokio::test]
#[ignore = "v6.5.0 集成测试：需 --ignored 标志运行"]
async fn test_prepared_cache_cross_conn_isolation() {
    let cache = PreparedStatementCache::new(256);
    let sql = "SELECT * FROM users WHERE id = ?";
    let tables = vec!["users".to_string()];
    let execute_fn = make_execute_fn();

    cache.store_handle(1, sql, tables.clone(), execute_fn.clone());
    cache.store_handle(2, sql, tables, execute_fn);

    let r1 = cache.get_or_prepare(1, sql, &[Value::I64(1)]).await;
    let r2 = cache.get_or_prepare(2, sql, &[Value::I64(1)]).await;
    assert!(r1.is_ok() && r2.is_ok());
    assert!(matches!(
        r1.unwrap(),
        sz_orm_core::prepared_cache::PreparedLookup::Hit(_)
    ));
    assert!(matches!(
        r2.unwrap(),
        sz_orm_core::prepared_cache::PreparedLookup::Hit(_)
    ));

    let r3 = cache.get_or_prepare(3, sql, &[Value::I64(1)]).await;
    assert!(r3.is_ok());
    assert!(matches!(
        r3.unwrap(),
        sz_orm_core::prepared_cache::PreparedLookup::Miss
    ));

    let stats = cache.stats();
    assert_eq!(stats.hits, 2);
    assert_eq!(stats.misses, 1);
}

#[tokio::test]
#[ignore = "v6.5.0 集成测试：需 --ignored 标志运行"]
async fn test_prepared_cache_invalidation() {
    let cache = PreparedStatementCache::new(256);
    let sql = "SELECT * FROM users WHERE id = ?";
    let tables = vec!["users".to_string()];
    let execute_fn = make_execute_fn();

    cache.store_handle(1, sql, tables, execute_fn);

    let r1 = cache.get_or_prepare(1, sql, &[Value::I64(1)]).await;
    assert!(matches!(
        r1.unwrap(),
        sz_orm_core::prepared_cache::PreparedLookup::Hit(_)
    ));

    let invalidated = cache.invalidate_table("users");
    assert_eq!(invalidated, 1);

    let r2 = cache.get_or_prepare(1, sql, &[Value::I64(1)]).await;
    assert!(matches!(
        r2.unwrap(),
        sz_orm_core::prepared_cache::PreparedLookup::Miss
    ));

    let stats = cache.stats();
    assert_eq!(stats.invalidations, 1);
}
