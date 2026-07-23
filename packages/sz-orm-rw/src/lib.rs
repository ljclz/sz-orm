#![allow(dead_code)]
//! # SZ-ORM RW — 读写分离
//!
//! 提供 master/slave 读写分离路由，支持轮询、随机、最少连接三种负载均衡策略，
//! 写请求路由至 master，读请求在 slave 集群间分配。
//!
//! ## 主要类型
//!
//! - [`ReadWriteRouter`] — 读写分离路由器
//! - [`LoadBalanceStrategy`] — 负载均衡策略
//! - [`HealthChecker`] — 健康检查与故障转移
//! - [`WeightedSlave`] — 加权 slave 配置
//! - [`ReadRationing`] — 读写比例控制
//! - [`LatencyStats`] — 延迟统计

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// 负载均衡策略
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LoadBalanceStrategy {
    /// 轮询：依次分配请求到各 slave
    RoundRobin,
    /// 随机：基于系统时间熵随机选择 slave
    Random,
    /// 最少连接：选择当前活跃连接数最少的 slave
    LeastConnections,
    /// 加权轮询：根据 slave 权重比例分配请求
    WeightedRoundRobin,
}

/// Slave 健康状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SlaveHealth {
    /// 健康，可正常服务
    Healthy,
    /// 不健康，已被故障转移排除
    Unhealthy,
    /// 临时不可用（如维护中）
    Drained,
}

/// 加权 slave 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightedSlave {
    /// slave 地址
    pub addr: String,
    /// 权重（>=1），权重越大被选中的概率越高
    pub weight: u32,
    /// 当前健康状态
    pub health: SlaveHealth,
}

impl WeightedSlave {
    pub fn new(addr: impl Into<String>, weight: u32) -> Self {
        Self {
            addr: addr.into(),
            weight: weight.max(1),
            health: SlaveHealth::Healthy,
        }
    }

    pub fn with_health(mut self, health: SlaveHealth) -> Self {
        self.health = health;
        self
    }
}

/// 单个 slave 的延迟统计快照
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LatencySnapshot {
    /// 已采样的请求总数
    pub samples: u64,
    /// 最小延迟（纳秒）
    pub min_ns: u64,
    /// 最大延迟（纳秒）
    pub max_ns: u64,
    /// 累计延迟（纳秒），用于计算平均值
    pub sum_ns: u128,
}

impl LatencySnapshot {
    pub fn record(&mut self, latency: Duration) {
        let ns = latency.as_nanos();
        self.samples += 1;
        if self.min_ns == 0 || ns < self.min_ns as u128 {
            self.min_ns = ns.min(u64::MAX as u128) as u64;
        }
        if ns > self.max_ns as u128 {
            self.max_ns = ns.min(u64::MAX as u128) as u64;
        }
        self.sum_ns = self.sum_ns.saturating_add(ns);
    }

    /// 平均延迟（纳秒），无样本时返回 0
    pub fn avg_ns(&self) -> u64 {
        if self.samples == 0 {
            0
        } else {
            (self.sum_ns / self.samples as u128) as u64
        }
    }

    pub fn avg(&self) -> Duration {
        Duration::from_nanos(self.avg_ns())
    }
}

/// 延迟统计：为每个 slave 维护一份独立的 [`LatencySnapshot`]
#[derive(Debug, Default)]
pub struct LatencyStats {
    inner: Mutex<HashMap<String, LatencySnapshot>>,
}

impl LatencyStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次 slave 的请求延迟
    pub fn record(&self, slave: &str, latency: Duration) {
        if let Ok(mut inner) = self.inner.lock() {
            inner
                .entry(slave.to_string())
                .or_default()
                .record(latency);
        }
    }

    /// 返回某个 slave 的快照副本
    pub fn snapshot(&self, slave: &str) -> LatencySnapshot {
        match self.inner.lock() {
            Ok(inner) => inner.get(slave).cloned().unwrap_or_default(),
            Err(_) => LatencySnapshot::default(),
        }
    }

    /// 列出所有 slave 的快照
    pub fn all(&self) -> Vec<(String, LatencySnapshot)> {
        match self.inner.lock() {
            Ok(inner) => inner
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 重置某个 slave 的统计
    pub fn reset(&self, slave: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.remove(slave);
        }
    }
}

/// 读写比例控制器：按比例把读请求路由到 master（强一致读）或 slave（弱一致读）
#[derive(Debug, Serialize, Deserialize)]
pub struct ReadRationing {
    /// 0..=100，表示有多少比例的读走 master
    /// 0 表示全部走 slave，100 表示全部走 master
    pub master_read_percent: u8,
    /// 内部计数器，用于轮询决定
    #[serde(skip)]
    counter: AtomicU64,
}

impl Clone for ReadRationing {
    fn clone(&self) -> Self {
        Self {
            master_read_percent: self.master_read_percent,
            counter: AtomicU64::new(self.counter.load(Ordering::Relaxed)),
        }
    }
}

impl ReadRationing {
    pub fn new(master_read_percent: u8) -> Self {
        Self {
            master_read_percent: master_read_percent.min(100),
            counter: AtomicU64::new(0),
        }
    }

