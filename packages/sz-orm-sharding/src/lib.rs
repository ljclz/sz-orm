//! # SZ-ORM Sharding — 分片路由
//!
//! 提供基于 FNV-1a + fmix64 终结化的确定性哈希与一致性哈希环分片路由，
//! 保证跨进程/重启后同一 key 的路由结果一致，避免相似 key 聚集。
//!
//! ## 主要模块
//!
//! - [`enhanced`] — 增强分片能力（虚拟节点等）
//! - [`routing`] — 分片键提取器（`ShardKeyExtractor` 等）
//! - [`scatter`] — 跨分片聚合（Scatter-Gather）
//! - [`cross_shard_tx`] — 跨分片事务协调（2PC / Best Effort）

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;

pub mod cross_shard_tx;
pub mod enhanced;
pub mod routing;
pub mod scatter;

#[cfg(feature = "shard-rebalance")]
pub mod rebalancer;

// 顶层再导出常用类型，方便用户直接 `use sz_orm_sharding::*`
pub use cross_shard_tx::{
    ShardParticipant, ShardTransactionCoordinator, ShardTxError, ShardTxResult,
};
pub use routing::{CompositeKeyExtractor, FieldExtractor, ShardKeyExtractor};
pub use scatter::ScatterGather;

/// FNV-1a 64-bit deterministic hash function (with MurmurHash3 fmix64 finalization).
///
/// Used for sharding routing to guarantee that the same key hashes to the same
/// value across processes and restarts. Does not depend on any random seed,
/// avoiding the nondeterminism of `DefaultHasher` (based on `RandomState`).
///
/// Note: pure FNV-1a has weak avalanche properties for short strings; keys with
/// similar prefixes (e.g. `key_0`, `key_1`) produce highly correlated hashes,
/// causing severely uneven distribution on the consistent-hash ring. The fmix64
/// finalization step breaks this structural correlation so that hash values are
/// approximately uniformly distributed in the 64-bit space.
fn fnv1a_hash(data: &str) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET_BASIS;
    for &byte in data.as_bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    // MurmurHash3 fmix64 终结化：保证良好雪崩特性，避免相似 key 聚集
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xff51afd7ed558ccd);
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xc4ceb9fe1a85ec53);
    hash ^= hash >> 33;
    hash
}

/// Sharding strategy.
///
/// Note: from v0.3.0 this is extended to a non-`Copy` enum (with new
/// `Enum`/`List`/`Directory`/`Composite` variants that carry data). The routing
/// behavior of the three original variants `Hash`/`Range`/`Date` remains
/// backward compatible.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ShardingStrategy {
    /// Hash sharding: hash the key and select a shard by modulo.
    Hash,
    /// Range sharding: select a shard by the byte-value range of the key.
    Range,
    /// Date sharding: select a shard by the date information (YYYY-MM-DD) in the key.
    Date,
    /// Enum sharding: explicit key → shard mapping; unmatched keys go to the default shard.
    Enum {
        /// Explicit mapping table.
        mapping: HashMap<String, String>,
        /// Default shard when no match.
        default: Option<String>,
    },
    /// List sharding: route to `target` if the key is in the predefined set, otherwise default.
    List {
        /// Predefined key set.
        keys: HashSet<String>,
        /// Target shard on hit.
        target: String,
        /// Default shard on miss.
        default: Option<String>,
    },
    /// Directory sharding: dynamically query a routing table (key → shard).
    Directory {
        /// Dynamic routing table.
        table: HashMap<String, String>,
    },
    /// Composite sharding: first route by `primary` to obtain a group, then route
    /// "group:key" with `secondary` for the second-level routing.
    Composite {
        /// Primary strategy (decides the group).
        primary: Box<ShardingStrategy>,
        /// Shard list used by the primary strategy (i.e. the set of group labels).
        primary_shards: Vec<String>,
        /// Secondary strategy (routes within the group).
        secondary: Box<ShardingStrategy>,
        /// Shard list used by the secondary strategy (final shard).
        secondary_shards: Vec<String>,
    },
}

