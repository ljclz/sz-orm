#![allow(missing_docs)]
//! 查询缓存 + 时间戳缓存（Query Cache + Timestamp Cache）
//!
//! 对标 Hibernate `QueryCache` + `UpdateTimestampsCache`。
//!
//! 缓存查询结果，当相关表被修改时自动失效。
//!
//! # 工作原理
//!
//! 1. **Query Cache**：缓存 `(SQL, params) → results` 映射
//! 2. **Timestamp Cache**：跟踪每个表的最后修改时间
//! 3. 查询时：如果 Query Cache 命中且所有相关表的 timestamp 未变，返回缓存结果
//! 4. 写入时：更新相关表的 timestamp，使依赖该表的 Query Cache 条目自动失效

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 查询缓存键
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueryCacheKey {
    pub sql: String,
    pub params_hash: u64,
}

impl QueryCacheKey {
    pub fn new(sql: &str, params_hash: u64) -> Self {
        Self {
            sql: sql.to_string(),
            params_hash,
        }
    }
}

/// 缓存的查询结果
#[derive(Debug, Clone)]
pub struct CachedQueryResult {
    pub rows: Vec<HashMap<String, crate::Value>>,
    pub cached_at: Instant,
    pub depends_on: Vec<String>,
}

/// 时间戳缓存
///
/// 跟踪每个表的最后修改时间。
#[derive(Default)]
pub struct TimestampCache {
    table_timestamps: Arc<Mutex<HashMap<String, Instant>>>,
}

impl TimestampCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// 获取表的最后修改时间
    pub fn get(&self, table: &str) -> Option<Instant> {
        self.table_timestamps.lock().unwrap().get(table).copied()
    }

    /// 更新表的修改时间
    pub fn touch(&self, table: &str) {
        self.table_timestamps
            .lock()
            .unwrap()
            .insert(table.to_string(), Instant::now());
    }

    /// 批量更新表
    pub fn touch_all(&self, tables: &[&str]) {
        let mut ts = self.table_timestamps.lock().unwrap();
        let now = Instant::now();
        for table in tables {
            ts.insert(table.to_string(), now);
        }
    }

    /// 检查查询是否过期（依赖的任何表在缓存后被修改）
    pub fn is_expired(&self, cached_at: Instant, depends_on: &[String]) -> bool {
        let ts = self.table_timestamps.lock().unwrap();
        for table in depends_on {
            if let Some(t) = ts.get(table) {
                if *t > cached_at {
                    return true;
                }
            }
        }
        false
    }

    /// 清除所有时间戳
    pub fn clear(&self) {
        self.table_timestamps.lock().unwrap().clear();
    }
}

/// 查询缓存
///
/// 缓存查询结果，配合 `TimestampCache` 实现自动失效。
pub struct QueryCache {
    cache: Arc<Mutex<HashMap<QueryCacheKey, CachedQueryResult>>>,
    timestamps: TimestampCache,
    max_entries: usize,
}

