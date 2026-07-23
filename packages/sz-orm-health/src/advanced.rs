//! 高级健康检查功能：缓存、级联检查、readiness/liveness 区分、超时
//!
//! 本模块在 [`DbHealthChecker`] 基础上补充生产级健康检查所需的核心能力：
//!
//! - **健康检查缓存**（[`HealthCheckCache`]）：带 TTL 的结果缓存，避免高频探活
//!   对后端造成压力。缓存命中时直接返回上次结果，过期后重新检查。
//! - **级联健康检查**（[`CascadingHealthChecker`]）：按依赖图检查资源及其依赖，
//!   任一依赖不健康则级联标记资源不健康。支持循环依赖检测。
//! - **Readiness / Liveness 探针**（[`ProbeManager`]）：区分 Kubernetes 风格的
//!   liveness（进程存活）与 readiness（就绪可接流量）探针，两者独立管理。
//! - **超时健康检查**（[`TimeoutHealthChecker`]）：通过独立线程 + mpsc 通道实现
//!   真实超时，超时后返回 Unhealthy。提供检查耗时与超时率统计。

use crate::{DbHealthChecker, HealthReport, HealthSnapshot, HealthStatus, HealthStatusProvider};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{mpsc, Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

// ============================================================================
// 健康检查缓存（HealthCheckCache）
// ============================================================================

/// 缓存的健康检查结果，记录检查时刻用于 TTL 判断。
struct CachedReport {
    report: HealthReport,
    cached_at: Instant,
}

/// 缓存统计信息
struct CacheStats {
    /// 缓存命中次数（TTL 内直接返回缓存）
    hits: u64,
    /// 缓存未命中次数（TTL 过期或无缓存，触发实际检查）
    misses: u64,
    /// 主动失效次数（调用 invalidate 清除缓存）
    evictions: u64,
}

/// 带 TTL 缓存的健康检查器。
///
/// 包装一个 [`DbHealthChecker`]，在 TTL 有效期内直接返回缓存的 [`HealthReport`]，
/// 过期后才执行实际检查。适用于高频探活场景（如 Kubernetes 每 2 秒 liveness 探针），
/// 避免每次探活都打到后端数据库。
///
/// # 线程安全
///
/// 内部使用 `RwLock` 保护缓存 map，`Mutex` 保护统计计数器，支持多线程并发访问。
pub struct HealthCheckCache {
    /// 被包装的实际健康检查器
    inner: Arc<dyn DbHealthChecker>,
    /// 缓存生存时间（TTL）
    ttl: Duration,
    /// 按 pool 名缓存的检查结果
    cache: RwLock<HashMap<String, CachedReport>>,
    /// 缓存统计
    stats: Mutex<CacheStats>,
}

impl HealthCheckCache {
    /// 创建带缓存的健康检查器
    ///
    /// # 参数
    /// - `inner`：被包装的实际检查器
    /// - `ttl`：缓存生存时间，超过此时间后下次检查会触发实际调用
    pub fn new(inner: Arc<dyn DbHealthChecker>, ttl: Duration) -> Self {
        Self {
            inner,
            ttl,
            cache: RwLock::new(HashMap::new()),
            stats: Mutex::new(CacheStats {
                hits: 0,
                misses: 0,
                evictions: 0,
            }),
        }
    }

    /// 检查指定 pool 的健康状态（带缓存）。
    ///
    /// 若缓存中存在且未过期（`cached_at + ttl > now`），直接返回缓存结果（hit）；
    /// 否则调用内部检查器执行实际检查，更新缓存后返回（miss）。
    pub fn check(&self, pool: &str) -> HealthReport {
        // 先尝试读缓存（读锁）
        if let Ok(cache) = self.cache.read() {
            if let Some(cached) = cache.get(pool) {
                if cached.cached_at.elapsed() < self.ttl {
                    // 缓存命中
                    if let Ok(mut stats) = self.stats.lock() {
                        stats.hits += 1;
                    }
                    return cached.report.clone();
                }
            }
        }

        // 缓存未命中或已过期，执行实际检查
        let report = self.inner.check(pool);
        let cached = CachedReport {
            report: report.clone(),
            cached_at: Instant::now(),
        };

        // 写入缓存（写锁）
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(pool.to_string(), cached);
        }
        if let Ok(mut stats) = self.stats.lock() {
            stats.misses += 1;
        }

        report
    }

    /// 批量检查多个 pool（带缓存），委托给 [`Self::check`] 逐个处理。
    pub fn check_all(&self, pools: &[&str]) -> Vec<HealthReport> {
        pools.iter().map(|p| self.check(p)).collect()
    }

    /// 主动失效指定 pool 的缓存。返回 `true` 表示之前有缓存被清除。
    pub fn invalidate(&self, pool: &str) -> bool {
        let removed = if let Ok(mut cache) = self.cache.write() {
            cache.remove(pool).is_some()
        } else {
            false
        };
        if removed {
            if let Ok(mut stats) = self.stats.lock() {
                stats.evictions += 1;
            }
        }
        removed
    }

    /// 清空所有缓存。
    pub fn clear(&self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
    }

    /// 获取缓存统计快照：`(hits, misses, evictions)`
    pub fn stats(&self) -> (u64, u64, u64) {
        if let Ok(stats) = self.stats.lock() {
            (stats.hits, stats.misses, stats.evictions)
        } else {
            (0, 0, 0)
        }
    }

    /// 获取缓存命中率（0.0..=1.0）。无请求时返回 0.0。
    pub fn hit_rate(&self) -> f64 {
        if let Ok(stats) = self.stats.lock() {
            let total = stats.hits + stats.misses;
            if total == 0 {
                0.0
            } else {
                stats.hits as f64 / total as f64
            }
        } else {
            0.0
        }
    }

    /// 获取 TTL
    pub fn ttl(&self) -> Duration {
        self.ttl
    }
}