/// Sharding routing error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShardingError {
    /// No shard configured; cannot route.
    NoShardsConfigured,
    /// `Enum`/`List`/`Directory` strategy did not match the key and no default shard is set.
    NoMappingForKey(String),
    /// Worker thread panicked (used in `ScatterGather` parallel scenarios).
    ThreadPanic,
}

impl fmt::Display for ShardingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShardingError::NoShardsConfigured => {
                write!(f, "ShardingRouter has no shards configured")
            }
            ShardingError::NoMappingForKey(key) => write!(f, "no mapping for key: {}", key),
            ShardingError::ThreadPanic => write!(f, "worker thread panicked"),
        }
    }
}

impl Error for ShardingError {}

/// Sharding router.
///
/// Routes a key to the corresponding shard according to `ShardingStrategy`.
/// From v0.3.0 supports new strategies `Enum`/`List`/`Directory`/`Composite`.
pub struct ShardingRouter {
    strategy: ShardingStrategy,
    /// Used only by Hash/Range/Date; other strategies carry their own data and ignore this field.
    shards: Vec<String>,
}

impl ShardingRouter {
    /// Create a router (legacy API: takes a strategy and a shard list).
    pub fn new(strategy: ShardingStrategy, shards: Vec<&str>) -> Self {
        Self {
            strategy,
            shards: shards.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Construct an enum-sharding router.
    pub fn new_enum(mapping: HashMap<String, String>, default: Option<String>) -> Self {
        Self {
            strategy: ShardingStrategy::Enum { mapping, default },
            shards: vec![],
        }
    }

    /// Construct a list-sharding router.
    pub fn new_list(keys: HashSet<String>, target: String, default: Option<String>) -> Self {
        Self {
            strategy: ShardingStrategy::List {
                keys,
                target,
                default,
            },
            shards: vec![],
        }
    }

    /// Construct a directory-sharding router.
    pub fn new_directory(table: HashMap<String, String>) -> Self {
        Self {
            strategy: ShardingStrategy::Directory { table },
            shards: vec![],
        }
    }

    /// Construct a composite-sharding router.
    pub fn new_composite(
        primary: ShardingStrategy,
        primary_shards: Vec<String>,
        secondary: ShardingStrategy,
        secondary_shards: Vec<String>,
    ) -> Self {
        Self {
            strategy: ShardingStrategy::Composite {
                primary: Box::new(primary),
                primary_shards,
                secondary: Box::new(secondary),
                secondary_shards,
            },
            shards: vec![],
        }
    }

    /// Route a key to the corresponding shard.
    ///
    /// # Errors
    ///
    /// - Returns [`ShardingError::NoShardsConfigured`] when `shards` is empty
    ///   under Hash/Range/Date strategies.
    /// - Returns [`ShardingError::NoMappingForKey`] when Enum/List/Directory
    ///   strategies miss and no default is set.
    pub fn route(&self, key: &str) -> Result<&str, ShardingError> {
        route_strategy(&self.strategy, &self.shards, key)
    }

    /// Route via a data object and an extractor: first extract a key from `data`,
    /// then call `route(key)`.
    ///
    /// # Errors
    ///
    /// Returns the corresponding [`ShardingError`] when extraction or routing fails.
    pub fn route_by_data(
        &self,
        data: &dyn std::any::Any,
        extractor: &dyn ShardKeyExtractor,
    ) -> Result<&str, ShardingError> {
        let key = extractor.extract(data)?;
        self.route(&key)
    }

    /// Return all shards (for broadcast queries; only valid for Hash/Range/Date).
    pub fn query_all(&self) -> &[String] {
        &self.shards
    }

    /// Return the current strategy (cloned).
    pub fn strategy(&self) -> ShardingStrategy {
        self.strategy.clone()
    }

    /// Return the number of shards.
    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }
}

/// Generic routing dispatch: select a shard according to the strategy.
///
/// Implemented as a free function so that `Composite` can reuse the same logic
/// during recursive calls. The output lifetime `'a` is bound to `strategy` and
/// `shards` (the result borrows from one of them) and is independent of the
/// lifetime of `key`.
fn route_strategy<'a>(
    strategy: &'a ShardingStrategy,
    shards: &'a [String],
    key: &str,
) -> Result<&'a str, ShardingError> {
    match strategy {
        ShardingStrategy::Hash => {
            if shards.is_empty() {
                return Err(ShardingError::NoShardsConfigured);
            }
            Ok(route_hash(shards, key))
        }
        ShardingStrategy::Range => {
            if shards.is_empty() {
                return Err(ShardingError::NoShardsConfigured);
            }
            Ok(route_range(shards, key))
        }
        ShardingStrategy::Date => {
            if shards.is_empty() {
                return Err(ShardingError::NoShardsConfigured);
            }
            Ok(route_date(shards, key))
        }
        ShardingStrategy::Enum { mapping, default } => {
            if let Some(shard) = mapping.get(key) {
                Ok(shard.as_str())
            } else if let Some(d) = default {
                Ok(d.as_str())
            } else {
                Err(ShardingError::NoMappingForKey(key.to_string()))
            }
        }
        ShardingStrategy::List {
            keys,
            target,
            default,
        } => {
            if keys.contains(key) {
                Ok(target.as_str())
            } else if let Some(d) = default {
                Ok(d.as_str())
            } else {
                Err(ShardingError::NoMappingForKey(key.to_string()))
            }
        }
        ShardingStrategy::Directory { table } => table
            .get(key)
            .map(|s| s.as_str())
            .ok_or_else(|| ShardingError::NoMappingForKey(key.to_string())),
        ShardingStrategy::Composite {
            primary,
            primary_shards,
            secondary,
            secondary_shards,
        } => {
            // 一级路由得到 group 标签
            let group = route_strategy(primary, primary_shards, key)?;
            // 用 "group:key" 作为二级 key，让二级策略在 group 命名空间内路由
            let composite_key = format!("{}:{}", group, key);
            route_strategy(secondary, secondary_shards, &composite_key)
        }
    }
}

