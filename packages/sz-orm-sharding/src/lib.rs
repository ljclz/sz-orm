use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

pub mod enhanced;

/// FNV-1a 64-bit 确定性哈希函数（带 MurmurHash3 fmix64 终结化）
///
/// 用于分片路由，保证跨进程/重启后同一 key 的哈希结果一致。
/// 不依赖任何随机种子，避免 `DefaultHasher`（基于 `RandomState`）的不确定性。
///
/// 注意：纯 FNV-1a 对短字符串的雪崩特性较弱，相似前缀的 key（如 `key_0`、`key_1`）
/// 哈希值高度相关，会导致一致性哈希环上分布严重不均。追加 fmix64 终结化步骤
/// 打破这种结构相关性，使哈希值在 64-bit 空间中近似均匀分布。
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

/// 分片策略
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ShardingStrategy {
    /// 哈希分片：对 key 做哈希后取模选择 shard
    Hash,
    /// 范围分片：按 key 的字节值范围选择 shard
    Range,
    /// 日期分片：按 key 中包含的日期信息（YYYY-MM-DD）选择 shard
    Date,
}

/// 分片路由错误
#[derive(Debug)]
pub enum ShardingError {
    /// 未配置任何 shard，无法路由
    NoShardsConfigured,
}

impl fmt::Display for ShardingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShardingError::NoShardsConfigured => {
                write!(f, "ShardingRouter has no shards configured")
            }
        }
    }
}

impl Error for ShardingError {}

/// 分片路由器
///
/// 根据 `ShardingStrategy` 将 key 路由到对应的 shard。
pub struct ShardingRouter {
    strategy: ShardingStrategy,
    shards: Vec<String>,
}

impl ShardingRouter {
    pub fn new(strategy: ShardingStrategy, shards: Vec<&str>) -> Self {
        Self {
            strategy,
            shards: shards.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    /// 根据 key 路由到对应的 shard
    ///
    /// # Errors
    ///
    /// 当 shards 为空时返回 [`ShardingError::NoShardsConfigured`]。
    pub fn route(&self, key: &str) -> Result<&str, ShardingError> {
        if self.shards.is_empty() {
            return Err(ShardingError::NoShardsConfigured);
        }
        let shard = match self.strategy {
            ShardingStrategy::Hash => self.route_hash(key),
            ShardingStrategy::Range => self.route_range(key),
            ShardingStrategy::Date => self.route_date(key),
        };
        Ok(shard)
    }

    /// 返回所有 shard（用于广播查询）
    pub fn query_all(&self) -> &[String] {
        &self.shards
    }

    /// 返回当前策略
    pub fn strategy(&self) -> ShardingStrategy {
        self.strategy
    }

    /// 返回 shard 数量
    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }

    fn route_hash(&self, key: &str) -> &str {
        let hash = fnv1a_hash(key);
        let idx = (hash as usize) % self.shards.len();
        &self.shards[idx]
    }

    fn route_range(&self, key: &str) -> &str {
        // 按 key 的首字节将 keyspace [0, 256) 均分到各 shard
        let first_byte = key.bytes().next().unwrap_or(0) as usize;
        let idx = (first_byte * self.shards.len()) / 256;
        &self.shards[idx.min(self.shards.len() - 1)]
    }

    fn route_date(&self, key: &str) -> &str {
        if let Some(date) = extract_date(key) {
            // 用日期中的"日"（day of month）取模
            if let Some(day) = date.get(8..10).and_then(|s| s.parse::<usize>().ok()) {
                if day >= 1 {
                    let idx = (day - 1) % self.shards.len();
                    return &self.shards[idx];
                }
            }
            // 日期解析失败，回退到日期字符串的哈希
            let hash = fnv1a_hash(&date);
            let idx = (hash as usize) % self.shards.len();
            return &self.shards[idx];
        }
        // 没有日期信息，回退到 key 整体哈希
        let hash = fnv1a_hash(key);
        let idx = (hash as usize) % self.shards.len();
        &self.shards[idx]
    }
}

/// 从字符串中提取 YYYY-MM-DD 格式的日期
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
}
