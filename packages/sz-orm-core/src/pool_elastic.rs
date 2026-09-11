//! v6.7.0 连接池弹性：动态扩缩容 + 多级熔断 + 健康检查 + 连接预热。
//!
//! `PoolElasticController` 周期采集池 stats，waiting > threshold 扩容，idle > threshold 缩容。
//! `TieredCircuitBreaker` 三级熔断（连接→节点→全局），`ConnectionHealthChecker` 连续 3 次失败才剔除。

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::circuit_breaker::{CircuitBreaker, CircuitState, DefaultCircuitBreaker};

// ============================================================================
// 配置
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElasticConfig {
    pub min_connections: u32,
    pub max_connections: u32,
    pub scale_up_threshold: u32,
    pub scale_down_threshold: u32,
    pub scale_interval: Duration,
    pub health_check_interval: Duration,
    pub health_check_sql: String,
}

impl Default for ElasticConfig {
    fn default() -> Self {
        Self {
            min_connections: 5,
            max_connections: 50,
            scale_up_threshold: 10,
            scale_down_threshold: 20,
            scale_interval: Duration::from_secs(1),
            health_check_interval: Duration::from_secs(30),
            health_check_sql: "SELECT 1".to_string(),
        }
    }
}

impl ElasticConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.min_connections < 1 || self.min_connections > 100 {
            return Err("min_connections 必须在 [1,100]".to_string());
        }
        if self.max_connections < self.min_connections || self.max_connections > 1000 {
            return Err("max_connections 必须在 [min,1000]".to_string());
        }
        if self.scale_interval < Duration::from_secs(1)
            || self.scale_interval > Duration::from_secs(60)
        {
            return Err("scale_interval 必须在 [1s,60s]".to_string());
        }
        if self.health_check_interval < Duration::from_secs(10)
            || self.health_check_interval > Duration::from_secs(300)
        {
            return Err("health_check_interval 必须在 [10s,300s]".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScaleReason {
    HighLoad,
    LowLoad,
    HealthCheckFailure,
}

#[derive(Debug, Clone)]
pub struct ScaleEvent {
    pub from: u32,
    pub to: u32,
    pub reason: ScaleReason,
    pub timestamp: Instant,
    pub elapsed: Duration,
}

// ============================================================================
// 多级熔断器
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitTier {
    Connection,
    Node,
    Global,
}

pub struct TieredCircuitBreaker {
    connection_level: Mutex<DefaultCircuitBreaker>,
    node_level: Mutex<DefaultCircuitBreaker>,
    global_level: Mutex<DefaultCircuitBreaker>,
}

impl TieredCircuitBreaker {
    pub fn new(failure_threshold: usize, reset_timeout: Duration) -> Self {
        Self {
            connection_level: Mutex::new(DefaultCircuitBreaker::new(
                failure_threshold,
                reset_timeout,
            )),
            node_level: Mutex::new(DefaultCircuitBreaker::new(failure_threshold, reset_timeout)),
            global_level: Mutex::new(DefaultCircuitBreaker::new(failure_threshold, reset_timeout)),
        }
    }

    pub fn can_execute(&self, tier: CircuitTier) -> bool {
        let cb = match tier {
            CircuitTier::Connection => &self.connection_level,
            CircuitTier::Node => &self.node_level,
            CircuitTier::Global => &self.global_level,
        };
        cb.lock().unwrap().can_execute()
    }

    pub fn record_success(&self, tier: CircuitTier) {
        let cb = match tier {
            CircuitTier::Connection => &self.connection_level,
            CircuitTier::Node => &self.node_level,
            CircuitTier::Global => &self.global_level,
        };
        cb.lock().unwrap().record_success();
    }

    pub fn record_failure(&self, tier: CircuitTier) {
        let cb = match tier {
            CircuitTier::Connection => &self.connection_level,
            CircuitTier::Node => &self.node_level,
            CircuitTier::Global => &self.global_level,
        };
        cb.lock().unwrap().record_failure();
    }

    pub fn state(&self, tier: CircuitTier) -> CircuitState {
        let cb = match tier {
            CircuitTier::Connection => &self.connection_level,
            CircuitTier::Node => &self.node_level,
            CircuitTier::Global => &self.global_level,
        };
        cb.lock().unwrap().state()
    }

    pub fn can_execute_any(&self) -> bool {
        self.can_execute(CircuitTier::Global)
            && self.can_execute(CircuitTier::Node)
            && self.can_execute(CircuitTier::Connection)
    }
}

// ============================================================================
// 健康检查器
// ============================================================================

pub struct ConnectionHealthChecker {
    failure_counts: Mutex<std::collections::HashMap<String, u32>>,
    failure_threshold: u32,
}

impl ConnectionHealthChecker {
    pub fn new(failure_threshold: u32) -> Self {
        Self {
            failure_counts: Mutex::new(std::collections::HashMap::new()),
            failure_threshold,
        }
    }

    pub fn record_failure(&self, conn_id: &str) -> bool {
        let mut counts = self.failure_counts.lock().unwrap();
        let count = counts.entry(conn_id.to_string()).or_insert(0);
        *count += 1;
        *count >= self.failure_threshold
    }

    pub fn record_success(&self, conn_id: &str) {
        let mut counts = self.failure_counts.lock().unwrap();
        counts.insert(conn_id.to_string(), 0);
    }

    pub fn should_evict(&self, conn_id: &str) -> bool {
        let counts = self.failure_counts.lock().unwrap();
        *counts.get(conn_id).unwrap_or(&0) >= self.failure_threshold
    }

    pub fn failure_count(&self, conn_id: &str) -> u32 {
        let counts = self.failure_counts.lock().unwrap();
        *counts.get(conn_id).unwrap_or(&0)
    }
}

// ============================================================================
// 池统计
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct PoolStats {
    pub total_connections: u32,
    pub idle_connections: u32,
    pub waiting_requests: u32,
}

impl PoolStats {
    pub fn active_connections(&self) -> u32 {
        self.total_connections.saturating_sub(self.idle_connections)
    }
}

// ============================================================================
// 弹性控制器
// ============================================================================

pub struct PoolElasticController {
    config: ElasticConfig,
    circuit_breaker: TieredCircuitBreaker,
    health_checker: ConnectionHealthChecker,
    scale_events: Mutex<Vec<ScaleEvent>>,
    current_size: Mutex<u32>,
}

impl PoolElasticController {
    pub fn new(config: ElasticConfig) -> Self {
        Self {
            config,
            circuit_breaker: TieredCircuitBreaker::new(5, Duration::from_secs(30)),
            health_checker: ConnectionHealthChecker::new(3),
            scale_events: Mutex::new(Vec::new()),
            current_size: Mutex::new(5),
        }
    }

    pub fn should_scale_up(stats: &PoolStats, config: &ElasticConfig) -> bool {
        stats.waiting_requests > config.scale_up_threshold
    }

    pub fn should_scale_down(stats: &PoolStats, config: &ElasticConfig) -> bool {
        stats.idle_connections > config.scale_down_threshold
    }

    pub fn evaluate_and_scale(&self, stats: &PoolStats) -> Option<ScaleEvent> {
        let mut current = self.current_size.lock().unwrap();
        let start = Instant::now();

        if Self::should_scale_up(stats, &self.config) {
            let target = (*current + stats.waiting_requests).min(self.config.max_connections);
            if target > *current {
                let event = ScaleEvent {
                    from: *current,
                    to: target,
                    reason: ScaleReason::HighLoad,
                    timestamp: Instant::now(),
                    elapsed: start.elapsed(),
                };
                *current = target;
                self.scale_events.lock().unwrap().push(event.clone());
                return Some(event);
            }
        }

        if Self::should_scale_down(stats, &self.config) {
            let target = (*current / 2).max(self.config.min_connections);
            if target < *current {
                let event = ScaleEvent {
                    from: *current,
                    to: target,
                    reason: ScaleReason::LowLoad,
                    timestamp: Instant::now(),
                    elapsed: start.elapsed(),
                };
                *current = target;
                self.scale_events.lock().unwrap().push(event.clone());
                return Some(event);
            }
        }

        None
    }

    pub fn scale_events(&self) -> Vec<ScaleEvent> {
        self.scale_events.lock().unwrap().clone()
    }

    pub fn current_size(&self) -> u32 {
        *self.current_size.lock().unwrap()
    }

    pub fn circuit_breaker(&self) -> &TieredCircuitBreaker {
        &self.circuit_breaker
    }

    pub fn health_checker(&self) -> &ConnectionHealthChecker {
        &self.health_checker
    }

    pub fn config(&self) -> &ElasticConfig {
        &self.config
    }

    pub fn prewarm(&self, target: u32) -> u32 {
        let mut current = self.current_size.lock().unwrap();
        let target = target
            .min(self.config.max_connections)
            .max(self.config.min_connections);
        *current = target;
        target
    }
}

// ============================================================================
// v6.8.0 PERF-POOL-01：连接池 IO 复用
// ============================================================================

/// 连接池 IO 复用通道
///
/// 在连接上复用 prepared statement 通道，减少重复 prepare 开销。
/// 首次执行 SQL 时 prepare 并缓存句柄，后续执行直接复用缓存句柄。
///
/// 需启用 `pool-io-reuse` feature。
#[cfg(feature = "pool-io-reuse")]
pub struct IoReuseChannel {
    cache: crate::prepared_cache::PreparedStatementCache,
}

#[cfg(feature = "pool-io-reuse")]
impl IoReuseChannel {
    /// 创建 IO 复用通道，内部持有 `PreparedStatementCache`
    ///
    /// `max_size_per_conn` 为每连接最大缓存句柄数（默认 256）
    #[must_use]
    pub fn new(max_size_per_conn: usize) -> Self {
        Self {
            cache: crate::prepared_cache::PreparedStatementCache::new(max_size_per_conn),
        }
    }

    /// 复用 prepared statement 通道执行查询
    ///
    /// 流程：
    /// 1. 查找缓存句柄 → 命中则直接执行返回
    /// 2. 未命中 → 调用 `prepare_fn` 获取执行闭包 → 缓存 → 执行返回
    ///
    /// # 参数
    /// - `conn_id`: 连接唯一标识
    /// - `sql`: SQL 文本
    /// - `params`: 参数列表
    /// - `tables`: 涉及的表名列表（用于表级失效索引）
    /// - `prepare_fn`: 首次执行时的 prepare 闭包，返回执行函数
    pub async fn execute_reuse(
        &self,
        conn_id: crate::prepared_cache::ConnId,
        sql: &str,
        params: &[crate::value::Value],
        tables: Vec<String>,
        prepare_fn: impl FnOnce() -> crate::prepared_cache::ExecuteFn,
    ) -> Result<crate::pool::QueryRows, crate::error::DbError> {
        use crate::prepared_cache::PreparedLookup;

        match self.cache.get_or_prepare(conn_id, sql, params).await? {
            PreparedLookup::Hit(rows) => Ok(rows),
            PreparedLookup::Miss => {
                let execute_fn = prepare_fn();
                self.cache
                    .store_handle(conn_id, sql, tables, std::sync::Arc::clone(&execute_fn));
                execute_fn(params).await
            }
        }
    }

    /// 返回缓存统计快照
    pub fn stats(&self) -> crate::prepared_cache::PreparedStatementCacheStatsSnapshot {
        self.cache.stats()
    }

    /// 失效连接级缓存（连接关闭时调用）
    pub fn invalidate_conn(&self, conn_id: crate::prepared_cache::ConnId) {
        self.cache.invalidate_conn(conn_id);
    }

    /// 失效表级缓存（表结构变更时调用）
    pub fn invalidate_table(&self, table: &str) {
        self.cache.invalidate_table(table);
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elastic_config_default_valid() {
        let config = ElasticConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn elastic_config_invalid_min() {
        let mut config = ElasticConfig::default();
        config.min_connections = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn elastic_config_invalid_max() {
        let mut config = ElasticConfig::default();
        config.max_connections = 3;
        assert!(config.validate().is_err());
    }

    #[test]
    fn tiered_circuit_independent() {
        let cb = TieredCircuitBreaker::new(2, Duration::from_secs(60));
        assert!(cb.can_execute(CircuitTier::Connection));
        assert!(cb.can_execute(CircuitTier::Node));
        assert!(cb.can_execute(CircuitTier::Global));

        cb.record_failure(CircuitTier::Connection);
        cb.record_failure(CircuitTier::Connection);
        assert_eq!(cb.state(CircuitTier::Connection), CircuitState::Open);
        assert_eq!(cb.state(CircuitTier::Node), CircuitState::Closed);
        assert_eq!(cb.state(CircuitTier::Global), CircuitState::Closed);
    }

    #[test]
    fn tiered_circuit_global_blocks_all() {
        let cb = TieredCircuitBreaker::new(1, Duration::from_secs(60));
        cb.record_failure(CircuitTier::Global);
        assert_eq!(cb.state(CircuitTier::Global), CircuitState::Open);
        assert!(!cb.can_execute_any());
    }

    #[test]
    fn tiered_circuit_half_open_recovery() {
        let cb = TieredCircuitBreaker::new(1, Duration::from_millis(10));
        cb.record_failure(CircuitTier::Connection);
        assert_eq!(cb.state(CircuitTier::Connection), CircuitState::Open);
        std::thread::sleep(Duration::from_millis(20));
        assert!(cb.can_execute(CircuitTier::Connection));
        assert_eq!(cb.state(CircuitTier::Connection), CircuitState::HalfOpen);
        cb.record_success(CircuitTier::Connection);
        assert_eq!(cb.state(CircuitTier::Connection), CircuitState::Closed);
    }

    #[test]
    fn health_check_single_failure_no_evict() {
        let checker = ConnectionHealthChecker::new(3);
        assert!(!checker.record_failure("conn-1"));
        assert!(!checker.should_evict("conn-1"));
    }

    #[test]
    fn health_check_three_failures_evict() {
        let checker = ConnectionHealthChecker::new(3);
        checker.record_failure("conn-1");
        checker.record_failure("conn-1");
        assert!(checker.record_failure("conn-1"));
        assert!(checker.should_evict("conn-1"));
    }

    #[test]
    fn health_check_success_resets() {
        let checker = ConnectionHealthChecker::new(3);
        checker.record_failure("conn-1");
        checker.record_failure("conn-1");
        checker.record_success("conn-1");
        assert_eq!(checker.failure_count("conn-1"), 0);
        assert!(!checker.should_evict("conn-1"));
    }

    #[test]
    fn scale_up_on_high_load() {
        let controller = PoolElasticController::new(ElasticConfig::default());
        let stats = PoolStats {
            total_connections: 10,
            idle_connections: 0,
            waiting_requests: 20,
        };
        let event = controller.evaluate_and_scale(&stats);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.reason, ScaleReason::HighLoad);
        assert!(event.to > event.from);
    }

    #[test]
    fn scale_down_on_low_load() {
        let config = ElasticConfig {
            min_connections: 2,
            max_connections: 50,
            scale_up_threshold: 10,
            scale_down_threshold: 5,
            scale_interval: Duration::from_secs(1),
            health_check_interval: Duration::from_secs(30),
            health_check_sql: "SELECT 1".to_string(),
        };
        let controller = PoolElasticController::new(config);
        *controller.current_size.lock().unwrap() = 20;
        let stats = PoolStats {
            total_connections: 20,
            idle_connections: 18,
            waiting_requests: 0,
        };
        let event = controller.evaluate_and_scale(&stats);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.reason, ScaleReason::LowLoad);
        assert!(event.to < event.from);
    }

    #[test]
    fn scale_respects_max() {
        let config = ElasticConfig {
            min_connections: 1,
            max_connections: 15,
            scale_up_threshold: 5,
            scale_down_threshold: 20,
            scale_interval: Duration::from_secs(1),
            health_check_interval: Duration::from_secs(30),
            health_check_sql: "SELECT 1".to_string(),
        };
        let controller = PoolElasticController::new(config);
        *controller.current_size.lock().unwrap() = 10;
        let stats = PoolStats {
            total_connections: 10,
            idle_connections: 0,
            waiting_requests: 100,
        };
        let event = controller.evaluate_and_scale(&stats).unwrap();
        assert_eq!(event.to, 15, "不应超过 max_connections");
    }

    #[test]
    fn scale_respects_min() {
        let config = ElasticConfig {
            min_connections: 5,
            max_connections: 50,
            scale_up_threshold: 10,
            scale_down_threshold: 3,
            scale_interval: Duration::from_secs(1),
            health_check_interval: Duration::from_secs(30),
            health_check_sql: "SELECT 1".to_string(),
        };
        let controller = PoolElasticController::new(config);
        *controller.current_size.lock().unwrap() = 8;
        let stats = PoolStats {
            total_connections: 8,
            idle_connections: 7,
            waiting_requests: 0,
        };
        let event = controller.evaluate_and_scale(&stats).unwrap();
        assert_eq!(event.to, 5, "不应低于 min_connections");
    }

    #[test]
    fn no_scale_when_stable() {
        let controller = PoolElasticController::new(ElasticConfig::default());
        let stats = PoolStats {
            total_connections: 10,
            idle_connections: 5,
            waiting_requests: 3,
        };
        let event = controller.evaluate_and_scale(&stats);
        assert!(event.is_none());
    }

    #[test]
    fn prewarm_to_min() {
        let config = ElasticConfig {
            min_connections: 5,
            max_connections: 50,
            scale_up_threshold: 10,
            scale_down_threshold: 20,
            scale_interval: Duration::from_secs(1),
            health_check_interval: Duration::from_secs(30),
            health_check_sql: "SELECT 1".to_string(),
        };
        let controller = PoolElasticController::new(config);
        let warmed = controller.prewarm(5);
        assert_eq!(warmed, 5);
        assert_eq!(controller.current_size(), 5);
    }

    #[test]
    fn prewarm_clamps_to_max() {
        let config = ElasticConfig {
            min_connections: 1,
            max_connections: 10,
            scale_up_threshold: 10,
            scale_down_threshold: 20,
            scale_interval: Duration::from_secs(1),
            health_check_interval: Duration::from_secs(30),
            health_check_sql: "SELECT 1".to_string(),
        };
        let controller = PoolElasticController::new(config);
        let warmed = controller.prewarm(100);
        assert_eq!(warmed, 10);
    }

    #[test]
    fn scale_events_recorded() {
        let controller = PoolElasticController::new(ElasticConfig::default());
        let stats = PoolStats {
            total_connections: 5,
            idle_connections: 0,
            waiting_requests: 20,
        };
        controller.evaluate_and_scale(&stats);
        assert_eq!(controller.scale_events().len(), 1);
    }

    #[test]
    fn wiring_public_api() {
        let controller = PoolElasticController::new(ElasticConfig::default());
        assert!(controller.config().validate().is_ok());
        assert!(controller
            .circuit_breaker()
            .can_execute(CircuitTier::Global));
        assert_eq!(controller.current_size(), 5);
    }
}
