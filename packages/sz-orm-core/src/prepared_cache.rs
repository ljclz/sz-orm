//! v6.5.0 PreparedStatement 句柄缓存
//!
//! 缓存数据库侧 PreparedStatement 句柄，按连接分桶（句柄绑定连接），
//! 跨连接共享 LRU 淘汰顺序。与既有 `PlanCache`（应用侧 AST 缓存）职责分离。
//!
//! 特性：
//! - 按连接 ID 分桶（SQLx prepared statement 绑定连接）
//! - bucket 级别 LRU 淘汰（max_size_per_conn 上限）
//! - 表级精确失效（table_index 索引）
//! - 连接级失效（invalidate_conn，连接关闭时调用）
//! - 原子统计（命中/未命中/淘汰/降级/失效）
//! - 句柄以闭包形式存储（避免在 sz-orm-core 暴露 SQLx 类型）

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock;

use crate::error::DbError;
use crate::pool::QueryRows;
use crate::value::Value;

/// 连接唯一标识（连接生命周期内不变）
pub type ConnId = u64;

/// 句柄执行闭包类型（Arc 包装，支持从 RwLock 中 clone 出来后释放锁再 await）
pub type ExecuteFn = Arc<
    dyn Fn(
            &[Value],
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<QueryRows, DbError>> + Send + 'static>,
        > + Send
        + Sync,
>;

/// 缓存查找结果
#[derive(Debug)]
pub enum PreparedLookup {
    /// 缓存命中，已执行并返回结果
    Hit(QueryRows),
    /// 缓存未命中，调用方需 prepare 后调用 `store_handle`
    Miss,
}

/// PreparedStatement 句柄缓存
///
/// 按连接 ID 分桶存储句柄，bucket 级别 LRU 淘汰。
/// 句柄以闭包形式存储，避免在 sz-orm-core 暴露 SQLx 类型。
pub struct PreparedStatementCache {
    /// 按连接 ID 分桶
    buckets: RwLock<HashMap<ConnId, PreparedStatementBucket>>,
    /// 统计
    stats: PreparedStatementCacheStats,
    /// 每连接最大缓存数
    max_size_per_conn: usize,
}

/// 连接级句柄桶
struct PreparedStatementBucket {
    /// SQL hash → 条目
    entries: HashMap<u64, PreparedStatementEntry>,
    /// SQL hash → 最后使用时间（LRU 淘汰依据）
    lru: HashMap<u64, Instant>,
    /// 表名 → SQL hash 列表（表级失效索引）
    table_index: HashMap<String, Vec<u64>>,
}

/// 单个句柄条目
struct PreparedStatementEntry {
    /// 句柄执行闭包
    execute_fn: ExecuteFn,
}

/// PreparedStatement 缓存统计（原子计数器）
pub struct PreparedStatementCacheStats {
    hits: AtomicU64,
    misses: AtomicU64,
    evictions: AtomicU64,
    degradations: AtomicU64,
    invalidations: AtomicU64,
}

/// 统计快照（用于读取一致性视图）
#[derive(Debug, Clone)]
pub struct PreparedStatementCacheStatsSnapshot {
    /// 命中次数
    pub hits: u64,
    /// 未命中次数
    pub misses: u64,
    /// LRU 淘汰次数
    pub evictions: u64,
    /// 降级次数（缓存内部错误时降级直接执行）
    pub degradations: u64,
    /// 失效次数（表级 + 连接级）
    pub invalidations: u64,
    /// 当前缓存条目总数
    pub size: usize,
    /// 命中率（0.0 ~ 1.0）
    pub hit_rate: f64,
}

impl PreparedStatementCacheStats {
    fn new() -> Self {
        Self {
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
            degradations: AtomicU64::new(0),
            invalidations: AtomicU64::new(0),
        }
    }

    fn record_hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }

    fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    fn record_eviction(&self) {
        self.evictions.fetch_add(1, Ordering::Relaxed);
    }

    fn record_degradation(&self) {
        self.degradations.fetch_add(1, Ordering::Relaxed);
    }

    fn record_invalidation(&self, count: usize) {
        self.invalidations
            .fetch_add(count as u64, Ordering::Relaxed);
    }
}

impl Default for PreparedStatementCacheStats {
    fn default() -> Self {
        Self::new()
    }
}

impl PreparedStatementCache {
    /// 创建新缓存，指定每连接最大缓存数
    ///
    /// `max_size_per_conn` 默认 256（与 design.md 2.1.2 配置项一致）
    #[must_use]
    pub fn new(max_size_per_conn: usize) -> Self {
        Self {
            buckets: RwLock::new(HashMap::new()),
            stats: PreparedStatementCacheStats::new(),
            max_size_per_conn,
        }
    }

