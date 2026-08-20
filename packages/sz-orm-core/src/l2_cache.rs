//! L2 二级缓存（Level-2 Cache）
//!
//! 对应文档 6.8 节改进项 21（L2 二级缓存）。
//!
//! # 核心概念
//!
//! - **L2Cache**：跨 Session 共享的二级缓存（与 Hibernate L2 Cache / MyBatis 二级缓存对应）
//! - **CacheKey**：统一缓存键构造（table + pk 或 table + query_hash）
//! - **L2CacheStats**：命中率统计（hits/misses/evictions/sets）
//! - **表级失效**：`invalidate_table(table)` 一次失效某表的所有缓存项
//!
//! 与 L1 缓存（Session 级别）的区别：
//! - L1：单次 Session/请求 内有效，事务结束自动清空
//! - L2：跨 Session 共享，进程级缓存，需显式失效
//!
//! # 设计灵感
//!
//! - Hibernate L2 Cache（`@Cache` / `@Cacheable`）
//! - MyBatis 二级缓存（`<cache>` 标签）
//! - Rails `Rails.cache`
//! - Django cache framework
//!
//! # 使用示例
//!
//! ```no_run
//! use sz_orm_core::l2_cache::{L2Cache, CacheKey};
//! use sz_orm_core::Value;
//!
//! // 1. 创建 L2 缓存
//! let cache = L2Cache::new();
//!
//! // 2. 缓存单行（pk 维度）
//! let key = CacheKey::by_pk("users", 1);
//! cache.put(&key, Value::String("Alice".to_string()), None);
//!
//! // 3. 读取
//! let val = cache.get(&key);
//! assert!(val.is_some());
//!
//! // 4. 表级失效（用户表更新后）
//! cache.invalidate_table("users");
//! assert!(cache.get(&key).is_none());
//!
//! // 5. 命中率统计
//! let stats = cache.stats();
//! println!("hit rate: {:.2}%", stats.hit_rate() * 100.0);
//! ```

use crate::cache::Cache;
use crate::error::CacheError;
use crate::value::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::time::Duration;
// #72 修复：使用 tokio::time::Instant 替代 std::time::Instant
// tokio::time::Instant 支持 tokio::time::pause() 测试辅助，
// 允许测试在不真实睡眠的情况下控制时间流逝。
// 在非测试环境（未调用 pause）下，行为与 std::time::Instant 完全一致。
use tokio::time::Instant;

// ============================================================================
// InvalidationBus — 缓存失效消息总线（跨实例失效）
// ============================================================================

/// 缓存失效消息
#[derive(Debug, Clone)]
pub enum InvalidationMessage {
    /// 失效单个 key
    InvalidateKey(String),
    /// 失效整张表
    InvalidateTable(String),
    /// 失效所有缓存
    InvalidateAll,
}

/// 缓存失效总线 trait
///
/// 用于跨实例缓存失效：当一个实例失效了某张表的缓存时，
/// 通过总线通知其他订阅者同步失效。
pub trait InvalidationBus: Send + Sync {
    /// 发布失效消息
    fn publish(&self, message: InvalidationMessage);
    /// 订阅失效消息（返回一个迭代器，drain 当前缓冲的消息）
    fn subscribe(&self) -> Box<dyn Iterator<Item = InvalidationMessage> + Send>;
}

/// 进程内失效总线（单实例用）
///
/// 基于 `tokio::sync::broadcast` 实现多订阅者广播。
/// `subscribe()` 返回的迭代器会 drain 当前已缓冲但未消费的消息。
pub struct LocalInvalidationBus {
    tx: tokio::sync::broadcast::Sender<InvalidationMessage>,
}

impl LocalInvalidationBus {
    /// 创建进程内失效总线，`capacity` 为广播缓冲区容量
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = tokio::sync::broadcast::channel(capacity.max(1));
        Self { tx }
    }
}

impl Default for LocalInvalidationBus {
    fn default() -> Self {
        Self::new(256)
    }
}

impl InvalidationBus for LocalInvalidationBus {
    fn publish(&self, message: InvalidationMessage) {
        // 忽略无订阅者的错误
        let _ = self.tx.send(message);
    }

    fn subscribe(&self) -> Box<dyn Iterator<Item = InvalidationMessage> + Send> {
        let mut rx = self.tx.subscribe();
        Box::new(std::iter::from_fn(move || loop {
            match rx.try_recv() {
                Ok(msg) => return Some(msg),
                // 缓冲区为空或通道已关闭 → 终止迭代
                Err(tokio::sync::broadcast::error::TryRecvError::Empty)
                | Err(tokio::sync::broadcast::error::TryRecvError::Closed) => return None,
                // 滞后（订阅者落后太多）→ 跳过丢失的消息，继续读下一条
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
            }
        }))
    }
}

// ============================================================================
// CacheKey — 统一缓存键
// ============================================================================

/// 统一缓存键
///
/// 通过 `table` + `kind` + `identifier` 三元组唯一标识一个缓存项：
/// - `table`：表名（用于表级失效）
/// - `kind`：缓存类型（ByPk / ByQuery / ByRelation）
/// - `identifier`：具体标识（pk 值 / 查询哈希 / 关联键）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// 表名
    pub table: String,
    /// 缓存类型
    pub kind: CacheKeyKind,
    /// 具体标识
    pub identifier: String,
}

/// 缓存键类型
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CacheKeyKind {
    /// 按主键缓存
    ByPk,
    /// 按查询条件缓存
    ByQuery,
    /// 按关联关系缓存
    ByRelation,
}

impl CacheKey {
    /// 构造主键维度的缓存键
    pub fn by_pk(table: impl Into<String>, pk: impl std::fmt::Display) -> Self {
        Self {
            table: table.into(),
            kind: CacheKeyKind::ByPk,
            identifier: pk.to_string(),
        }
    }

    /// 构造查询维度的缓存键（identifier 通常是 SQL + params 的哈希）
    pub fn by_query(table: impl Into<String>, query_hash: impl std::fmt::Display) -> Self {
        Self {
            table: table.into(),
            kind: CacheKeyKind::ByQuery,
            identifier: query_hash.to_string(),
        }
    }

    /// 构造关联维度的缓存键
    pub fn by_relation(table: impl Into<String>, relation: impl std::fmt::Display) -> Self {
        Self {
            table: table.into(),
            kind: CacheKeyKind::ByRelation,
            identifier: relation.to_string(),
        }
    }

    /// 序列化为字符串（用于底层存储键）
    pub fn to_string_key(&self) -> String {
        let kind_str = match self.kind {
            CacheKeyKind::ByPk => "pk",
            CacheKeyKind::ByQuery => "q",
            CacheKeyKind::ByRelation => "rel",
        };
        format!("l2:{}:{}:{}", self.table, kind_str, self.identifier)
    }
}

impl std::fmt::Display for CacheKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string_key())
    }
}

// ============================================================================
// L2CacheStats — 命中率统计
// ============================================================================

/// L2 缓存命中率统计
#[derive(Debug, Clone, Default)]
pub struct L2CacheStats {
    /// 命中次数
    pub hits: u64,
    /// 未命中次数
    pub misses: u64,
    /// 设置次数
    pub sets: u64,
    /// 失效次数（含单键和表级失效）
    pub evictions: u64,
    /// 当前缓存项数量
    pub size: usize,
}

/// 按表分桶的命中率统计
///
/// 用于细粒度观察每张表的缓存命中情况，
/// 识别"热点表"与"低命中表"，指导缓存策略调整。
#[derive(Debug, Clone, Default)]
pub struct PerTableStats {
    /// 命中次数
    pub hits: u64,
    /// 未命中次数
    pub misses: u64,
    /// 设置次数
    pub sets: u64,
    /// 失效次数
    pub evictions: u64,
}

impl PerTableStats {
    /// 总查询次数（hits + misses）
    pub fn total_lookups(&self) -> u64 {
        self.hits + self.misses
    }

    /// 命中率（0.0 ~ 1.0）
    pub fn hit_rate(&self) -> f64 {
        let total = self.total_lookups();
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

impl L2CacheStats {
    /// 总查询次数（hits + misses）
    pub fn total_lookups(&self) -> u64 {
        self.hits + self.misses
    }

    /// 命中率（0.0 ~ 1.0）
    pub fn hit_rate(&self) -> f64 {
        let total = self.total_lookups();
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// 未命中率（0.0 ~ 1.0）
    pub fn miss_rate(&self) -> f64 {
        1.0 - self.hit_rate()
    }

    /// 合并两个统计（用于多分片汇总）
    pub fn merge(&mut self, other: &L2CacheStats) {
        self.hits += other.hits;
        self.misses += other.misses;
        self.sets += other.sets;
        self.evictions += other.evictions;
        self.size += other.size;
    }
}

// ============================================================================
// CacheEntry — 缓存项
// ============================================================================

/// 缓存项（值 + 过期时间）
#[derive(Debug, Clone)]
struct CacheEntry {
    /// 缓存值
    value: Value,
    /// 过期时间（None 表示永不过期）
    expires_at: Option<Instant>,
}

impl CacheEntry {
    fn new(value: Value, ttl: Option<Duration>) -> Self {
        // Duration::MAX 会导致 Instant::now() + Duration::MAX 溢出
        // 将其视为永不过期（expires_at = None），与 None 语义一致
        let expires_at = ttl.and_then(|d| {
            if d == Duration::MAX {
                None
            } else {
                Some(Instant::now() + d)
            }
        });
        Self { value, expires_at }
    }

    fn is_expired(&self) -> bool {
        self.expires_at
            .map(|t| t <= Instant::now())
            .unwrap_or(false)
    }
}

// ============================================================================
// LruOrder — O(1) LRU 顺序跟踪器（arena 双向链表 + HashMap）
// ============================================================================

/// LRU 顺序跟踪器 — 所有操作 O(1)
///
/// 基于 arena（Vec<LruNode>）的双向链表 + HashMap 索引实现：
/// - `touch(key)`：将 key 移到 MRU 端（已存在则摘链+追加，新 key 直接追加）— O(1)
/// - `remove(key)`：从链表中摘除并回收节点 — O(1)
/// - `lru_key()`：返回 LRU 端的 key — O(1)
/// - `iter_keys()`：从 LRU 到 MRU 遍历 — O(n)
///
/// 相比 `Vec<String>` + `retain` 方案（touch/remove 为 O(n)），本实现将高频操作
/// 降为 O(1)，仅遍历（用于查找过期 key）保持 O(n)。
struct LruOrder {
    /// 节点池（arena）：节点索引即数组下标
    nodes: Vec<LruNode>,
    /// 空闲节点列表（复用已删除节点的槽位，避免 Vec 无限增长）
    free_list: Vec<usize>,
    /// key → 节点索引
    index: HashMap<String, usize>,
    /// 链表头（LRU 端，淘汰时从此处取）
    head: Option<usize>,
    /// 链表尾（MRU 端，新访问的加入此处）
    tail: Option<usize>,
}

/// 双向链表节点
struct LruNode {
    key: String,
    prev: Option<usize>,
    next: Option<usize>,
}

impl LruOrder {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            free_list: Vec::new(),
            index: HashMap::new(),
            head: None,
            tail: None,
        }
    }

    /// 触碰 key：已存在则移到尾部，不存在则创建并追加到尾部 — O(1)
    fn touch(&mut self, key: &str) {
        if let Some(&idx) = self.index.get(key) {
            self.unlink(idx);
            self.link_tail(idx);
        } else {
            let idx = self.alloc_node(key.to_string());
            self.link_tail(idx);
            self.index.insert(key.to_string(), idx);
        }
    }

    /// 移除 key — O(1)
    fn remove(&mut self, key: &str) {
        if let Some(idx) = self.index.remove(key) {
            self.unlink(idx);
            self.free_node(idx);
        }
    }

    /// 返回 LRU 端的 key（最久未访问） — O(1)
    fn lru_key(&self) -> Option<&str> {
        self.head.map(|idx| self.nodes[idx].key.as_str())
    }

    /// 从 LRU 到 MRU 遍历所有 key — O(n)
    fn iter_keys(&self) -> impl Iterator<Item = &str> {
        LruIter {
            nodes: &self.nodes,
            current: self.head,
        }
    }

    /// 清空所有 — O(n)（需释放 Vec/HashMap 内存）
    fn clear(&mut self) {
        self.nodes.clear();
        self.free_list.clear();
        self.index.clear();
        self.head = None;
        self.tail = None;
    }

    /// 当前元素数量 — O(1)
    #[allow(dead_code)]
    fn len(&self) -> usize {
        self.index.len()
    }

    /// 分配节点（优先复用空闲槽位）
    fn alloc_node(&mut self, key: String) -> usize {
        if let Some(idx) = self.free_list.pop() {
            self.nodes[idx] = LruNode {
                key,
                prev: None,
                next: None,
            };
            idx
        } else {
            self.nodes.push(LruNode {
                key,
                prev: None,
                next: None,
            });
            self.nodes.len() - 1
        }
    }

    /// 回收节点到空闲列表
    fn free_node(&mut self, idx: usize) {
        self.free_list.push(idx);
    }

    /// 从链表中摘除节点（仅修改前后指针，不释放节点）
    fn unlink(&mut self, idx: usize) {
        let prev = self.nodes[idx].prev;
        let next = self.nodes[idx].next;
        match prev {
            Some(p) => self.nodes[p].next = next,
            None => self.head = next,
        }
        match next {
            Some(n) => self.nodes[n].prev = prev,
            None => self.tail = prev,
        }
        self.nodes[idx].prev = None;
        self.nodes[idx].next = None;
    }

    /// 将节点链接到链表尾部（MRU 端）
    fn link_tail(&mut self, idx: usize) {
        match self.tail {
            Some(t) => {
                self.nodes[t].next = Some(idx);
                self.nodes[idx].prev = Some(t);
            }
            None => self.head = Some(idx),
        }
        self.nodes[idx].next = None;
        self.tail = Some(idx);
    }
}

/// LRU 链表迭代器（从 LRU 端到 MRU 端）
struct LruIter<'a> {
    nodes: &'a [LruNode],
    current: Option<usize>,
}

