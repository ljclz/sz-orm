//! 任务分片器（Task Sharder）
//!
//! 将大任务拆分为多个小分片，支持并行处理。
//! 适用于批量查询、数据迁移等场景。

use std::collections::HashMap;

/// 分片策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ShardStrategy {
    /// 按范围分片
    Range,
    /// 按哈希分片
    Hash,
    /// 轮询分片
    RoundRobin,
    /// 按键分片
    ByKey,
}

impl ShardStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            ShardStrategy::Range => "range",
            ShardStrategy::Hash => "hash",
            ShardStrategy::RoundRobin => "round_robin",
            ShardStrategy::ByKey => "by_key",
        }
    }
}

/// 分片
#[derive(Debug, Clone, serde::Serialize)]
pub struct Shard {
    pub id: usize,
    pub start: u64,
    pub end: u64,
    pub estimated_items: u64,
}

impl Shard {
    pub fn new(id: usize, start: u64, end: u64) -> Self {
        Self {
            id,
            start,
            end,
            estimated_items: end.saturating_sub(start) + 1,
        }
    }

    pub fn size(&self) -> u64 {
        self.estimated_items
    }

    pub fn contains(&self, value: u64) -> bool {
        (self.start..=self.end).contains(&value)
    }
}

/// 任务分片器
pub struct TaskSharder {
    strategy: ShardStrategy,
    shard_count: usize,
}

impl TaskSharder {
    pub fn new(strategy: ShardStrategy, shard_count: usize) -> Self {
        Self {
            strategy,
            shard_count: shard_count.max(1),
        }
    }

    pub fn strategy(&self) -> ShardStrategy {
        self.strategy
    }

    pub fn shard_count(&self) -> usize {
        self.shard_count
    }

    /// 按范围分片
    ///
    /// 将 `[0, total)` 均匀拆分为 `shard_count` 个分片。
    pub fn shard_range(&self, total: u64) -> Vec<Shard> {
        if total == 0 {
            return Vec::new();
        }
        let shard_size = total.div_ceil(self.shard_count as u64);
        let mut shards = Vec::with_capacity(self.shard_count);
        for i in 0..self.shard_count {
            let start = i as u64 * shard_size;
            if start >= total {
                break;
            }
            let end = ((i as u64 + 1) * shard_size)
                .saturating_sub(1)
                .min(total - 1);
            shards.push(Shard::new(i, start, end));
        }
        shards
    }

    /// 按哈希分片
    ///
    /// 将键分配到固定数量的分片。
    pub fn shard_by_hash(&self, key: &str) -> usize {
        let hash = Self::hash_key(key);
        (hash % self.shard_count as u64) as usize
    }

    fn hash_key(key: &str) -> u64 {
        let mut hash: u64 = 0;
        for byte in key.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
        hash
    }

    /// 轮询分片
    ///
    /// 根据索引轮流分配分片。
    pub fn shard_round_robin(&self, index: usize) -> usize {
        index % self.shard_count
    }

    /// 按键分片
    ///
    /// 将键映射到分片（相同键总是到同一分片）。
    pub fn shard_by_key(&self, key: &str) -> usize {
        match self.strategy {
            ShardStrategy::Hash => self.shard_by_hash(key),
            _ => self.shard_by_hash(key),
        }
    }

    /// 将键列表分配到分片
    pub fn distribute_keys(&self, keys: &[String]) -> HashMap<usize, Vec<String>> {
        let mut distribution: HashMap<usize, Vec<String>> = HashMap::new();
        for (i, key) in keys.iter().enumerate() {
            let shard_id = match self.strategy {
                ShardStrategy::RoundRobin => self.shard_round_robin(i),
                _ => self.shard_by_key(key),
            };
            distribution.entry(shard_id).or_default().push(key.clone());
        }
        distribution
    }

    /// 将索引范围分配到分片
    pub fn distribute_indices(&self, total: u64) -> HashMap<usize, Shard> {
        let shards = self.shard_range(total);
        shards.into_iter().map(|s| (s.id, s)).collect()
    }

    /// 估算每个分片的负载
    pub fn estimate_load(&self, total: u64) -> Vec<u64> {
        let shards = self.shard_range(total);
        let mut loads = vec![0u64; self.shard_count];
        for shard in shards {
            loads[shard.id] = shard.size();
        }
        loads
    }