    /// 查找或执行缓存句柄
    ///
    /// - 命中：调用 `execute_fn(params)` 返回 `PreparedLookup::Hit(rows)`
    /// - 未命中：返回 `PreparedLookup::Miss`，调用方需 prepare 后调用 `store_handle`
    /// - 执行错误：返回 `Err(DbError)`，调用方应降级直接执行
    pub async fn get_or_prepare(
        &self,
        conn_id: ConnId,
        sql: &str,
        params: &[Value],
    ) -> Result<PreparedLookup, DbError> {
        let sql_hash = compute_sql_hash(sql);

        let execute_fn = {
            let buckets = self.buckets.read();
            if let Some(bucket) = buckets.get(&conn_id) {
                if let Some(entry) = bucket.entries.get(&sql_hash) {
                    Some(Arc::clone(&entry.execute_fn))
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(execute_fn) = execute_fn {
            match execute_fn(params).await {
                Ok(rows) => {
                    self.stats.record_hit();
                    self.touch_lru(conn_id, sql_hash);
                    return Ok(PreparedLookup::Hit(rows));
                }
                Err(e) => {
                    self.stats.record_degradation();
                    return Err(e);
                }
            }
        }

        self.stats.record_miss();
        Ok(PreparedLookup::Miss)
    }

    /// 存储句柄（prepare 后调用）
    ///
    /// 若 bucket 已满，LRU 淘汰最久未使用的条目。
    pub fn store_handle(
        &self,
        conn_id: ConnId,
        sql: &str,
        tables: Vec<String>,
        execute_fn: ExecuteFn,
    ) {
        let sql_hash = compute_sql_hash(sql);

        let mut buckets = self.buckets.write();
        let bucket = buckets
            .entry(conn_id)
            .or_insert_with(PreparedStatementBucket::new);

        if bucket.entries.len() >= self.max_size_per_conn && !bucket.entries.contains_key(&sql_hash)
        {
            if let Some(evict_hash) = bucket.find_lru_key() {
                bucket.entries.remove(&evict_hash);
                bucket.lru.remove(&evict_hash);
                for table_list in bucket.table_index.values_mut() {
                    table_list.retain(|&h| h != evict_hash);
                }
                self.stats.record_eviction();
            }
        }

        for table in &tables {
            bucket
                .table_index
                .entry(table.clone())
                .or_default()
                .push(sql_hash);
        }

        bucket.lru.insert(sql_hash, Instant::now());
        bucket
            .entries
            .insert(sql_hash, PreparedStatementEntry { execute_fn });
    }

    /// 表级失效：清除该表关联的所有句柄
    ///
    /// 返回失效条目数
    pub fn invalidate_table(&self, table: &str) -> usize {
        let mut buckets = self.buckets.write();
        let mut total = 0;

        for bucket in buckets.values_mut() {
            if let Some(hashes) = bucket.table_index.remove(table) {
                for hash in &hashes {
                    bucket.entries.remove(hash);
                    bucket.lru.remove(hash);
                }
                total += hashes.len();
            }
        }

        self.stats.record_invalidation(total);
        total
    }

    /// 连接级失效：清除该连接整个 bucket
    ///
    /// 连接关闭时调用，返回失效条目数
    pub fn invalidate_conn(&self, conn_id: ConnId) -> usize {
        let mut buckets = self.buckets.write();
        if let Some(bucket) = buckets.remove(&conn_id) {
            let count = bucket.entries.len();
            self.stats.record_invalidation(count);
            count
        } else {
            0
        }
    }

    /// 统计快照
    #[must_use]
    pub fn stats(&self) -> PreparedStatementCacheStatsSnapshot {
        let buckets = self.buckets.read();
        let size: usize = buckets.values().map(|b| b.entries.len()).sum();
        let hits = self.stats.hits.load(Ordering::Relaxed);
        let misses = self.stats.misses.load(Ordering::Relaxed);
        let evictions = self.stats.evictions.load(Ordering::Relaxed);
        let degradations = self.stats.degradations.load(Ordering::Relaxed);
        let invalidations = self.stats.invalidations.load(Ordering::Relaxed);
        let total = hits + misses;
        let hit_rate = if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        };

        PreparedStatementCacheStatsSnapshot {
            hits,
            misses,
            evictions,
            degradations,
            invalidations,
            size,
            hit_rate,
        }
    }

    /// 更新 LRU 时间戳
    fn touch_lru(&self, conn_id: ConnId, sql_hash: u64) {
        let mut buckets = self.buckets.write();
        if let Some(bucket) = buckets.get_mut(&conn_id) {
            bucket.lru.insert(sql_hash, Instant::now());
        }
    }

    /// 当前条目数（调试/测试用）
    #[must_use]
    pub fn len(&self) -> usize {
        let buckets = self.buckets.read();
        buckets.values().map(|b| b.entries.len()).sum()
    }

    /// 是否为空
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl PreparedStatementBucket {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            lru: HashMap::new(),
            table_index: HashMap::new(),
        }
    }

    fn find_lru_key(&self) -> Option<u64> {
        self.lru.iter().min_by_key(|(_, &t)| t).map(|(&h, _)| h)
    }
}

impl Default for PreparedStatementCache {
    fn default() -> Self {
        Self::new(256)
    }
}

/// 计算 SQL hash（归一化后）
fn compute_sql_hash(sql: &str) -> u64 {
    let normalized = sql.trim().to_lowercase();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    normalized.hash(&mut hasher);
    hasher.finish()
}

/// 从 SQL 中提取表名（简单字符串扫描，不依赖 sqlparser）
///
/// 扫描 FROM / JOIN / INTO / UPDATE 后面的表名
#[must_use]
pub fn extract_tables_simple(sql: &str) -> Vec<String> {
    let lower = sql.to_lowercase();
    let keywords = ["from", "join", "into", "update", "table"];
    let mut tables = Vec::new();
    let tokens: Vec<&str> = lower.split_whitespace().collect();

    for (i, token) in tokens.iter().enumerate() {
        if keywords.contains(token) {
            if let Some(table) = tokens.get(i + 1) {
                let clean = table
                    .trim_matches(|c: char| c == ',' || c == ';' || c == '(' || c == ')')
                    .trim();
                if !clean.is_empty()
                    && !clean.starts_with('?')
                    && !clean.starts_with('$')
                    && !clean.starts_with('(')
                {
                    tables.push(clean.to_string());
                }
            }
        }
    }

    tables.sort();
    tables.dedup();
    tables
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_execute_fn() -> ExecuteFn {
        Arc::new(|_params: &[Value]| {
            Box::pin(async {
                Ok(vec![{
                    let mut row = std::collections::HashMap::new();
                    row.insert("id".to_string(), Value::I64(1));
                    row
                }])
            })
        })
    }

    fn make_failing_execute_fn() -> ExecuteFn {
        Arc::new(|_params: &[Value]| {
            Box::pin(async { Err(DbError::QueryError("handle execution failed".into())) })
        })
    }

    #[tokio::test]
    async fn test_cache_hit() {
        let cache = PreparedStatementCache::new(256);
        let conn_id = 1;
        let sql = "SELECT * FROM users WHERE id = ?";

        let result = cache.get_or_prepare(conn_id, sql, &[]).await.unwrap();
        assert!(matches!(result, PreparedLookup::Miss));

        cache.store_handle(conn_id, sql, vec!["users".into()], make_execute_fn());

        let result = cache.get_or_prepare(conn_id, sql, &[]).await.unwrap();
        assert!(matches!(result, PreparedLookup::Hit(_)));

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache = PreparedStatementCache::new(256);
        let result = cache
            .get_or_prepare(1, "SELECT * FROM users", &[])
            .await
            .unwrap();
        assert!(matches!(result, PreparedLookup::Miss));
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().hits, 0);
    }