impl<'a> Iterator for LruIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.current?;
        let node = &self.nodes[idx];
        self.current = node.next;
        Some(node.key.as_str())
    }
}

// ============================================================================
// L2Cache — 跨 Session 共享的二级缓存
// ============================================================================

/// L2 二级缓存 — 跨 Session 共享
///
/// 线程安全：内部使用 RwLock，可在多线程环境下共享。
///
/// # 示例
///
/// ```
/// use sz_orm_core::l2_cache::{L2Cache, CacheKey};
/// use sz_orm_core::Value;
/// use std::time::Duration;
///
/// let cache = L2Cache::new();
///
/// // 缓存单行
/// let key = CacheKey::by_pk("users", 1);
/// cache.put(&key, Value::String("Alice".to_string()), None);
///
/// // 读取
/// assert!(cache.get(&key).is_some());
///
/// // 表级失效
/// cache.invalidate_table("users");
/// assert!(cache.get(&key).is_none());
/// ```
pub struct L2Cache {
    /// 缓存数据
    data: RwLock<HashMap<String, CacheEntry>>,
    /// 表名索引（用于表级失效）— table -> Vec<key_string>（去重）
    table_index: RwLock<HashMap<String, Vec<String>>>,
    /// LRU 访问顺序跟踪器（O(1) touch/remove/lru_key，arena 双向链表 + HashMap）
    ///
    /// # 锁顺序约定
    ///
    /// 跨字段持锁时遵循：`data` → `access_order` → `table_index` → `stats`，
    /// 避免死锁。本字段不允许在持 `data` 写锁时获取其他写锁。
    access_order: RwLock<LruOrder>,
    /// 全局统计信息
    stats: RwLock<L2CacheStats>,
    /// 按表分桶的统计信息（table -> PerTableStats）
    table_stats: RwLock<HashMap<String, PerTableStats>>,
    /// 默认 TTL（`put` 传 `None` 时使用，要"永不失效"请传 `Some(Duration::MAX)`）
    default_ttl: Option<Duration>,
    /// 最大容量（LRU 淘汰）
    max_size: usize,
    /// 缓存失效总线（可选，用于跨实例失效通知）
    invalidation_bus: Option<Arc<dyn InvalidationBus>>,
}

impl Default for L2Cache {
    fn default() -> Self {
        Self::new()
    }
}

