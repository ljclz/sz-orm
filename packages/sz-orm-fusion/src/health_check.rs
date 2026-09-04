//! 数据源健康检查（DataSource Health Check）
//!
//! 定期检测数据源可用性，支持主动探测和被动记录。
//! 适用于多数据源融合场景下的故障转移决策。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// 数据源状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

impl HealthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            HealthStatus::Healthy => "healthy",
            HealthStatus::Degraded => "degraded",
            HealthStatus::Unhealthy => "unhealthy",
            HealthStatus::Unknown => "unknown",
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, HealthStatus::Healthy | HealthStatus::Degraded)
    }

    pub fn severity(&self) -> u8 {
        match self {
            HealthStatus::Healthy => 0,
            HealthStatus::Degraded => 1,
            HealthStatus::Unhealthy => 2,
            HealthStatus::Unknown => 3,
        }
    }
}

/// 单次健康检查结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthCheckResult {
    pub source: String,
    pub status: HealthStatus,
    pub latency_ms: u64,
    pub message: String,
    pub timestamp_ms: i64,
}

impl HealthCheckResult {
    pub fn healthy(source: &str, latency_ms: u64) -> Self {
        Self {
            source: source.to_string(),
            status: HealthStatus::Healthy,
            latency_ms,
            message: String::new(),
            timestamp_ms: now_ms(),
        }
    }

    pub fn degraded(source: &str, latency_ms: u64, msg: &str) -> Self {
        Self {
            source: source.to_string(),
            status: HealthStatus::Degraded,
            latency_ms,
            message: msg.to_string(),
            timestamp_ms: now_ms(),
        }
    }

    pub fn unhealthy(source: &str, msg: &str) -> Self {
        Self {
            source: source.to_string(),
            status: HealthStatus::Unhealthy,
            latency_ms: 0,
            message: msg.to_string(),
            timestamp_ms: now_ms(),
        }
    }
}

/// 数据源健康记录
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthRecord {
    pub source: String,
    pub status: HealthStatus,
    pub last_check_ms: i64,
    pub consecutive_failures: u32,
    pub total_checks: u64,
    pub total_failures: u64,
    pub avg_latency_ms: u64,
    pub max_latency_ms: u64,
    pub min_latency_ms: u64,
}

impl Default for HealthRecord {
    fn default() -> Self {
        Self {
            source: String::new(),
            status: HealthStatus::Unknown,
            last_check_ms: 0,
            consecutive_failures: 0,
            total_checks: 0,
            total_failures: 0,
            avg_latency_ms: 0,
            max_latency_ms: 0,
            min_latency_ms: u64::MAX,
        }
    }
}

/// 健康检查器
///
/// 管理多个数据源的健康状态，支持主动探测和被动记录。
pub struct HealthChecker {
    records: Arc<RwLock<HashMap<String, HealthRecord>>>,
    failure_threshold: u32,
    degraded_latency_ms: u64,
    unhealthy_latency_ms: u64,
    total_checks: AtomicU64,
    total_failures: AtomicU64,
}

impl HealthChecker {
    /// 创建健康检查器
    ///
    /// - `failure_threshold`：连续失败多少次标记为 Unhealthy
    /// - `degraded_latency_ms`：延迟超过此值标记为 Degraded
    /// - `unhealthy_latency_ms`：延迟超过此值标记为 Unhealthy
    pub fn new(
        failure_threshold: u32,
        degraded_latency_ms: u64,
        unhealthy_latency_ms: u64,
    ) -> Self {
        Self {
            records: Arc::new(RwLock::new(HashMap::new())),
            failure_threshold,
            degraded_latency_ms,
            unhealthy_latency_ms,
            total_checks: AtomicU64::new(0),
            total_failures: AtomicU64::new(0),
        }
    }

    /// 记算状态
    fn compute_status(&self, latency_ms: u64, consecutive_failures: u32) -> HealthStatus {
        if consecutive_failures >= self.failure_threshold {
            return HealthStatus::Unhealthy;
        }
        if latency_ms >= self.unhealthy_latency_ms {
            return HealthStatus::Unhealthy;
        }
        if latency_ms >= self.degraded_latency_ms {
            return HealthStatus::Degraded;
        }
        if consecutive_failures > 0 {
            return HealthStatus::Degraded;
        }
        HealthStatus::Healthy
    }