impl QueryCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            timestamps: TimestampCache::new(),
            max_entries,
        }
    }

    /// 尝试从缓存获取查询结果
    ///
    /// 如果缓存命中且未过期，返回 `Some(results)`。
    /// 否则返回 `None`。
    pub fn get(&self, key: &QueryCacheKey) -> Option<Vec<HashMap<String, crate::Value>>> {
        let cache = self.cache.lock().unwrap();
        if let Some(entry) = cache.get(key) {
            if !self
                .timestamps
                .is_expired(entry.cached_at, &entry.depends_on)
            {
                return Some(entry.rows.clone());
            }
        }
        None
    }

    /// 缓存查询结果
    ///
    /// `depends_on` 指定此查询依赖的表列表。
    /// 当这些表被修改时，缓存条目自动失效。
    pub fn put(
        &self,
        key: QueryCacheKey,
        rows: Vec<HashMap<String, crate::Value>>,
        depends_on: Vec<String>,
    ) {
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= self.max_entries {
            let oldest_key = cache
                .iter()
                .min_by_key(|(_, v)| v.cached_at)
                .map(|(k, _)| k.clone());
            if let Some(k) = oldest_key {
                cache.remove(&k);
            }
        }
        cache.insert(
            key,
            CachedQueryResult {
                rows,
                cached_at: Instant::now(),
                depends_on,
            },
        );
    }

    /// 通知表被修改（使依赖该表的缓存条目失效）
    pub fn invalidate_table(&self, table: &str) {
        self.timestamps.touch(table);
    }

    /// 批量失效
    pub fn invalidate_tables(&self, tables: &[&str]) {
        self.timestamps.touch_all(tables);
    }

    /// 清除所有缓存
    pub fn clear(&self) {
        self.cache.lock().unwrap().clear();
        self.timestamps.clear();
    }

    /// 当前缓存条目数
    pub fn len(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 获取时间戳缓存引用
    pub fn timestamps(&self) -> &TimestampCache {
        &self.timestamps
    }
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;
    use std::time::Duration;

    fn make_row(name: &str, age: i64) -> HashMap<String, Value> {
        let mut m = HashMap::new();
        m.insert("name".to_string(), Value::String(name.to_string()));
        m.insert("age".to_string(), Value::I64(age));
        m
    }

    #[test]
    fn test_query_cache_put_get() {
        let cache = QueryCache::new(100);
        let key = QueryCacheKey::new("SELECT * FROM users", 0);

        cache.put(
            key.clone(),
            vec![make_row("alice", 25)],
            vec!["users".to_string()],
        );

        let result = cache.get(&key).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["name"], Value::String("alice".to_string()));
    }

    #[test]
    fn test_query_cache_miss() {
        let cache = QueryCache::new(100);
        let key = QueryCacheKey::new("SELECT * FROM users", 0);
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_query_cache_invalidate_on_table_modify() {
        let cache = QueryCache::new(100);
        let key = QueryCacheKey::new("SELECT * FROM users", 0);

        cache.put(
            key.clone(),
            vec![make_row("alice", 25)],
            vec!["users".to_string()],
        );
        assert!(cache.get(&key).is_some());

        cache.invalidate_table("users");
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_query_cache_unaffected_table_no_invalidate() {
        let cache = QueryCache::new(100);
        let key = QueryCacheKey::new("SELECT * FROM users", 0);

        cache.put(
            key.clone(),
            vec![make_row("alice", 25)],
            vec!["users".to_string()],
        );

        cache.invalidate_table("orders");
        assert!(cache.get(&key).is_some());
    }

    #[test]
    fn test_query_cache_multi_table_dependency() {
        let cache = QueryCache::new(100);
        let key = QueryCacheKey::new(
            "SELECT u.*, o.* FROM users u JOIN orders o ON u.id = o.user_id",
            0,
        );

        cache.put(
            key.clone(),
            vec![make_row("alice", 25)],
            vec!["users".to_string(), "orders".to_string()],
        );
        assert!(cache.get(&key).is_some());

        cache.invalidate_table("orders");
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_query_cache_lru_eviction() {
        let cache = QueryCache::new(2);

        let key1 = QueryCacheKey::new("SELECT * FROM users WHERE id = 1", 0);
        let key2 = QueryCacheKey::new("SELECT * FROM users WHERE id = 2", 0);
        let key3 = QueryCacheKey::new("SELECT * FROM users WHERE id = 3", 0);

        cache.put(key1.clone(), vec![], vec![]);
        cache.put(key2.clone(), vec![], vec![]);
        cache.put(key3.clone(), vec![], vec![]);

        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_timestamp_cache_touch() {
        let ts = TimestampCache::new();
        assert!(ts.get("users").is_none());

        ts.touch("users");
        assert!(ts.get("users").is_some());
    }

    #[test]
    fn test_timestamp_cache_is_expired() {
        let ts = TimestampCache::new();
        let cached_at = Instant::now();

        std::thread::sleep(Duration::from_millis(1));
        ts.touch("users");

        assert!(ts.is_expired(cached_at, &["users".to_string()]));
        assert!(!ts.is_expired(cached_at, &["orders".to_string()]));
    }

    #[test]
    fn test_query_cache_clear() {
        let cache = QueryCache::new(100);
        let key = QueryCacheKey::new("SELECT * FROM users", 0);
        cache.put(key, vec![], vec![]);
        assert_eq!(cache.len(), 1);

        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_query_cache_different_params() {
        let cache = QueryCache::new(100);
        let key1 = QueryCacheKey::new("SELECT * FROM users WHERE age > ?", 1);
        let key2 = QueryCacheKey::new("SELECT * FROM users WHERE age > ?", 2);

        cache.put(key1.clone(), vec![make_row("alice", 25)], vec![]);
        cache.put(key2.clone(), vec![make_row("bob", 30)], vec![]);

        assert_eq!(
            cache.get(&key1).unwrap()[0]["name"],
            Value::String("alice".into())
        );
        assert_eq!(
            cache.get(&key2).unwrap()[0]["name"],
            Value::String("bob".into())
        );
    }

    #[test]
    fn test_invalidate_tables_batch() {
        let cache = QueryCache::new(100);
        let key = QueryCacheKey::new("SELECT * FROM users JOIN orders", 0);

        cache.put(
            key.clone(),
            vec![],
            vec!["users".to_string(), "orders".to_string()],
        );
        assert!(cache.get(&key).is_some());

        cache.invalidate_tables(&["users", "orders"]);
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_e2e_query_cache_hit_miss_cycle() {
        let cache = QueryCache::new(100);
        let query_count = Arc::new(Mutex::new(0));

        let execute_query = |_sql: &str, qc: Arc<Mutex<usize>>| -> Vec<HashMap<String, Value>> {
            *qc.lock().unwrap() += 1;
            vec![make_row("alice", 25)]
        };

        let key = QueryCacheKey::new("SELECT * FROM users WHERE id = ?", 1);
        let sql = "SELECT * FROM users WHERE id = ?";

        let result = execute_query(sql, Arc::clone(&query_count));
        cache.put(key.clone(), result, vec!["users".to_string()]);
        assert_eq!(*query_count.lock().unwrap(), 1);

        let cached = cache.get(&key);
        assert!(cached.is_some());
        assert_eq!(*query_count.lock().unwrap(), 1);

        cache.invalidate_table("users");

        let cached_after = cache.get(&key);
        assert!(cached_after.is_none());

        let result2 = execute_query(sql, Arc::clone(&query_count));
        cache.put(key.clone(), result2, vec!["users".to_string()]);
        assert_eq!(*query_count.lock().unwrap(), 2);

        assert!(cache.get(&key).is_some());
        assert_eq!(*query_count.lock().unwrap(), 2);
    }

    #[test]
    fn test_e2e_join_query_multi_table_invalidation() {
        let cache = QueryCache::new(100);

        let join_sql = "SELECT u.name, o.amount FROM users u JOIN orders o ON u.id = o.user_id";
        let key = QueryCacheKey::new(join_sql, 0);

        let mut row1 = HashMap::new();
        row1.insert("name".to_string(), Value::String("alice".into()));
        row1.insert("amount".to_string(), Value::F64(99.5));
        let mut row2 = HashMap::new();
        row2.insert("name".to_string(), Value::String("alice".into()));
        row2.insert("amount".to_string(), Value::F64(200.0));

        cache.put(
            key.clone(),
            vec![row1, row2],
            vec!["users".to_string(), "orders".to_string()],
        );

        assert_eq!(cache.get(&key).unwrap().len(), 2);

        cache.invalidate_table("orders");
        assert!(cache.get(&key).is_none());

        cache.put(
            key.clone(),
            vec![make_row("alice", 25)],
            vec!["users".to_string(), "orders".to_string()],
        );
        assert!(cache.get(&key).is_some());

        cache.invalidate_table("users");
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_e2e_cache_with_params_isolation() {
        let cache = QueryCache::new(100);
        let sql = "SELECT * FROM users WHERE age > ?";

        let key_young = QueryCacheKey::new(sql, 18);
        let key_old = QueryCacheKey::new(sql, 60);

        cache.put(
            key_young.clone(),
            vec![make_row("alice", 25), make_row("bob", 30)],
            vec!["users".to_string()],
        );
        cache.put(
            key_old.clone(),
            vec![make_row("charlie", 65)],
            vec!["users".to_string()],
        );

        assert_eq!(cache.get(&key_young).unwrap().len(), 2);
        assert_eq!(cache.get(&key_old).unwrap().len(), 1);

        cache.invalidate_table("users");
        assert!(cache.get(&key_young).is_none());
        assert!(cache.get(&key_old).is_none());
    }
}