/// Hash routing: hash the key and select a shard by modulo.
///
/// The return value is borrowed from `shards` (lifetime `'a`) and is
/// independent of `key`.
fn route_hash<'a>(shards: &'a [String], key: &str) -> &'a str {
    let hash = fnv1a_hash(key);
    let idx = (hash as usize) % shards.len();
    &shards[idx]
}

/// Range routing: split the keyspace [0, 256) evenly across shards by the first
/// byte of the key.
///
/// The return value is borrowed from `shards` (lifetime `'a`) and is
/// independent of `key`.
fn route_range<'a>(shards: &'a [String], key: &str) -> &'a str {
    let first_byte = key.bytes().next().unwrap_or(0) as usize;
    let idx = (first_byte * shards.len()) / 256;
    &shards[idx.min(shards.len() - 1)]
}

/// Date routing: select a shard by taking the "day" component of the date
/// information (YYYY-MM-DD) in the key modulo the number of shards.
///
/// The return value is borrowed from `shards` (lifetime `'a`) and is
/// independent of `key`.
fn route_date<'a>(shards: &'a [String], key: &str) -> &'a str {
    if let Some(date) = extract_date(key) {
        // 用日期中的"日"（day of month）取模
        if let Some(day) = date.get(8..10).and_then(|s| s.parse::<usize>().ok()) {
            if day >= 1 {
                let idx = (day - 1) % shards.len();
                return &shards[idx];
            }
        }
        // 日期解析失败，回退到日期字符串的哈希
        let hash = fnv1a_hash(&date);
        let idx = (hash as usize) % shards.len();
        return &shards[idx];
    }
    // 没有日期信息，回退到 key 整体哈希
    let hash = fnv1a_hash(key);
    let idx = (hash as usize) % shards.len();
    &shards[idx]
}

/// Extract a date in YYYY-MM-DD format from a string.
fn extract_date(key: &str) -> Option<String> {
    let bytes = key.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    for i in 0..=bytes.len() - 10 {
        if is_digit(bytes[i])
            && is_digit(bytes[i + 1])
            && is_digit(bytes[i + 2])
            && is_digit(bytes[i + 3])
            && bytes[i + 4] == b'-'
            && is_digit(bytes[i + 5])
            && is_digit(bytes[i + 6])
            && bytes[i + 7] == b'-'
            && is_digit(bytes[i + 8])
            && is_digit(bytes[i + 9])
        {
            return String::from_utf8(bytes[i..i + 10].to_vec()).ok();
        }
    }
    None
}