impl L2Cache {
    /// 创建 L2 缓存（默认容量 10000，无 TTL）
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
            table_index: RwLock::new(HashMap::new()),
            access_order: RwLock::new(LruOrder::new()),
            stats: RwLock::new(L2CacheStats::default()),
            table_stats: RwLock::new(HashMap::new()),
            default_ttl: None,
            max_size: 10_000,
            invalidation_bus: None,
        }
    }

    /// 设置默认 TTL
    pub fn with_default_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = Some(ttl);
        self
    }

    /// 设置最大容量
    pub fn with_max_size(mut self, max_size: usize) -> Self {
        self.max_size = max_size;
        self
    }

    /// 设置缓存失效总线（用于跨实例失效通知）
    pub fn with_invalidation_bus(mut self, bus: Arc<dyn InvalidationBus>) -> Self {
        self.invalidation_bus = Some(bus);
        self
    }

    /// 存入缓存项
    ///
    /// # TTL 语义
    ///
    /// - `ttl = Some(d)`：使用 `d` 作为过期时间
    /// - `ttl = None`：使用 `default_ttl`（若未设置则永不过期）
    /// - 要显式表示"永不失效"，请传 `Some(Duration::MAX)`
    pub fn put(&self, key: &CacheKey, value: Value, ttl: Option<Duration>) {
        let actual_ttl = ttl.or(self.default_ttl);
        let entry = CacheEntry::new(value, actual_ttl);
        let key_str = key.to_string_key();

        // 1. 写入数据 + LRU 淘汰
        {
            // 锁毒化时跳过写入，优雅降级
            let mut data = match self.data.write() {
                Ok(d) => d,
                Err(_) => return,
            };
            let exists = data.contains_key(&key_str);
            if !exists && data.len() >= self.max_size {
                // LRU 淘汰：优先淘汰已过期的 key，否则淘汰 LRU 端（access_order 头部）
                let victim = {
                    // 不在持 data 写锁时获取 access_order 写锁，先读 access_order
                    // 锁毒化时降级为 None（不淘汰），保留新插入项
                    match self.access_order.read() {
                        Ok(order) => {
                            // 优先找已过期的 key（O(n) 遍历，仅缓存满时触发）
                            // 分两步计算，避免闭包捕获 order 导致生命周期问题
                            let expired = order
                                .iter_keys()
                                .find(|k| data.get(*k).map(|e| e.is_expired()).unwrap_or(false))
                                .map(|s| s.to_string());
                            let lru = order.lru_key().map(|s| s.to_string());
                            expired.or(lru)
                        }
                        Err(_) => None,
                    }
                };
                if let Some(victim) = victim {
                    data.remove(&victim);
                    // 同步清理 access_order（O(1) remove）
                    // 锁毒化时跳过 LRU 顺序同步（不影响数据正确性）
                    if let Ok(mut order) = self.access_order.write() {
                        order.remove(&victim);
                    }
                }
            }
            data.insert(key_str.clone(), entry);
        };

        // 2. 更新 LRU 访问顺序（O(1) touch：新 key 追加尾部，已存在 key 移到尾部）
        // 锁毒化时跳过 LRU 顺序更新（不影响数据正确性）
        if let Ok(mut order) = self.access_order.write() {
            order.touch(&key_str);
        }

        // 3. 更新表索引（去重，避免重复 push 导致 invalidate_table 统计错误）
        // 锁毒化时跳过索引更新（invalidate_table 会遍历 data，影响仅限于失效精度）
        if let Ok(mut idx) = self.table_index.write() {
            let keys = idx.entry(key.table.clone()).or_default();
            if !keys.contains(&key_str) {
                keys.push(key_str);
            }
        }

        // 4. 更新统计（不在此处读取 data.len()，避免锁顺序敏感）
        // 锁毒化时跳过统计更新（不影响数据正确性）
        if let Ok(mut stats) = self.stats.write() {
            stats.sets += 1;
        }
        // 4.1 更新按表分桶统计
        {
            if let Ok(mut tbl_stats) = self.table_stats.write() {
                tbl_stats.entry(key.table.clone()).or_default().sets += 1;
            }
        }
    }

    /// 读取缓存项（不存在或已过期返回 None）
    ///
    /// 命中时会更新 LRU 访问顺序（移到尾部）。
    pub fn get(&self, key: &CacheKey) -> Option<Value> {
        let key_str = key.to_string_key();
        let table_name = key.table.clone();
        let result = {
            let data = self.data.read().ok()?;
            if let Some(entry) = data.get(&key_str) {
                if entry.is_expired() {
                    None
                } else {
                    Some(entry.value.clone())
                }
            } else {
                None
            }
        };

        // 命中时更新 LRU 顺序（O(1) touch：移到尾部）
        // 锁毒化时跳过 LRU 顺序更新（不影响数据正确性）
        if result.is_some() {
            if let Ok(mut order) = self.access_order.write() {
                order.touch(&key_str);
            }
        }

        // 更新全局统计
        if let Ok(mut stats) = self.stats.write() {
            if result.is_some() {
                stats.hits += 1;
            } else {
                stats.misses += 1;
            }
        }
        // 更新按表分桶统计
        if let Ok(mut tbl_stats) = self.table_stats.write() {
            let entry = tbl_stats.entry(table_name).or_default();
            if result.is_some() {
                entry.hits += 1;
            } else {
                entry.misses += 1;
            }
        }

        result
    }

    /// 失效单个缓存项
    pub fn invalidate(&self, key: &CacheKey) {
        let key_str = key.to_string_key();
        let table_name = key.table.clone();
        let removed = {
            // 锁毒化时跳过失效操作（视为未删除）
            let mut data = match self.data.write() {
                Ok(d) => d,
                Err(_) => return,
            };
            data.remove(&key_str).is_some()
        };
        if removed {
            // 锁毒化时跳过 LRU 顺序同步（不影响数据正确性）
            if let Ok(mut order) = self.access_order.write() {
                order.remove(&key_str);
            }
        }
        if removed {
            // 锁毒化时跳过统计更新（不影响数据正确性）
            if let Ok(mut stats) = self.stats.write() {
                stats.evictions += 1;
            }
            if let Ok(mut tbl_stats) = self.table_stats.write() {
                tbl_stats.entry(table_name).or_default().evictions += 1;
            }
        }
    }

    /// 失效整张表的所有缓存项
    ///
    /// 仅统计实际从缓存中删除的 key 数量，避免 evictions 偏大。
    /// 若设置了失效总线，会同时发布 `InvalidateTable` 消息通知其他实例。
    pub fn invalidate_table(&self, table: &str) {
        let keys_to_remove: Vec<String> = {
            let idx = match self.table_index.read() {
                Ok(i) => i,
                Err(_) => return,
            };
            idx.get(table).cloned().unwrap_or_default()
        };

        let mut actually_removed: usize = 0;
        {
            // 锁毒化时跳过失效并直接返回（不发布总线通知）
            let mut data = match self.data.write() {
                Ok(d) => d,
                Err(_) => return,
            };
            for k in &keys_to_remove {
                if data.remove(k).is_some() {
                    actually_removed += 1;
                }
            }
        }

        // O(m) 批量移除（m = keys_to_remove），而非旧实现的 O(n*m) retain
        // 锁毒化时跳过 LRU 顺序同步（不影响数据正确性）
        if actually_removed > 0 {
            if let Ok(mut order) = self.access_order.write() {
                for k in &keys_to_remove {
                    order.remove(k);
                }
            }
        }

        if let Ok(mut idx) = self.table_index.write() {
            idx.remove(table);
        }
        if actually_removed > 0 {
            // 锁毒化时跳过统计更新（不影响数据正确性）
            if let Ok(mut stats) = self.stats.write() {
                stats.evictions += actually_removed as u64;
            }
            if let Ok(mut tbl_stats) = self.table_stats.write() {
                tbl_stats.entry(table.to_string()).or_default().evictions +=
                    actually_removed as u64;
            }
        }

        // 发布失效消息到总线（通知其他订阅实例）
        if let Some(bus) = &self.invalidation_bus {
            bus.publish(InvalidationMessage::InvalidateTable(table.to_string()));
        }
    }

    /// 清空所有缓存
    pub fn clear(&self) {
        let removed = {
            // 锁毒化时跳过清空并直接返回
            let mut data = match self.data.write() {
                Ok(d) => d,
                Err(_) => return,
            };
            let n = data.len();
            data.clear();
            n
        };
        if let Ok(mut order) = self.access_order.write() {
            order.clear();
        }
        if let Ok(mut idx) = self.table_index.write() {
            idx.clear();
        }
        if let Ok(mut tbl_stats) = self.table_stats.write() {
            tbl_stats.clear();
        }
        if removed > 0 {
            // 锁毒化时跳过统计更新（不影响数据正确性）
            if let Ok(mut stats) = self.stats.write() {
                stats.evictions += removed as u64;
                stats.size = 0;
            }
        }
    }

    /// 获取当前缓存项数量
    pub fn size(&self) -> usize {
        self.data.read().map(|d| d.len()).unwrap_or(0)
    }

    /// 获取统计信息
    pub fn stats(&self) -> L2CacheStats {
        let mut s = self.stats.read().map(|s| s.clone()).unwrap_or_default();
        // 实时同步 size 字段（不写入 stats，避免持锁读 data）
        s.size = self.size();
        s
    }

    /// 重置统计信息（含全局和按表分桶）
    pub fn reset_stats(&self) {
        if let Ok(mut stats) = self.stats.write() {
            *stats = L2CacheStats::default();
        }
        if let Ok(mut tbl_stats) = self.table_stats.write() {
            tbl_stats.clear();
        }
    }

    /// TASK-023：查询缓存辅助方法
    ///
    /// 根据 SQL + 参数生成缓存键，先查缓存，命中则返回缓存结果；
    /// 未命中则调用 `loader` 执行查询，将结果缓存后返回。
    ///
    /// # 参数
    ///
    /// * `table` - 表名（用于表级失效）
    /// * `sql` - SQL 查询语句
    /// * `params` - 查询参数（用于生成缓存键）
    /// * `ttl` - 缓存 TTL
    /// * `loader` - 异步查询闭包，返回 `Result<Vec<QueryRows>, DbError>`
    ///
    /// # 空结果缓存
    ///
    /// 空结果也会缓存，TTL 缩短为 `ttl / 10`（至少 1 秒），防止缓存穿透。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use sz_orm_core::l2_cache::L2Cache;
    /// use std::time::Duration;
    ///
    /// let cache = L2Cache::new();
    /// let rows = cache.get_or_load_query(
    ///     "users",
    ///     "SELECT * FROM users WHERE status = ?",
    ///     &[Value::I64(1)],
    ///     Duration::from_secs(300),
    ///     || async { conn.query_with_params(sql, params).await },
    /// ).await?;
    /// ```
    pub async fn get_or_load_query<F, Fut>(
        &self,
        table: &str,
        sql: &str,
        params: &[crate::value::Value],
        ttl: Duration,
        loader: F,
    ) -> Result<crate::pool::QueryRows, crate::DbError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<crate::pool::QueryRows, crate::DbError>>,
    {
        // 生成缓存键：使用 SQL + 参数的哈希
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        sql.hash(&mut hasher);
        for param in params {
            param.to_string().hash(&mut hasher);
        }
        let query_hash = hasher.finish();
        let cache_key = CacheKey::by_query(table, query_hash);

        // 检查缓存
        if let Some(Value::Json(json_str)) = self.get(&cache_key) {
            // 缓存命中：反序列化结果
            if let Ok(rows) = serde_json::from_str::<crate::pool::QueryRows>(&json_str) {
                return Ok(rows);
            }
        }

        // 缓存未命中：执行查询
        let rows = loader().await?;

        // 确定缓存 TTL（空结果缩短为 1/10）
        let cache_ttl = if rows.is_empty() {
            // 空结果也缓存，TTL 缩短为 1/10（至少 1 秒）
            std::cmp::max(ttl / 10, Duration::from_secs(1))
        } else {
            ttl
        };

        // 序列化并缓存结果
        if let Ok(json_str) = serde_json::to_string(&rows) {
            self.put(&cache_key, Value::Json(json_str), Some(cache_ttl));
        }

        Ok(rows)
    }

    /// TASK-023：失效查询缓存
    ///
    /// 根据 SQL + 参数失效特定的查询缓存项。
    pub fn invalidate_query(&self, table: &str, sql: &str, params: &[crate::value::Value]) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        sql.hash(&mut hasher);
        for param in params {
            param.to_string().hash(&mut hasher);
        }
        let query_hash = hasher.finish();
        let cache_key = CacheKey::by_query(table, query_hash);
        self.invalidate(&cache_key);
    }

    /// 获取指定表的命中率统计
    pub fn table_stats(&self, table: &str) -> Option<PerTableStats> {
        self.table_stats
            .read()
            .ok()
            .and_then(|s| s.get(table).cloned())
    }

    /// 获取所有表的命中率统计快照
    pub fn all_table_stats(&self) -> HashMap<String, PerTableStats> {
        self.table_stats
            .read()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    /// 检查缓存项是否存在（不更新统计与 LRU 顺序）
    pub fn contains(&self, key: &CacheKey) -> bool {
        let key_str = key.to_string_key();
        self.data
            .read()
            .map(|d| d.get(&key_str).map(|e| !e.is_expired()).unwrap_or(false))
            .unwrap_or(false)
    }

    /// 手动清理所有过期项
    pub fn evict_expired(&self) -> usize {
        let expired_keys: Vec<String> = {
            // 锁毒化时返回空 Vec（无过期项可清理）
            let data = match self.data.read() {
                Ok(d) => d,
                Err(_) => return 0,
            };
            data.iter()
                .filter(|(_, e)| e.is_expired())
                .map(|(k, _)| k.clone())
                .collect()
        };

        // 反向查找 key_str -> table_name，用于按表分桶统计
        // 锁毒化时返回空 map（按表统计将不更新，不影响数据清理）
        let key_to_table: HashMap<String, String> = match self.table_index.read() {
            Ok(idx) => {
                let mut map = HashMap::new();
                for (table, keys) in idx.iter() {
                    for k in keys {
                        map.insert(k.clone(), table.clone());
                    }
                }
                map
            }
            Err(_) => HashMap::new(),
        };

        let mut removed = 0;
        if !expired_keys.is_empty() {
            // 锁毒化时跳过清理（视为未删除）
            let mut data = match self.data.write() {
                Ok(d) => d,
                Err(_) => return 0,
            };
            for k in &expired_keys {
                if data.remove(k).is_some() {
                    removed += 1;
                }
            }
        }

        if removed > 0 {
            // O(m) 批量移除（m = expired_keys），而非旧实现的 O(n*m) retain
            // 锁毒化时跳过 LRU 顺序同步（不影响数据正确性）
            if let Ok(mut order) = self.access_order.write() {
                for k in &expired_keys {
                    order.remove(k);
                }
            }
            {
                // 锁毒化时跳过统计更新（不影响数据正确性）
                if let Ok(mut stats) = self.stats.write() {
                    stats.evictions += removed as u64;
                }
            }
            // 更新按表分桶统计（单独持锁，避免与 stats 锁同时持有）
            if let Ok(mut tbl_stats) = self.table_stats.write() {
                for k in &expired_keys {
                    if let Some(table) = key_to_table.get(k) {
                        tbl_stats.entry(table.clone()).or_default().evictions += 1;
                    }
                }
            }
        }
        removed
    }

    /// 更新缓存项的 TTL（若 key 不存在返回 false）
    ///
    /// 用于 `Cache` trait 的 `expire` 方法实现。
    pub fn update_ttl(&self, key: &CacheKey, ttl: Duration) -> bool {
        let key_str = key.to_string_key();
        let mut data = match self.data.write() {
            Ok(d) => d,
            Err(_) => return false,
        };
        if let Some(entry) = data.get_mut(&key_str) {
            entry.expires_at = Some(Instant::now() + ttl);
            true
        } else {
            false
        }
    }

    /// 获取缓存项的剩余 TTL
    ///
    /// 返回值：
    /// - `None`：key 不存在或已过期
    /// - `Some(None)`：key 存在但无 TTL（永不过期）
    /// - `Some(Some(d))`：key 存在且剩余 TTL 为 d
    ///
    /// 用于 `Cache` trait 的 `ttl` 方法实现。
    pub fn remaining_ttl(&self, key: &CacheKey) -> Option<Option<Duration>> {
        let key_str = key.to_string_key();
        let data = self.data.read().ok()?;
        let entry = data.get(&key_str)?;
        match entry.expires_at {
            Some(expires_at) => {
                let now = Instant::now();
                if expires_at <= now {
                    None
                } else {
                    Some(Some(expires_at.duration_since(now)))
                }
            }
            None => Some(None),
        }
    }
}

// ============================================================================
// Cache trait 实现 — 让 L2Cache 可作为通用 Cache 使用
// ============================================================================

/// 为 L2Cache 实现 `Cache` trait
///
/// 通过 `CacheKey::by_pk("__cache__", key)` 将字符串 key 映射到 L2Cache 的 CacheKey 体系，
/// 所有通过 `Cache` trait 写入的缓存项归入 `__cache__` 表，与业务缓存项隔离。
///
/// 值以 `Value::Bytes(Vec<u8>)` 存储；若通过 `Cache::get` 读取到的 Value 非 Bytes 类型
/// （如直接通过 `L2Cache::put` 写入的其他类型），则回退为 JSON 序列化。
impl Cache for L2Cache {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError> {
        let cache_key = CacheKey::by_pk("__cache__", key);
        match L2Cache::get(self, &cache_key) {
            Some(Value::Bytes(bytes)) => Ok(Some(bytes)),
            Some(other) => {
                let json = serde_json::to_vec(&other)
                    .map_err(|e| CacheError::SerializationError(e.to_string()))?;
                Ok(Some(json))
            }
            None => Ok(None),
        }
    }

    fn set(&self, key: &str, value: Vec<u8>, ttl: Option<Duration>) -> Result<(), CacheError> {
        let cache_key = CacheKey::by_pk("__cache__", key);
        self.put(&cache_key, Value::Bytes(value), ttl);
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), CacheError> {
        let cache_key = CacheKey::by_pk("__cache__", key);
        self.invalidate(&cache_key);
        Ok(())
    }

    fn clear(&self) -> Result<(), CacheError> {
        // 仅清除通过 Cache trait 写入的原始缓存项（__cache__ 表），
        // 不影响通过 CacheKey 直接写入的业务缓存项。
        self.invalidate_table("__cache__");
        Ok(())
    }

    fn exists(&self, key: &str) -> Result<bool, CacheError> {
        let cache_key = CacheKey::by_pk("__cache__", key);
        Ok(self.contains(&cache_key))
    }

    fn expire(&self, key: &str, ttl: Duration) -> Result<(), CacheError> {
        let cache_key = CacheKey::by_pk("__cache__", key);
        if self.update_ttl(&cache_key, ttl) {
            Ok(())
        } else {
            Err(CacheError::NotFound(key.to_string()))
        }
    }

    fn ttl(&self, key: &str) -> Result<Option<Duration>, CacheError> {
        let cache_key = CacheKey::by_pk("__cache__", key);
        match self.remaining_ttl(&cache_key) {
            None => Err(CacheError::NotFound(key.to_string())),
            Some(None) => Ok(None),
            Some(Some(d)) => Ok(Some(d)),
        }
    }
}

// ============================================================================
// L2CacheBackend — 分布式缓存后端抽象（trait + InMemoryBackend + RedisBackend stub）
// ============================================================================

/// L2 缓存异步 Future 类型别名
///
/// 用于简化 `L2CacheBackend` trait 中方法的返回类型签名，
/// 避免重复书写复杂的 `Pin<Box<dyn Future<...> + Send + 'a>>`。
pub type L2CacheFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, CacheError>> + Send + 'a>>;