// ============================================================================
// 级联健康检查（CascadingHealthChecker）
// ============================================================================

/// 级联健康检查结果：包含资源自身的报告及其所有依赖的报告。
#[derive(Debug, Clone)]
pub struct CascadingReport {
    /// 资源自身的健康报告
    pub report: HealthReport,
    /// 所有依赖的健康报告（按检查顺序）
    pub dependencies: Vec<HealthReport>,
    /// 资源及其所有依赖是否全部健康
    pub all_healthy: bool,
}

/// 级联健康检查器：检查资源时同时检查其依赖链。
///
/// 在 Kubernetes / 微服务架构中，一个服务的健康通常依赖下游服务。
/// [`CascadingHealthChecker`] 维护一个 pool -> 依赖列表 的映射，检查时先检查
/// 资源自身，再递归检查所有依赖。若任一依赖不健康，则 `all_healthy` 为 `false`。
///
/// # 循环依赖检测
///
/// 内部使用 `visited` 集合检测循环依赖，避免无限递归。遇到已访问的 pool 时跳过。
pub struct CascadingHealthChecker {
    /// 实际执行检查的检查器
    checker: Arc<dyn DbHealthChecker>,
    /// 依赖关系图：pool -> [依赖 pool 列表]
    dependencies: RwLock<HashMap<String, Vec<String>>>,
}

impl CascadingHealthChecker {
    /// 创建级联健康检查器
    pub fn new(checker: Arc<dyn DbHealthChecker>) -> Self {
        Self {
            checker,
            dependencies: RwLock::new(HashMap::new()),
        }
    }

    /// 为指定 pool 添加一个依赖
    pub fn add_dependency(&self, pool: &str, depends_on: impl Into<String>) {
        if let Ok(mut deps) = self.dependencies.write() {
            deps.entry(pool.to_string())
                .or_default()
                .push(depends_on.into());
        }
    }

    /// 为指定 pool 批量添加依赖
    pub fn add_dependencies(&self, pool: &str, deps: Vec<String>) {
        if let Ok(mut map) = self.dependencies.write() {
            map.entry(pool.to_string()).or_default().extend(deps);
        }
    }

    /// 移除指定 pool 的某个依赖。返回 `true` 表示成功移除。
    pub fn remove_dependency(&self, pool: &str, depends_on: &str) -> bool {
        if let Ok(mut map) = self.dependencies.write() {
            if let Some(deps) = map.get_mut(pool) {
                let before = deps.len();
                deps.retain(|d| d != depends_on);
                return deps.len() < before;
            }
        }
        false
    }

