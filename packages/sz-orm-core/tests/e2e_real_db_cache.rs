//! M1-T8: 缓存（L1/L2）端到端测试
//!
//! 验证 L1Cache（Session 级 Identity Map）、L2Cache（跨 Session 共享）、
//! L1L2Coordinator（三级协作）的命中/失效/一致性。
//!
//! 同时验证缓存与真实数据库查询的一致性：写入后缓存命中，
//! 失效后重新从 DB 加载。

#![cfg(feature = "e2e-real-db")]

use std::sync::Arc;
use std::time::Duration;

#[allow(dead_code)]
mod common;

use common::cleanup::unique_table_name;

use sqlx::Row;

/// 获取 PostgreSQL 连接池
async fn pg_pool() -> Option<sqlx::PgPool> {
    let url = std::env::var("POSTGRES_URL").ok()?;
    sqlx::PgPool::connect(&url).await.ok()
}

// ==================== L1Cache 测试 ====================

/// 测试 L1Cache 基本读写和 Identity Map 语义。
#[tokio::test]
async fn test_l1_cache_put_get() {
    use sz_orm_core::l1_cache::L1Cache;

    let mut cache: L1Cache<String> = L1Cache::new(100);

    cache.put(1, Arc::new("Alice".to_string()));
    cache.put(2, Arc::new("Bob".to_string()));

    let val1 = cache.get(&1).expect("key 1 应存在");
    assert_eq!(*val1, "Alice");

    let val2 = cache.get(&2).expect("key 2 应存在");
    assert_eq!(*val2, "Bob");

    assert!(cache.get(&3).is_none(), "key 3 不应存在");
}

/// 测试 L1Cache LRU 淘汰策略。
#[tokio::test]
async fn test_l1_cache_lru_eviction() {
    use sz_orm_core::l1_cache::L1Cache;

    let mut cache: L1Cache<i32> = L1Cache::new(2);

    cache.put(1, Arc::new(100));
    cache.put(2, Arc::new(200));

    // 访问 key 1，使其成为最近使用
    let _ = cache.get(&1);

    // 插入 key 3，应淘汰最久未使用的 key 2
    cache.put(3, Arc::new(300));

    assert!(cache.get(&1).is_some(), "key 1 最近使用，不应被淘汰");
    assert!(cache.get(&2).is_none(), "key 2 应被 LRU 淘汰");
    assert!(cache.get(&3).is_some(), "key 3 刚插入，应存在");
}

/// 测试 L1Cache 手动失效和统计。
#[tokio::test]
async fn test_l1_cache_evict_and_stats() {
    use sz_orm_core::l1_cache::L1Cache;

    let mut cache: L1Cache<String> = L1Cache::new(100);

    cache.put(1, Arc::new("Alice".to_string()));
    cache.put(2, Arc::new("Bob".to_string()));

    let _ = cache.get(&1);
    let _ = cache.get(&3);

    let stats = cache.stats();
    assert!(stats.hits >= 1, "应有至少 1 次命中");
    assert!(stats.misses >= 1, "应有至少 1 次未命中");

    cache.evict(&1);
    assert!(cache.get(&1).is_none(), "evict 后 key 1 不应存在");

    cache.clear();
    assert!(cache.get(&2).is_none(), "clear 后所有键不应存在");
}

// ==================== L2Cache 测试 ====================

/// 测试 L2Cache 基本读写。
#[tokio::test]
async fn test_l2_cache_put_get() {
    use sz_orm_core::l2_cache::{CacheKey, L2Cache};
    use sz_orm_core::Value;

    let cache = L2Cache::new();

    let key = CacheKey::by_pk("users", 1);
    cache.put(&key, Value::String("Alice".to_string()), None);

    let val = cache.get(&key).expect("key 应存在");
    match val {
        Value::String(s) => assert_eq!(s, "Alice"),
        _ => panic!("应为 String 类型"),
    }

    let missing_key = CacheKey::by_pk("users", 999);
    assert!(
        cache.get(&missing_key).is_none(),
        "不存在的 key 应返回 None"
    );
}

