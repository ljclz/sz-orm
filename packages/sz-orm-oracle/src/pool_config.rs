//! Oracle 连接池配置模块
//!
//! 提供 [`OraclePoolConfig`] 用于配置连接池参数，包括大小上限、空闲超时、
//! 健康检查间隔等；[`PoolStats`] 用于统计连接池运行指标。

use std::fmt;
use std::time::Duration;

/// Oracle 连接池配置
///
/// 控制 `OraclePoolHandle` 的行为参数。所有字段均通过 builder 风格的
/// `with_*` 方法设置，或通过 [`OraclePoolConfig::from_env`] 从环境变量加载。
#[derive(Debug, Clone)]
pub struct OraclePoolConfig {
    /// 连接池大小上限（默认 10）
    pub max_size: usize,
    /// 空闲连接超时（默认 30 分钟），超时后连接将被关闭
    pub idle_timeout: Duration,
    /// 健康检查间隔（默认 60 秒），定期 ping 空闲连接
    pub health_check_interval: Duration,
    /// 获取连接超时（默认 5 秒），超时返回错误
    pub acquire_timeout: Duration,
    /// 连接建立超时（默认 10 秒）
    pub connect_timeout: Duration,
    /// 最大连接生命周期（默认 1 小时），超时后连接将被重建
    pub max_lifetime: Duration,
    /// 是否启用连接健康检查（默认 true）
    pub health_check_enabled: bool,
    /// 最小空闲连接数（默认 0），低于此数将预创建连接
    pub min_idle: usize,
}

impl Default for OraclePoolConfig {
    fn default() -> Self {
        Self {
            max_size: 10,
            idle_timeout: Duration::from_secs(1800),
            health_check_interval: Duration::from_secs(60),
            acquire_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(10),
            max_lifetime: Duration::from_secs(3600),
            health_check_enabled: true,
            min_idle: 0,
        }
    }
}

impl OraclePoolConfig {
    /// 创建新的连接池配置（使用默认值）
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置连接池大小上限（最小为 1）
    #[must_use]
    pub fn with_max_size(mut self, max_size: usize) -> Self {
        self.max_size = max_size.max(1);
        self
    }

    /// 设置空闲连接超时
    #[must_use]
    pub fn with_idle_timeout(mut self, timeout: Duration) -> Self {
        self.idle_timeout = timeout;
        self
    }

    /// 设置健康检查间隔
    #[must_use]
    pub fn with_health_check_interval(mut self, interval: Duration) -> Self {
        self.health_check_interval = interval;
        self
    }

    /// 设置获取连接超时
    #[must_use]
    pub fn with_acquire_timeout(mut self, timeout: Duration) -> Self {
        self.acquire_timeout = timeout;
        self
    }

    /// 设置连接建立超时
    #[must_use]
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// 设置最大连接生命周期
    #[must_use]
    pub fn with_max_lifetime(mut self, lifetime: Duration) -> Self {
        self.max_lifetime = lifetime;
        self
    }

    /// 启用或禁用健康检查
    #[must_use]
    pub fn with_health_check(mut self, enabled: bool) -> Self {
        self.health_check_enabled = enabled;
        self
    }

    /// 设置最小空闲连接数
    #[must_use]
    pub fn with_min_idle(mut self, min_idle: usize) -> Self {
        self.min_idle = min_idle;
        self
    }

    /// 验证配置有效性
    ///
    /// # Errors
    ///
    /// 若配置无效返回 `Err(String)` 描述错误原因。
    pub fn validate(&self) -> Result<(), String> {
        if self.max_size == 0 {
            return Err("max_size must be greater than 0".to_string());
        }
        if self.min_idle > self.max_size {
            return Err("min_idle cannot exceed max_size".to_string());
        }
        if self.idle_timeout.as_secs() == 0 {
            return Err("idle_timeout must be greater than 0".to_string());
        }
        if self.acquire_timeout.as_secs() == 0 {
            return Err("acquire_timeout must be greater than 0".to_string());
        }
        Ok(())
    }