    /// 获取指定 pool 的依赖列表（拷贝）
    pub fn dependencies(&self, pool: &str) -> Vec<String> {
        if let Ok(map) = self.dependencies.read() {
            map.get(pool).cloned().unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// 清除指定 pool 的所有依赖
    pub fn clear_dependencies(&self, pool: &str) {
        if let Ok(mut map) = self.dependencies.write() {
            map.remove(pool);
        }
    }

    /// 检查指定 pool 及其所有依赖（递归），返回级联报告。
    ///
    /// 使用 `visited` 集合防止循环依赖导致的无限递归。
    pub fn check_with_deps(&self, pool: &str) -> CascadingReport {
        let mut visited = HashSet::new();
        visited.insert(pool.to_string());
        let mut dep_reports = Vec::new();
        self.collect_dep_reports(pool, &mut visited, &mut dep_reports);

        let report = self.checker.check(pool);
        let all_healthy = report.status == HealthStatus::Healthy
            && dep_reports
                .iter()
                .all(|r| r.status == HealthStatus::Healthy);

        CascadingReport {
            report,
            dependencies: dep_reports,
            all_healthy,
        }
    }

    /// 递归收集依赖的健康报告
    fn collect_dep_reports(
        &self,
        pool: &str,
        visited: &mut HashSet<String>,
        reports: &mut Vec<HealthReport>,
    ) {
        let deps = self.dependencies(pool);
        for dep in deps {
            if visited.contains(&dep) {
                // 循环依赖检测：跳过已访问的节点
                continue;
            }
            visited.insert(dep.clone());
            reports.push(self.checker.check(&dep));
            self.collect_dep_reports(&dep, visited, reports);
        }
    }
}

// ============================================================================
// Readiness / Liveness 探针（ProbeManager）
// ============================================================================

/// 探针类型：liveness（存活）或 readiness（就绪）。
///
/// 在 Kubernetes 中：
/// - **Liveness** 探针失败会导致 Pod 重启（进程不健康）
/// - **Readiness** 探针失败会从 Service Endpoints 中摘除 Pod（不接流量）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProbeKind {
    /// 存活探针：进程是否在运行
    Liveness,
    /// 就绪探针：是否准备好接收流量
    Readiness,
}

/// 探针检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    /// 探针类型
    pub kind: ProbeKind,
    /// 健康状态
    pub status: HealthStatus,
    /// 附加消息
    pub message: String,
    /// 检查时间戳（RFC3339）
    pub timestamp: String,
}

/// 探针管理器：独立管理 liveness 和 readiness 探针。
///
/// 两种探针的状态独立设置和查询，互不影响。例如：
/// - 进程刚启动时 liveness=Healthy 但 readiness=Unhealthy（正在加载缓存）
/// - 依赖下游故障时 readiness=Unhealthy 但 liveness=Healthy（进程本身没问题）
pub struct ProbeManager {
    /// liveness 探针状态
    liveness: RwLock<HashMap<String, HealthSnapshot>>,
    /// readiness 探针状态
    readiness: RwLock<HashMap<String, HealthSnapshot>>,
}

impl Default for ProbeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProbeManager {
    /// 创建探针管理器
    pub fn new() -> Self {
        Self {
            liveness: RwLock::new(HashMap::new()),
            readiness: RwLock::new(HashMap::new()),
        }
    }

    /// 设置 liveness 探针状态
    pub fn set_liveness(&self, name: &str, snapshot: HealthSnapshot) {
        if let Ok(mut map) = self.liveness.write() {
            map.insert(name.to_string(), snapshot);
        }
    }

    /// 设置 readiness 探针状态
    pub fn set_readiness(&self, name: &str, snapshot: HealthSnapshot) {
        if let Ok(mut map) = self.readiness.write() {
            map.insert(name.to_string(), snapshot);
        }
    }