fn is_digit(b: u8) -> bool {
    b.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // --- 基础测试 ---

    #[test]
    fn test_router_creation() {
        let router = ShardingRouter::new(ShardingStrategy::Hash, vec!["shard0", "shard1"]);
        assert_eq!(router.shard_count(), 2);
        assert_eq!(router.strategy(), ShardingStrategy::Hash);
    }

    #[test]
    fn test_query_all() {
        let router = ShardingRouter::new(ShardingStrategy::Range, vec!["s1", "s2", "s3"]);
        assert_eq!(router.query_all().len(), 3);
        assert_eq!(router.query_all()[0], "s1");
        assert_eq!(router.query_all()[2], "s3");
    }

    #[test]
    fn test_empty_shards_returns_error() {
        let router = ShardingRouter::new(ShardingStrategy::Hash, vec![]);
        let result = router.route("any_key");
        assert!(matches!(result, Err(ShardingError::NoShardsConfigured)));
        if let Err(err) = result {
            let msg = format!("{}", err);
            assert!(
                msg.contains("no shards configured"),
                "error message should mention empty shards, got: {}",
                msg
            );
        }
    }

    #[test]
    fn test_single_shard_always_returns_it() {
        let router = ShardingRouter::new(ShardingStrategy::Hash, vec!["only"]);
        assert_eq!(router.route("any_key").unwrap(), "only");
        assert_eq!(router.route("different").unwrap(), "only");

        let router = ShardingRouter::new(ShardingStrategy::Range, vec!["only"]);
        assert_eq!(router.route("any_key").unwrap(), "only");

        let router = ShardingRouter::new(ShardingStrategy::Date, vec!["only"]);
        assert_eq!(router.route("2026-07-18").unwrap(), "only");
    }

    // --- Hash 策略测试 ---

    #[test]
    fn test_hash_deterministic() {
        let router = ShardingRouter::new(ShardingStrategy::Hash, vec!["s0", "s1", "s2"]);
        // 同一 key 应总是路由到同一 shard
        let first = router.route("user:123").unwrap();
        for _ in 0..5 {
            assert_eq!(
                router.route("user:123").unwrap(),
                first,
                "Hash 路由应确定性"
            );
        }
    }

    #[test]
    fn test_hash_different_keys_distribute() {
        let router = ShardingRouter::new(ShardingStrategy::Hash, vec!["s0", "s1", "s2", "s3"]);
        // 大量不同 key 应分布到多个 shard
        let mut shards_hit = HashSet::new();
        for i in 0..100 {
            let key = format!("key_{}", i);
            shards_hit.insert(router.route(&key).unwrap().to_string());
        }
        assert!(
            shards_hit.len() >= 2,
            "Hash 策略在 100 个不同 key 上应至少命中 2 个 shard，实际: {}",
            shards_hit.len()
        );
    }

    #[test]
    fn test_hash_same_key_same_shard() {
        let router = ShardingRouter::new(ShardingStrategy::Hash, vec!["s0", "s1", "s2"]);
        let r1 = router.route("consistent_key").unwrap();
        let r2 = router.route("consistent_key").unwrap();
        let r3 = router.route("consistent_key").unwrap();
        assert_eq!(r1, r2);
        assert_eq!(r2, r3);
    }

    #[test]
    fn test_hash_empty_key() {
        let router = ShardingRouter::new(ShardingStrategy::Hash, vec!["s0", "s1"]);
        let shard = router.route("").unwrap();
        // 空 key 也应路由到某个有效 shard
        assert!(shard == "s0" || shard == "s1");
    }

    // --- Range 策略测试 ---

    #[test]
    fn test_range_ascii_vs_non_ascii() {
        // 2 个 shard：首字节 0-127 -> s0, 128+ -> s1
        let router = ShardingRouter::new(ShardingStrategy::Range, vec!["s0", "s1"]);
        // ASCII 字符首字节 0-127，路由到 s0
        assert_eq!(router.route("Hello").unwrap(), "s0");
        assert_eq!(router.route("world").unwrap(), "s0");
        assert_eq!(router.route("A").unwrap(), "s0");
        assert_eq!(router.route("a").unwrap(), "s0");
        // 非 ASCII 字符首字节 >= 194，路由到 s1
        assert_eq!(router.route("你好").unwrap(), "s1");
        assert_eq!(router.route("é").unwrap(), "s1");
    }

    #[test]
    fn test_range_different_keys_hit_different_shards() {
        // 3 个 shard，构造能命中所有 shard 的 key
        let router = ShardingRouter::new(ShardingStrategy::Range, vec!["s0", "s1", "s2"]);
        let mut shards_hit = HashSet::new();
        // 'A' = 65 -> (65*3)/256 = 0 -> s0
        shards_hit.insert(router.route("A").unwrap().to_string());
        // 'a' = 97 -> (97*3)/256 = 1 -> s1
        shards_hit.insert(router.route("a").unwrap().to_string());
        // 'é' 首字节 195 -> (195*3)/256 = 2 -> s2
        shards_hit.insert(router.route("é").unwrap().to_string());
        assert_eq!(
            shards_hit.len(),
            3,
            "Range 策略应能命中所有 3 个 shard，实际: {:?}",
            shards_hit
        );
    }

    #[test]
    fn test_range_deterministic() {
        let router = ShardingRouter::new(ShardingStrategy::Range, vec!["s0", "s1", "s2"]);
        let first = router.route("hello").unwrap();
        assert_eq!(router.route("hello").unwrap(), first);
        assert_eq!(router.route("hello").unwrap(), first);
    }

    #[test]
    fn test_range_empty_key_uses_zero_byte() {
        let router = ShardingRouter::new(ShardingStrategy::Range, vec!["s0", "s1"]);
        // 空 key 的首字节视为 0，应该路由到 s0
        assert_eq!(router.route("").unwrap(), "s0");
    }

    #[test]
    fn test_range_keys_with_similar_prefixes_cluster() {
        // 相似前缀的 key 应路由到相同 shard（Range 的核心特性）
        let router = ShardingRouter::new(ShardingStrategy::Range, vec!["s0", "s1"]);
        let shard1 = router.route("user:123").unwrap();
        let shard2 = router.route("user:456").unwrap();
        let shard3 = router.route("user:789").unwrap();
        assert_eq!(shard1, shard2);
        assert_eq!(shard2, shard3);
    }

    // --- Date 策略测试 ---

    #[test]
    fn test_date_day_based_routing() {
        // 3 个 shard，day 1..31 取模 3
        let router = ShardingRouter::new(ShardingStrategy::Date, vec!["s0", "s1", "s2"]);
        // day=1 -> (1-1)%3 = 0 -> s0
        assert_eq!(router.route("2026-07-01").unwrap(), "s0");
        // day=2 -> (2-1)%3 = 1 -> s1
        assert_eq!(router.route("2026-07-02").unwrap(), "s1");
        // day=3 -> (3-1)%3 = 2 -> s2
        assert_eq!(router.route("2026-07-03").unwrap(), "s2");
        // day=4 -> (4-1)%3 = 0 -> s0
        assert_eq!(router.route("2026-07-04").unwrap(), "s0");
    }

    #[test]
    fn test_date_different_days_distribute() {
        let router = ShardingRouter::new(ShardingStrategy::Date, vec!["s0", "s1", "s2", "s3"]);
        let mut shards_hit = HashSet::new();
        for day in 1..=28 {
            let key = format!("2026-07-{:02}", day);
            shards_hit.insert(router.route(&key).unwrap().to_string());
        }
        // 28 天应分布到所有 4 个 shard
        assert_eq!(
            shards_hit.len(),
            4,
            "Date 策略 28 天应命中所有 4 个 shard，实际: {}",
            shards_hit.len()
        );
    }

    #[test]
    fn test_date_extract_from_longer_key() {
        let router = ShardingRouter::new(ShardingStrategy::Date, vec!["s0", "s1", "s2"]);
        // 日期嵌入在更长的 key 中
        let shard1 = router.route("log:2026-07-15:entry1").unwrap();
        let shard2 = router.route("2026-07-15").unwrap();
        assert_eq!(shard1, shard2, "包含相同日期的 key 应路由到相同 shard");
    }

    #[test]
    fn test_date_deterministic() {
        let router = ShardingRouter::new(ShardingStrategy::Date, vec!["s0", "s1", "s2"]);
        let first = router.route("2026-07-18").unwrap();
        assert_eq!(router.route("2026-07-18").unwrap(), first);
    }

    #[test]
    fn test_date_no_date_falls_back_to_hash() {
        let router = ShardingRouter::new(ShardingStrategy::Date, vec!["s0", "s1", "s2"]);
        // 没有日期信息的 key 应回退到哈希路由（仍返回有效 shard）
        let shard = router.route("plain_key_without_date").unwrap();
        assert!(shard == "s0" || shard == "s1" || shard == "s2");
        // 且确定性
        assert_eq!(router.route("plain_key_without_date").unwrap(), shard);
    }

    #[test]
    fn test_date_different_months_same_day_same_shard() {
        let router = ShardingRouter::new(ShardingStrategy::Date, vec!["s0", "s1", "s2"]);
        // 同一天不同月应路由到相同 shard（因为只看 day）
        let july_15 = router.route("2026-07-15").unwrap();
        let aug_15 = router.route("2026-08-15").unwrap();
        assert_eq!(july_15, aug_15);
    }

    #[test]
    fn test_date_invalid_date_falls_back() {
        let router = ShardingRouter::new(ShardingStrategy::Date, vec!["s0", "s1"]);
        // "2026-00-00" 日为 00，无法解析为有效 day（parse::<usize> 得到 0，不满足 >= 1）
        // 应回退到日期字符串的哈希
        let shard = router.route("2026-00-00").unwrap();
        assert!(shard == "s0" || shard == "s1");
    }

    // --- 跨策略测试 ---

    #[test]
    fn test_different_strategies_may_route_differently() {
        let key = "2026-07-15";
        let hash_router = ShardingRouter::new(ShardingStrategy::Hash, vec!["s0", "s1", "s2"]);
        let date_router = ShardingRouter::new(ShardingStrategy::Date, vec!["s0", "s1", "s2"]);

        // 不要求一定不同，但都应返回有效 shard
        let hash_shard = hash_router.route(key).unwrap();
        let date_shard = date_router.route(key).unwrap();
        assert!(!hash_shard.is_empty());
        assert!(!date_shard.is_empty());
    }

    // --- extract_date 单元测试 ---

    #[test]
    fn test_extract_date_pure_date() {
        assert_eq!(extract_date("2026-07-18"), Some("2026-07-18".to_string()));
        assert_eq!(extract_date("2025-01-01"), Some("2025-01-01".to_string()));
    }

    #[test]
    fn test_extract_date_embedded() {
        assert_eq!(
            extract_date("log:2026-07-18:entry"),
            Some("2026-07-18".to_string())
        );
    }

    #[test]
    fn test_extract_date_no_date() {
        assert_eq!(extract_date("no date here"), None);
        assert_eq!(extract_date("2026/07/18"), None);
        assert_eq!(extract_date(""), None);
        assert_eq!(extract_date("short"), None);
    }

    #[test]
    fn test_extract_date_invalid_format() {
        assert_eq!(extract_date("2026-7-18"), None); // 月需要 2 位
        assert_eq!(extract_date("2026-07-8"), None); // 日需要 2 位
        assert_eq!(extract_date("abcd-07-18"), None); // 年需为数字
    }

    // ==================== v0.3.0 新增策略测试 ====================

    // --- Enum 策略测试 ---

    #[test]
    fn test_enum_route_hit() {
        let mut mapping = HashMap::new();
        mapping.insert("cn".to_string(), "shard_cn".to_string());
        mapping.insert("us".to_string(), "shard_us".to_string());
        mapping.insert("eu".to_string(), "shard_eu".to_string());
        let router = ShardingRouter::new_enum(mapping, None);
        assert_eq!(router.route("cn").unwrap(), "shard_cn");
        assert_eq!(router.route("us").unwrap(), "shard_us");
        assert_eq!(router.route("eu").unwrap(), "shard_eu");
    }

    #[test]
    fn test_enum_route_miss_with_default() {
        let mut mapping = HashMap::new();
        mapping.insert("cn".to_string(), "shard_cn".to_string());
        let router = ShardingRouter::new_enum(mapping, Some("shard_default".to_string()));
        assert_eq!(router.route("unknown").unwrap(), "shard_default");
        // 命中映射的仍返回映射值
        assert_eq!(router.route("cn").unwrap(), "shard_cn");
    }

    #[test]
    fn test_enum_route_miss_no_default_errors() {
        let router = ShardingRouter::new_enum(HashMap::new(), None);
        let result = router.route("unknown");
        assert!(matches!(result, Err(ShardingError::NoMappingForKey(_))));
        if let Err(ShardingError::NoMappingForKey(key)) = result {
            assert_eq!(key, "unknown");
        } else {
            panic!("expected NoMappingForKey");
        }
    }

    #[test]
    fn test_enum_route_deterministic() {
        let mut mapping = HashMap::new();
        mapping.insert("k1".to_string(), "s_a".to_string());
        let router = ShardingRouter::new_enum(mapping, Some("s_def".to_string()));
        let r1 = router.route("k1").unwrap();
        let r2 = router.route("k1").unwrap();
        assert_eq!(r1, r2);
        assert_eq!(router.route("k2").unwrap(), "s_def");
    }

    // --- List 策略测试 ---

    #[test]
    fn test_list_route_hit() {
        let mut keys = HashSet::new();
        keys.insert("vip1".to_string());
        keys.insert("vip2".to_string());
        keys.insert("vip3".to_string());
        let router = ShardingRouter::new_list(keys, "vip_shard".to_string(), None);
        assert_eq!(router.route("vip1").unwrap(), "vip_shard");
        assert_eq!(router.route("vip2").unwrap(), "vip_shard");
        assert_eq!(router.route("vip3").unwrap(), "vip_shard");
    }

    #[test]
    fn test_list_route_miss_with_default() {
        let keys = HashSet::new();
        let router = ShardingRouter::new_list(
            keys,
            "vip_shard".to_string(),
            Some("normal_shard".to_string()),
        );
        // 命中列表的 key 路由到 target
        assert_eq!(router.route("any_non_listed").unwrap(), "normal_shard");
    }

    #[test]
    fn test_list_route_miss_no_default_errors() {
        let router = ShardingRouter::new_list(HashSet::new(), "vip_shard".to_string(), None);
        let result = router.route("unknown");
        assert!(matches!(result, Err(ShardingError::NoMappingForKey(_))));
    }

    #[test]
    fn test_list_route_with_members_and_default() {
        let mut keys = HashSet::new();
        keys.insert("gold".to_string());
        let router = ShardingRouter::new_list(
            keys,
            "premium_shard".to_string(),
            Some("standard_shard".to_string()),
        );
        assert_eq!(router.route("gold").unwrap(), "premium_shard");
        assert_eq!(router.route("silver").unwrap(), "standard_shard");
    }

    // --- Directory 策略测试 ---

    #[test]
    fn test_directory_route_hit() {
        let mut table = HashMap::new();
        table.insert("user:1".to_string(), "dir_shard_a".to_string());
        table.insert("user:2".to_string(), "dir_shard_b".to_string());
        let router = ShardingRouter::new_directory(table);
        assert_eq!(router.route("user:1").unwrap(), "dir_shard_a");
        assert_eq!(router.route("user:2").unwrap(), "dir_shard_b");
    }

    #[test]
    fn test_directory_route_miss_errors() {
        let router = ShardingRouter::new_directory(HashMap::new());
        assert!(matches!(
            router.route("missing"),
            Err(ShardingError::NoMappingForKey(_))
        ));
    }

    #[test]
    fn test_directory_route_deterministic() {
        let mut table = HashMap::new();
        table.insert("k".to_string(), "v".to_string());
        let router = ShardingRouter::new_directory(table);
        let r1 = router.route("k").unwrap();
        let r2 = router.route("k").unwrap();
        assert_eq!(r1, r2);
        assert_eq!(r1, "v");
    }

    // --- Composite 策略测试 ---

    #[test]
    fn test_composite_route_basic() {
        // 一级：Hash 在 [g0, g1] 上路由得 group
        // 二级：Hash 在 [s0, s1, s2] 上路由得最终 shard
        let router = ShardingRouter::new_composite(
            ShardingStrategy::Hash,
            vec!["g0".to_string(), "g1".to_string()],
            ShardingStrategy::Hash,
            vec!["s0".to_string(), "s1".to_string(), "s2".to_string()],
        );
        let result = router.route("user:123").unwrap();
        assert!(
            result == "s0" || result == "s1" || result == "s2",
            "composite result should be in secondary shards, got {}",
            result
        );
    }

    #[test]
    fn test_composite_route_deterministic() {
        let router = ShardingRouter::new_composite(
            ShardingStrategy::Hash,
            vec!["g0".to_string(), "g1".to_string()],
            ShardingStrategy::Hash,
            vec!["s0".to_string(), "s1".to_string()],
        );
        let r1 = router.route("user:123").unwrap();
        for _ in 0..5 {
            assert_eq!(router.route("user:123").unwrap(), r1);
        }
    }

    #[test]
    fn test_composite_uses_group_in_secondary_key() {
        // 构造两个不同 group 的场景：一级用 Enum 强制分组
        // groupA 的所有 key 经二级 Hash 在 [s0, s1] 路由
        // groupB 的所有 key 经二级 Hash 在 [s0, s1] 路由，但二级 key 含不同 group 前缀
        let mut mapping = HashMap::new();
        mapping.insert("a".to_string(), "grpA".to_string());
        mapping.insert("b".to_string(), "grpB".to_string());
        let router = ShardingRouter::new_composite(
            ShardingStrategy::Enum {
                mapping,
                default: None,
            },
            vec!["grpA".to_string(), "grpB".to_string()], // primary_shards（仅占位，Enum 不使用）
            ShardingStrategy::Hash,
            vec!["s0".to_string(), "s1".to_string()],
        );
        // key "a" 与 "b" 走不同 group，二级用 "grpA:a" / "grpB:b" 路由
        let ra = router.route("a").unwrap();
        let rb = router.route("b").unwrap();
        // 都应路由到有效 shard
        assert!(ra == "s0" || ra == "s1");
        assert!(rb == "s0" || rb == "s1");
    }

    #[test]
    fn test_composite_primary_empty_shards_errors() {
        // primary 用 Hash 但 primary_shards 为空 → NoShardsConfigured
        let router = ShardingRouter::new_composite(
            ShardingStrategy::Hash,
            vec![],
            ShardingStrategy::Hash,
            vec!["s0".to_string()],
        );
        assert!(matches!(
            router.route("k"),
            Err(ShardingError::NoShardsConfigured)
        ));
    }

    #[test]
    fn test_composite_secondary_empty_shards_errors() {
        // secondary 用 Hash 但 secondary_shards 为空 → NoShardsConfigured
        let router = ShardingRouter::new_composite(
            ShardingStrategy::Hash,
            vec!["g0".to_string()],
            ShardingStrategy::Hash,
            vec![],
        );
        assert!(matches!(
            router.route("k"),
            Err(ShardingError::NoShardsConfigured)
        ));
    }

    // --- ShardingError 新增变体测试 ---

    #[test]
    fn test_sharding_error_no_mapping_display() {
        let err = ShardingError::NoMappingForKey("k1".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("no mapping for key: k1"));
    }

    #[test]
    fn test_sharding_error_thread_panic_display() {
        let err = ShardingError::ThreadPanic;
        let msg = format!("{}", err);
        assert!(msg.contains("panicked"));
    }
}
