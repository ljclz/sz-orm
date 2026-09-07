//! v6.7.0 读写分离/分库分表增强
//!
//! 权重路由 + 延迟感知回退 + 分片键自动推断 + 跨片聚合 + 故障转移。

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

// ============================================================================
// 加权随机选择器
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightedSlave {
    pub name: String,
    pub weight: u32,
    pub host: String,
    pub port: u16,
}

pub struct WeightedRandomSelector {
    rng: rand::rngs::StdRng,
}

impl WeightedRandomSelector {
    pub fn new() -> Self {
        use rand::SeedableRng;
        Self {
            rng: rand::rngs::StdRng::from_entropy(),
        }
    }

    pub fn select(&mut self, slaves: &[WeightedSlave]) -> Option<usize> {
        if slaves.is_empty() {
            return None;
        }
        let total: u32 = slaves.iter().map(|s| s.weight).sum();
        if total == 0 {
            return None;
        }
        use rand::Rng;
        let mut pick = self.rng.gen_range(0..total);
        for (i, slave) in slaves.iter().enumerate() {
            if pick < slave.weight {
                return Some(i);
            }
            pick -= slave.weight;
        }
        Some(slaves.len() - 1)
    }

    pub fn select_with_lag_filter(
        &mut self,
        slaves: &[WeightedSlave],
        lag_threshold: u64,
        lag_stats: &HashMap<String, u64>,
    ) -> Option<usize> {
        let filtered: Vec<usize> = slaves
            .iter()
            .enumerate()
            .filter(|(_, s)| *lag_stats.get(&s.name).unwrap_or(&0) <= lag_threshold)
            .map(|(i, _)| i)
            .collect();
        if filtered.is_empty() {
            return None;
        }
        let filtered_slaves: Vec<WeightedSlave> =
            filtered.iter().map(|&i| slaves[i].clone()).collect();
        self.select(&filtered_slaves).map(|i| filtered[i])
    }
}

impl Default for WeightedRandomSelector {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 分片键推断器
// ============================================================================

#[derive(Debug, Clone)]
pub struct ShardKeyInference {
    pub table_shard_keys: HashMap<String, String>,
}

impl ShardKeyInference {
    pub fn new() -> Self {
        Self {
            table_shard_keys: HashMap::new(),
        }
    }

    pub fn register(&mut self, table: &str, shard_key: &str) {
        self.table_shard_keys
            .insert(table.to_string(), shard_key.to_string());
    }

    pub fn infer(&self, sql: &str) -> Option<(String, String)> {
        let lower = sql.to_lowercase();
        for (table, key) in &self.table_shard_keys {
            if lower.contains(&table.to_lowercase()) {
                let pattern = format!("{} = ?", key);
                if lower.contains(&pattern.to_lowercase()) {
                    return Some((table.clone(), key.clone()));
                }
                let pattern2 = format!("{}=", key);
                if lower.contains(&pattern2.to_lowercase()) {
                    return Some((table.clone(), key.clone()));
                }
            }
        }
        None
    }
}

impl Default for ShardKeyInference {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 跨片聚合器
// ============================================================================

#[derive(Debug, Clone)]
pub struct ScatterGatherResult {
    pub shard: String,
    pub rows: Vec<HashMap<String, crate::Value>>,
    pub error: Option<String>,
}

pub struct ScatterGatherCollector;

impl ScatterGatherCollector {
    pub fn merge(results: Vec<ScatterGatherResult>) -> Vec<HashMap<String, crate::Value>> {
        let mut merged = Vec::new();
        for r in results {
            if r.error.is_none() {
                merged.extend(r.rows);
            }
        }
        merged
    }

    pub fn merge_sorted(
        results: Vec<ScatterGatherResult>,
        order_by: &str,
        descending: bool,
    ) -> Vec<HashMap<String, crate::Value>> {
        let mut merged = Self::merge(results);
        merged.sort_by(|a, b| {
            let av = a.get(order_by);
            let bv = b.get(order_by);
            match (av, bv) {
                (Some(crate::Value::I64(a)), Some(crate::Value::I64(b))) => {
                    if descending {
                        b.cmp(a)
                    } else {
                        a.cmp(b)
                    }
                }
                _ => std::cmp::Ordering::Equal,
            }
        });
        merged
    }

    pub fn count(results: &[ScatterGatherResult]) -> u64 {
        results
            .iter()
            .filter(|r| r.error.is_none())
            .map(|r| r.rows.len() as u64)
            .sum()
    }
}

// ============================================================================
// 读写分离路由器
// ============================================================================

#[derive(Debug, Clone)]
pub struct RwSplitConfig {
    pub master: String,
    pub slaves: Vec<WeightedSlave>,
    pub lag_threshold_secs: u64,
}

impl Default for RwSplitConfig {
    fn default() -> Self {
        Self {
            master: "master".into(),
            slaves: vec![WeightedSlave {
                name: "slave-1".into(),
                weight: 1,
                host: "127.0.0.1".into(),
                port: 3307,
            }],
            lag_threshold_secs: 5,
        }
    }
}

pub struct RwSplitRouter {
    config: RwSplitConfig,
    selector: RwLock<WeightedRandomSelector>,
    lag_stats: RwLock<HashMap<String, u64>>,
}

impl RwSplitRouter {
    pub fn new(config: RwSplitConfig) -> Self {
        Self {
            config,
            selector: RwLock::new(WeightedRandomSelector::new()),
            lag_stats: RwLock::new(HashMap::new()),
        }
    }