    /// 查询单个探针的 liveness 状态
    pub fn check_liveness(&self, name: &str) -> ProbeResult {
        let snapshot = self.read_probe(ProbeKind::Liveness, name);
        ProbeResult {
            kind: ProbeKind::Liveness,
            status: snapshot.status,
            message: snapshot.message,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// 查询单个探针的 readiness 状态
    pub fn check_readiness(&self, name: &str) -> ProbeResult {
        let snapshot = self.read_probe(ProbeKind::Readiness, name);
        ProbeResult {
            kind: ProbeKind::Readiness,
            status: snapshot.status,
            message: snapshot.message,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// 查询所有 liveness 探针
    pub fn liveness_all(&self) -> Vec<ProbeResult> {
        self.all_probes(ProbeKind::Liveness)
    }

    /// 查询所有 readiness 探针
    pub fn readiness_all(&self) -> Vec<ProbeResult> {
        self.all_probes(ProbeKind::Readiness)
    }

    /// 聚合 liveness 状态：任一不健康则整体不健康，任一未知则整体未知
    pub fn overall_liveness(&self) -> HealthStatus {
        self.overall(ProbeKind::Liveness)
    }

    /// 聚合 readiness 状态：任一不健康则整体不健康，任一未知则整体未知
    pub fn overall_readiness(&self) -> HealthStatus {
        self.overall(ProbeKind::Readiness)
    }

    /// 读取指定探针的状态快照
    fn read_probe(&self, kind: ProbeKind, name: &str) -> HealthSnapshot {
        let map = match kind {
            ProbeKind::Liveness => self.liveness.read(),
            ProbeKind::Readiness => self.readiness.read(),
        };
        match map {
            Ok(guard) => guard
                .get(name)
                .cloned()
                .unwrap_or_else(|| HealthSnapshot {
                    status: HealthStatus::Unknown,
                    connection_count: 0,
                    slow_queries: 0,
                    message: format!("no {:?} probe registered for '{}'", kind, name),
                }),
            Err(_) => HealthSnapshot {
                status: HealthStatus::Unknown,
                connection_count: 0,
                slow_queries: 0,
                message: "lock poisoned".to_string(),
            },
        }
    }

    /// 查询指定类型的所有探针
    fn all_probes(&self, kind: ProbeKind) -> Vec<ProbeResult> {
        let map = match kind {
            ProbeKind::Liveness => self.liveness.read(),
            ProbeKind::Readiness => self.readiness.read(),
        };
        let timestamp = chrono::Utc::now().to_rfc3339();
        match map {
            Ok(guard) => {
                let mut results: Vec<ProbeResult> = guard
                    .values()
                    .map(|snap| ProbeResult {
                        kind,
                        status: snap.status.clone(),
                        message: snap.message.clone(),
                        timestamp: timestamp.clone(),
                    })
                    .collect();
                // 按 message 排序保证输出确定性
                results.sort_by(|a, b| a.message.cmp(&b.message));
                results
            }
            Err(_) => Vec::new(),
        }
    }

    /// 聚合指定类型的整体状态
    fn overall(&self, kind: ProbeKind) -> HealthStatus {
        let map = match kind {
            ProbeKind::Liveness => self.liveness.read(),
            ProbeKind::Readiness => self.readiness.read(),
        };
        match map {
            Ok(guard) => {
                if guard.is_empty() {
                    return HealthStatus::Unknown;
                }
                let mut any_unknown = false;
                for snap in guard.values() {
                    match snap.status {
                        HealthStatus::Unhealthy => return HealthStatus::Unhealthy,
                        HealthStatus::Unknown => any_unknown = true,
                        HealthStatus::Healthy => {}
                    }
                }
                if any_unknown {
                    HealthStatus::Unknown
                } else {
                    HealthStatus::Healthy
                }
            }
            Err(_) => HealthStatus::Unknown,
        }
    }
}

// ============================================================================
// 超时健康检查（TimeoutHealthChecker）
// ============================================================================

/// 超时检查统计
struct TimeoutStats {
    /// 总检查次数
    total_checks: u64,
    /// 超时次数
    timeouts: u64,
    /// 累计检查耗时
    total_duration: Duration,
}

/// 超时统计快照（不可变视图）
#[derive(Debug, Clone)]
pub struct TimeoutStatsSnapshot {
    /// 总检查次数
    pub total_checks: u64,
    /// 超时次数
    pub timeouts: u64,
    /// 累计检查耗时
    pub total_duration: Duration,
    /// 平均检查耗时
    pub avg_duration: Duration,
    /// 超时率（0.0..=1.0）
    pub timeout_rate: f64,
}

/// 带超时的健康检查提供者。
///
/// 通过独立线程 + mpsc 通道实现真实超时：在调用内部 provider 的 `snapshot` 时，
/// 若超过 `timeout` 仍未返回结果，则立即返回 Unhealthy（超时消息）。
///
/// 适用于包装可能阻塞的 provider（如远程 HTTP 健康检查），防止探活请求挂起
/// 导致整个健康检查系统卡死。
///
/// # 注意
///
/// 超时后内部线程会被分离（detach），仍会在后台完成。这是 sync Rust 的限制：
/// 无法真正中断一个正在执行的 sync 函数。
pub struct TimeoutHealthChecker {
    /// 被包装的 provider
    inner: Arc<dyn HealthStatusProvider>,
    /// 超时时长
    timeout: Duration,
    /// 统计信息
    stats: Mutex<TimeoutStats>,
}

impl TimeoutHealthChecker {
    /// 创建带超时的健康检查提供者
    ///
    /// # 参数
    /// - `inner`：被包装的 provider
    /// - `timeout`：超时时长，超过此时间未返回则标记为 Unhealthy
    pub fn new(inner: Arc<dyn HealthStatusProvider>, timeout: Duration) -> Self {
        Self {
            inner,
            timeout,
            stats: Mutex::new(TimeoutStats {
                total_checks: 0,
                timeouts: 0,
                total_duration: Duration::ZERO,
            }),
        }
    }

    /// 获取超时时长
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// 获取统计快照
    pub fn stats(&self) -> TimeoutStatsSnapshot {
        if let Ok(stats) = self.stats.lock() {
            let avg = if stats.total_checks > 0 {
                stats.total_duration / stats.total_checks as u32
            } else {
                Duration::ZERO
            };
            let rate = if stats.total_checks > 0 {
                stats.timeouts as f64 / stats.total_checks as f64
            } else {
                0.0
            };
            TimeoutStatsSnapshot {
                total_checks: stats.total_checks,
                timeouts: stats.timeouts,
                total_duration: stats.total_duration,
                avg_duration: avg,
                timeout_rate: rate,
            }
        } else {
            TimeoutStatsSnapshot {
                total_checks: 0,
                timeouts: 0,
                total_duration: Duration::ZERO,
                avg_duration: Duration::ZERO,
                timeout_rate: 0.0,
            }
        }
    }
}

impl HealthStatusProvider for TimeoutHealthChecker {
    fn snapshot(&self, pool: &str) -> HealthSnapshot {
        let start = Instant::now();

        // 通过 channel 实现超时：在独立线程中执行实际检查
        let (tx, rx) = mpsc::channel();
        let inner = Arc::clone(&self.inner);
        let pool_owned = pool.to_string();

        // 分离线程执行检查（线程会在发送结果后自动结束）
        std::thread::spawn(move || {
            let result = inner.snapshot(&pool_owned);
            let _ = tx.send(result);
        });

        // 等待结果或超时
        let result = match rx.recv_timeout(self.timeout) {
            Ok(snapshot) => snapshot,
            Err(_) => {
                // 超时：返回 Unhealthy
                HealthSnapshot {
                    status: HealthStatus::Unhealthy,
                    connection_count: 0,
                    slow_queries: 0,
                    message: format!(
                        "health check timed out after {:?} for pool '{}'",
                        self.timeout, pool
                    ),
                }
            }
        };

        // 更新统计
        let elapsed = start.elapsed();
        if let Ok(mut stats) = self.stats.lock() {
            stats.total_checks += 1;
            stats.total_duration += elapsed;
            if result.status == HealthStatus::Unhealthy && result.message.contains("timed out") {
                stats.timeouts += 1;
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    // ===================== 测试辅助类型 =====================

    /// 记录调用次数的 mock 检查器
    struct CountingChecker {
        call_count: AtomicU32,
        status: HealthStatus,
    }

    impl CountingChecker {
        fn new(status: HealthStatus) -> Self {
            Self {
                call_count: AtomicU32::new(0),
                status,
            }
        }

        fn calls(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    impl DbHealthChecker for CountingChecker {
        fn check(&self, pool: &str) -> HealthReport {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            HealthReport::new(pool).set_status(self.status.clone())
        }

        fn check_all(&self, pools: &[&str]) -> Vec<HealthReport> {
            pools.iter().map(|p| self.check(p)).collect()
        }
    }

    /// 模拟延迟的 provider
    struct SlowProvider {
        delay: Duration,
    }

    impl HealthStatusProvider for SlowProvider {
        fn snapshot(&self, _pool: &str) -> HealthSnapshot {
            std::thread::sleep(self.delay);
            HealthSnapshot::healthy()
        }
    }

    /// 快速返回的 provider
    struct FastProvider;

    impl HealthStatusProvider for FastProvider {
        fn snapshot(&self, _pool: &str) -> HealthSnapshot {
            HealthSnapshot::healthy()
        }
    }

    // ===================== HealthCheckCache 测试 =====================

    #[test]
    fn test_cache_first_call_misses() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cache = HealthCheckCache::new(checker.clone(), Duration::from_secs(60));

        let report = cache.check("pool-a");
        assert_eq!(report.status, HealthStatus::Healthy);
        assert_eq!(checker.calls(), 1);

        let (hits, misses, _) = cache.stats();
        assert_eq!(hits, 0);
        assert_eq!(misses, 1);
    }

    #[test]
    fn test_cache_second_call_within_ttl_hits() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cache = HealthCheckCache::new(checker.clone(), Duration::from_secs(60));

        cache.check("pool-a");
        cache.check("pool-a");

        // 第二次应命中缓存，不增加调用次数
        assert_eq!(checker.calls(), 1);
        let (hits, misses, _) = cache.stats();
        assert_eq!(hits, 1);
        assert_eq!(misses, 1);
    }

    #[test]
    fn test_cache_expired_triggers_recheck() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cache = HealthCheckCache::new(checker.clone(), Duration::from_millis(50));

        cache.check("pool-a");
        std::thread::sleep(Duration::from_millis(60));
        cache.check("pool-a");

        // TTL 过期后应重新检查
        assert_eq!(checker.calls(), 2);
        let (hits, misses, _) = cache.stats();
        assert_eq!(hits, 0);
        assert_eq!(misses, 2);
    }

    #[test]
    fn test_cache_invalidate_clears_entry() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cache = HealthCheckCache::new(checker.clone(), Duration::from_secs(60));

        cache.check("pool-a");
        assert!(cache.invalidate("pool-a"));
        cache.check("pool-a");

        // 失效后应重新检查
        assert_eq!(checker.calls(), 2);
        let (_, _, evictions) = cache.stats();
        assert_eq!(evictions, 1);
    }

    #[test]
    fn test_cache_invalidate_missing_returns_false() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cache = HealthCheckCache::new(checker.clone(), Duration::from_secs(60));
        assert!(!cache.invalidate("never-cached"));
    }

    #[test]
    fn test_cache_clear_empties_all() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cache = HealthCheckCache::new(checker.clone(), Duration::from_secs(60));

        cache.check("a");
        cache.check("b");
        cache.clear();
        cache.check("a");

        // 清空后应重新检查
        assert_eq!(checker.calls(), 3);
    }

    #[test]
    fn test_cache_hit_rate() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cache = HealthCheckCache::new(checker.clone(), Duration::from_secs(60));

        cache.check("p"); // miss
        cache.check("p"); // hit
        cache.check("p"); // hit

        let rate = cache.hit_rate();
        assert!((rate - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn test_cache_hit_rate_no_requests() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cache = HealthCheckCache::new(checker.clone(), Duration::from_secs(60));
        assert_eq!(cache.hit_rate(), 0.0);
    }

    #[test]
    fn test_cache_ttl_accessor() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cache = HealthCheckCache::new(checker, Duration::from_secs(30));
        assert_eq!(cache.ttl(), Duration::from_secs(30));
    }

    #[test]
    fn test_cache_check_all_uses_cache() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cache = HealthCheckCache::new(checker.clone(), Duration::from_secs(60));

        cache.check_all(&["a", "b"]);
        cache.check_all(&["a", "b"]);

        // 第二次全部命中缓存
        assert_eq!(checker.calls(), 2);
    }

    #[test]
    fn test_cache_different_pools_independent() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cache = HealthCheckCache::new(checker.clone(), Duration::from_secs(60));

        cache.check("a");
        cache.check("b");
        cache.check("a"); // hit
        cache.check("b"); // hit

        assert_eq!(checker.calls(), 2);
        let (hits, misses, _) = cache.stats();
        assert_eq!(hits, 2);
        assert_eq!(misses, 2);
    }

    // ===================== CascadingHealthChecker 测试 =====================

    #[test]
    fn test_cascading_no_deps_returns_own_report() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cascading = CascadingHealthChecker::new(checker);

        let result = cascading.check_with_deps("pool-a");
        assert_eq!(result.report.status, HealthStatus::Healthy);
        assert!(result.dependencies.is_empty());
        assert!(result.all_healthy);
    }