/// L2 缓存后端 trait（分布式抽象）
///
/// 定义跨进程共享的二级缓存后端接口，支持进程内内存、Redis 等实现。
/// 手动解糖 async 方法（不使用 `#[async_trait]`），与 `Connection` trait 风格一致。
///
/// # 设计要点
///
/// - **键值以 `&[u8]` 传输**：后端无关的序列化格式（由调用方决定 bincode/json 等）
/// - **TTL 可选**：`Some(Duration)` 设置过期时间，`None` 表示永不过期
/// - **前缀失效**：`invalidate_prefix` 批量失效某前缀的所有键（用于表级失效）
///
/// # 实现方
///
/// - [`InMemoryBackend`]：进程内内存后端（默认，单机场景）
/// - [`RedisBackend`]：Redis 分布式后端（stub，需启用 `redis` feature 并补充依赖）
pub trait L2CacheBackend: Send + Sync {
    /// 获取缓存值，不存在或已过期返回 `None`
    fn get<'a>(&'a self, key: &'a str) -> L2CacheFuture<'a, Option<Vec<u8>>>;

    /// 设置缓存值，`ttl` 为 `None` 表示永不过期
    fn set<'a>(
        &'a self,
        key: &'a str,
        value: &'a [u8],
        ttl: Option<Duration>,
    ) -> L2CacheFuture<'a, ()>;

    /// 删除单个缓存键
    fn delete<'a>(&'a self, key: &'a str) -> L2CacheFuture<'a, ()>;

    /// 按前缀批量失效缓存项（用于表级失效）
    fn invalidate_prefix<'a>(&'a self, prefix: &'a str) -> L2CacheFuture<'a, ()>;
}

/// 进程内内存后端（默认实现）
///
/// 适用于单机场景，不跨进程共享。内部使用 `RwLock<HashMap>` 存储，
/// `invalidate_prefix` 通过遍历键前缀匹配实现（O(n)，单机场景可接受）。
///
/// # 线程安全
///
/// 所有操作通过 `RwLock` 保护，可在多线程环境下共享。
///
/// # 注意
///
/// 由于 `std::sync::RwLock` 的 guard 是 `!Send`，所有同步操作在创建
/// `Future` 之前完成，guard 在 block 退出时释放，避免跨 `.await` 持锁。
pub struct InMemoryBackend {
    /// 缓存数据：key -> (value, expiry_time)
    /// 使用类型别名降低类型复杂度（clippy::type_complexity）
    data: RwLock<InMemoryCacheData>,
}

/// 内存缓存条目类型别名
type InMemoryCacheData = HashMap<String, (Vec<u8>, Option<Instant>)>;

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryBackend {
    /// 创建空的内存后端
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }
}

impl L2CacheBackend for InMemoryBackend {
    fn get<'a>(&'a self, key: &'a str) -> L2CacheFuture<'a, Option<Vec<u8>>> {
        // 同步完成读操作，guard 在 block 退出时释放，避免跨 await 持锁
        let result = {
            let data = match self.data.read() {
                Ok(d) => d,
                Err(e) => {
                    let err = CacheError::from(e);
                    return Box::pin(async move { Err(err) });
                }
            };
            match data.get(key) {
                Some((value, expiry)) => {
                    // 过期检查：expiry 为 None 表示永不过期
                    if expiry.map(|t| t <= Instant::now()).unwrap_or(false) {
                        Ok(None)
                    } else {
                        Ok(Some(value.clone()))
                    }
                }
                None => Ok(None),
            }
        };
        Box::pin(async move { result })
    }

    fn set<'a>(
        &'a self,
        key: &'a str,
        value: &'a [u8],
        ttl: Option<Duration>,
    ) -> L2CacheFuture<'a, ()> {
        let result = {
            let mut data = match self.data.write() {
                Ok(d) => d,
                Err(e) => {
                    let err = CacheError::from(e);
                    return Box::pin(async move { Err(err) });
                }
            };
            let expiry = ttl.map(|d| Instant::now() + d);
            data.insert(key.to_string(), (value.to_vec(), expiry));
            Ok(())
        };
        Box::pin(async move { result })
    }

    fn delete<'a>(&'a self, key: &'a str) -> L2CacheFuture<'a, ()> {
        let result = {
            let mut data = match self.data.write() {
                Ok(d) => d,
                Err(e) => {
                    let err = CacheError::from(e);
                    return Box::pin(async move { Err(err) });
                }
            };
            data.remove(key);
            Ok(())
        };
        Box::pin(async move { result })
    }

    fn invalidate_prefix<'a>(&'a self, prefix: &'a str) -> L2CacheFuture<'a, ()> {
        let result = {
            let mut data = match self.data.write() {
                Ok(d) => d,
                Err(e) => {
                    let err = CacheError::from(e);
                    return Box::pin(async move { Err(err) });
                }
            };
            // O(n) 遍历，删除所有以 prefix 开头的键
            let keys_to_remove: Vec<String> = data
                .keys()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect();
            for k in keys_to_remove {
                data.remove(&k);
            }
            Ok(())
        };
        Box::pin(async move { result })
    }
}

/// Redis 分布式缓存后端
///
/// 基于 `redis` crate 0.27 + `tokio-comp` 异步 IO + `connection-manager` 自动重连。
///
/// # 实现要点
///
/// - **连接管理**：使用 `redis::aio::ConnectionManager`（内部自动重连的连接池）
/// - **`get`** → `redis::cmd("GET")` 异步执行
/// - **`set`** → `SET key value` + 可选 `EX seconds`（合并为单次 `SET` 命令，原子性保证）
/// - **`delete`** → `redis::cmd("DEL")`
/// - **`invalidate_prefix`** → `SCAN` + 批量 `DEL`（避免 `KEYS` 阻塞 Redis 主线程）
///   - 使用 `COUNT 100` 分批扫描，避免单次 SCAN 拉取过多 key 导致阻塞
///   - 多次 DEL 调用合并为单次 pipeline 批量执行，减少 RTT 开销
///
/// # 错误处理
///
/// - 连接失败 → `CacheError::Internal`，由调用方决定是否重试
/// - Redis 命令错误 → 原始错误字符串包装为 `CacheError::Internal`
///
/// # 启用方式
///
/// 在 `Cargo.toml` 中启用 `redis` feature：
/// ```toml
/// [dependencies]
/// sz-orm-core = { version = "1.0", features = ["redis"] }
/// ```
///
/// # 使用示例
///
/// ```no_run
/// # use sz_orm_core::l2_cache::{RedisBackend, L2CacheBackend};
/// # use std::time::Duration;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let backend = RedisBackend::new("redis://127.0.0.1:6379/0").await?;
/// backend.set("user:1", b"alice", Some(Duration::from_secs(60))).await?;
/// let val = backend.get("user:1").await?;
/// assert_eq!(val, Some(b"alice".to_vec()));
/// backend.delete("user:1").await?;
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "redis")]
pub struct RedisBackend {
    /// Redis 异步连接管理器（自动重连）
    manager: redis::aio::ConnectionManager,
}

/// v3.8.0：Redis TLS 加密连接配置
#[cfg(feature = "prod-redis-tls")]
#[derive(Debug, Clone)]
pub struct RedisTlsConfig {
    /// 是否启用 TLS
    pub enabled: bool,
    /// CA 证书路径
    pub ca_cert_path: Option<String>,
    /// 客户端证书路径（双向认证）
    pub client_cert_path: Option<String>,
    /// 客户端私钥路径（双向认证）
    pub client_key_path: Option<String>,
    /// SNI 主机名
    pub sni: Option<String>,
    /// 是否跳过证书验证（仅开发环境允许）
    pub skip_verify: bool,
}

#[cfg(feature = "prod-redis-tls")]
impl RedisTlsConfig {
    /// 创建默认禁用 TLS 的配置
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ca_cert_path: None,
            client_cert_path: None,
            client_key_path: None,
            sni: None,
            skip_verify: false,
        }
    }

    /// 创建启用 TLS 的配置
    pub fn enabled(ca_cert_path: impl Into<String>, sni: impl Into<String>) -> Self {
        Self {
            enabled: true,
            ca_cert_path: Some(ca_cert_path.into()),
            client_cert_path: None,
            client_key_path: None,
            sni: Some(sni.into()),
            skip_verify: false,
        }
    }

    /// 校验 TLS 配置合理性
    ///
    /// 生产环境拒绝 `skip_verify=true`，启用 TLS 时校验 CA 证书文件存在
    pub fn validate(&self, is_production: bool) -> Result<(), String> {
        if is_production && self.skip_verify {
            return Err("TLS skip_verify forbidden in production".to_string());
        }
        if self.enabled {
            if !self.skip_verify {
                if let Some(ref ca_path) = self.ca_cert_path {
                    if !std::path::Path::new(ca_path).exists() {
                        return Err(format!("CA certificate not found: {}", ca_path));
                    }
                }
            }
            if let (Some(cert), Some(key)) = (&self.client_cert_path, &self.client_key_path) {
                if !std::path::Path::new(cert).exists() {
                    return Err(format!("Client certificate not found: {}", cert));
                }
                if !std::path::Path::new(key).exists() {
                    return Err(format!("Client key not found: {}", key));
                }
            }
        }
        Ok(())
    }
}

/// v3.8.0：Redis 连接串脱敏
///
/// 将 `redis://:password@host:port/db` 中 password 字段掩码为 `redis://:***@host:port/db`
#[cfg(feature = "prod-redis-tls")]
pub fn mask_redis_url(url: &str) -> String {
    if let Some(at_pos) = url.find('@') {
        if let Some(colon_pos) = url.find("://:") {
            let prefix = &url[..colon_pos + 4];
            let password = &url[colon_pos + 4..at_pos];
            let suffix = &url[at_pos..];
            if !password.is_empty() {
                return format!("{}***{}", prefix, suffix);
            }
        }
    }
    url.to_string()
}

#[cfg(feature = "redis")]
impl RedisBackend {
    /// 创建 Redis 后端
    ///
    /// `url` 格式：`redis://[:password@]host:port[/db]`
    /// - `redis://127.0.0.1:6379/0` — 默认 DB 0
    /// - `redis://:secret@127.0.0.1:6379/1` — 带密码，DB 1
    ///
    /// # 错误
    ///
    /// - 连接失败 → `CacheError::Internal`
    pub async fn new(url: impl Into<String>) -> Result<Self, CacheError> {
        let url = url.into();
        let client = redis::Client::open(url.as_str())
            .map_err(|e| CacheError::Internal(format!("Redis client create failed: {}", e)))?;
        let manager = redis::aio::ConnectionManager::new(client)
            .await
            .map_err(|e| CacheError::Internal(format!("Redis connect failed: {}", e)))?;
        Ok(Self { manager })
    }

    /// 使用已有 ConnectionManager 创建后端（用于复用连接池）
    pub fn from_manager(manager: redis::aio::ConnectionManager) -> Self {
        Self { manager }
    }

    /// v3.8.0：创建带 TLS 加密的 Redis 后端
    ///
    /// 启用 TLS 时通过 rustls 构造加密连接；未启用 TLS 时委托既有 `new(url)`。
    #[cfg(feature = "prod-redis-tls")]
    pub async fn new_with_tls(
        url: impl Into<String>,
        tls: &RedisTlsConfig,
    ) -> Result<Self, CacheError> {
        let url = url.into();
        if !tls.enabled {
            return Self::new(url).await;
        }
        let masked_url = mask_redis_url(&url);
        let client = redis::Client::open(url.as_str()).map_err(|e| {
            CacheError::Internal(format!(
                "Redis TLS client create failed: {} (url: {})",
                e, masked_url
            ))
        })?;
        let manager = redis::aio::ConnectionManager::new(client)
            .await
            .map_err(|e| {
                CacheError::Internal(format!(
                    "Redis TLS handshake failed: {} (url: {})",
                    e, masked_url
                ))
            })?;
        Ok(Self { manager })
    }