    /// 从环境变量构建配置
    ///
    /// 支持的环境变量：
    /// - `SZ_ORM_ORACLE_POOL_MAX_SIZE`：连接池大小上限
    /// - `SZ_ORM_ORACLE_POOL_MIN_IDLE`：最小空闲连接数
    /// - `SZ_ORM_ORACLE_POOL_IDLE_TIMEOUT`：空闲超时（秒）
    /// - `SZ_ORM_ORACLE_POOL_ACQUIRE_TIMEOUT`：获取超时（秒）
    /// - `SZ_ORM_ORACLE_POOL_MAX_LIFETIME`：最大生命周期（秒）
    #[must_use]
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Ok(val) = std::env::var("SZ_ORM_ORACLE_POOL_MAX_SIZE") {
            if let Ok(n) = val.parse::<usize>() {
                if n > 0 {
                    config.max_size = n;
                }
            }
        }
        if let Ok(val) = std::env::var("SZ_ORM_ORACLE_POOL_MIN_IDLE") {
            if let Ok(n) = val.parse::<usize>() {
                config.min_idle = n.min(config.max_size);
            }
        }
        if let Ok(val) = std::env::var("SZ_ORM_ORACLE_POOL_IDLE_TIMEOUT") {
            if let Ok(n) = val.parse::<u64>() {
                config.idle_timeout = Duration::from_secs(n);
            }
        }
        if let Ok(val) = std::env::var("SZ_ORM_ORACLE_POOL_ACQUIRE_TIMEOUT") {
            if let Ok(n) = val.parse::<u64>() {
                config.acquire_timeout = Duration::from_secs(n);
            }
        }
        if let Ok(val) = std::env::var("SZ_ORM_ORACLE_POOL_MAX_LIFETIME") {
            if let Ok(n) = val.parse::<u64>() {
                config.max_lifetime = Duration::from_secs(n);
            }
        }
        config
    }

    /// 计算有效连接数（`max(max_size, min_idle)`）
    #[must_use]
    pub fn effective_connections(&self) -> usize {
        self.max_size.max(self.min_idle)
    }

    /// 是否需要预创建连接
    #[must_use]
    pub fn needs_preconnect(&self) -> bool {
        self.min_idle > 0
    }

    /// 估算内存占用（字节）
    ///
    /// 每个连接约 64 KiB 缓冲区 + 4 KiB 元数据。
    #[must_use]
    pub fn estimated_memory_bytes(&self) -> u64 {
        const PER_CONN_BYTES: u64 = 68 * 1024;
        self.max_size as u64 * PER_CONN_BYTES
    }
}

/// 连接池统计信息
#[derive(Debug, Clone, Default)]
pub struct PoolStats {
    /// 当前空闲连接数
    pub idle_count: usize,
    /// 当前在用连接数
    pub in_use_count: usize,
    /// 累计创建连接数
    pub total_created: u64,
    /// 累计关闭连接数
    pub total_closed: u64,
    /// 累计获取连接次数
    pub total_acquired: u64,
    /// 累计归还连接次数
    pub total_released: u64,
    /// 累计获取超时次数
    pub acquire_timeouts: u64,
    /// 累计健康检查失败次数
    pub health_check_failures: u64,
}

impl PoolStats {
    /// 创建新的统计信息
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前总连接数（空闲 + 在用）
    #[must_use]
    pub fn total_connections(&self) -> usize {
        self.idle_count + self.in_use_count
    }

    /// 连接利用率（在用 / 总数）
    #[must_use]
    pub fn utilization(&self) -> f64 {
        let total = self.total_connections();
        if total == 0 {
            0.0
        } else {
            self.in_use_count as f64 / total as f64
        }
    }

    /// 获取超时率
    #[must_use]
    pub fn timeout_rate(&self) -> f64 {
        if self.total_acquired == 0 {
            0.0
        } else {
            self.acquire_timeouts as f64 / self.total_acquired as f64
        }
    }

    /// 健康检查失败率
    #[must_use]
    pub fn health_check_failure_rate(&self) -> f64 {
        let total_checks = self.health_check_failures + self.total_acquired;
        if total_checks == 0 {
            0.0
        } else {
            self.health_check_failures as f64 / total_checks as f64
        }
    }

    /// 合并两个统计信息（用于多池聚合）
    #[must_use]
    pub fn merge(&self, other: &PoolStats) -> PoolStats {
        PoolStats {
            idle_count: self.idle_count + other.idle_count,
            in_use_count: self.in_use_count + other.in_use_count,
            total_created: self.total_created + other.total_created,
            total_closed: self.total_closed + other.total_closed,
            total_acquired: self.total_acquired + other.total_acquired,
            total_released: self.total_released + other.total_released,
            acquire_timeouts: self.acquire_timeouts + other.acquire_timeouts,
            health_check_failures: self.health_check_failures + other.health_check_failures,
        }
    }

    /// 记录一次获取连接
    pub fn record_acquire(&mut self) {
        self.total_acquired += 1;
        self.in_use_count += 1;
        if self.idle_count > 0 {
            self.idle_count -= 1;
        }
    }

    /// 记录一次归还连接
    pub fn record_release(&mut self) {
        self.total_released += 1;
        self.idle_count += 1;
        if self.in_use_count > 0 {
            self.in_use_count -= 1;
        }
    }

    /// 记录一次获取超时
    pub fn record_timeout(&mut self) {
        self.acquire_timeouts += 1;
    }

    /// 记录一次健康检查失败
    pub fn record_health_check_failure(&mut self) {
        self.health_check_failures += 1;
    }
}