    /// 负载均衡度（0.0~1.0，1.0 表示完全均衡）
    pub fn balance_score(&self, loads: &[u64]) -> f64 {
        if loads.is_empty() {
            return 1.0;
        }
        let total: u64 = loads.iter().sum();
        if total == 0 {
            return 1.0;
        }
        let avg = total as f64 / loads.len() as f64;
        if avg == 0.0 {
            return 1.0;
        }
        let variance: f64 =
            loads.iter().map(|&l| (l as f64 - avg).powi(2)).sum::<f64>() / loads.len() as f64;
        let std_dev = variance.sqrt();
        (1.0 - std_dev / avg).max(0.0)
    }
}

/// 动态分片器
///
/// 根据运行时负载动态调整分片大小。
pub struct DynamicSharder {
    base_sharder: TaskSharder,
    load_history: std::sync::RwLock<Vec<Vec<u64>>>,
    adjustment_threshold: f64,
}

impl DynamicSharder {
    pub fn new(shard_count: usize, adjustment_threshold: f64) -> Self {
        Self {
            base_sharder: TaskSharder::new(ShardStrategy::Range, shard_count),
            load_history: std::sync::RwLock::new(Vec::new()),
            adjustment_threshold,
        }
    }

    pub fn record_load(&self, loads: Vec<u64>) {
        if let Ok(mut history) = self.load_history.write() {
            history.push(loads);
            if history.len() > 100 {
                history.remove(0);
            }
        }
    }

    pub fn shard_count(&self) -> usize {
        self.base_sharder.shard_count()
    }

    pub fn shard_range(&self, total: u64) -> Vec<Shard> {
        self.base_sharder.shard_range(total)
    }

    pub fn needs_rebalance(&self) -> bool {
        let history = self.load_history.read().ok();
        match history {
            Some(h) => {
                if h.is_empty() {
                    return false;
                }
                let last = h.last().unwrap();
                let score = self.base_sharder.balance_score(last);
                score < self.adjustment_threshold
            }
            None => false,
        }
    }