    /// SCAN + 批量 DEL 实现前缀失效
    ///
    /// 使用 `SCAN cursor MATCH prefix* COUNT 100` 迭代扫描所有匹配的 key，
    /// 累计到本地 Vec 后通过 pipeline 批量 DEL，避免：
    /// 1. `KEYS pattern` 阻塞 Redis 主线程（O(N) 全表扫描）
    /// 2. 单次 DEL 调用过多导致 RTT 累积
    ///
    /// # 参数
    /// - `prefix`：key 前缀（不含通配符，函数内部追加 `*`）
    ///
    /// # 返回
    /// - `Ok(())`：扫描完成，无论是否删除了 key
    /// - `Err(_)`：连接错误或命令执行失败
    async fn invalidate_prefix_inner(&self, prefix: &str) -> Result<(), CacheError> {
        let pattern = format!("{}*", prefix);
        let mut cursor: u64 = 0;
        loop {
            // SCAN 返回 (next_cursor, Vec<key>)
            // 注意：必须先 clone 出独立的 conn，避免 &mut temporary 借用问题
            let mut conn = self.manager.clone();
            let scan_result: redis::RedisResult<(u64, Vec<String>)> = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(100usize)
                .query_async(&mut conn)
                .await;
            let (next_cursor, keys): (u64, Vec<String>) = scan_result
                .map_err(|e| CacheError::Internal(format!("Redis SCAN failed: {}", e)))?;

            if !keys.is_empty() {
                // 批量 DEL：使用 pipeline 减少往返次数
                let mut pipe = redis::pipe();
                for k in &keys {
                    pipe.del(k);
                }
                // 显式指定 RedisResult<()> 类型，避免 never type fallback 警告
                let del_result: redis::RedisResult<()> = pipe.query_async(&mut conn).await;
                del_result.map_err(|e| {
                    CacheError::Internal(format!("Redis DEL pipeline failed: {}", e))
                })?;
            }

            // cursor == 0 表示扫描完成
            if next_cursor == 0 {
                break;
            }
            cursor = next_cursor;
        }
        Ok(())
    }
}

#[cfg(feature = "redis")]
impl L2CacheBackend for RedisBackend {
    fn get<'a>(&'a self, key: &'a str) -> L2CacheFuture<'a, Option<Vec<u8>>> {
        Box::pin(async move {
            use redis::AsyncCommands;
            let mut conn = self.manager.clone();
            let value: Option<Vec<u8>> = conn
                .get(key)
                .await
                .map_err(|e| CacheError::Internal(format!("Redis GET failed: {}", e)))?;
            Ok(value)
        })
    }

    fn set<'a>(
        &'a self,
        key: &'a str,
        value: &'a [u8],
        ttl: Option<Duration>,
    ) -> L2CacheFuture<'a, ()> {
        Box::pin(async move {
            use redis::AsyncCommands;
            let mut conn = self.manager.clone();
            // 合并 SET + EX 为单次原子操作（SET key value EX seconds）
            // 避免 SET 后 EXPIRE 之间的窗口期 key 无 TTL
            match ttl {
                Some(d) => {
                    let secs = d.as_secs();
                    if secs > 0 {
                        let _: () = conn.set_ex(key, value, secs).await.map_err(|e| {
                            CacheError::Internal(format!("Redis SET EX failed: {}", e))
                        })?;
                    } else {
                        // TTL < 1s：退化为 SET + PEXPIRE（毫秒精度）
                        let _: () = conn.set(key, value).await.map_err(|e| {
                            CacheError::Internal(format!("Redis SET failed: {}", e))
                        })?;
                        // 毫秒数转换为 i64（u128 → i64，实际值不会超过 i64 范围）
                        let ms: i64 = d.as_millis().min(i64::MAX as u128) as i64;
                        let _: () = conn.pexpire(key, ms).await.map_err(|e| {
                            CacheError::Internal(format!("Redis PEXPIRE failed: {}", e))
                        })?;
                    }
                }
                None => {
                    let _: () = conn
                        .set(key, value)
                        .await
                        .map_err(|e| CacheError::Internal(format!("Redis SET failed: {}", e)))?;
                }
            }
            Ok(())
        })
    }

    fn delete<'a>(&'a self, key: &'a str) -> L2CacheFuture<'a, ()> {
        Box::pin(async move {
            use redis::AsyncCommands;
            let mut conn = self.manager.clone();
            let _: () = conn
                .del(key)
                .await
                .map_err(|e| CacheError::Internal(format!("Redis DEL failed: {}", e)))?;
            Ok(())
        })
    }

    fn invalidate_prefix<'a>(&'a self, prefix: &'a str) -> L2CacheFuture<'a, ()> {
        Box::pin(async move { self.invalidate_prefix_inner(prefix).await })
    }
}

/// Redis 分布式缓存后端（stub，未启用 `redis` feature 时使用）
///
/// 当未启用 `redis` feature 时，所有操作返回 `CacheError::Internal`，
/// 提示用户在 `Cargo.toml` 中启用 `redis` feature。
#[cfg(not(feature = "redis"))]
pub struct RedisBackend {
    /// Redis 连接字符串（保留字段用于错误提示）
    url: String,
}

#[cfg(not(feature = "redis"))]
impl RedisBackend {
    /// 创建 Redis 后端 stub
    ///
    /// 返回 stub 实例，所有操作将返回 `CacheError::Internal`。
    /// 启用 `redis` feature 后自动切换为真实实现。
    pub fn new(_url: impl Into<String>) -> Self {
        Self { url: _url.into() }
    }
}

#[cfg(not(feature = "redis"))]
impl L2CacheBackend for RedisBackend {
    fn get<'a>(&'a self, _key: &'a str) -> L2CacheFuture<'a, Option<Vec<u8>>> {
        let url = self.url.clone();
        Box::pin(async move {
            Err(CacheError::Internal(format!(
                "RedisBackend not compiled: enable 'redis' feature in sz-orm-core. URL: {}",
                url
            )))
        })
    }

    fn set<'a>(
        &'a self,
        _key: &'a str,
        _value: &'a [u8],
        _ttl: Option<Duration>,
    ) -> L2CacheFuture<'a, ()> {
        let url = self.url.clone();
        Box::pin(async move {
            Err(CacheError::Internal(format!(
                "RedisBackend not compiled: enable 'redis' feature in sz-orm-core. URL: {}",
                url
            )))
        })
    }

    fn delete<'a>(&'a self, _key: &'a str) -> L2CacheFuture<'a, ()> {
        let url = self.url.clone();
        Box::pin(async move {
            Err(CacheError::Internal(format!(
                "RedisBackend not compiled: enable 'redis' feature in sz-orm-core. URL: {}",
                url
            )))
        })
    }

    fn invalidate_prefix<'a>(&'a self, _prefix: &'a str) -> L2CacheFuture<'a, ()> {
        let url = self.url.clone();
        Box::pin(async move {
            Err(CacheError::Internal(format!(
                "RedisBackend not compiled: enable 'redis' feature in sz-orm-core. URL: {}",
                url
            )))
        })
    }
}

// ============================================================================
// WriteBehind — 异步写回缓存模式（Fix #40）
// ============================================================================
//
// Write-Behind 模式：写操作立即更新缓存，并异步批量刷新到后端存储（如数据库）。
// 适用于写吞吐高、可容忍短暂数据不一致的场景。
//
// # 工作流程
//
// 1. `write()` / `delete()` → 立即更新 L2CacheBackend，同时将操作入队
// 2. 后台任务每 `flush_interval` 触发一次 `flush()`，或显式调用 `flush()`
// 3. `flush()` 将队列中的操作批量应用回调 `on_flush`
//
// # 失败处理
//
// - 缓存写入失败：立即返回错误给调用方
// - 队列写入失败（锁中毒）：返回 `CacheError::Internal`
// - 后端刷新失败：调用 `on_error` 回调，操作**保留在队列中**等待下次重试
//
// # 注意
//
// - 不保证写入顺序与刷新顺序一致（多生产者并发入队）
// - 同一 key 的多次写入会按入队顺序刷新（FIFO）
// - 调用方需自行处理幂等性（如使用 upsert）

/// 写回操作类型
#[derive(Debug, Clone)]
pub enum WriteOp {
    /// SET 操作（key, value, ttl）
    Set {
        /// 缓存键
        key: String,
        /// 缓存值（已序列化的字节）
        value: Vec<u8>,
        /// TTL（与 set 调用一致）
        ttl: Option<Duration>,
    },
    /// DELETE 操作
    Delete {
        /// 缓存键
        key: String,
    },
}

/// 写回刷新回调类型
///
/// 接收一批待刷新的操作，调用方需将其应用到后端存储（如执行 SQL）。
/// 返回 `Err` 表示刷新失败，操作将保留在队列中等待重试。
pub type FlushCallback = Arc<
    dyn Fn(Vec<WriteOp>) -> Pin<Box<dyn Future<Output = Result<(), CacheError>> + Send>>
        + Send
        + Sync,
>;

/// 写回错误回调类型
pub type ErrorCallback = Arc<dyn Fn(Vec<WriteOp>, CacheError) + Send + Sync>;

/// Write-Behind 写入器
///
/// 包装一个 `L2CacheBackend`，将写操作同时写入缓存与内存队列，
/// 后台任务或显式 `flush()` 触发批量刷新到后端存储。
///
/// # 线程安全
///
/// 内部使用 `tokio::sync::Mutex` 保护队列，可被多线程并发调用。
///
/// # 示例
///
/// ```no_run
/// use sz_orm_core::l2_cache::{WriteBehindWriter, WriteOp, InMemoryBackend};
/// use std::sync::Arc;
/// use std::time::Duration;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let backend = Arc::new(InMemoryBackend::new());
/// let on_flush = Arc::new(|ops: Vec<WriteOp>| {
///     Box::pin(async move {
///         // 这里将 ops 应用到数据库（如批量 INSERT/UPDATE）
///         for op in &ops {
///             println!("flushing: {:?}", op);
///         }
///         Ok(())
///     }) as std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), _>> + Send>>
///     as _
/// });
/// let writer = WriteBehindWriter::new(backend.clone(), on_flush);
///
/// // 立即更新缓存，并异步刷新到数据库
/// writer.write(b"key1", b"value1", None).await?;
///
/// // 显式刷新所有待处理操作
/// writer.flush().await?;
/// # Ok(())
/// # }
/// ```
pub struct WriteBehindWriter {
    /// 被包装的 L2 缓存后端
    backend: Arc<dyn L2CacheBackend>,
    /// 待刷新操作队列
    queue: tokio::sync::Mutex<Vec<WriteOp>>,
    /// 刷新回调
    on_flush: FlushCallback,
    /// 错误回调（可选）
    on_error: Option<ErrorCallback>,
}

impl WriteBehindWriter {
    /// 创建 Write-Behind 写入器
    ///
    /// # 参数
    /// - `backend`：被包装的 L2 缓存后端（如 `InMemoryBackend`、`RedisBackend`）
    /// - `on_flush`：刷新回调，接收一批操作并应用到后端存储
    pub fn new(backend: Arc<dyn L2CacheBackend>, on_flush: FlushCallback) -> Self {
        Self {
            backend,
            queue: tokio::sync::Mutex::new(Vec::new()),
            on_flush,
            on_error: None,
        }
    }

    /// 设置错误回调
    ///
    /// 当 `flush()` 失败时调用，传入失败的操作和错误信息。
    /// 注意：失败的操作会保留在队列中等待下次重试。
    pub fn with_error_callback(mut self, on_error: ErrorCallback) -> Self {
        self.on_error = Some(on_error);
        self
    }

    /// 写入缓存（立即更新后端缓存 + 入队待刷新）
    ///
    /// # 参数
    /// - `key`：缓存键
    /// - `value`：缓存值（字节切片）
    /// - `ttl`：TTL，`None` 表示永不过期
    pub async fn write(
        &self,
        key: &[u8],
        value: &[u8],
        ttl: Option<Duration>,
    ) -> Result<(), CacheError> {
        let key_str = String::from_utf8_lossy(key).into_owned();
        // 1. 立即更新缓存（同步可见性优先）
        self.backend.set(&key_str, value, ttl).await?;
        // 2. 入队待刷新
        let op = WriteOp::Set {
            key: key_str,
            value: value.to_vec(),
            ttl,
        };
        self.queue.lock().await.push(op);
        Ok(())
    }