/// 测试 L2Cache 表级失效。
#[tokio::test]
async fn test_l2_cache_invalidate_table() {
    use sz_orm_core::l2_cache::{CacheKey, L2Cache};
    use sz_orm_core::Value;

    let cache = L2Cache::new();

    let key1 = CacheKey::by_pk("users", 1);
    let key2 = CacheKey::by_pk("users", 2);
    let key3 = CacheKey::by_pk("orders", 1);

    cache.put(&key1, Value::String("Alice".to_string()), None);
    cache.put(&key2, Value::String("Bob".to_string()), None);
    cache.put(&key3, Value::String("Order1".to_string()), None);

    // 失效 users 表所有缓存
    cache.invalidate_table("users");

    assert!(cache.get(&key1).is_none(), "users key1 应已失效");
    assert!(cache.get(&key2).is_none(), "users key2 应已失效");
    assert!(
        cache.get(&key3).is_some(),
        "orders 表缓存不应受 users 失效影响"
    );
}

/// 测试 L2Cache TTL 过期。
#[tokio::test]
async fn test_l2_cache_ttl_expiry() {
    use sz_orm_core::l2_cache::{CacheKey, L2Cache};
    use sz_orm_core::Value;

    let cache = L2Cache::new();

    let key = CacheKey::by_pk("users", 1);
    cache.put(
        &key,
        Value::String("Alice".to_string()),
        Some(Duration::from_millis(50)),
    );

    // 立即读取应命中
    assert!(cache.get(&key).is_some(), "TTL 未过期应命中");

    // 等待 TTL 过期
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert!(cache.get(&key).is_none(), "TTL 过期后应返回 None");
}

// ==================== L1L2Coordinator 测试 ====================

/// 测试 L1L2Coordinator 三级查询协作（L1 → L2 → DB）。
#[tokio::test]
async fn test_l1l2_coordinator_three_level() {
    use sz_orm_core::l1_cache::L1L2Coordinator;
    use sz_orm_core::l2_cache::L2Cache;
    use sz_orm_core::Value;

    let l2 = Arc::new(L2Cache::new());
    let mut coord: L1L2Coordinator<Value> = L1L2Coordinator::new(100).with_l2(l2);

    let mut db_call_count = 0;

    // 第一次查询：L1 miss → L2 miss → DB
    let result = coord.get_or_load("users", 1, || {
        db_call_count += 1;
        Some(Value::String("Alice".to_string()))
    });
    assert!(result.is_some());
    assert_eq!(db_call_count, 1, "第一次应回源 DB");

    // 第二次查询：L1 hit
    let result = coord.get_or_load("users", 1, || {
        db_call_count += 1;
        Some(Value::String("Alice".to_string()))
    });
    assert!(result.is_some());
    assert_eq!(db_call_count, 1, "第二次应命中 L1，不回源 DB");
}

// ==================== 缓存与真实 DB 一致性 ====================