    #[test]
    fn test_cascading_with_healthy_deps() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cascading = CascadingHealthChecker::new(checker);

        cascading.add_dependency("app", "database");
        cascading.add_dependency("app", "cache");

        let result = cascading.check_with_deps("app");
        assert_eq!(result.report.status, HealthStatus::Healthy);
        assert_eq!(result.dependencies.len(), 2);
        assert!(result.all_healthy);
    }

    #[test]
    fn test_cascading_unhealthy_dependency_propagates() {
        struct MixedChecker;
        impl DbHealthChecker for MixedChecker {
            fn check(&self, pool: &str) -> HealthReport {
                if pool == "db-down" {
                    HealthReport::new(pool).set_status(HealthStatus::Unhealthy)
                } else {
                    HealthReport::new(pool).set_healthy()
                }
            }
            fn check_all(&self, pools: &[&str]) -> Vec<HealthReport> {
                pools.iter().map(|p| self.check(p)).collect()
            }
        }

        let checker = Arc::new(MixedChecker);
        let cascading = CascadingHealthChecker::new(checker);

        cascading.add_dependency("app", "db-down");
        let result = cascading.check_with_deps("app");
        assert!(!result.all_healthy);
        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(result.dependencies[0].status, HealthStatus::Unhealthy);
    }

    #[test]
    fn test_cascading_nested_deps() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cascading = CascadingHealthChecker::new(checker);

        cascading.add_dependency("app", "middleware");
        cascading.add_dependency("middleware", "database");

        let result = cascading.check_with_deps("app");
        assert_eq!(result.dependencies.len(), 2);
        assert!(result.all_healthy);
    }

    #[test]
    fn test_cascading_circular_dependency_no_infinite_loop() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cascading = CascadingHealthChecker::new(checker);

        // 创建循环依赖：A -> B -> A
        cascading.add_dependency("a", "b");
        cascading.add_dependency("b", "a");

        // 不应死循环
        let result = cascading.check_with_deps("a");
        assert_eq!(result.dependencies.len(), 1); // 只检查 b，b 的依赖 a 已访问
    }

    #[test]
    fn test_cascading_remove_dependency() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cascading = CascadingHealthChecker::new(checker);

        cascading.add_dependency("app", "db");
        cascading.add_dependency("app", "cache");
        assert_eq!(cascading.dependencies("app").len(), 2);

        assert!(cascading.remove_dependency("app", "db"));
        assert_eq!(cascading.dependencies("app").len(), 1);
        assert_eq!(cascading.dependencies("app")[0], "cache");
    }

    #[test]
    fn test_cascading_remove_missing_dependency_returns_false() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cascading = CascadingHealthChecker::new(checker);
        assert!(!cascading.remove_dependency("app", "never-added"));
    }

    #[test]
    fn test_cascading_clear_dependencies() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cascading = CascadingHealthChecker::new(checker);

        cascading.add_dependency("app", "db");
        cascading.add_dependency("app", "cache");
        cascading.clear_dependencies("app");

        assert!(cascading.dependencies("app").is_empty());
    }

    #[test]
    fn test_cascading_add_dependencies_batch() {
        let checker = Arc::new(CountingChecker::new(HealthStatus::Healthy));
        let cascading = CascadingHealthChecker::new(checker);

        cascading.add_dependencies(
            "app",
            vec!["db".to_string(), "cache".to_string(), "queue".to_string()],
        );
        assert_eq!(cascading.dependencies("app").len(), 3);
    }

    #[test]
    fn test_cascading_unknown_dependency_pool_returns_unknown() {
        struct UnknownChecker;
        impl DbHealthChecker for UnknownChecker {
            fn check(&self, pool: &str) -> HealthReport {
                HealthReport::new(pool) // status defaults to Unknown
            }
            fn check_all(&self, pools: &[&str]) -> Vec<HealthReport> {
                pools.iter().map(|p| self.check(p)).collect()
            }
        }

        let checker = Arc::new(UnknownChecker);
        let cascading = CascadingHealthChecker::new(checker);
        cascading.add_dependency("app", "unknown-dep");

        let result = cascading.check_with_deps("app");
        assert!(!result.all_healthy); // Unknown is not Healthy
    }

    // ===================== ProbeManager 测试 =====================

    #[test]
    fn test_probe_manager_default_is_unknown() {
        let mgr = ProbeManager::new();
        let result = mgr.check_liveness("svc");
        assert_eq!(result.status, HealthStatus::Unknown);
        assert_eq!(result.kind, ProbeKind::Liveness);
        assert!(!result.message.is_empty());
    }

    #[test]
    fn test_probe_manager_set_liveness_healthy() {
        let mgr = ProbeManager::new();
        mgr.set_liveness("svc", HealthSnapshot::healthy());

        let result = mgr.check_liveness("svc");
        assert_eq!(result.status, HealthStatus::Healthy);
        assert_eq!(result.kind, ProbeKind::Liveness);
    }

    #[test]
    fn test_probe_manager_set_readiness_unhealthy() {
        let mgr = ProbeManager::new();
        mgr.set_readiness("svc", HealthSnapshot::unhealthy("dependency down"));

        let result = mgr.check_readiness("svc");
        assert_eq!(result.status, HealthStatus::Unhealthy);
        assert_eq!(result.message, "dependency down");
        assert_eq!(result.kind, ProbeKind::Readiness);
    }

    #[test]
    fn test_probe_manager_liveness_and_readiness_independent() {
        let mgr = ProbeManager::new();

        // 进程存活但未就绪（正在启动）
        mgr.set_liveness("svc", HealthSnapshot::healthy());
        mgr.set_readiness("svc", HealthSnapshot::unhealthy("warming up"));

        assert_eq!(mgr.check_liveness("svc").status, HealthStatus::Healthy);
        assert_eq!(mgr.check_readiness("svc").status, HealthStatus::Unhealthy);
    }

    #[test]
    fn test_probe_manager_overall_liveness_all_healthy() {
        let mgr = ProbeManager::new();
        mgr.set_liveness("a", HealthSnapshot::healthy());
        mgr.set_liveness("b", HealthSnapshot::healthy());
        assert_eq!(mgr.overall_liveness(), HealthStatus::Healthy);
    }

    #[test]
    fn test_probe_manager_overall_readiness_one_unhealthy() {
        let mgr = ProbeManager::new();
        mgr.set_readiness("a", HealthSnapshot::healthy());
        mgr.set_readiness("b", HealthSnapshot::unhealthy("down"));
        assert_eq!(mgr.overall_readiness(), HealthStatus::Unhealthy);
    }

    #[test]
    fn test_probe_manager_overall_empty_returns_unknown() {
        let mgr = ProbeManager::new();
        assert_eq!(mgr.overall_liveness(), HealthStatus::Unknown);
        assert_eq!(mgr.overall_readiness(), HealthStatus::Unknown);
    }

    #[test]
    fn test_probe_manager_overall_one_unknown_no_unhealthy() {
        let mgr = ProbeManager::new();
        mgr.set_liveness("a", HealthSnapshot::healthy());
        mgr.set_liveness("b", HealthSnapshot::unknown());
        assert_eq!(mgr.overall_liveness(), HealthStatus::Unknown);
    }

    #[test]
    fn test_probe_manager_liveness_all_returns_all_probes() {
        let mgr = ProbeManager::new();
        mgr.set_liveness("a", HealthSnapshot::healthy());
        mgr.set_liveness("b", HealthSnapshot::healthy());

        let results = mgr.liveness_all();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.kind == ProbeKind::Liveness));
    }

    #[test]
    fn test_probe_manager_readiness_all_returns_all_probes() {
        let mgr = ProbeManager::new();
        mgr.set_readiness("x", HealthSnapshot::healthy());

        let results = mgr.readiness_all();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, ProbeKind::Readiness);
    }

    #[test]
    fn test_probe_result_serialization_roundtrip() {
        let result = ProbeResult {
            kind: ProbeKind::Readiness,
            status: HealthStatus::Unhealthy,
            message: "db down".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&result).expect("serialize");
        let back: ProbeResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.kind, ProbeKind::Readiness);
        assert_eq!(back.status, HealthStatus::Unhealthy);
        assert_eq!(back.message, "db down");
    }

    #[test]
    fn test_probe_kind_eq() {
        assert_eq!(ProbeKind::Liveness, ProbeKind::Liveness);
        assert_ne!(ProbeKind::Liveness, ProbeKind::Readiness);
    }

    // ===================== TimeoutHealthChecker 测试 =====================

    #[test]
    fn test_timeout_checker_fast_provider_succeeds() {
        let provider = Arc::new(FastProvider);
        let checker = TimeoutHealthChecker::new(provider, Duration::from_secs(1));

        let snap = checker.snapshot("pool");
        assert_eq!(snap.status, HealthStatus::Healthy);

        let stats = checker.stats();
        assert_eq!(stats.total_checks, 1);
        assert_eq!(stats.timeouts, 0);
        assert_eq!(stats.timeout_rate, 0.0);
    }

    #[test]
    fn test_timeout_checker_slow_provider_times_out() {
        let provider = Arc::new(SlowProvider {
            delay: Duration::from_millis(200),
        });
        let checker = TimeoutHealthChecker::new(provider, Duration::from_millis(50));

        let snap = checker.snapshot("pool");
        assert_eq!(snap.status, HealthStatus::Unhealthy);
        assert!(snap.message.contains("timed out"));

        let stats = checker.stats();
        assert_eq!(stats.total_checks, 1);
        assert_eq!(stats.timeouts, 1);
        assert!((stats.timeout_rate - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_timeout_checker_timeout_accessor() {
        let provider = Arc::new(FastProvider);
        let checker = TimeoutHealthChecker::new(provider, Duration::from_millis(500));
        assert_eq!(checker.timeout(), Duration::from_millis(500));
    }

    #[test]
    fn test_timeout_checker_stats_avg_duration() {
        let provider = Arc::new(FastProvider);
        let checker = TimeoutHealthChecker::new(provider, Duration::from_secs(1));

        checker.snapshot("a");
        checker.snapshot("b");

        let stats = checker.stats();
        assert_eq!(stats.total_checks, 2);
        assert!(stats.avg_duration < Duration::from_millis(100));
    }

    #[test]
    fn test_timeout_checker_stats_empty() {
        let provider = Arc::new(FastProvider);
        let checker = TimeoutHealthChecker::new(provider, Duration::from_secs(1));

        let stats = checker.stats();
        assert_eq!(stats.total_checks, 0);
        assert_eq!(stats.timeouts, 0);
        assert_eq!(stats.timeout_rate, 0.0);
        assert_eq!(stats.avg_duration, Duration::ZERO);
    }

    #[test]
    fn test_timeout_checker_implements_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TimeoutHealthChecker>();
        assert_send_sync::<HealthCheckCache>();
        assert_send_sync::<CascadingHealthChecker>();
        assert_send_sync::<ProbeManager>();
    }
}
