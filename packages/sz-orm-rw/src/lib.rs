//! # SZ-ORM RW — 读写分离
//!
//! 提供 master/slave 读写分离路由，支持轮询、随机、最少连接三种负载均衡策略，
//! 写请求路由至 master，读请求在 slave 集群间分配。
//!
//! ## 主要类型
//!
//! - [`ReadWriteRouter`] — 读写分离路由器
//! - [`LoadBalanceStrategy`] — 负载均衡策略

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// 负载均衡策略
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LoadBalanceStrategy {
    /// 轮询：依次分配请求到各 slave
    RoundRobin,
    /// 随机：基于系统时间熵随机选择 slave
    Random,
    /// 最少连接：选择当前活跃连接数最少的 slave
    LeastConnections,
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
}

impl ReadWriteRouter {
    pub fn new(master: &str, slaves: Vec<&str>) -> Self {
        let slave_count = slaves.len();
        Self {
            master: master.to_string(),
            slaves: slaves.into_iter().map(|s| s.to_string()).collect(),
            strategy: LoadBalanceStrategy::RoundRobin,
            round_robin_counter: AtomicUsize::new(0),
            connection_counts: Mutex::new(vec![0; slave_count]),
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
    pub fn slave(&self) -> &str {
        if self.slaves.is_empty() {
            return &self.master;
        }
        match self.strategy {
            LoadBalanceStrategy::RoundRobin => self.select_round_robin(),
            LoadBalanceStrategy::Random => self.select_random(),
            LoadBalanceStrategy::LeastConnections => self.select_least_connections(),
        }
    }

    /// 设置负载均衡策略
    pub fn set_strategy(&mut self, strategy: LoadBalanceStrategy) {
        self.strategy = strategy;
    }

    /// 获取当前策略
    pub fn strategy(&self) -> LoadBalanceStrategy {
        self.strategy
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
}