impl fmt::Display for PoolStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PoolStats(idle={}, in_use={}, total={}, acquired={}, timeouts={})",
            self.idle_count,
            self.in_use_count,
            self.total_connections(),
            self.total_acquired,
            self.acquire_timeouts
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_config_default() {
        let config = OraclePoolConfig::default();
        assert_eq!(config.max_size, 10);
        assert_eq!(config.min_idle, 0);
        assert!(config.health_check_enabled);
        assert_eq!(config.idle_timeout, Duration::from_secs(1800));
    }

    #[test]
    fn test_pool_config_new() {
        let config = OraclePoolConfig::new();
        assert_eq!(config.max_size, 10);
    }

    #[test]
    fn test_pool_config_builder() {
        let config = OraclePoolConfig::new()
            .with_max_size(20)
            .with_min_idle(5)
            .with_idle_timeout(Duration::from_secs(600))
            .with_health_check(false);
        assert_eq!(config.max_size, 20);
        assert_eq!(config.min_idle, 5);
        assert_eq!(config.idle_timeout, Duration::from_secs(600));
        assert!(!config.health_check_enabled);
    }

    #[test]
    fn test_pool_config_max_size_min_one() {
        let config = OraclePoolConfig::new().with_max_size(0);
        assert_eq!(config.max_size, 1);
    }

    #[test]
    fn test_pool_config_validate_ok() {
        let config = OraclePoolConfig::new();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_pool_config_validate_min_idle_exceeds_max() {
        let config = OraclePoolConfig::new().with_max_size(5).with_min_idle(10);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_pool_config_from_env_default() {
        std::env::remove_var("SZ_ORM_ORACLE_POOL_MAX_SIZE");
        let config = OraclePoolConfig::from_env();
        assert_eq!(config.max_size, 10);
    }

    #[test]
    fn test_pool_config_effective_connections() {
        let config = OraclePoolConfig::new().with_max_size(20).with_min_idle(5);
        assert_eq!(config.effective_connections(), 20);
    }

    #[test]
    fn test_pool_config_needs_preconnect() {
        let config = OraclePoolConfig::new().with_min_idle(3);
        assert!(config.needs_preconnect());
        let config2 = OraclePoolConfig::new();
        assert!(!config2.needs_preconnect());
    }

    #[test]
    fn test_pool_config_estimated_memory() {
        let config = OraclePoolConfig::new().with_max_size(10);
        let mem = config.estimated_memory_bytes();
        assert!(mem > 0);
        assert_eq!(mem, 10 * 68 * 1024);
    }

    #[test]
    fn test_pool_stats_default() {
        let stats = PoolStats::default();
        assert_eq!(stats.idle_count, 0);
        assert_eq!(stats.in_use_count, 0);
    }

    #[test]
    fn test_pool_stats_total_connections() {
        let stats = PoolStats {
            idle_count: 3,
            in_use_count: 5,
            ..PoolStats::default()
        };
        assert_eq!(stats.total_connections(), 8);
    }

    #[test]
    fn test_pool_stats_utilization() {
        let stats = PoolStats {
            idle_count: 2,
            in_use_count: 8,
            ..PoolStats::default()
        };
        let util = stats.utilization();
        assert!((util - 0.8).abs() < 1e-10);
    }

    #[test]
    fn test_pool_stats_utilization_zero() {
        let stats = PoolStats::default();
        assert_eq!(stats.utilization(), 0.0);
    }

    #[test]
    fn test_pool_stats_timeout_rate() {
        let stats = PoolStats {
            total_acquired: 100,
            acquire_timeouts: 5,
            ..PoolStats::default()
        };
        let rate = stats.timeout_rate();
        assert!((rate - 0.05).abs() < 1e-10);
    }

    #[test]
    fn test_pool_stats_merge() {
        let s1 = PoolStats {
            idle_count: 3,
            in_use_count: 2,
            total_acquired: 10,
            ..PoolStats::default()
        };
        let s2 = PoolStats {
            idle_count: 1,
            in_use_count: 4,
            total_acquired: 20,
            ..PoolStats::default()
        };
        let merged = s1.merge(&s2);
        assert_eq!(merged.idle_count, 4);
        assert_eq!(merged.in_use_count, 6);
        assert_eq!(merged.total_acquired, 30);
    }

    #[test]
    fn test_pool_stats_record_acquire() {
        let mut stats = PoolStats {
            idle_count: 5,
            ..PoolStats::default()
        };
        stats.record_acquire();
        assert_eq!(stats.in_use_count, 1);
        assert_eq!(stats.idle_count, 4);
        assert_eq!(stats.total_acquired, 1);
    }

    #[test]
    fn test_pool_stats_record_release() {
        let mut stats = PoolStats {
            in_use_count: 3,
            ..PoolStats::default()
        };
        stats.record_release();
        assert_eq!(stats.in_use_count, 2);
        assert_eq!(stats.idle_count, 1);
        assert_eq!(stats.total_released, 1);
    }

    #[test]
    fn test_pool_stats_record_timeout() {
        let mut stats = PoolStats::default();
        stats.record_timeout();
        assert_eq!(stats.acquire_timeouts, 1);
    }

    #[test]
    fn test_pool_stats_record_health_check_failure() {
        let mut stats = PoolStats::default();
        stats.record_health_check_failure();
        assert_eq!(stats.health_check_failures, 1);
    }

    #[test]
    fn test_pool_stats_display() {
        let stats = PoolStats {
            idle_count: 3,
            in_use_count: 2,
            total_acquired: 10,
            ..PoolStats::default()
        };
        let s = format!("{}", stats);
        assert!(s.contains("idle=3"));
        assert!(s.contains("in_use=2"));
    }
}