    #[tokio::test]
    async fn test_lru_eviction() {
        let cache = PreparedStatementCache::new(2);
        let conn_id = 1;

        cache.store_handle(
            conn_id,
            "SELECT * FROM a",
            vec!["a".into()],
            make_execute_fn(),
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
        cache.store_handle(
            conn_id,
            "SELECT * FROM b",
            vec!["b".into()],
            make_execute_fn(),
        );
        std::thread::sleep(std::time::Duration::from_millis(10));

        let _ = cache.get_or_prepare(conn_id, "SELECT * FROM a", &[]).await;
        std::thread::sleep(std::time::Duration::from_millis(10));

        cache.store_handle(
            conn_id,
            "SELECT * FROM c",
            vec!["c".into()],
            make_execute_fn(),
        );

        let stats = cache.stats();
        assert!(stats.evictions >= 1);
        assert_eq!(cache.len(), 2);
    }

    #[tokio::test]
    async fn test_invalidate_table() {
        let cache = PreparedStatementCache::new(256);
        let conn_id = 1;

        cache.store_handle(
            conn_id,
            "SELECT * FROM users",
            vec!["users".into()],
            make_execute_fn(),
        );
        cache.store_handle(
            conn_id,
            "SELECT * FROM orders",
            vec!["orders".into()],
            make_execute_fn(),
        );

        let count = cache.invalidate_table("users");
        assert_eq!(count, 1);
        assert_eq!(cache.len(), 1);

        let result = cache
            .get_or_prepare(conn_id, "SELECT * FROM users", &[])
            .await
            .unwrap();
        assert!(matches!(result, PreparedLookup::Miss));
    }

    #[tokio::test]
    async fn test_invalidate_conn() {
        let cache = PreparedStatementCache::new(256);

        cache.store_handle(1, "SELECT * FROM a", vec!["a".into()], make_execute_fn());
        cache.store_handle(1, "SELECT * FROM b", vec!["b".into()], make_execute_fn());
        cache.store_handle(2, "SELECT * FROM c", vec!["c".into()], make_execute_fn());

        let count = cache.invalidate_conn(1);
        assert_eq!(count, 2);
        assert_eq!(cache.len(), 1);

        let result = cache
            .get_or_prepare(1, "SELECT * FROM a", &[])
            .await
            .unwrap();
        assert!(matches!(result, PreparedLookup::Miss));
    }

    #[tokio::test]
    async fn test_capacity_limit() {
        let cache = PreparedStatementCache::new(3);
        let conn_id = 1;

        for i in 0..5 {
            cache.store_handle(
                conn_id,
                &format!("SELECT * FROM table_{i}"),
                vec![format!("table_{i}")],
                make_execute_fn(),
            );
        }

        assert_eq!(cache.len(), 3);
        assert!(cache.stats().evictions >= 2);
    }

    #[tokio::test]
    async fn test_concurrent_safety() {
        use std::sync::Arc;
        let cache = Arc::new(PreparedStatementCache::new(256));
        let mut handles = Vec::new();

        for thread_id in 0..4 {
            let cache = cache.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..10 {
                    let sql = format!("SELECT * FROM t_{thread_id}_{i}");
                    let _ = cache.get_or_prepare(thread_id, &sql, &[]).await;
                    cache.store_handle(thread_id, &sql, vec![], make_execute_fn());
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        assert_eq!(cache.len(), 40);
    }

    #[tokio::test]
    async fn test_degradation_on_handle_error() {
        let cache = PreparedStatementCache::new(256);
        let conn_id = 1;
        let sql = "SELECT * FROM users";

        cache.store_handle(conn_id, sql, vec![], make_failing_execute_fn());

        let result = cache.get_or_prepare(conn_id, sql, &[]).await;
        assert!(result.is_err());
        assert_eq!(cache.stats().degradations, 1);
    }

    #[tokio::test]
    async fn test_handle_not_exposed() {
        let cache = PreparedStatementCache::new(256);
        cache.store_handle(1, "SELECT 1", vec![], make_execute_fn());

        let buckets = cache.buckets.read();
        let bucket = buckets.get(&1).unwrap();
        assert!(bucket.entries.contains_key(&compute_sql_hash("SELECT 1")));
    }

    #[tokio::test]
    async fn test_normalization_hit() {
        let cache = PreparedStatementCache::new(256);
        let conn_id = 1;

        cache.store_handle(conn_id, "SELECT * FROM users", vec![], make_execute_fn());

        let result = cache
            .get_or_prepare(conn_id, "select * from users", &[])
            .await
            .unwrap();
        assert!(
            matches!(result, PreparedLookup::Hit(_)),
            "大小写归一化应命中"
        );
    }

    #[tokio::test]
    async fn test_cross_conn_isolation() {
        let cache = PreparedStatementCache::new(256);
        let sql = "SELECT * FROM users";

        cache.store_handle(1, sql, vec![], make_execute_fn());

        let result = cache.get_or_prepare(2, sql, &[]).await.unwrap();
        assert!(matches!(result, PreparedLookup::Miss), "句柄不跨连接复用");
    }

    #[tokio::test]
    async fn test_stats_accuracy() {
        let cache = PreparedStatementCache::new(256);

        cache.store_handle(1, "SELECT * FROM a", vec!["a".into()], make_execute_fn());
        cache.store_handle(1, "SELECT * FROM b", vec!["b".into()], make_execute_fn());

        let _ = cache.get_or_prepare(1, "SELECT * FROM a", &[]).await;
        let _ = cache.get_or_prepare(1, "SELECT * FROM a", &[]).await;
        let _ = cache.get_or_prepare(1, "SELECT * FROM b", &[]).await;
        let _ = cache.get_or_prepare(1, "SELECT * FROM c", &[]).await;

        cache.invalidate_table("a");

        let stats = cache.stats();
        assert_eq!(stats.hits, 3);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.invalidations, 1);
        assert_eq!(stats.size, 1);
    }

    #[test]
    fn test_extract_tables_simple() {
        let tables =
            extract_tables_simple("SELECT * FROM users JOIN orders ON users.id = orders.user_id");
        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"orders".to_string()));

        let tables = extract_tables_simple("INSERT INTO logs VALUES (1)");
        assert!(tables.contains(&"logs".to_string()));

        let tables = extract_tables_simple("UPDATE accounts SET balance = 0");
        assert!(tables.contains(&"accounts".to_string()));
    }
}