    /// 默认 0% 走 master（全部走 slave）
    pub fn default_slave_only() -> Self {
        Self::new(0)
    }

    /// 默认 100% 走 master（强一致读）
    pub fn default_master_only() -> Self {
        Self::new(100)
    }

    /// 决定本次读请求是否走 master
    pub fn should_read_master(&self) -> bool {
        if self.master_read_percent == 0 {
            return false;
        }
        if self.master_read_percent == 100 {
            return true;
        }
        let idx = self.counter.fetch_add(1, Ordering::Relaxed);
        // 每 100 次循环内，前 master_read_percent 次走 master
        (idx % 100) < self.master_read_percent as u64
    }

    /// 修改比例（运行时热更新）
    pub fn set_percent(&mut self, percent: u8) {
        self.master_read_percent = percent.min(100);
        self.counter.store(0, Ordering::Relaxed);
    }
}

impl Default for ReadRationing {
    fn default() -> Self {
        Self::default_slave_only()
    }
}

/// 健康检查器：跟踪每个 slave 的健康状态，并支持故障转移
pub struct HealthChecker {
    states: Mutex<HashMap<String, SlaveHealth>>,
    /// 连续失败次数阈值，达到后标记为 Unhealthy
    pub failure_threshold: u32,
    /// 连续失败次数计数
    failure_counts: Mutex<HashMap<String, u32>>,
    /// 自动故障转移恢复时间（健康检查恢复后多久重新加入集群）
    pub recovery_cooldown: Duration,
}

impl HealthChecker {
    pub fn new(failure_threshold: u32) -> Self {
        Self {
            states: Mutex::new(HashMap::new()),
            failure_threshold,
            failure_counts: Mutex::new(HashMap::new()),
            recovery_cooldown: Duration::from_secs(30),
        }
    }

    /// 注册一个 slave，初始状态为 Healthy
    pub fn register(&self, slave: &str) {
        if let Ok(mut states) = self.states.lock() {
            states
                .entry(slave.to_string())
                .or_insert(SlaveHealth::Healthy);
        }
    }

    /// 标记 slave 为指定健康状态
    pub fn set_health(&self, slave: &str, health: SlaveHealth) {
        if let Ok(mut states) = self.states.lock() {
            states.insert(slave.to_string(), health);
        }
        if let Ok(mut counts) = self.failure_counts.lock() {
            if health == SlaveHealth::Healthy {
                counts.remove(slave);
            }
        }
    }

    /// 记录一次失败，达到阈值后自动标记为 Unhealthy
    /// 返回 true 表示触发了故障转移
    pub fn record_failure(&self, slave: &str) -> bool {
        let mut triggered = false;
        if let Ok(mut counts) = self.failure_counts.lock() {
            let count = counts.entry(slave.to_string()).or_insert(0);
            *count = count.saturating_add(1);
            if *count >= self.failure_threshold {
                triggered = true;
            }
        }
        if triggered {
            self.set_health(slave, SlaveHealth::Unhealthy);
        }
        triggered
    }

    /// 记录一次成功，重置失败计数
    pub fn record_success(&self, slave: &str) {
        if let Ok(mut counts) = self.failure_counts.lock() {
            counts.remove(slave);
        }
    }

    /// 查询 slave 健康状态，未注册返回 None
    pub fn health(&self, slave: &str) -> Option<SlaveHealth> {
        self.states
            .lock()
            .ok()
            .and_then(|s| s.get(slave).copied())
    }