    pub fn average_load(&self) -> Vec<f64> {
        let history = self.load_history.read().ok();
        match history {
            Some(h) => {
                if h.is_empty() {
                    return Vec::new();
                }
                let count = h.len() as f64;
                let shard_count = h[0].len();
                let mut avg = vec![0.0; shard_count];
                for loads in h.iter() {
                    for (i, &l) in loads.iter().enumerate() {
                        if i < shard_count {
                            avg[i] += l as f64;
                        }
                    }
                }
                for a in &mut avg {
                    *a /= count;
                }
                avg
            }
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shard_strategy_as_str() {
        assert_eq!(ShardStrategy::Range.as_str(), "range");
        assert_eq!(ShardStrategy::Hash.as_str(), "hash");
        assert_eq!(ShardStrategy::RoundRobin.as_str(), "round_robin");
        assert_eq!(ShardStrategy::ByKey.as_str(), "by_key");
    }

    #[test]
    fn test_shard_new() {
        let s = Shard::new(0, 10, 20);
        assert_eq!(s.size(), 11);
        assert!(s.contains(15));
        assert!(!s.contains(5));
    }

    #[test]
    fn test_task_sharder_range() {
        let sharder = TaskSharder::new(ShardStrategy::Range, 4);
        let shards = sharder.shard_range(100);
        assert_eq!(shards.len(), 4);
        let total: u64 = shards.iter().map(|s| s.size()).sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn test_task_sharder_range_uneven() {
        let sharder = TaskSharder::new(ShardStrategy::Range, 3);
        let shards = sharder.shard_range(10);
        assert_eq!(shards.len(), 3);
        let total: u64 = shards.iter().map(|s| s.size()).sum();
        assert_eq!(total, 10);
    }

    #[test]
    fn test_task_sharder_range_zero() {
        let sharder = TaskSharder::new(ShardStrategy::Range, 4);
        let shards = sharder.shard_range(0);
        assert!(shards.is_empty());
    }

    #[test]
    fn test_task_sharder_hash() {
        let sharder = TaskSharder::new(ShardStrategy::Hash, 4);
        let s1 = sharder.shard_by_hash("key1");
        let s2 = sharder.shard_by_hash("key1");
        assert_eq!(s1, s2);
        assert!(s1 < 4);
    }

    #[test]
    fn test_task_sharder_round_robin() {
        let sharder = TaskSharder::new(ShardStrategy::RoundRobin, 3);
        assert_eq!(sharder.shard_round_robin(0), 0);
        assert_eq!(sharder.shard_round_robin(1), 1);
        assert_eq!(sharder.shard_round_robin(2), 2);
        assert_eq!(sharder.shard_round_robin(3), 0);
    }

    #[test]
    fn test_task_sharder_distribute_keys() {
        let sharder = TaskSharder::new(ShardStrategy::Hash, 3);
        let keys = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        let dist = sharder.distribute_keys(&keys);
        let total: usize = dist.values().map(|v| v.len()).sum();
        assert_eq!(total, 4);
    }

    #[test]
    fn test_task_sharder_distribute_indices() {
        let sharder = TaskSharder::new(ShardStrategy::Range, 4);
        let dist = sharder.distribute_indices(100);
        assert_eq!(dist.len(), 4);
    }

    #[test]
    fn test_task_sharder_estimate_load() {
        let sharder = TaskSharder::new(ShardStrategy::Range, 4);
        let loads = sharder.estimate_load(100);
        let total: u64 = loads.iter().sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn test_task_sharder_balance_score_perfect() {
        let sharder = TaskSharder::new(ShardStrategy::Range, 4);
        let loads = vec![25, 25, 25, 25];
        assert!((sharder.balance_score(&loads) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_task_sharder_balance_score_unbalanced() {
        let sharder = TaskSharder::new(ShardStrategy::Range, 4);
        let loads = vec![100, 0, 0, 0];
        let score = sharder.balance_score(&loads);
        assert!(score < 0.5);
    }

    #[test]
    fn test_task_sharder_balance_score_empty() {
        let sharder = TaskSharder::new(ShardStrategy::Range, 4);
        assert_eq!(sharder.balance_score(&[]), 1.0);
    }

    #[test]
    fn test_task_sharder_strategy() {
        let sharder = TaskSharder::new(ShardStrategy::Hash, 4);
        assert_eq!(sharder.strategy(), ShardStrategy::Hash);
    }

    #[test]
    fn test_task_sharder_shard_count() {
        let sharder = TaskSharder::new(ShardStrategy::Range, 8);
        assert_eq!(sharder.shard_count(), 8);
    }

    #[test]
    fn test_task_sharder_shard_by_key() {
        let sharder = TaskSharder::new(ShardStrategy::ByKey, 4);
        let s1 = sharder.shard_by_key("test");
        let s2 = sharder.shard_by_key("test");
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_dynamic_sharder_new() {
        let sharder = DynamicSharder::new(4, 0.8);
        assert_eq!(sharder.shard_count(), 4);
    }

    #[test]
    fn test_dynamic_sharder_record_load() {
        let sharder = DynamicSharder::new(4, 0.8);
        sharder.record_load(vec![25, 25, 25, 25]);
        assert!(!sharder.needs_rebalance());
    }

    #[test]
    fn test_dynamic_sharder_needs_rebalance() {
        let sharder = DynamicSharder::new(4, 0.8);
        sharder.record_load(vec![100, 0, 0, 0]);
        assert!(sharder.needs_rebalance());
    }

    #[test]
    fn test_dynamic_sharder_average_load() {
        let sharder = DynamicSharder::new(4, 0.8);
        sharder.record_load(vec![10, 20, 30, 40]);
        sharder.record_load(vec![20, 30, 40, 50]);
        let avg = sharder.average_load();
        assert_eq!(avg.len(), 4);
        assert!((avg[0] - 15.0).abs() < 0.001);
    }

    #[test]
    fn test_dynamic_sharder_shard_range() {
        let sharder = DynamicSharder::new(4, 0.8);
        let shards = sharder.shard_range(100);
        assert_eq!(shards.len(), 4);
    }

    #[test]
    fn test_dynamic_sharder_no_history() {
        let sharder = DynamicSharder::new(4, 0.8);
        assert!(!sharder.needs_rebalance());
        assert!(sharder.average_load().is_empty());
    }
}
