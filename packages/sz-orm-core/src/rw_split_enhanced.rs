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
// ============================================================================
// v7.3.0 自动主备故障转移协调器（auto-failover feature gate）
// ============================================================================

#[cfg(feature = "auto-failover")]
mod failover {
    use super::*;
    use crate::{FailbackStrategy, FailoverConfig};
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
    use std::time::Instant;

    /// 故障转移错误
    #[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
    pub enum FailoverError {
        /// 故障转移正在进行中（并发防护）
        #[error("failover already in progress")]
        AlreadyInProgress,
        /// 位点校验失败（备库落后主库，可能丢失事务）
        #[error("replica LSN {replica_lsn} behind primary {primary_lsn}, potential data loss")]
        ReplicaBehind { replica_lsn: u64, primary_lsn: u64 },
        /// 回切条件不满足
        #[error("failback conditions not met: {0}")]
        FailbackConditionsNotMet(String),
        /// 探活失败
        #[error("probe failed: {0}")]
        ProbeFailed(String),
    }

    /// 故障转移决策
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FailoverDecision {
        /// 决策时间戳（毫秒）
        pub timestamp_ms: i64,
        /// 触发原因
        pub reason: String,
        /// 是否执行了切换
        pub switched: bool,
        /// 位点校验结果（None 表示未校验，Some 表示校验结果）
        pub lsn_check: Option<LsnCheckResult>,
    }

    /// 位点校验结果
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LsnCheckResult {
        /// 主库故障时 LSN
        pub primary_lsn: u64,
        /// 备库当前 LSN
        pub replica_lsn: u64,
        /// 是否落后（true 表示可能丢失事务）
        pub behind: bool,
        /// 可能丢失的事务数（behind=true 时有意义）
        pub potential_lost_txns: u64,
    }

    /// 探活结果
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ProbeResult {
        /// 是否成功
        pub success: bool,
        /// 探活时间戳（毫秒）
        pub timestamp_ms: i64,
        /// 失败原因（success=false 时有意义）
        pub error: Option<String>,
    }

    /// 自动故障转移协调器（v7.3.0）
    ///
    /// 持有主备拓扑 + 探活循环 + 位点校验 + 回切策略 + 决策历史。
    /// 复用 `failover_in_progress` AtomicBool CAS 进行并发防护。
    /// RTO ≤ 5s（由 `FailoverConfig.probe_interval ≤ 5s` 保证）。
    pub struct AutoFailoverCoordinator {
        config: FailoverConfig,
        /// 并发防护：CAS 保证同一时刻只有一个故障转移在进行
        failover_in_progress: AtomicBool,
        /// 当前是否已故障转移到备库
        failed_over: AtomicBool,
        /// 连续探活失败计数
        consecutive_probe_failures: AtomicU32,
        /// 决策历史
        decision_history: RwLock<Vec<FailoverDecision>>,
        /// 主库 LSN（故障位点）
        primary_lsn: AtomicU64,
        /// 备库 LSN
        replica_lsn: AtomicU64,
        /// 总探活次数
        total_probes: AtomicU64,
        /// 探活成功次数
        successful_probes: AtomicU64,
        /// 上次探活时间
        last_probe_at: RwLock<Option<Instant>>,
    }

    impl AutoFailoverCoordinator {
        /// 创建协调器
        pub fn new(config: FailoverConfig) -> Self {
            Self {
                config,
                failover_in_progress: AtomicBool::new(false),
                failed_over: AtomicBool::new(false),
                consecutive_probe_failures: AtomicU32::new(0),
                decision_history: RwLock::new(Vec::new()),
                primary_lsn: AtomicU64::new(0),
                replica_lsn: AtomicU64::new(0),
                total_probes: AtomicU64::new(0),
                successful_probes: AtomicU64::new(0),
                last_probe_at: RwLock::new(None),
            }
        }

        /// 配置引用
        pub fn config(&self) -> &FailoverConfig {
            &self.config
        }

        /// 记录一次探活结果（由探活循环或外部探活器调用）
        ///
        /// 连续失败达 `probe_failure_threshold` 时自动触发故障转移。
        pub fn record_probe(&self, result: ProbeResult) {
            self.total_probes.fetch_add(1, Ordering::SeqCst);
            if let Ok(mut guard) = self.last_probe_at.write() {
                *guard = Some(Instant::now());
            }
            if result.success {
                self.successful_probes.fetch_add(1, Ordering::SeqCst);
                self.consecutive_probe_failures.store(0, Ordering::SeqCst);
            } else {
                let failures = self
                    .consecutive_probe_failures
                    .fetch_add(1, Ordering::SeqCst)
                    + 1;
                if failures >= self.config.probe_failure_threshold
                    && !self.failed_over.load(Ordering::SeqCst)
                {
                    // 达到阈值且未已故障转移，触发故障转移
                    let _ = self.trigger_failover_internal(&result);
                }
            }
        }