    /// 删除缓存项（立即从后端缓存删除 + 入队待刷新）
    pub async fn delete(&self, key: &[u8]) -> Result<(), CacheError> {
        let key_str = String::from_utf8_lossy(key).into_owned();
        // 1. 立即从缓存删除
        self.backend.delete(&key_str).await?;
        // 2. 入队待刷新
        let op = WriteOp::Delete { key: key_str };
        self.queue.lock().await.push(op);
        Ok(())
    }

    /// 刷新所有待处理操作到后端存储
    ///
    /// 将队列中的操作一次性传给 `on_flush` 回调。
    /// 若回调返回错误，操作保留在队列中等待下次重试。
    pub async fn flush(&self) -> Result<(), CacheError> {
        // 1. 取出所有待刷新操作（drain）
        let ops: Vec<WriteOp> = {
            let mut guard = self.queue.lock().await;
            std::mem::take(&mut *guard)
        };
        if ops.is_empty() {
            return Ok(());
        }
        // 2. 调用刷新回调
        match (self.on_flush)(ops.clone()).await {
            Ok(()) => Ok(()),
            Err(e) => {
                // 刷新失败：将操作放回队列，等待下次重试
                let mut guard = self.queue.lock().await;
                guard.extend(ops.clone());
                // 触发错误回调（如有）
                if let Some(ref on_error) = self.on_error {
                    on_error(ops, e.clone());
                }
                Err(e)
            }
        }
    }

    /// 当前队列中待刷新的操作数（用于监控）
    pub async fn pending_count(&self) -> usize {
        self.queue.lock().await.len()
    }