    /// 列出所有处于指定状态的 slave
    pub fn list_by_health(&self, health: SlaveHealth) -> Vec<String> {
        match self.states.lock() {
            Ok(states) => states
                .iter()
                .filter(|(_, h)| **h == health)
                .map(|(k, _)| k.clone())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 返回所有健康 slave 列表
    pub fn healthy_slaves(&self) -> Vec<String> {
        self.list_by_health(SlaveHealth::Healthy)
    }

    /// 返回所有不健康 slave 列表
    pub fn unhealthy_slaves(&self) -> Vec<String> {
        self.list_by_health(SlaveHealth::Unhealthy)
    }

    /// 返回当前连续失败次数
    pub fn failure_count(&self, slave: &str) -> u32 {
        self.failure_counts
            .lock()
            .ok()
            .and_then(|c| c.get(slave).copied())
            .unwrap_or(0)
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new(3)
    }
}

/// 读写分离路由器
///
/// master 处理写请求，slave 集群处理读请求。
/// 根据 `LoadBalanceStrategy` 在多个 slave 间分配读请求。
pub struct ReadWriteRouter {
    master: String,
    slaves: Vec<String>,
    strategy: LoadBalanceStrategy,
    round_robin_counter: AtomicUsize,
    connection_counts: Mutex<Vec<usize>>,
    /// 加权 slave 配置（addr -> weight）
    weights: Mutex<HashMap<String, u32>>,
    /// 健康检查器
    health_checker: HealthChecker,
    /// 延迟统计
    latency_stats: LatencyStats,
    /// 读写比例控制器（决定是否走 master 读）
    rationing: Mutex<ReadRationing>,
}

impl ReadWriteRouter {
    pub fn new(master: &str, slaves: Vec<&str>) -> Self {
        let slave_count = slaves.len();
        let mut weights = HashMap::new();
        let health_checker = HealthChecker::default();
        for s in &slaves {
            weights.insert(s.to_string(), 1u32);
            health_checker.register(s);
        }
        Self {
            master: master.to_string(),
            slaves: slaves.into_iter().map(|s| s.to_string()).collect(),
            strategy: LoadBalanceStrategy::RoundRobin,
            round_robin_counter: AtomicUsize::new(0),
            connection_counts: Mutex::new(vec![0; slave_count]),
            weights: Mutex::new(weights),
            health_checker,
            latency_stats: LatencyStats::new(),
            rationing: Mutex::new(ReadRationing::default_slave_only()),
        }
    }

    /// 返回 master 节点
    pub fn master(&self) -> &str {
        &self.master
    }

    /// 返回 slave 列表
    pub fn slaves(&self) -> &[String] {
        &self.slaves
    }

    /// 根据当前策略选择一个 slave
    ///
    /// 如果 slaves 为空，回退到 master。
    /// 如果启用健康检查，会跳过 Unhealthy/Drained 的 slave。
    pub fn slave(&self) -> &str {
        if self.slaves.is_empty() {
            return &self.master;
        }
        // 若配置了 master 读比例，按比例分流到 master
        if let Ok(rationing) = self.rationing.lock() {
            if rationing.should_read_master() {
                return &self.master;
            }
        }
        // 先尝试选择健康的 slave
        if let Some(healthy) = self.select_healthy_slave() {
            return healthy;
        }
        // 所有 slave 都不健康时降级到 master（故障转移）
        &self.master
    }

    /// 在所有健康 slave 中按策略选择
    fn select_healthy_slave(&self) -> Option<&str> {
        let healthy_indices: Vec<usize> = self
            .slaves
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                self.health_checker
                    .health(s)
                    .unwrap_or(SlaveHealth::Healthy)
                    == SlaveHealth::Healthy
            })
            .map(|(i, _)| i)
            .collect();

        if healthy_indices.is_empty() {
            return None;
        }

        let idx = match self.strategy {
            LoadBalanceStrategy::RoundRobin => {
                let counter = self.round_robin_counter.fetch_add(1, Ordering::SeqCst);
                healthy_indices[counter % healthy_indices.len()]
            }
            LoadBalanceStrategy::Random => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default();
                static COUNTER: AtomicUsize = AtomicUsize::new(0);
                let c = COUNTER.fetch_add(1, Ordering::Relaxed);
                let seed = (now.as_nanos() as usize)
                    .wrapping_mul(2654435761)
                    .wrapping_add(std::process::id() as usize)
                    .wrapping_add(c.wrapping_mul(0x9e3779b9));
                healthy_indices[seed % healthy_indices.len()]
            }
            LoadBalanceStrategy::LeastConnections => {
                let counts = match self.connection_counts.lock() {
                    Ok(c) => c,
                    Err(_) => return Some(&self.slaves[healthy_indices[0]]),
                };
                let mut min_idx = healthy_indices[0];
                let mut min_count = counts[min_idx];
                for &i in healthy_indices.iter().skip(1) {
                    if counts[i] < min_count {
                        min_count = counts[i];
                        min_idx = i;
                    }
                }
                min_idx
            }
            LoadBalanceStrategy::WeightedRoundRobin => {
                self.select_weighted_index(&healthy_indices)
            }
        };
        Some(&self.slaves[idx])
    }

    /// 加权轮询：根据权重在 healthy_indices 中选择
    fn select_weighted_index(&self, healthy_indices: &[usize]) -> usize {
        let weights = match self.weights.lock() {
            Ok(w) => w,
            Err(_) => return healthy_indices[0],
        };
        let total: u64 = healthy_indices
            .iter()
            .map(|i| weights.get(&self.slaves[*i]).copied().unwrap_or(1) as u64)
            .sum();
        if total == 0 {
            return healthy_indices[0];
        }
        let counter = self.round_robin_counter.fetch_add(1, Ordering::SeqCst);
        let mut pick = (counter as u64) % total;
        for &idx in healthy_indices.iter() {
            let w = weights.get(&self.slaves[idx]).copied().unwrap_or(1) as u64;
            if pick < w {
                return idx;
            }
            pick -= w;
        }
        healthy_indices[healthy_indices.len() - 1]
    }

    /// 设置负载均衡策略
    pub fn set_strategy(&mut self, strategy: LoadBalanceStrategy) {
        self.strategy = strategy;
    }

    /// 获取当前策略
    pub fn strategy(&self) -> LoadBalanceStrategy {
        self.strategy
    }

    /// 设置 slave 权重
    pub fn set_weight(&self, slave: &str, weight: u32) -> Result<(), String> {
        if !self.slaves.iter().any(|s| s == slave) {
            return Err(format!("unknown slave: {}", slave));
        }
        if let Ok(mut weights) = self.weights.lock() {
            weights.insert(slave.to_string(), weight.max(1));
        }
        Ok(())
    }

    /// 获取 slave 权重
    pub fn weight(&self, slave: &str) -> Option<u32> {
        self.weights
            .lock()
            .ok()
            .and_then(|w| w.get(slave).copied())
    }

    /// 健康检查器引用
    pub fn health_checker(&self) -> &HealthChecker {
        &self.health_checker
    }

    /// 延迟统计引用
    pub fn latency_stats(&self) -> &LatencyStats {
        &self.latency_stats
    }

    /// 记录一次 slave 请求的延迟
    pub fn record_latency(&self, slave: &str, latency: Duration) {
        self.latency_stats.record(slave, latency);
    }

    /// 测量并记录一次 slave 调用的耗时
    pub fn measure<F, T>(&self, slave: &str, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        let start = Instant::now();
        let result = f();
        self.record_latency(slave, start.elapsed());
        result
    }

    /// 配置读写比例
    pub fn set_read_rationing(&self, percent: u8) {
        if let Ok(mut r) = self.rationing.lock() {
            r.set_percent(percent);
        }
    }

    /// 获取当前读写比例
    pub fn read_rationing_percent(&self) -> u8 {
        self.rationing
            .lock()
            .map(|r| r.master_read_percent)
            .unwrap_or(0)
    }

    fn select_round_robin(&self) -> &str {
        let idx = self.round_robin_counter.fetch_add(1, Ordering::SeqCst);
        &self.slaves[idx % self.slaves.len()]
    }

    fn select_random(&self) -> &str {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        // 组合纳秒、pid 和一个静态计数器增加熵
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let c = COUNTER.fetch_add(1, Ordering::Relaxed);
        let seed = (now.as_nanos() as usize)
            .wrapping_mul(2654435761)
            .wrapping_add(std::process::id() as usize)
            .wrapping_add(c.wrapping_mul(0x9e3779b9));
        &self.slaves[seed % self.slaves.len()]
    }

    fn select_least_connections(&self) -> &str {
        // lock poisoned 时降级到 round-robin，避免级联 panic。
        // lock poisoned 意味着另一线程已 panic，此时统计计数不可信。
        let counts = match self.connection_counts.lock() {
            Ok(c) => c,
            Err(_) => return self.select_round_robin(),
        };
        let mut min_idx = 0usize;
        let mut min_count = counts[0];
        for (i, &count) in counts.iter().enumerate() {
            if count < min_count {
                min_count = count;
                min_idx = i;
            }
        }
        &self.slaves[min_idx]
    }

    /// 在某个 slave 上获取连接（增加连接计数，用于 LeastConnections）
    pub fn acquire(&self, slave: &str) -> Result<(), String> {
        if let Some(idx) = self.slaves.iter().position(|s| s == slave) {
            let mut counts = self
                .connection_counts
                .lock()
                .map_err(|e| format!("lock error: {}", e))?;
            counts[idx] = counts[idx].saturating_add(1);
            Ok(())
        } else {
            Err(format!("unknown slave: {}", slave))
        }
    }

    /// 释放某个 slave 的连接（减少连接计数）
    pub fn release(&self, slave: &str) -> Result<(), String> {
        if let Some(idx) = self.slaves.iter().position(|s| s == slave) {
            let mut counts = self
                .connection_counts
                .lock()
                .map_err(|e| format!("lock error: {}", e))?;
            if counts[idx] > 0 {
                counts[idx] -= 1;
            }
            Ok(())
        } else {
            Err(format!("unknown slave: {}", slave))
        }
    }

    /// 查询某个 slave 的当前连接数（用于测试和监控）
    pub fn connection_count(&self, slave: &str) -> Option<usize> {
        let idx = self.slaves.iter().position(|s| s == slave)?;
        let counts = self.connection_counts.lock().ok()?;
        Some(counts[idx])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_router_master() {
        let router = ReadWriteRouter::new("master:3306", vec!["slave1:3306", "slave2:3306"]);
        assert_eq!(router.master(), "master:3306");
    }

    #[test]
    fn test_router_slaves_list() {
        let router = ReadWriteRouter::new("m", vec!["s1", "s2", "s3"]);
        assert_eq!(router.slaves().len(), 3);
        assert_eq!(router.slaves()[0], "s1");
        assert_eq!(router.slaves()[2], "s3");
    }

    #[test]
    fn test_default_strategy_is_round_robin() {
        let router = ReadWriteRouter::new("m", vec!["s1", "s2"]);
        assert_eq!(router.strategy(), LoadBalanceStrategy::RoundRobin);
    }

    #[test]
    fn test_set_strategy() {
        let mut router = ReadWriteRouter::new("m", vec!["s1", "s2"]);
        router.set_strategy(LoadBalanceStrategy::Random);
        assert_eq!(router.strategy(), LoadBalanceStrategy::Random);
        router.set_strategy(LoadBalanceStrategy::LeastConnections);
        assert_eq!(router.strategy(), LoadBalanceStrategy::LeastConnections);
    }

    // --- RoundRobin 策略测试 ---

    #[test]
    fn test_round_robin_cycles_through_slaves() {
        let mut router = ReadWriteRouter::new("m", vec!["s1", "s2", "s3"]);
        router.set_strategy(LoadBalanceStrategy::RoundRobin);

        // 第一次调用应该返回 s1，第二次 s2，第三次 s3，第四次回到 s1
        let first = router.slave().to_string();
        let second = router.slave().to_string();
        let third = router.slave().to_string();
        let fourth = router.slave().to_string();

        assert_eq!(first, "s1");
        assert_eq!(second, "s2");
        assert_eq!(third, "s3");
        assert_eq!(fourth, "s1", "RoundRobin 应在第 4 次回到 s1");
    }

    #[test]
    fn test_round_robin_single_slave() {
        let mut router = ReadWriteRouter::new("m", vec!["only_slave"]);
        router.set_strategy(LoadBalanceStrategy::RoundRobin);
        for _ in 0..5 {
            assert_eq!(router.slave(), "only_slave");
        }
    }

    #[test]
    fn test_round_robin_visits_all_slaves() {
        let mut router = ReadWriteRouter::new("m", vec!["s1", "s2", "s3", "s4"]);
        router.set_strategy(LoadBalanceStrategy::RoundRobin);

        let mut visited = HashSet::new();
        for _ in 0..4 {
            visited.insert(router.slave().to_string());
        }
        assert_eq!(visited.len(), 4, "一轮轮询应该访问所有 4 个 slave");
    }

    // --- Random 策略测试 ---

    #[test]
    fn test_random_returns_valid_slave() {
        let mut router = ReadWriteRouter::new("m", vec!["s1", "s2", "s3"]);
        router.set_strategy(LoadBalanceStrategy::Random);

        let slaves: HashSet<&str> = ["s1", "s2", "s3"].iter().copied().collect();
        for _ in 0..20 {
            let picked = router.slave();
            assert!(
                slaves.contains(picked),
                "随机策略返回了未知 slave: {}",
                picked
            );
        }
    }

    #[test]
    fn test_random_eventually_visits_multiple_slaves() {
        let mut router = ReadWriteRouter::new("m", vec!["s1", "s2", "s3", "s4"]);
        router.set_strategy(LoadBalanceStrategy::Random);

        let mut visited = HashSet::new();
        // 大量调用后，应该至少访问 2 个不同的 slave（统计性验证）
        for _ in 0..200 {
            visited.insert(router.slave().to_string());
        }
        assert!(
            visited.len() >= 2,
            "随机策略在 200 次调用后应至少访问 2 个 slave，实际: {}",
            visited.len()
        );
    }

    #[test]
    fn test_random_single_slave() {
        let mut router = ReadWriteRouter::new("m", vec!["only"]);
        router.set_strategy(LoadBalanceStrategy::Random);
        for _ in 0..10 {
            assert_eq!(router.slave(), "only");
        }
    }

    // --- LeastConnections 策略测试 ---

    #[test]
    fn test_least_connections_picks_zero_load_slave() {
        let mut router = ReadWriteRouter::new("m", vec!["s1", "s2", "s3"]);
        router.set_strategy(LoadBalanceStrategy::LeastConnections);

        // 初始所有 slave 连接数都为 0，应选第一个
        assert_eq!(router.slave(), "s1");
    }

    #[test]
    fn test_least_connections_picks_least_loaded() {
        let mut router = ReadWriteRouter::new("m", vec!["s1", "s2", "s3"]);
        router.set_strategy(LoadBalanceStrategy::LeastConnections);

        // 给 s1 和 s2 增加连接
        router.acquire("s1").unwrap();
        router.acquire("s1").unwrap();
        router.acquire("s2").unwrap();

        // s3 连接数为 0，应被选中
        assert_eq!(router.slave(), "s3");

        // 给 s3 也加一个连接
        router.acquire("s3").unwrap();
        // 现在 s1=2, s2=1, s3=1，应选 s2（索引靠前的最少连接）
        assert_eq!(router.slave(), "s2");
    }

    #[test]
    fn test_least_connections_after_release() {
        let mut router = ReadWriteRouter::new("m", vec!["s1", "s2"]);
        router.set_strategy(LoadBalanceStrategy::LeastConnections);

        router.acquire("s1").unwrap();
        router.acquire("s1").unwrap();
        router.acquire("s2").unwrap();

        // s1=2, s2=1，选 s2
        assert_eq!(router.slave(), "s2");

        // 释放 s1 的两个连接
        router.release("s1").unwrap();
        router.release("s1").unwrap();

        // 现在 s1=0, s2=1，应选 s1
        assert_eq!(router.slave(), "s1");
    }

    #[test]
    fn test_acquire_unknown_slave_returns_error() {
        let router = ReadWriteRouter::new("m", vec!["s1"]);
        assert!(router.acquire("nonexistent").is_err());
    }

    #[test]
    fn test_release_unknown_slave_returns_error() {
        let router = ReadWriteRouter::new("m", vec!["s1"]);
        assert!(router.release("nonexistent").is_err());
    }

    #[test]
    fn test_release_below_zero_clamped() {
        let router = ReadWriteRouter::new("m", vec!["s1"]);
        // 释放不存在的连接不应导致负数
        router.release("s1").unwrap();
        assert_eq!(router.connection_count("s1"), Some(0));
    }

    #[test]
    fn test_connection_count_query() {
        let router = ReadWriteRouter::new("m", vec!["s1", "s2"]);
        assert_eq!(router.connection_count("s1"), Some(0));
        router.acquire("s1").unwrap();
        router.acquire("s1").unwrap();
        assert_eq!(router.connection_count("s1"), Some(2));
        assert_eq!(router.connection_count("s2"), Some(0));
        assert_eq!(router.connection_count("unknown"), None);
    }

    #[test]
    fn test_empty_slaves_falls_back_to_master() {
        let router = ReadWriteRouter::new("only_master", vec![]);
        // 任何策略下都应回退到 master
        assert_eq!(router.slave(), "only_master");

        let mut router_rr = router;
        router_rr.set_strategy(LoadBalanceStrategy::RoundRobin);
        assert_eq!(router_rr.slave(), "only_master");

        router_rr.set_strategy(LoadBalanceStrategy::Random);
        assert_eq!(router_rr.slave(), "only_master");

        router_rr.set_strategy(LoadBalanceStrategy::LeastConnections);
        assert_eq!(router_rr.slave(), "only_master");
    }

    #[test]
    fn test_round_robin_concurrent_safe() {
        // 验证 AtomicUsize 在多线程下不会 panic，且计数正确
        // 默认策略即为 RoundRobin
        let router = std::sync::Arc::new(ReadWriteRouter::new("m", vec!["s1", "s2", "s3"]));

        let mut handles = vec![];
        for _ in 0..4 {
            let r = std::sync::Arc::clone(&router);
            handles.push(std::thread::spawn(move || {
                for _ in 0..10 {
                    let _ = r.slave();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // 40 次调用后计数器应该是 40
        assert_eq!(router.round_robin_counter.load(Ordering::SeqCst), 40);
    }

    // ====================================================================
    // 健康检查与故障转移测试
    // ====================================================================

    #[test]
    fn test_health_checker_new_slave_is_healthy() {
        let checker = HealthChecker::new(3);
        checker.register("s1");
        assert_eq!(checker.health("s1"), Some(SlaveHealth::Healthy));
    }

    #[test]
    fn test_health_checker_unregistered_slave_returns_none() {
        let checker = HealthChecker::new(3);
        assert_eq!(checker.health("unknown"), None);
    }

    #[test]
    fn test_health_checker_mark_unhealthy() {
        let checker = HealthChecker::new(3);
        checker.register("s1");
        checker.set_health("s1", SlaveHealth::Unhealthy);
        assert_eq!(checker.health("s1"), Some(SlaveHealth::Unhealthy));
    }

    #[test]
    fn test_health_checker_mark_drained() {
        let checker = HealthChecker::new(3);
        checker.register("s1");
        checker.set_health("s1", SlaveHealth::Drained);
        assert_eq!(checker.health("s1"), Some(SlaveHealth::Drained));
    }

    #[test]
    fn test_record_failure_below_threshold_keeps_healthy() {
        let checker = HealthChecker::new(3);
        checker.register("s1");
        // 失败 2 次（< 阈值 3）仍应保持健康
        assert!(!checker.record_failure("s1"));
        assert!(!checker.record_failure("s1"));
        assert_eq!(checker.health("s1"), Some(SlaveHealth::Healthy));
        assert_eq!(checker.failure_count("s1"), 2);
    }

    #[test]
    fn test_record_failure_at_threshold_marks_unhealthy() {
        let checker = HealthChecker::new(3);
        checker.register("s1");
        assert!(!checker.record_failure("s1"));
        assert!(!checker.record_failure("s1"));
        // 第 3 次失败应触发故障转移
        assert!(checker.record_failure("s1"));
        assert_eq!(checker.health("s1"), Some(SlaveHealth::Unhealthy));
    }

    #[test]
    fn test_record_success_resets_failure_count() {
        let checker = HealthChecker::new(3);
        checker.register("s1");
        checker.record_failure("s1");
        checker.record_failure("s1");
        assert_eq!(checker.failure_count("s1"), 2);
        checker.record_success("s1");
        assert_eq!(checker.failure_count("s1"), 0);
    }

    #[test]
    fn test_set_healthy_resets_failure_count() {
        let checker = HealthChecker::new(2);
        checker.register("s1");
        checker.record_failure("s1");
        checker.record_failure("s1");
        assert_eq!(checker.health("s1"), Some(SlaveHealth::Unhealthy));
        // 恢复
        checker.set_health("s1", SlaveHealth::Healthy);
        assert_eq!(checker.failure_count("s1"), 0);
    }

    #[test]
    fn test_list_by_health() {
        let checker = HealthChecker::new(3);
        checker.register("s1");
        checker.register("s2");
        checker.register("s3");
        checker.set_health("s2", SlaveHealth::Unhealthy);
        let mut healthy = checker.healthy_slaves();
        healthy.sort();
        assert_eq!(healthy, vec!["s1".to_string(), "s3".to_string()]);
        assert_eq!(checker.unhealthy_slaves(), vec!["s2".to_string()]);
    }

    #[test]
    fn test_router_failover_to_master_when_all_unhealthy() {
        let router = ReadWriteRouter::new("m", vec!["s1", "s2"]);
        router.health_checker().set_health("s1", SlaveHealth::Unhealthy);
        router.health_checker().set_health("s2", SlaveHealth::Unhealthy);
        // 全部不健康时降级到 master
        assert_eq!(router.slave(), "m");
    }

    #[test]
    fn test_router_skips_unhealthy_slave() {
        let mut router = ReadWriteRouter::new("m", vec!["s1", "s2", "s3"]);
        router.set_strategy(LoadBalanceStrategy::RoundRobin);
        router.health_checker().set_health("s2", SlaveHealth::Unhealthy);

        // 100 次调用都不应命中 s2
        for _ in 0..100 {
            let picked = router.slave().to_string();
            assert_ne!(picked, "s2", "不应选中不健康的 slave");
        }
    }

    #[test]
    fn test_router_failover_skips_drained_slave() {
        let router = ReadWriteRouter::new("m", vec!["s1", "s2"]);
        router.health_checker().set_health("s1", SlaveHealth::Drained);
        // 只有 s2 健康
        for _ in 0..5 {
            assert_eq!(router.slave(), "s2");
        }
    }

    #[test]
    fn test_router_default_health_checker_threshold_is_3() {
        let router = ReadWriteRouter::new("m", vec!["s1"]);
        assert_eq!(router.health_checker().failure_threshold, 3);
    }

    // ====================================================================
    // 权重配置测试
    // ====================================================================

    #[test]
    fn test_set_weight_for_known_slave() {
        let router = ReadWriteRouter::new("m", vec!["s1", "s2"]);
        router.set_weight("s1", 10).unwrap();
        assert_eq!(router.weight("s1"), Some(10));
        assert_eq!(router.weight("s2"), Some(1));
    }

    #[test]
    fn test_set_weight_for_unknown_slave_errors() {
        let router = ReadWriteRouter::new("m", vec!["s1"]);
        assert!(router.set_weight("ghost", 10).is_err());
    }

    #[test]
    fn test_set_weight_zero_clamped_to_one() {
        let router = ReadWriteRouter::new("m", vec!["s1"]);
        router.set_weight("s1", 0).unwrap();
        assert_eq!(router.weight("s1"), Some(1));
    }

    #[test]
    fn test_weighted_round_robin_respects_weights() {
        let mut router = ReadWriteRouter::new("m", vec!["s1", "s2"]);
        router.set_strategy(LoadBalanceStrategy::WeightedRoundRobin);
        router.set_weight("s1", 9).unwrap();
        router.set_weight("s2", 1).unwrap();

        // 100 次调用后，s1 应该被选中的次数 >> s2
        let mut s1_count = 0usize;
        let mut s2_count = 0usize;
        for _ in 0..100 {
            match router.slave() {
                "s1" => s1_count += 1,
                "s2" => s2_count += 1,
                _ => {}
            }
        }
        assert_eq!(s1_count + s2_count, 100);
        assert!(
            s1_count > s2_count * 3,
            "权重 9:1 应使 s1 命中次数远多于 s2，实际 s1={}, s2={}",
            s1_count,
            s2_count
        );
    }

    #[test]
    fn test_weighted_round_robin_with_equal_weights_visits_all() {
        let mut router = ReadWriteRouter::new("m", vec!["s1", "s2", "s3"]);
        router.set_strategy(LoadBalanceStrategy::WeightedRoundRobin);

        let mut visited = HashSet::new();
        for _ in 0..30 {
            visited.insert(router.slave().to_string());
        }
        assert_eq!(visited.len(), 3);
    }

    #[test]
    fn test_weighted_round_robin_skips_unhealthy() {
        let mut router = ReadWriteRouter::new("m", vec!["s1", "s2"]);
        router.set_strategy(LoadBalanceStrategy::WeightedRoundRobin);
        router.set_weight("s1", 100).unwrap();
        router.set_weight("s2", 1).unwrap();
        router.health_checker().set_health("s1", SlaveHealth::Unhealthy);

        // s1 不健康，所有请求应落到 s2
        for _ in 0..10 {
            assert_eq!(router.slave(), "s2");
        }
    }

    // ====================================================================
    // 读写比例控制测试
    // ====================================================================

    #[test]
    fn test_read_rationing_default_is_zero_percent_master() {
        let router = ReadWriteRouter::new("m", vec!["s1"]);
        assert_eq!(router.read_rationing_percent(), 0);
    }

    #[test]
    fn test_read_rationing_all_master() {
        let router = ReadWriteRouter::new("m", vec!["s1", "s2"]);
        router.set_read_rationing(100);
        // 100% 走 master
        for _ in 0..10 {
            assert_eq!(router.slave(), "m");
        }
    }

    #[test]
    fn test_read_rationing_all_slave() {
        let router = ReadWriteRouter::new("m", vec!["s1", "s2"]);
        router.set_read_rationing(0);
        // 0% 走 master，全部走 slave
        for _ in 0..10 {
            let picked = router.slave();
            assert!(picked == "s1" || picked == "s2");
        }
    }

    #[test]
    fn test_read_rationing_partial_distribution() {
        let router = ReadWriteRouter::new("m", vec!["s1", "s2"]);
        router.set_read_rationing(30);

        let mut master_count = 0usize;
        let mut slave_count = 0usize;
        for _ in 0..100 {
            let picked = router.slave();
            if picked == "m" {
                master_count += 1;
            } else {
                slave_count += 1;
            }
        }
        // 30% 应该走 master，允许 ±5 浮动
        assert!(
            (25..=35).contains(&master_count),
            "30% 比例下 master 命中数应在 25-35 之间，实际: {}",
            master_count
        );
        assert_eq!(master_count + slave_count, 100);
    }

    #[test]
    fn test_read_rationing_clamps_above_100() {
        let r = ReadRationing::new(150);
        assert_eq!(r.master_read_percent, 100);
    }

    #[test]
    fn test_read_rationing_set_percent_resets_counter() {
        let mut r = ReadRationing::new(50);
        // 调用几次
        for _ in 0..5 {
            let _ = r.should_read_master();
        }
        r.set_percent(80);
        // counter 应该被重置
        assert_eq!(r.counter.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_read_rationing_master_only_always_returns_true() {
        let r = ReadRationing::default_master_only();
        for _ in 0..10 {
            assert!(r.should_read_master());
        }
    }

    #[test]
    fn test_read_rationing_slave_only_always_returns_false() {
        let r = ReadRationing::default_slave_only();
        for _ in 0..10 {
            assert!(!r.should_read_master());
        }
    }

    // ====================================================================
    // 延迟统计测试
    // ====================================================================

    #[test]
    fn test_latency_stats_record_and_snapshot() {
        let stats = LatencyStats::new();
        stats.record("s1", Duration::from_millis(10));
        stats.record("s1", Duration::from_millis(20));
        stats.record("s1", Duration::from_millis(30));

        let snap = stats.snapshot("s1");
        assert_eq!(snap.samples, 3);
        assert!(snap.min_ns > 0);
        assert!(snap.max_ns >= snap.min_ns);
        // 平均应该在 10ms-30ms 之间
        let avg = snap.avg();
        assert!(avg >= Duration::from_millis(9));
        assert!(avg <= Duration::from_millis(31));
    }

    #[test]
    fn test_latency_stats_unknown_slave_returns_default() {
        let stats = LatencyStats::new();
        let snap = stats.snapshot("ghost");
        assert_eq!(snap.samples, 0);
        assert_eq!(snap.avg_ns(), 0);
    }

    #[test]
    fn test_latency_stats_reset() {
        let stats = LatencyStats::new();
        stats.record("s1", Duration::from_millis(10));
        assert_eq!(stats.snapshot("s1").samples, 1);
        stats.reset("s1");
        assert_eq!(stats.snapshot("s1").samples, 0);
    }

    #[test]
    fn test_latency_stats_all_returns_all_slaves() {
        let stats = LatencyStats::new();
        stats.record("s1", Duration::from_millis(10));
        stats.record("s2", Duration::from_millis(20));
        let all = stats.all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_router_measure_records_latency() {
        let router = ReadWriteRouter::new("m", vec!["s1"]);
        let result = router.measure("s1", || 42);
        assert_eq!(result, 42);
        let snap = router.latency_stats().snapshot("s1");
        assert_eq!(snap.samples, 1);
        // min_ns 可能在高分辨率计时器上为 0（闭包执行极快），不强制大于 0
    }

    #[test]
    fn test_router_record_latency_increases_sample_count() {
        let router = ReadWriteRouter::new("m", vec!["s1", "s2"]);
        router.record_latency("s1", Duration::from_micros(100));
        router.record_latency("s1", Duration::from_micros(200));
        router.record_latency("s2", Duration::from_micros(50));

        assert_eq!(router.latency_stats().snapshot("s1").samples, 2);
        assert_eq!(router.latency_stats().snapshot("s2").samples, 1);
    }

    #[test]
    fn test_latency_snapshot_avg_with_zero_samples() {
        let snap = LatencySnapshot::default();
        assert_eq!(snap.avg_ns(), 0);
        assert_eq!(snap.avg(), Duration::ZERO);
    }

    #[test]
    fn test_latency_snapshot_min_updates() {
        let mut snap = LatencySnapshot::default();
        snap.record(Duration::from_millis(50));
        assert_eq!(snap.min_ns, 50_000_000);
        snap.record(Duration::from_millis(10));
        assert_eq!(snap.min_ns, 10_000_000);
        snap.record(Duration::from_millis(100));
        assert_eq!(snap.min_ns, 10_000_000);
    }

    #[test]
    fn test_latency_snapshot_max_updates() {
        let mut snap = LatencySnapshot::default();
        snap.record(Duration::from_millis(10));
        assert_eq!(snap.max_ns, 10_000_000);
        snap.record(Duration::from_millis(50));
        assert_eq!(snap.max_ns, 50_000_000);
        snap.record(Duration::from_millis(20));
        assert_eq!(snap.max_ns, 50_000_000);
    }

    #[test]
    fn test_weighted_slave_default_health_is_healthy() {
        let ws = WeightedSlave::new("s1:3306", 5);
        assert_eq!(ws.health, SlaveHealth::Healthy);
        assert_eq!(ws.weight, 5);
    }

    #[test]
    fn test_weighted_slave_zero_weight_clamped() {
        let ws = WeightedSlave::new("s1:3306", 0);
        assert_eq!(ws.weight, 1);
    }

    #[test]
    fn test_weighted_slave_with_health_builder() {
        let ws = WeightedSlave::new("s1:3306", 5).with_health(SlaveHealth::Unhealthy);
        assert_eq!(ws.health, SlaveHealth::Unhealthy);
    }
}