        /// 触发故障转移（并发防护：CAS 保证同一时刻只有一个故障转移）
        ///
        /// 返回决策记录。若已有故障转移在进行中，返回 `AlreadyInProgress`。
        pub fn trigger_failover(&self) -> Result<FailoverDecision, FailoverError> {
            self.trigger_failover_internal(&ProbeResult {
                success: false,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                error: Some("manual trigger".to_string()),
            })
        }

        fn trigger_failover_internal(
            &self,
            probe: &ProbeResult,
        ) -> Result<FailoverDecision, FailoverError> {
            // CAS 并发防护
            if self
                .failover_in_progress
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                return Err(FailoverError::AlreadyInProgress);
            }

            // 位点校验
            let primary_lsn = self.primary_lsn.load(Ordering::SeqCst);
            let replica_lsn = self.replica_lsn.load(Ordering::SeqCst);
            let lsn_check = if primary_lsn > replica_lsn {
                Some(LsnCheckResult {
                    primary_lsn,
                    replica_lsn,
                    behind: true,
                    potential_lost_txns: primary_lsn - replica_lsn,
                })
            } else {
                Some(LsnCheckResult {
                    primary_lsn,
                    replica_lsn,
                    behind: false,
                    potential_lost_txns: 0,
                })
            };

            // 执行切换
            self.failed_over.store(true, Ordering::SeqCst);
            self.consecutive_probe_failures.store(0, Ordering::SeqCst);

            let decision = FailoverDecision {
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                reason: format!(
                    "probe failed: {:?}, consecutive failures reached threshold {}",
                    probe.error, self.config.probe_failure_threshold
                ),
                switched: true,
                lsn_check: lsn_check.clone(),
            };

            // 记录决策历史
            if let Ok(mut history) = self.decision_history.write() {
                history.push(decision.clone());
            }

            // 释放 CAS 锁
            self.failover_in_progress.store(false, Ordering::SeqCst);

            // 位点落后时返回错误（不静默丢数据）
            if let Some(check) = &lsn_check {
                if check.behind {
                    return Err(FailoverError::ReplicaBehind {
                        replica_lsn: check.replica_lsn,
                        primary_lsn: check.primary_lsn,
                    });
                }
            }

            Ok(decision)
        }

        /// 回切到主库
        ///
        /// - `Manual`：等待运维确认，直接执行回切
        /// - `Auto`：经健康+一致性校验后自动回切
        pub fn failback(&self, strategy: FailbackStrategy) -> Result<(), FailoverError> {
            if !self.failed_over.load(Ordering::SeqCst) {
                return Err(FailoverError::FailbackConditionsNotMet(
                    "not in failed-over state".to_string(),
                ));
            }

            match strategy {
                FailbackStrategy::Manual => {
                    // 运维确认后直接回切
                    self.failed_over.store(false, Ordering::SeqCst);
                    self.consecutive_probe_failures.store(0, Ordering::SeqCst);
                    Ok(())
                }
                FailbackStrategy::Auto => {
                    // 自动回切：校验主库健康（连续探活失败为 0）+ 位点一致性
                    let failures = self.consecutive_probe_failures.load(Ordering::SeqCst);
                    if failures > 0 {
                        return Err(FailoverError::FailbackConditionsNotMet(format!(
                            "primary still unhealthy: {failures} consecutive failures"
                        )));
                    }
                    let primary_lsn = self.primary_lsn.load(Ordering::SeqCst);
                    let replica_lsn = self.replica_lsn.load(Ordering::SeqCst);
                    if replica_lsn < primary_lsn {
                        return Err(FailoverError::FailbackConditionsNotMet(format!(
                            "replica LSN {replica_lsn} behind primary {primary_lsn}"
                        )));
                    }
                    self.failed_over.store(false, Ordering::SeqCst);
                    Ok(())
                }
            }
        }

        /// 决策历史
        pub fn decision_history(&self) -> Vec<FailoverDecision> {
            self.decision_history
                .read()
                .map(|h| h.clone())
                .unwrap_or_default()
        }

        /// 当前是否已故障转移
        pub fn is_failed_over(&self) -> bool {
            self.failed_over.load(Ordering::SeqCst)
        }

        /// 连续探活失败次数
        pub fn consecutive_probe_failures(&self) -> u32 {
            self.consecutive_probe_failures.load(Ordering::SeqCst)
        }

        /// 更新主库 LSN（由位点追踪器调用）
        pub fn set_primary_lsn(&self, lsn: u64) {
            self.primary_lsn.store(lsn, Ordering::SeqCst);
        }

        /// 更新备库 LSN（由位点追踪器调用）
        pub fn set_replica_lsn(&self, lsn: u64) {
            self.replica_lsn.store(lsn, Ordering::SeqCst);
        }

        /// 总探活次数
        pub fn total_probes(&self) -> u64 {
            self.total_probes.load(Ordering::SeqCst)
        }

        /// 探活成功次数
        pub fn successful_probes(&self) -> u64 {
            self.successful_probes.load(Ordering::SeqCst)
        }
    }
}

#[cfg(feature = "auto-failover")]
pub use failover::{
    AutoFailoverCoordinator, FailoverDecision, FailoverError, LsnCheckResult, ProbeResult,
};