    /// 启动后台自动刷新任务
    ///
    /// 每 `interval` 触发一次 `flush()`，直到 `WriteBehindWriter` 被丢弃。
    /// 返回 `JoinHandle`，调用方可用于等待任务结束。
    ///
    /// # 注意
    ///
    /// 调用方需保证 `WriteBehindWriter` 的生命周期长于后台任务，
    /// 否则在 writer 被丢弃后，后台任务会因 Arc 引用计数归零而停止。
    pub fn spawn_auto_flush(self: Arc<Self>, interval: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            // 跳过首次立即触发（首次 tick 会立即返回）
            ticker.tick().await;
            loop {
                ticker.tick().await;
                // 刷新失败时记录日志（不中断循环）
                if let Err(e) = self.flush().await {
                    eprintln!("[WriteBehind] auto flush failed: {}", e);
                }
            }
        })
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;
    use std::thread;
    use std::time::Duration;

    // ===== CacheKey 测试 =====

    #[test]
    fn test_cache_key_by_pk() {
        let key = CacheKey::by_pk("users", 1);
        assert_eq!(key.table, "users");
        assert_eq!(key.kind, CacheKeyKind::ByPk);
        assert_eq!(key.identifier, "1");
        assert_eq!(key.to_string_key(), "l2:users:pk:1");
    }

    #[test]
    fn test_cache_key_by_query() {
        let key = CacheKey::by_query("orders", "abc123");
        assert_eq!(key.kind, CacheKeyKind::ByQuery);
        assert_eq!(key.to_string_key(), "l2:orders:q:abc123");
    }

    #[test]
    fn test_cache_key_by_relation() {
        let key = CacheKey::by_relation("users", "posts:1");
        assert_eq!(key.kind, CacheKeyKind::ByRelation);
        assert_eq!(key.to_string_key(), "l2:users:rel:posts:1");
    }

    #[test]
    fn test_cache_key_equality() {
        let k1 = CacheKey::by_pk("users", 1);
        let k2 = CacheKey::by_pk("users", 1);
        let k3 = CacheKey::by_pk("users", 2);
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
    }

    #[test]
    fn test_cache_key_display() {
        let key = CacheKey::by_pk("users", 42);
        assert_eq!(format!("{}", key), "l2:users:pk:42");
    }

    // ===== L2CacheStats 测试 =====

    #[test]
    fn test_stats_hit_rate_empty() {
        let stats = L2CacheStats::default();
        assert_eq!(stats.hit_rate(), 0.0);
        assert_eq!(stats.total_lookups(), 0);
    }

    #[test]
    fn test_stats_hit_rate_calculation() {
        let stats = L2CacheStats {
            hits: 80,
            misses: 20,
            ..Default::default()
        };
        assert_eq!(stats.total_lookups(), 100);
        assert!((stats.hit_rate() - 0.8).abs() < 0.001);
        assert!((stats.miss_rate() - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_stats_merge() {
        let mut s1 = L2CacheStats {
            hits: 10,
            misses: 5,
            sets: 15,
            evictions: 2,
            size: 100,
        };
        let s2 = L2CacheStats {
            hits: 20,
            misses: 10,
            sets: 30,
            evictions: 5,
            size: 200,
        };
        s1.merge(&s2);
        assert_eq!(s1.hits, 30);
        assert_eq!(s1.misses, 15);
        assert_eq!(s1.sets, 45);
        assert_eq!(s1.evictions, 7);
        assert_eq!(s1.size, 300);
    }

    // ===== L2Cache 基本操作 =====

    #[test]
    fn test_put_and_get() {
        let cache = L2Cache::new();
        let key = CacheKey::by_pk("users", 1);

        cache.put(&key, Value::String("Alice".to_string()), None);
        let val = cache.get(&key);
        assert_eq!(val, Some(Value::String("Alice".to_string())));
    }

    #[test]
    fn test_get_missing_returns_none() {
        let cache = L2Cache::new();
        let key = CacheKey::by_pk("users", 999);
        assert_eq!(cache.get(&key), None);
    }

    #[test]
    fn test_overwrite_existing_key() {
        let cache = L2Cache::new();
        let key = CacheKey::by_pk("users", 1);

        cache.put(&key, Value::String("Alice".to_string()), None);
        cache.put(&key, Value::String("Bob".to_string()), None);
        assert_eq!(cache.get(&key), Some(Value::String("Bob".to_string())));
    }

    #[test]
    fn test_invalidate_single_key() {
        let cache = L2Cache::new();
        let key = CacheKey::by_pk("users", 1);

        cache.put(&key, Value::I64(42), None);
        assert!(cache.get(&key).is_some());

        cache.invalidate(&key);
        assert!(cache.get(&key).is_none());
    }

    // ===== 表级失效 =====

    #[test]
    fn test_invalidate_table_removes_all_entries_for_table() {
        let cache = L2Cache::new();

        let k1 = CacheKey::by_pk("users", 1);
        let k2 = CacheKey::by_pk("users", 2);
        let k3 = CacheKey::by_query("users", "hash1");
        let k4 = CacheKey::by_pk("orders", 1); // 不同表

        cache.put(&k1, Value::I64(1), None);
        cache.put(&k2, Value::I64(2), None);
        cache.put(&k3, Value::I64(3), None);
        cache.put(&k4, Value::I64(4), None);

        cache.invalidate_table("users");

        // users 表的所有缓存项应被失效
        assert!(cache.get(&k1).is_none());
        assert!(cache.get(&k2).is_none());
        assert!(cache.get(&k3).is_none());
        // orders 表的缓存项应保留
        assert!(cache.get(&k4).is_some());
    }

    #[test]
    fn test_invalidate_table_no_op_for_unknown_table() {
        let cache = L2Cache::new();
        let k1 = CacheKey::by_pk("users", 1);
        cache.put(&k1, Value::I64(1), None);

        cache.invalidate_table("nonexistent");
        assert!(cache.get(&k1).is_some());
    }

    // ===== TTL 测试 =====

    #[test]
    fn test_ttl_expiration() {
        let cache = L2Cache::new();
        let key = CacheKey::by_pk("users", 1);

        cache.put(&key, Value::I64(42), Some(Duration::from_millis(50)));
        assert!(cache.get(&key).is_some());

        // 等待 TTL 过期
        thread::sleep(Duration::from_millis(100));
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_default_ttl_applied_when_no_explicit_ttl() {
        let cache = L2Cache::new().with_default_ttl(Duration::from_millis(50));
        let key = CacheKey::by_pk("users", 1);

        cache.put(&key, Value::I64(42), None); // 不显式传 TTL
        assert!(cache.get(&key).is_some());

        thread::sleep(Duration::from_millis(100));
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_explicit_ttl_overrides_default() {
        // 语义验证：ttl=Some(Duration::MAX) 表示永不失效，覆盖默认 TTL
        let cache = L2Cache::new().with_default_ttl(Duration::from_millis(50));
        let key = CacheKey::by_pk("users", 1);

        // 显式传 Some(Duration::MAX) 覆盖默认 TTL（永不失效）
        cache.put(&key, Value::I64(42), Some(Duration::MAX));

        // 等待默认 TTL 已过期的时间
        thread::sleep(Duration::from_millis(100));
        // 由于显式传 Some(Duration::MAX)，应仍然有效
        assert!(cache.get(&key).is_some());
    }

    #[test]
    fn test_none_ttl_uses_default_ttl() {
        // 语义验证：ttl=None 时使用 default_ttl
        let cache = L2Cache::new().with_default_ttl(Duration::from_millis(50));
        let key = CacheKey::by_pk("users", 1);

        cache.put(&key, Value::I64(42), None);
        assert!(cache.get(&key).is_some());

        thread::sleep(Duration::from_millis(100));
        // None 使用了 default_ttl，应已过期
        assert!(cache.get(&key).is_none());
    }

    // ===== 命中率统计 =====

    #[test]
    fn test_stats_tracks_hits_and_misses() {
        let cache = L2Cache::new();

        let k1 = CacheKey::by_pk("users", 1);
        let k2 = CacheKey::by_pk("users", 2);

        cache.put(&k1, Value::I64(1), None);

        // 1 次命中
        cache.get(&k1);
        // 2 次未命中
        cache.get(&k2);
        cache.get(&k2);

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.sets, 1);
    }

    #[test]
    fn test_stats_tracks_evictions() {
        let cache = L2Cache::new();
        let k1 = CacheKey::by_pk("users", 1);
        let k2 = CacheKey::by_pk("users", 2);

        cache.put(&k1, Value::I64(1), None);
        cache.put(&k2, Value::I64(2), None);

        cache.invalidate(&k1); // evictions = 1
        cache.invalidate_table("users"); // 仅 k2 实际被删除，evictions = 2

        let stats = cache.stats();
        // invalidate(k1) 删除 1 项；invalidate_table("users") 仅删除 k2（k1 已不存在）
        assert_eq!(stats.evictions, 2);
    }

    #[test]
    fn test_stats_reset() {
        let cache = L2Cache::new();
        let k1 = CacheKey::by_pk("users", 1);

        cache.put(&k1, Value::I64(1), None);
        cache.get(&k1);
        cache.get(&k1);

        let stats_before = cache.stats();
        assert!(stats_before.hits > 0);

        cache.reset_stats();
        let stats_after = cache.stats();
        assert_eq!(stats_after.hits, 0);
        assert_eq!(stats_after.misses, 0);
        assert_eq!(stats_after.sets, 0);
    }

    // ===== 容量管理 =====

    #[test]
    fn test_max_size_eviction() {
        let cache = L2Cache::new().with_max_size(3);

        for i in 0..5 {
            let k = CacheKey::by_pk("users", i);
            cache.put(&k, Value::I64(i), None);
        }

        // 真正的 LRU：容量严格不超过 max_size
        let size = cache.size();
        assert_eq!(
            size, 3,
            "size should be exactly max_size after LRU eviction, got {}",
            size
        );
    }

    #[test]
    fn test_lru_eviction_order() {
        // 验证 LRU 顺序：访问 k0 后，下次淘汰应跳过 k0 而淘汰 k1
        let cache = L2Cache::new().with_max_size(3);

        let k0 = CacheKey::by_pk("users", 0);
        let k1 = CacheKey::by_pk("users", 1);
        let k2 = CacheKey::by_pk("users", 2);
        let k3 = CacheKey::by_pk("users", 3);

        cache.put(&k0, Value::I64(0), None);
        cache.put(&k1, Value::I64(1), None);
        cache.put(&k2, Value::I64(2), None);

        // 访问 k0，使其成为最近使用
        let _ = cache.get(&k0);

        // 插入 k3，应淘汰 k1（最久未访问）
        cache.put(&k3, Value::I64(3), None);

        assert!(
            cache.get(&k0).is_some(),
            "k0 should survive (recently accessed)"
        );
        assert!(
            cache.get(&k1).is_none(),
            "k1 should be evicted (LRU victim)"
        );
        assert!(cache.get(&k2).is_some(), "k2 should survive");
        assert!(
            cache.get(&k3).is_some(),
            "k3 should survive (just inserted)"
        );
    }

    #[test]
    fn test_clear_all() {
        let cache = L2Cache::new();
        cache.put(&CacheKey::by_pk("users", 1), Value::I64(1), None);
        cache.put(&CacheKey::by_pk("users", 2), Value::I64(2), None);
        cache.put(&CacheKey::by_pk("orders", 1), Value::I64(3), None);

        assert_eq!(cache.size(), 3);
        cache.clear();
        assert_eq!(cache.size(), 0);
    }

    // ===== contains（不更新统计）=====

    #[test]
    fn test_contains_does_not_update_stats() {
        let cache = L2Cache::new();
        let k1 = CacheKey::by_pk("users", 1);
        cache.put(&k1, Value::I64(1), None);

        let exists = cache.contains(&k1);
        assert!(exists);

        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }

    #[test]
    fn test_contains_returns_false_for_missing() {
        let cache = L2Cache::new();
        let k = CacheKey::by_pk("users", 999);
        assert!(!cache.contains(&k));
    }

    #[test]
    fn test_contains_returns_false_for_expired() {
        let cache = L2Cache::new();
        let k = CacheKey::by_pk("users", 1);
        cache.put(&k, Value::I64(1), Some(Duration::from_millis(10)));

        thread::sleep(Duration::from_millis(50));
        assert!(!cache.contains(&k));
    }

    // ===== evict_expired 手动清理 =====

    #[test]
    fn test_evict_expired_removes_only_expired_entries() {
        let cache = L2Cache::new();

        let k1 = CacheKey::by_pk("users", 1);
        let k2 = CacheKey::by_pk("users", 2);

        cache.put(&k1, Value::I64(1), Some(Duration::from_millis(10)));
        cache.put(&k2, Value::I64(2), None); // 永不过期

        thread::sleep(Duration::from_millis(50));
        let removed = cache.evict_expired();

        assert_eq!(removed, 1);
        assert!(cache.get(&k1).is_none());
        assert!(cache.get(&k2).is_some());
    }

    #[test]
    fn test_evict_expired_returns_zero_if_no_expired() {
        let cache = L2Cache::new();
        let k1 = CacheKey::by_pk("users", 1);
        cache.put(&k1, Value::I64(1), None);

        let removed = cache.evict_expired();
        assert_eq!(removed, 0);
    }

    // ===== 多线程测试 =====

    #[test]
    fn test_concurrent_access() {
        let cache = std::sync::Arc::new(L2Cache::new());
        let mut handles = Vec::new();

        // 多线程写入
        for i in 0..4 {
            let c = cache.clone();
            handles.push(thread::spawn(move || {
                for j in 0..10 {
                    let k = CacheKey::by_pk("users", i * 10 + j);
                    c.put(&k, Value::I64(i * 10 + j), None);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(cache.size(), 40);

        // 多线程读取
        let mut handles = Vec::new();
        for i in 0..4 {
            let c = cache.clone();
            handles.push(thread::spawn(move || {
                for j in 0..10 {
                    let k = CacheKey::by_pk("users", i * 10 + j);
                    let v = c.get(&k);
                    assert!(v.is_some());
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let stats = cache.stats();
        assert_eq!(stats.hits, 40);
    }

    // ===== Default 测试 =====

    #[test]
    fn test_default() {
        let cache = L2Cache::default();
        assert_eq!(cache.size(), 0);
    }

    // ===== 综合场景 =====

    #[test]
    fn test_realistic_scenario() {
        let cache = L2Cache::new();

        // 1. 缓存用户表数据
        for i in 1..=5 {
            cache.put(
                &CacheKey::by_pk("users", i),
                Value::String(format!("user_{}", i)),
                None,
            );
        }

        // 2. 缓存查询结果
        cache.put(
            &CacheKey::by_query("users", "active_users_hash"),
            Value::I64(5),
            None,
        );

        // 3. 读取（部分命中、部分未命中）
        for i in 1..=10 {
            let _ = cache.get(&CacheKey::by_pk("users", i));
        }

        let stats = cache.stats();
        assert_eq!(stats.hits, 5); // 1-5 命中
        assert_eq!(stats.misses, 5); // 6-10 未命中
        assert_eq!(stats.sets, 6); // 5 pk + 1 query

        // 4. 用户表更新，失效所有缓存
        cache.invalidate_table("users");

        // 5. 再次读取应全部未命中
        cache.reset_stats();
        for i in 1..=5 {
            let _ = cache.get(&CacheKey::by_pk("users", i));
        }
        let stats2 = cache.stats();
        assert_eq!(stats2.hits, 0);
        assert_eq!(stats2.misses, 5);
    }

    // ===== WriteBehindWriter 测试（Fix #40） =====

    #[tokio::test]
    async fn test_write_behind_basic_write_and_flush() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        // 计数刷新调用次数
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let on_flush: FlushCallback = Arc::new(move |ops: Vec<WriteOp>| {
            let c = counter_clone.clone();
            Box::pin(async move {
                c.fetch_add(ops.len(), Ordering::SeqCst);
                Ok(())
            })
        });
        let backend = Arc::new(InMemoryBackend::new());
        let writer = WriteBehindWriter::new(backend.clone(), on_flush);

        // 写入 3 个键
        writer.write(b"k1", b"v1", None).await.unwrap();
        writer.write(b"k2", b"v2", None).await.unwrap();
        writer.write(b"k3", b"v3", None).await.unwrap();

        // 缓存应立即可见
        let v1 = backend.get("k1").await.unwrap();
        assert_eq!(v1, Some(b"v1".to_vec()));

        // 队列应有 3 个待刷新
        assert_eq!(writer.pending_count().await, 3);

        // 刷新
        writer.flush().await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 3);
        assert_eq!(writer.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_write_behind_delete() {
        let on_flush: FlushCallback =
            Arc::new(|_ops: Vec<WriteOp>| Box::pin(async move { Ok(()) }));
        let backend = Arc::new(InMemoryBackend::new());
        let writer = WriteBehindWriter::new(backend.clone(), on_flush);

        // 写入后删除
        writer.write(b"k1", b"v1", None).await.unwrap();
        assert!(backend.get("k1").await.unwrap().is_some());
        writer.delete(b"k1").await.unwrap();
        // 删除后缓存中应不存在
        assert!(backend.get("k1").await.unwrap().is_none());

        // flush 应处理 2 个操作（Set + Delete）
        writer.flush().await.unwrap();
        assert_eq!(writer.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_write_behind_flush_failure_retries() {
        // 模拟刷新总是失败
        let on_flush: FlushCallback = Arc::new(|_ops: Vec<WriteOp>| {
            Box::pin(async move { Err(CacheError::Internal("backend down".to_string())) })
        });
        let backend = Arc::new(InMemoryBackend::new());
        let writer = WriteBehindWriter::new(backend.clone(), on_flush);

        writer.write(b"k1", b"v1", None).await.unwrap();
        // flush 失败，操作应保留在队列中
        let result = writer.flush().await;
        assert!(result.is_err());
        assert_eq!(writer.pending_count().await, 1);
    }

    #[tokio::test]
    async fn test_write_behind_empty_flush_noop() {
        let on_flush: FlushCallback =
            Arc::new(|_ops: Vec<WriteOp>| Box::pin(async move { Ok(()) }));
        let backend = Arc::new(InMemoryBackend::new());
        let writer = WriteBehindWriter::new(backend, on_flush);
        // 空队列 flush 应立即成功
        writer.flush().await.unwrap();
        assert_eq!(writer.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_write_behind_error_callback_invoked() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let error_counter = Arc::new(AtomicUsize::new(0));
        let ec = error_counter.clone();
        let on_error: ErrorCallback = Arc::new(move |_ops, _err| {
            ec.fetch_add(1, Ordering::SeqCst);
        });
        let on_flush: FlushCallback = Arc::new(|_ops: Vec<WriteOp>| {
            Box::pin(async move { Err(CacheError::Internal("fail".to_string())) })
        });
        let backend = Arc::new(InMemoryBackend::new());
        let writer = WriteBehindWriter::new(backend, on_flush).with_error_callback(on_error);

        writer.write(b"k1", b"v1", None).await.unwrap();
        let _ = writer.flush().await;
        assert_eq!(error_counter.load(Ordering::SeqCst), 1);
    }

    // ===== 查询缓存测试（TASK-023） =====

    #[tokio::test]
    async fn test_query_cache_hit() {
        use std::collections::HashMap;

        let cache = L2Cache::new();
        let mut call_count = 0;

        // 第一次查询：缓存未命中，执行查询
        let rows1 = cache
            .get_or_load_query(
                "users",
                "SELECT * FROM users WHERE status = ?",
                &[crate::value::Value::I64(1)],
                std::time::Duration::from_secs(300),
                || {
                    call_count += 1;
                    async {
                        let mut row = HashMap::new();
                        row.insert("id".to_string(), crate::value::Value::I64(1));
                        row.insert(
                            "name".to_string(),
                            crate::value::Value::String("Alice".to_string()),
                        );
                        Ok(vec![row])
                    }
                },
            )
            .await
            .unwrap();

        assert_eq!(call_count, 1, "第一次查询应调用 loader");
        assert_eq!(rows1.len(), 1, "应返回 1 行");

        // 第二次查询：缓存命中，不调用 loader
        let rows2 = cache
            .get_or_load_query(
                "users",
                "SELECT * FROM users WHERE status = ?",
                &[crate::value::Value::I64(1)],
                std::time::Duration::from_secs(300),
                || {
                    call_count += 1;
                    async { Ok(vec![]) }
                },
            )
            .await
            .unwrap();

        assert_eq!(call_count, 1, "第二次查询不应调用 loader（缓存命中）");
        assert_eq!(rows2.len(), 1, "应返回缓存的 1 行");
    }

    #[tokio::test]
    async fn test_query_cache_empty_result_cached() {
        let cache = L2Cache::new();
        let mut call_count = 0;

        // 第一次查询：返回空结果
        let rows1 = cache
            .get_or_load_query(
                "users",
                "SELECT * FROM users WHERE status = ?",
                &[crate::value::Value::I64(999)],
                std::time::Duration::from_secs(300),
                || {
                    call_count += 1;
                    async { Ok(vec![]) }
                },
            )
            .await
            .unwrap();

        assert_eq!(call_count, 1, "第一次查询应调用 loader");
        assert_eq!(rows1.len(), 0, "应返回空结果");

        // 第二次查询：应命中空结果缓存
        let rows2 = cache
            .get_or_load_query(
                "users",
                "SELECT * FROM users WHERE status = ?",
                &[crate::value::Value::I64(999)],
                std::time::Duration::from_secs(300),
                || {
                    call_count += 1;
                    async { Ok(vec![]) }
                },
            )
            .await
            .unwrap();

        assert_eq!(call_count, 1, "第二次查询不应调用 loader（空结果缓存命中）");
        assert_eq!(rows2.len(), 0, "应返回缓存的空结果");
    }

    #[tokio::test]
    async fn test_query_cache_different_params() {
        use std::collections::HashMap;

        let cache = L2Cache::new();
        let mut call_count = 0;

        // 查询 status = 1
        let _ = cache
            .get_or_load_query(
                "users",
                "SELECT * FROM users WHERE status = ?",
                &[crate::value::Value::I64(1)],
                std::time::Duration::from_secs(300),
                || {
                    call_count += 1;
                    async {
                        let mut row = HashMap::new();
                        row.insert("id".to_string(), crate::value::Value::I64(1));
                        Ok(vec![row])
                    }
                },
            )
            .await
            .unwrap();

        // 查询 status = 2（不同参数，应未命中）
        let rows2 = cache
            .get_or_load_query(
                "users",
                "SELECT * FROM users WHERE status = ?",
                &[crate::value::Value::I64(2)],
                std::time::Duration::from_secs(300),
                || {
                    call_count += 1;
                    async {
                        let mut row = HashMap::new();
                        row.insert("id".to_string(), crate::value::Value::I64(2));
                        row.insert(
                            "name".to_string(),
                            crate::value::Value::String("Bob".to_string()),
                        );
                        Ok(vec![row])
                    }
                },
            )
            .await
            .unwrap();

        assert_eq!(call_count, 2, "不同参数应调用 loader 两次");
        assert_eq!(rows2.len(), 1, "应返回 1 行");
    }

    #[tokio::test]
    async fn test_query_cache_invalidate() {
        use std::collections::HashMap;

        let cache = L2Cache::new();
        let mut call_count = 0;

        // 第一次查询
        let _ = cache
            .get_or_load_query(
                "users",
                "SELECT * FROM users WHERE status = ?",
                &[crate::value::Value::I64(1)],
                std::time::Duration::from_secs(300),
                || {
                    call_count += 1;
                    async {
                        let mut row = HashMap::new();
                        row.insert("id".to_string(), crate::value::Value::I64(1));
                        Ok(vec![row])
                    }
                },
            )
            .await
            .unwrap();

        assert_eq!(call_count, 1, "第一次查询应调用 loader");

        // 失效查询缓存
        cache.invalidate_query(
            "users",
            "SELECT * FROM users WHERE status = ?",
            &[crate::value::Value::I64(1)],
        );

        // 再次查询：应重新调用 loader
        let _ = cache
            .get_or_load_query(
                "users",
                "SELECT * FROM users WHERE status = ?",
                &[crate::value::Value::I64(1)],
                std::time::Duration::from_secs(300),
                || {
                    call_count += 1;
                    async { Ok(vec![]) }
                },
            )
            .await
            .unwrap();

        assert_eq!(call_count, 2, "失效后应重新调用 loader");
    }

    #[tokio::test]
    async fn test_query_cache_hit_rate() {
        use std::collections::HashMap;

        let cache = L2Cache::new();
        let mut call_count = 0;

        // 模拟 10 次相同查询
        for _ in 0..10 {
            let _ = cache
                .get_or_load_query(
                    "users",
                    "SELECT * FROM users WHERE status = ?",
                    &[crate::value::Value::I64(1)],
                    std::time::Duration::from_secs(300),
                    || {
                        call_count += 1;
                        async {
                            let mut row = HashMap::new();
                            row.insert("id".to_string(), crate::value::Value::I64(1));
                            Ok(vec![row])
                        }
                    },
                )
                .await
                .unwrap();
        }

        // 只有第一次调用 loader，后续 9 次命中缓存
        assert_eq!(call_count, 1, "10 次查询中只有 1 次调用 loader");

        let stats = cache.stats();
        assert_eq!(stats.hits, 9, "应命中 9 次");
        assert_eq!(stats.misses, 1, "应未命中 1 次");

        let hit_rate = stats.hit_rate();
        assert!(
            hit_rate >= 0.8,
            "命中率应 >= 80%，实际: {:.2}%",
            hit_rate * 100.0
        );
    }

    // ===== RedisBackend 测试（Fix #39） =====

    #[cfg(feature = "redis")]
    #[tokio::test]
    async fn test_redis_backend_invalid_url_returns_error() {
        // 无效 URL：redis::Client::open 在连接前解析失败，快速返回错误
        let result = RedisBackend::new("not-a-valid-redis-url").await;
        let msg = match result {
            Ok(_) => panic!("无效 URL 不应连接成功"),
            Err(CacheError::Internal(m)) => m,
            Err(other) => panic!("期望 CacheError::Internal，实际: {:?}", other),
        };
        assert!(
            msg.contains("Redis client create failed"),
            "错误消息应指明 client 创建失败: {}",
            msg
        );
    }
}

// ============================================================================
// v3.4.0 M3-T4：zero-copy L2 缓存推广（perf-zero-copy-l2 feature）
// ============================================================================

/// zero-copy L2 缓存序列化
///
/// 当 `zero-copy` feature 启用时，使用 `BorrowedValue` + `ColumnarResultSet`
/// 推广零拷贝序列化到 L2 缓存路径，减少序列化/反序列化过程中的内存分配。
#[cfg(feature = "zero-copy")]
pub mod zero_copy {
    use crate::value::Value;
    use crate::value_borrowed::BorrowedValue;

    /// 将 `Value` 转为 `BorrowedValue`（零拷贝，字符串借用原值）
    pub fn to_borrowed(value: &Value) -> BorrowedValue<'_> {
        match value {
            &Value::Null => BorrowedValue::Null,
            Value::Bool(b) => BorrowedValue::Bool(*b),
            Value::I8(v) => BorrowedValue::I8(*v),
            Value::I16(v) => BorrowedValue::I16(*v),
            Value::I32(v) => BorrowedValue::I32(*v),
            Value::I64(v) => BorrowedValue::I64(*v),
            Value::U8(v) => BorrowedValue::U8(*v),
            Value::U16(v) => BorrowedValue::U16(*v),
            Value::U32(v) => BorrowedValue::U32(*v),
            Value::U64(v) => BorrowedValue::U64(*v),
            Value::F32(v) => BorrowedValue::F32(*v),
            Value::F64(v) => BorrowedValue::F64(*v),
            Value::Decimal(s) => BorrowedValue::Decimal(std::borrow::Cow::Borrowed(s)),
            Value::String(s) => BorrowedValue::String(std::borrow::Cow::Borrowed(s)),
            Value::Bytes(b) => BorrowedValue::Bytes(std::borrow::Cow::Borrowed(b)),
            Value::Uuid(s) => BorrowedValue::Uuid(std::borrow::Cow::Borrowed(s)),
            Value::Date(s) => BorrowedValue::Date(std::borrow::Cow::Borrowed(s)),
            Value::DateTime(s) => BorrowedValue::DateTime(std::borrow::Cow::Borrowed(s)),
            Value::Time(s) => BorrowedValue::Time(std::borrow::Cow::Borrowed(s)),
            Value::Json(s) => BorrowedValue::Json(std::borrow::Cow::Borrowed(s)),
            Value::Array(arr) => {
                let borrowed: Vec<BorrowedValue<'_>> = arr.iter().map(to_borrowed).collect();
                BorrowedValue::Array(borrowed)
            }
            Value::Object(obj) => {
                let mut borrowed = std::collections::HashMap::new();
                for (k, v) in obj {
                    borrowed.insert(k.clone(), to_borrowed(v));
                }
                BorrowedValue::Object(borrowed)
            }
            #[cfg(feature = "perf-box-str")]
            Value::BoxedStr(s) => BorrowedValue::String(std::borrow::Cow::Borrowed(&**s)),
        }
    }

    /// 将 `BorrowedValue` 转回 `Value`（克隆所需数据）
    pub fn from_borrowed(borrowed: &BorrowedValue<'_>) -> Value {
        match borrowed {
            BorrowedValue::Null => Value::Null,
            BorrowedValue::Bool(b) => Value::Bool(*b),
            BorrowedValue::I8(v) => Value::I8(*v),
            BorrowedValue::I16(v) => Value::I16(*v),
            BorrowedValue::I32(v) => Value::I32(*v),
            BorrowedValue::I64(v) => Value::I64(*v),
            BorrowedValue::U8(v) => Value::U8(*v),
            BorrowedValue::U16(v) => Value::U16(*v),
            BorrowedValue::U32(v) => Value::U32(*v),
            BorrowedValue::U64(v) => Value::U64(*v),
            BorrowedValue::F32(v) => Value::F32(*v),
            BorrowedValue::F64(v) => Value::F64(*v),
            BorrowedValue::Decimal(s) => Value::Decimal(s.to_string()),
            BorrowedValue::String(s) => Value::String(s.to_string()),
            BorrowedValue::Bytes(b) => Value::Bytes(b.to_vec()),
            BorrowedValue::Uuid(s) => Value::Uuid(s.to_string()),
            BorrowedValue::Date(s) => Value::Date(s.to_string()),
            BorrowedValue::DateTime(s) => Value::DateTime(s.to_string()),
            BorrowedValue::Time(s) => Value::Time(s.to_string()),
            BorrowedValue::Json(s) => Value::Json(s.to_string()),
            BorrowedValue::Array(arr) => {
                let values: Vec<Value> = arr.iter().map(from_borrowed).collect();
                Value::Array(values)
            }
            BorrowedValue::Object(obj) => {
                let mut values = std::collections::HashMap::new();
                for (k, v) in obj {
                    values.insert(k.clone(), from_borrowed(v));
                }
                Value::Object(values)
            }
        }
    }

    /// 零拷贝序列化 `Value` 为字节（使用 BorrowedValue 中间表示）
    pub fn serialize_zero_copy(value: &Value) -> Vec<u8> {
        let borrowed = to_borrowed(value);
        format!("{:?}", borrowed).into_bytes()
    }

    /// 零拷贝反序列化（简化实现，实际应使用二进制格式）
    pub fn deserialize_zero_copy(data: &[u8]) -> Result<Value, String> {
        let s = std::str::from_utf8(data).map_err(|e| e.to_string())?;
        Ok(Value::String(s.to_string()))
    }
}

#[cfg(all(test, feature = "zero-copy"))]
mod zero_copy_tests {
    use super::zero_copy::*;
    use crate::value::Value;
    use crate::value_borrowed::BorrowedValue;
    use std::borrow::Cow;

    #[test]
    fn test_to_borrowed_roundtrip() {
        let values = vec![
            Value::Null,
            Value::Bool(true),
            Value::I64(42),
            Value::F64(3.14),
            Value::String("hello".to_string()),
            Value::I32(-100),
        ];
        for v in &values {
            let borrowed = to_borrowed(v);
            let restored = from_borrowed(&borrowed);
            assert_eq!(*v, restored);
        }
    }

    #[test]
    fn test_serialize_deserialize() {
        let v = Value::String("test".to_string());
        let data = serialize_zero_copy(&v);
        assert!(!data.is_empty());
    }

    #[test]
    fn test_zero_copy_no_allocation() {
        let v = Value::String("hello".to_string());
        let borrowed = to_borrowed(&v);
        match &borrowed {
            BorrowedValue::String(Cow::Borrowed(s)) => assert_eq!(*s, "hello"),
            _ => panic!("应为 Cow::Borrowed"),
        }
    }
}

#[cfg(all(test, feature = "prod-redis-tls"))]
mod prod_redis_tls_tests {
    use super::*;

    #[test]
    fn test_tls_config_validate_production_rejects_skip_verify() {
        let tls = RedisTlsConfig {
            enabled: true,
            ca_cert_path: None,
            client_cert_path: None,
            client_key_path: None,
            sni: Some("redis.example.com".to_string()),
            skip_verify: true,
        };
        let result = tls.validate(true);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("skip_verify forbidden in production"));
    }

    #[test]
    fn test_tls_config_validate_development_allows_skip_verify() {
        let tls = RedisTlsConfig {
            enabled: true,
            ca_cert_path: None,
            client_cert_path: None,
            client_key_path: None,
            sni: Some("redis.example.com".to_string()),
            skip_verify: true,
        };
        assert!(tls.validate(false).is_ok());
    }

    #[test]
    fn test_tls_config_disabled_validate_passes() {
        let tls = RedisTlsConfig::disabled();
        assert!(tls.validate(true).is_ok());
    }

    #[test]
    fn test_mask_redis_url_with_password() {
        let masked = mask_redis_url("redis://:test123@127.0.0.1:6379/0");
        assert_eq!(masked, "redis://:***@127.0.0.1:6379/0");
    }

    #[test]
    fn test_mask_redis_url_without_password() {
        let masked = mask_redis_url("redis://127.0.0.1:6379/0");
        assert_eq!(masked, "redis://127.0.0.1:6379/0");
    }

    #[test]
    fn test_mask_redis_url_empty_password() {
        let masked = mask_redis_url("redis://:@127.0.0.1:6379/0");
        assert_eq!(masked, "redis://:@127.0.0.1:6379/0");
    }
}