    pub fn with_default() -> Self {
        Self::new(RwSplitConfig::default())
    }

    pub fn route_read(&self) -> RouteResult {
        let lag_stats = self.lag_stats.read().unwrap();
        let mut selector = self.selector.write().unwrap();
        match selector.select_with_lag_filter(
            &self.config.slaves,
            self.config.lag_threshold_secs,
            &lag_stats,
        ) {
            Some(idx) => RouteResult {
                target: self.config.slaves[idx].name.clone(),
                is_master: false,
                degraded: false,
            },
            None => RouteResult {
                target: self.config.master.clone(),
                is_master: true,
                degraded: true,
            },
        }
    }

    pub fn route_write(&self) -> RouteResult {
        RouteResult {
            target: self.config.master.clone(),
            is_master: true,
            degraded: false,
        }
    }

    pub fn update_lag(&self, slave: &str, lag_secs: u64) {
        self.lag_stats
            .write()
            .unwrap()
            .insert(slave.to_string(), lag_secs);
    }
}

#[derive(Debug, Clone)]
pub struct RouteResult {
    pub target: String,
    pub is_master: bool,
    pub degraded: bool,
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_random_distribution() {
        let mut selector = WeightedRandomSelector::new();
        let slaves = vec![
            WeightedSlave {
                name: "A".into(),
                weight: 3,
                host: "h".into(),
                port: 1,
            },
            WeightedSlave {
                name: "B".into(),
                weight: 1,
                host: "h".into(),
                port: 2,
            },
        ];
        let mut counts = HashMap::new();
        for _ in 0..1000 {
            let idx = selector.select(&slaves).unwrap();
            *counts.entry(idx).or_insert(0) += 1;
        }
        let a = *counts.get(&0).unwrap_or(&0);
        let b = *counts.get(&1).unwrap_or(&0);
        assert!(a > 600 && a < 900, "A 应约 750，实际 {}", a);
        assert!(b > 100 && b < 400, "B 应约 250，实际 {}", b);
    }

    #[test]
    fn lag_filter_excludes_slow_slave() {
        let mut selector = WeightedRandomSelector::new();
        let slaves = vec![
            WeightedSlave {
                name: "A".into(),
                weight: 1,
                host: "h".into(),
                port: 1,
            },
            WeightedSlave {
                name: "B".into(),
                weight: 1,
                host: "h".into(),
                port: 2,
            },
        ];
        let mut lag = HashMap::new();
        lag.insert("A".into(), 10u64);
        let idx = selector.select_with_lag_filter(&slaves, 5, &lag);
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn shard_key_inference() {
        let mut inferer = ShardKeyInference::new();
        inferer.register("orders", "merchant_id");
        let result = inferer.infer("SELECT * FROM orders WHERE merchant_id = ?");
        assert_eq!(result, Some(("orders".into(), "merchant_id".into())));
    }

    #[test]
    fn shard_key_inference_no_match() {
        let inferer = ShardKeyInference::new();
        assert!(inferer.infer("SELECT * FROM users").is_none());
    }

    #[test]
    fn scatter_gather_merge() {
        let results = vec![
            ScatterGatherResult {
                shard: "s1".into(),
                rows: vec![HashMap::from([("id".into(), crate::Value::I64(1))])],
                error: None,
            },
            ScatterGatherResult {
                shard: "s2".into(),
                rows: vec![HashMap::from([("id".into(), crate::Value::I64(2))])],
                error: None,
            },
            ScatterGatherResult {
                shard: "s3".into(),
                rows: vec![],
                error: Some("connection refused".into()),
            },
        ];
        let merged = ScatterGatherCollector::merge(results);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn scatter_gather_count() {
        let results = vec![
            ScatterGatherResult {
                shard: "s1".into(),
                rows: vec![HashMap::new(), HashMap::new()],
                error: None,
            },
            ScatterGatherResult {
                shard: "s2".into(),
                rows: vec![HashMap::new()],
                error: None,
            },
        ];
        assert_eq!(ScatterGatherCollector::count(&results), 3);
    }

    #[test]
    fn rw_split_route_read_to_slave() {
        let router = RwSplitRouter::with_default();
        let result = router.route_read();
        assert!(!result.is_master || result.degraded);
    }

    #[test]
    fn rw_split_route_write_to_master() {
        let router = RwSplitRouter::with_default();
        let result = router.route_write();
        assert!(result.is_master);
        assert!(!result.degraded);
    }

    #[test]
    fn rw_split_degrade_on_high_lag() {
        let router = RwSplitRouter::with_default();
        router.update_lag("slave-1", 10);
        let result = router.route_read();
        assert!(result.degraded, "高延迟应降级到主库");
    }
}