    /// 记录一次健康检查
    pub fn record(&self, result: &HealthCheckResult) {
        self.total_checks.fetch_add(1, Ordering::Relaxed);
        if !result.status.is_available() {
            self.total_failures.fetch_add(1, Ordering::Relaxed);
        }
        if let Ok(mut records) = self.records.write() {
            let record = records
                .entry(result.source.clone())
                .or_insert_with(|| HealthRecord {
                    source: result.source.clone(),
                    ..Default::default()
                });
            record.last_check_ms = result.timestamp_ms;
            record.total_checks += 1;
            if !result.status.is_available() {
                record.consecutive_failures += 1;
                record.total_failures += 1;
            } else {
                record.consecutive_failures = 0;
            }
            if result.latency_ms > 0 {
                record.avg_latency_ms = if record.total_checks > 0 {
                    (record.avg_latency_ms * (record.total_checks - 1) + result.latency_ms)
                        .div_ceil(record.total_checks)
                } else {
                    result.latency_ms
                };
                record.max_latency_ms = record.max_latency_ms.max(result.latency_ms);
                record.min_latency_ms = record.min_latency_ms.min(result.latency_ms);
            }
            record.status = if result.status == HealthStatus::Unhealthy {
                HealthStatus::Unhealthy
            } else {
                self.compute_status(result.latency_ms, record.consecutive_failures)
            };
        }
    }

    /// 主动探测数据源
    ///
    /// `probe` 闭包返回 `(是否成功, 延迟ms)`
    pub fn probe<F>(&self, source: &str, probe: F) -> HealthCheckResult
    where
        F: FnOnce() -> Result<u64, String>,
    {
        let start = Instant::now();
        match probe() {
            Ok(latency_ms) => {
                let total_latency = start.elapsed().as_millis() as u64;
                let status = self.compute_status(total_latency, 0);
                let result = HealthCheckResult {
                    source: source.to_string(),
                    status,
                    latency_ms,
                    message: String::new(),
                    timestamp_ms: now_ms(),
                };
                self.record(&result);
                result
            }
            Err(msg) => {
                let result = HealthCheckResult::unhealthy(source, &msg);
                self.record(&result);
                result
            }
        }
    }

    /// 获取数据源状态
    pub fn status(&self, source: &str) -> HealthStatus {
        self.records
            .read()
            .ok()
            .and_then(|r| r.get(source).map(|rec| rec.status))
            .unwrap_or(HealthStatus::Unknown)
    }

    /// 获取数据源记录
    pub fn record_of(&self, source: &str) -> Option<HealthRecord> {
        self.records
            .read()
            .ok()
            .and_then(|r| r.get(source).cloned())
    }