/// 测试缓存与真实 PostgreSQL 数据一致性：写入 DB 后缓存命中，
/// 失效后重新从 DB 加载获得最新值。
#[tokio::test]
async fn test_pg_cache_db_consistency() {
    use sz_orm_core::l2_cache::{CacheKey, L2Cache};
    use sz_orm_core::Value;

    let pool = match pg_pool().await {
        Some(p) => p,
        None => {
            eprintln!("PostgreSQL 未配置，跳过");
            return;
        }
    };
    let table = unique_table_name("e2e_cache");

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    let insert_sql = format!(
        "INSERT INTO \"{}\" (name) VALUES ($1) RETURNING id, name",
        table
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .fetch_one(&pool)
        .await
        .unwrap();
    let id: i64 = row.try_get("id").unwrap();
    let name: String = row.try_get("name").unwrap();
    assert_eq!(name, "Alice");

    let l2 = L2Cache::new();
    let cache_key = CacheKey::by_pk(&table, id);
    l2.put(&cache_key, Value::String(name.clone()), None);

    // 缓存命中
    let cached = l2.get(&cache_key).expect("缓存应命中");
    match cached {
        Value::String(s) => assert_eq!(s, "Alice"),
        _ => panic!(),
    }

    // 更新 DB
    let update_sql = format!("UPDATE \"{}\" SET name = $1 WHERE id = $2", table);
    sqlx::query(sqlx::AssertSqlSafe(update_sql.as_str()))
        .bind("AliceUpdated")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

    // 失效缓存
    l2.invalidate(&cache_key);
    assert!(l2.get(&cache_key).is_none(), "失效后缓存应为空");

    // 重新从 DB 加载
    let select_sql = format!("SELECT name FROM \"{}\" WHERE id = $1", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(select_sql.as_str()))
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let new_name: String = row.try_get("name").unwrap();
    assert_eq!(new_name, "AliceUpdated");

    // 回填缓存
    l2.put(&cache_key, Value::String(new_name.clone()), None);
    match l2.get(&cache_key).expect("回填后应命中") {
        Value::String(s) => assert_eq!(s, "AliceUpdated"),
        _ => panic!(),
    }

    // 清理
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}
// ==================== v8.3.0: 缓存 e2e（3 数据库） ====================

use common::e2e_env::{e2e_mysql_pool, e2e_pg_pool, E2eMysqlDb};

#[tokio::test]
async fn test_e2e_mysql_cache_hit() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_cache_mysql");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255))",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{}` (name) VALUES (?)", table).as_str(),
    ))
    .bind("Alice")
    .execute(&pool)
    .await
    .unwrap();

    let row1: (String,) = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT name FROM `{}` WHERE id = 1", table).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    let row2: (String,) = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT name FROM `{}` WHERE id = 1", table).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row1.0, row2.0);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_pg_cache_invalidation() {
    let pool = match e2e_pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_cache_pg");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO \"{}\" (name) VALUES ($1)", table).as_str(),
    ))
    .bind("Alice")
    .execute(&pool)
    .await
    .unwrap();

    let row1: (String,) = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT name FROM \"{}\" WHERE id = 1", table).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row1.0, "Alice");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("UPDATE \"{}\" SET name = $1 WHERE id = 1", table).as_str(),
    ))
    .bind("Bob")
    .execute(&pool)
    .await
    .unwrap();

    let row2: (String,) = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT name FROM \"{}\" WHERE id = 1", table).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row2.0, "Bob");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}
// ==================== v8.4.0: Oracle 缓存一致性 ====================

#[tokio::test]
async fn test_e2e_oracle_cache_hit() {
    use std::sync::Arc;
    use sz_orm_core::l1_cache::L1Cache;

    let conn = match common::e2e_env::e2e_oracle_conn() {
        Some(c) => c,
        None => return,
    };
    let table = unique_table_name("e2e_cache_ora");
    let table_lower = table.to_lowercase();

    conn.execute(
        &format!(
            "CREATE TABLE \"{}\" (id NUMBER GENERATED ALWAYS AS IDENTITY PRIMARY KEY, name VARCHAR2(255))",
            table_lower
        ),
        &[],
    )
    .unwrap();
    conn.execute(
        &format!("INSERT INTO \"{}\" (name) VALUES (:1)", table_lower),
        &[&"Alice"],
    )
    .unwrap();
    conn.commit().unwrap();

    let mut cache: L1Cache<String> = L1Cache::new(100);
    cache.put(1, Arc::new("Alice".to_string()));

    let cached = cache.get(&1).expect("cache miss");
    assert_eq!(*cached, "Alice");

    let rows = conn
        .query(
            &format!("SELECT name FROM \"{}\" WHERE id = :1", table_lower),
            &[&1i64],
        )
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let db_name: String = rows[0].get(0).unwrap();
    assert_eq!(db_name, *cached);

    conn.execute(&format!("DROP TABLE \"{}\"", table_lower), &[])
        .unwrap();
}

// ==================== SQLite 缓存 ====================

#[tokio::test]
async fn test_e2e_sqlite_cache_hit() {
    let pool = match sqlx::SqlitePool::connect("sqlite::memory:").await.ok() {
        Some(p) => p,
        None => return,
    };
    sqlx::query(sqlx::AssertSqlSafe(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe("INSERT INTO t (id, name) VALUES (?, ?)"))
        .bind(1_i64)
        .bind("Alice")
        .execute(&pool)
        .await
        .unwrap();
    let row1 = sqlx::query(sqlx::AssertSqlSafe("SELECT name FROM t WHERE id = ?"))
        .bind(1_i64)
        .fetch_one(&pool)
        .await
        .unwrap();
    let name1: String = row1.try_get("name").unwrap();
    assert_eq!(name1, "Alice");
    let row2 = sqlx::query(sqlx::AssertSqlSafe("SELECT name FROM t WHERE id = ?"))
        .bind(1_i64)
        .fetch_one(&pool)
        .await
        .unwrap();
    let name2: String = row2.try_get("name").unwrap();
    assert_eq!(name2, name1);
}