    /// 获取所有可用数据源
    pub fn available_sources(&self) -> Vec<String> {
        self.records
            .read()
            .ok()
            .map(|r| {
                r.iter()
                    .filter(|(_, rec)| rec.status.is_available())
                    .map(|(k, _)| k.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 获取所有数据源记录
    pub fn all_records(&self) -> Vec<HealthRecord> {
        self.records
            .read()
            .ok()
            .map(|r| r.values().cloned().collect())
            .unwrap_or_default()
    }

    /// 选择最健康的可用数据源
    pub fn select_best(&self) -> Option<String> {
        self.records.read().ok().and_then(|r| {
            r.iter()
                .filter(|(_, rec)| rec.status.is_available())
                .min_by_key(|(_, rec)| (rec.status.severity(), rec.avg_latency_ms))
                .map(|(k, _)| k.clone())
        })
    }

    /// 数据源数量
    pub fn source_count(&self) -> usize {
        self.records.read().map(|r| r.len()).unwrap_or(0)
    }

    /// 总检查次数
    pub fn total_checks(&self) -> u64 {
        self.total_checks.load(Ordering::Relaxed)
    }

    /// 总失败次数
    pub fn total_failures(&self) -> u64 {
        self.total_failures.load(Ordering::Relaxed)
    }

    /// 重置数据源状态
    pub fn reset(&self, source: &str) {
        if let Ok(mut records) = self.records.write() {
            if let Some(rec) = records.get_mut(source) {
                rec.consecutive_failures = 0;
                rec.status = HealthStatus::Healthy;
            }
        }
    }

    /// 导出为 JSON
    pub fn to_json(&self) -> serde_json::Value {
        let records = self.all_records();
        serde_json::json!({
            "sources": records,
            "total_checks": self.total_checks(),
            "total_failures": self.total_failures(),
        })
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new(3, 500, 2000)
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// 健康检查调度器
///
/// 定期对数据源执行健康检查。
pub struct HealthCheckScheduler {
    checker: Arc<HealthChecker>,
    interval: Duration,
    last_check: RwLock<Option<Instant>>,
}

impl HealthCheckScheduler {
    /// 创建调度器
    pub fn new(checker: Arc<HealthChecker>, interval: Duration) -> Self {
        Self {
            checker,
            interval,
            last_check: RwLock::new(None),
        }
    }

    /// 是否到了下次检查时间
    pub fn should_check(&self) -> bool {
        match self.last_check.read().ok().and_then(|r| *r) {
            None => true,
            Some(last) => last.elapsed() >= self.interval,
        }
    }

    /// 执行检查并更新时间
    pub fn run_check<F>(&self, source: &str, probe: F) -> HealthCheckResult
    where
        F: FnOnce() -> Result<u64, String>,
    {
        let result = self.checker.probe(source, probe);
        if let Ok(mut last) = self.last_check.write() {
            *last = Some(Instant::now());
        }
        result
    }

    /// 获取检查器
    pub fn checker(&self) -> &Arc<HealthChecker> {
        &self.checker
    }

    /// 检查间隔
    pub fn interval(&self) -> Duration {
        self.interval
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_as_str() {
        assert_eq!(HealthStatus::Healthy.as_str(), "healthy");
        assert_eq!(HealthStatus::Degraded.as_str(), "degraded");
        assert_eq!(HealthStatus::Unhealthy.as_str(), "unhealthy");
        assert_eq!(HealthStatus::Unknown.as_str(), "unknown");
    }

    #[test]
    fn test_health_status_is_available() {
        assert!(HealthStatus::Healthy.is_available());
        assert!(HealthStatus::Degraded.is_available());
        assert!(!HealthStatus::Unhealthy.is_available());
        assert!(!HealthStatus::Unknown.is_available());
    }

    #[test]
    fn test_health_status_severity() {
        assert_eq!(HealthStatus::Healthy.severity(), 0);
        assert_eq!(HealthStatus::Degraded.severity(), 1);
        assert_eq!(HealthStatus::Unhealthy.severity(), 2);
        assert_eq!(HealthStatus::Unknown.severity(), 3);
    }

    #[test]
    fn test_health_check_result_healthy() {
        let r = HealthCheckResult::healthy("db1", 10);
        assert_eq!(r.status, HealthStatus::Healthy);
        assert_eq!(r.latency_ms, 10);
    }

    #[test]
    fn test_health_check_result_degraded() {
        let r = HealthCheckResult::degraded("db1", 500, "slow");
        assert_eq!(r.status, HealthStatus::Degraded);
        assert_eq!(r.message, "slow");
    }

    #[test]
    fn test_health_check_result_unhealthy() {
        let r = HealthCheckResult::unhealthy("db1", "down");
        assert_eq!(r.status, HealthStatus::Unhealthy);
        assert_eq!(r.message, "down");
    }

    #[test]
    fn test_health_checker_basic() {
        let checker = HealthChecker::new(3, 500, 2000);
        let r = HealthCheckResult::healthy("db1", 10);
        checker.record(&r);
        assert_eq!(checker.status("db1"), HealthStatus::Healthy);
    }

    #[test]
    fn test_health_checker_degraded_by_latency() {
        let checker = HealthChecker::new(3, 500, 2000);
        let r = HealthCheckResult::healthy("db1", 600);
        checker.record(&r);
        assert_eq!(checker.status("db1"), HealthStatus::Degraded);
    }

    #[test]
    fn test_health_checker_unhealthy_by_latency() {
        let checker = HealthChecker::new(3, 500, 2000);
        let r = HealthCheckResult::healthy("db1", 3000);
        checker.record(&r);
        assert_eq!(checker.status("db1"), HealthStatus::Unhealthy);
    }

    #[test]
    fn test_health_checker_consecutive_failures() {
        let checker = HealthChecker::new(3, 500, 2000);
        for _ in 0..3 {
            checker.record(&HealthCheckResult::unhealthy("db1", "down"));
        }
        assert_eq!(checker.status("db1"), HealthStatus::Unhealthy);
    }

    #[test]
    fn test_health_checker_probe_success() {
        let checker = HealthChecker::new(3, 500, 2000);
        let result = checker.probe("db1", || Ok(10));
        assert_eq!(result.status, HealthStatus::Healthy);
    }

    #[test]
    fn test_health_checker_probe_failure() {
        let checker = HealthChecker::new(3, 500, 2000);
        let result = checker.probe("db1", || Err("connection refused".to_string()));
        assert_eq!(result.status, HealthStatus::Unhealthy);
    }

    #[test]
    fn test_health_checker_available_sources() {
        let checker = HealthChecker::new(3, 500, 2000);
        checker.record(&HealthCheckResult::healthy("db1", 10));
        checker.record(&HealthCheckResult::unhealthy("db2", "down"));
        let available = checker.available_sources();
        assert!(available.contains(&"db1".to_string()));
        assert!(!available.contains(&"db2".to_string()));
    }

    #[test]
    fn test_health_checker_select_best() {
        let checker = HealthChecker::new(3, 500, 2000);
        checker.record(&HealthCheckResult::healthy("db1", 100));
        checker.record(&HealthCheckResult::healthy("db2", 50));
        let best = checker.select_best().unwrap();
        assert_eq!(best, "db2");
    }

    #[test]
    fn test_health_checker_select_best_none() {
        let checker = HealthChecker::new(3, 500, 2000);
        checker.record(&HealthCheckResult::unhealthy("db1", "down"));
        assert!(checker.select_best().is_none());
    }

    #[test]
    fn test_health_checker_record_of() {
        let checker = HealthChecker::new(3, 500, 2000);
        checker.record(&HealthCheckResult::healthy("db1", 10));
        let rec = checker.record_of("db1").unwrap();
        assert_eq!(rec.total_checks, 1);
        assert_eq!(rec.avg_latency_ms, 10);
    }

    #[test]
    fn test_health_checker_all_records() {
        let checker = HealthChecker::new(3, 500, 2000);
        checker.record(&HealthCheckResult::healthy("db1", 10));
        checker.record(&HealthCheckResult::healthy("db2", 20));
        assert_eq!(checker.all_records().len(), 2);
    }

    #[test]
    fn test_health_checker_source_count() {
        let checker = HealthChecker::new(3, 500, 2000);
        assert_eq!(checker.source_count(), 0);
        checker.record(&HealthCheckResult::healthy("db1", 10));
        assert_eq!(checker.source_count(), 1);
    }

    #[test]
    fn test_health_checker_total_checks() {
        let checker = HealthChecker::new(3, 500, 2000);
        checker.record(&HealthCheckResult::healthy("db1", 10));
        checker.record(&HealthCheckResult::healthy("db1", 20));
        assert_eq!(checker.total_checks(), 2);
    }

    #[test]
    fn test_health_checker_reset() {
        let checker = HealthChecker::new(3, 500, 2000);
        for _ in 0..3 {
            checker.record(&HealthCheckResult::unhealthy("db1", "down"));
        }
        assert_eq!(checker.status("db1"), HealthStatus::Unhealthy);
        checker.reset("db1");
        assert_eq!(checker.status("db1"), HealthStatus::Healthy);
    }

    #[test]
    fn test_health_checker_to_json() {
        let checker = HealthChecker::new(3, 500, 2000);
        checker.record(&HealthCheckResult::healthy("db1", 10));
        let json = checker.to_json();
        assert_eq!(json["total_checks"], 1);
    }

    #[test]
    fn test_health_checker_recovery() {
        let checker = HealthChecker::new(3, 500, 2000);
        checker.record(&HealthCheckResult::unhealthy("db1", "down"));
        assert_eq!(checker.status("db1"), HealthStatus::Unhealthy);
        checker.record(&HealthCheckResult::healthy("db1", 10));
        assert_eq!(checker.status("db1"), HealthStatus::Healthy);
    }

    #[test]
    fn test_scheduler_should_check_initial() {
        let checker = Arc::new(HealthChecker::default());
        let scheduler = HealthCheckScheduler::new(checker, Duration::from_secs(60));
        assert!(scheduler.should_check());
    }

    #[test]
    fn test_scheduler_run_check() {
        let checker = Arc::new(HealthChecker::default());
        let scheduler = HealthCheckScheduler::new(checker, Duration::from_secs(60));
        let result = scheduler.run_check("db1", || Ok(10));
        assert_eq!(result.status, HealthStatus::Healthy);
    }

    #[test]
    fn test_scheduler_interval() {
        let checker = Arc::new(HealthChecker::default());
        let scheduler = HealthCheckScheduler::new(checker, Duration::from_secs(30));
        assert_eq!(scheduler.interval(), Duration::from_secs(30));
    }
}
