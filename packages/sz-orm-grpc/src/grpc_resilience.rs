//! gRPC 弹性能力：拦截器链、多维度超时配置、重试构建器、负载均衡、服务发现。
//!
//! 复用 [`crate::Interceptor`] / [`crate::GrpcError`] / [`crate::RetryPolicy`] 等既有类型，
//! 提供组合式 API 便于在生产中装配客户端/服务端弹性策略。
//!
//! ## 主要类型
//!
//! - [`InterceptorChain`] — 显式拦截器链，可在多个 channel 间共享
//! - [`TimeoutConfig`] / [`TimeoutPhase`] — 多维度超时配置
//! - [`RetryPolicyBuilder`] — 重试策略构建器
//! - [`LoadBalancer`] / [`LoadBalancingStrategy`] — 负载均衡器
//! - [`ServiceDiscovery`] / [`StaticDiscovery`] / [`CachedDiscovery`] — 服务发现

use crate::{GrpcError, Interceptor, InterceptorRequest, RetryableErrorKind};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

// =========================================================================
// InterceptorChain — 显式拦截器链
// =========================================================================

/// 显式拦截器链，按添加顺序执行。
///
/// 与 [`crate::GrpcChannel::with_interceptor`] 互补：
/// `with_interceptor` 是 builder 风格、链与 channel 绑定；
/// `InterceptorChain` 是独立类型，可在多个 channel 间共享、动态组合。
///
/// 任一拦截器失败立即短路，后续拦截器不再执行。
#[derive(Default)]
pub struct InterceptorChain {
    interceptors: Vec<Arc<dyn Interceptor>>,
}

impl InterceptorChain {
    /// 创建空链。
    pub fn new() -> Self {
        Self::default()
    }

    /// 从已有拦截器列表构造。
    pub fn from_vec(interceptors: Vec<Arc<dyn Interceptor>>) -> Self {
        Self { interceptors }
    }

    /// 追加一个拦截器到链末尾。
    pub fn push(mut self, interceptor: Arc<dyn Interceptor>) -> Self {
        self.interceptors.push(interceptor);
        self
    }

    /// 当前链长度。
    pub fn len(&self) -> usize {
        self.interceptors.len()
    }

    /// 链是否为空。
    pub fn is_empty(&self) -> bool {
        self.interceptors.is_empty()
    }

    /// 按顺序执行全部拦截器。任一失败立即返回错误。
    pub fn execute(&self, request: &InterceptorRequest) -> Result<(), GrpcError> {
        for interceptor in &self.interceptors {
            interceptor.call(request)?;
        }
        Ok(())
    }

    /// 返回内部拦截器切片（用于检查/调试）。
    pub fn interceptors(&self) -> &[Arc<dyn Interceptor>] {
        &self.interceptors
    }
}

impl std::fmt::Debug for InterceptorChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InterceptorChain")
            .field("len", &self.interceptors.len())
            .finish()
    }
}

// =========================================================================
// TimeoutConfig — 多维度超时配置
// =========================================================================

/// 超时阶段标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutPhase {
    /// 连接建立阶段。
    Connect,
    /// 读取阶段。
    Read,
    /// 写入阶段。
    Write,
    /// 整体 RPC 阶段。
    Rpc,
}

/// 多维度超时配置：连接、读取、写入、整体 RPC 分别设上限。
///
/// 与 [`crate::TimeoutPolicy`]（单一 deadline）互补：
/// `TimeoutPolicy` 适用于"整体 RPC 不得超过 X"；
/// `TimeoutConfig` 适用于"各阶段分别有不同上限"。
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    /// 连接建立超时（默认 5s）。
    pub connect_timeout: Duration,
    /// 单次读取超时（默认 10s）。
    pub read_timeout: Duration,
    /// 单次写入超时（默认 10s）。
    pub write_timeout: Duration,
    /// 整体 RPC deadline（默认 30s）。
    pub rpc_deadline: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(10),
            write_timeout: Duration::from_secs(10),
            rpc_deadline: Duration::from_secs(30),
        }
    }
}

impl TimeoutConfig {
    /// 创建默认配置。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置连接超时。
    pub fn with_connect_timeout(mut self, d: Duration) -> Self {
        self.connect_timeout = d;
        self
    }

    /// 设置读取超时。
    pub fn with_read_timeout(mut self, d: Duration) -> Self {
        self.read_timeout = d;
        self
    }

    /// 设置写入超时。
    pub fn with_write_timeout(mut self, d: Duration) -> Self {
        self.write_timeout = d;
        self
    }

    /// 设置整体 RPC deadline。
    pub fn with_rpc_deadline(mut self, d: Duration) -> Self {
        self.rpc_deadline = d;
        self
    }

    /// 检查给定阶段耗时是否超时。
    pub fn check(&self, phase: TimeoutPhase, elapsed: Duration) -> Result<(), GrpcError> {
        let limit = match phase {
            TimeoutPhase::Connect => self.connect_timeout,
            TimeoutPhase::Read => self.read_timeout,
            TimeoutPhase::Write => self.write_timeout,
            TimeoutPhase::Rpc => self.rpc_deadline,
        };
        if elapsed > limit {
            return Err(GrpcError::Timeout(format!(
                "{:?} phase elapsed {:?} exceeds limit {:?}",
                phase, elapsed, limit
            )));
        }
        Ok(())
    }

    /// 最严格的 deadline（四个中的最小值），用于整体预算。
    pub fn effective_deadline(&self) -> Duration {
        self.connect_timeout
            .min(self.read_timeout)
            .min(self.write_timeout)
            .min(self.rpc_deadline)
    }
}

// =========================================================================
// RetryPolicyBuilder — 重试策略构建器
// =========================================================================

/// [`crate::RetryPolicy`] 的构建器，提供链式 API。
pub struct RetryPolicyBuilder {
    max_retries: u32,
    initial_delay_ms: u64,
    max_delay_ms: u64,
    multiplier: f64,
    retryable_errors: Vec<RetryableErrorKind>,
}

impl Default for RetryPolicyBuilder {
    fn default() -> Self {
        let p = crate::RetryPolicy::default();
        Self {
            max_retries: p.max_retries,
            initial_delay_ms: p.initial_delay_ms,
            max_delay_ms: p.max_delay_ms,
            multiplier: p.multiplier,
            retryable_errors: p.retryable_errors,
        }
    }
}

impl RetryPolicyBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn max_retries(mut self, n: u32) -> Self {
        self.max_retries = n;
        self
    }

    pub fn initial_delay(mut self, d: Duration) -> Self {
        self.initial_delay_ms = d.as_millis() as u64;
        self
    }

    pub fn max_delay(mut self, d: Duration) -> Self {
        self.max_delay_ms = d.as_millis() as u64;
        self
    }

    pub fn multiplier(mut self, m: f64) -> Self {
        self.multiplier = m;
        self
    }

    pub fn retryable(mut self, kinds: Vec<RetryableErrorKind>) -> Self {
        self.retryable_errors = kinds;
        self
    }

    pub fn build(self) -> crate::RetryPolicy {
        crate::RetryPolicy {
            max_retries: self.max_retries,
            initial_delay_ms: self.initial_delay_ms,
            max_delay_ms: self.max_delay_ms,
            multiplier: self.multiplier,
            retryable_errors: self.retryable_errors,
        }
    }
}

// =========================================================================
// LoadBalancingStrategy / LoadBalancer — 负载均衡
// =========================================================================

/// 负载均衡策略枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadBalancingStrategy {
    /// 轮询（Round-Robin）。
    RoundRobin,
    /// 随机（确定性 LCG 伪随机，避免引入 rand 依赖）。
    Random,
    /// 最少连接数（需配合 [`LoadBalancer::on_connect`] / [`LoadBalancer::on_disconnect`]）。
    LeastConnections,
    /// 一致性哈希（按 key 路由到固定节点，便于粘性会话）。
    ConsistentHash,
}

impl LoadBalancingStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RoundRobin => "round-robin",
            Self::Random => "random",
            Self::LeastConnections => "least-connections",
            Self::ConsistentHash => "consistent-hash",
        }
    }
}

/// 负载均衡器：按策略从地址列表中选择一个节点。
///
/// `Random` 策略使用 `AtomicUsize` + 线性同余生成器（LCG）实现确定性伪随机，
/// 避免引入 `rand` crate 依赖。
pub struct LoadBalancer {
    strategy: LoadBalancingStrategy,
    /// 轮询/随机游标。
    cursor: AtomicUsize,
    /// 各节点活跃连接数（仅 LeastConnections 使用）。
    connections: Mutex<HashMap<String, u64>>,
    /// 节点列表。
    endpoints: Mutex<Vec<String>>,
}

impl LoadBalancer {
    /// 创建负载均衡器。
    pub fn new(strategy: LoadBalancingStrategy, endpoints: Vec<String>) -> Self {
        Self {
            strategy,
            cursor: AtomicUsize::new(0),
            connections: Mutex::new(HashMap::new()),
            endpoints: Mutex::new(endpoints),
        }
    }

    /// 返回当前策略。
    pub fn strategy(&self) -> LoadBalancingStrategy {
        self.strategy
    }

    /// 返回节点列表快照。
    pub fn endpoints(&self) -> Vec<String> {
        self.endpoints.lock().clone()
    }

    /// 更新节点列表（热更新）。
    pub fn update_endpoints(&self, new_endpoints: Vec<String>) {
        *self.endpoints.lock() = new_endpoints;
    }

    /// 选择一个节点。无节点时返回 `None`。
    pub fn select(&self) -> Option<String> {
        let endpoints = self.endpoints.lock();
        if endpoints.is_empty() {
            return None;
        }
        match self.strategy {
            LoadBalancingStrategy::RoundRobin => {
                let idx = self.cursor.fetch_add(1, Ordering::Relaxed) % endpoints.len();
                Some(endpoints[idx].clone())
            }
            LoadBalancingStrategy::Random => {
                // LCG（Numerical Recipes 常数），避免引入 rand 依赖。
                let prev = self.cursor.load(Ordering::Relaxed);
                let next = prev.wrapping_mul(1664525).wrapping_add(1013904223);
                self.cursor.store(next, Ordering::Relaxed);
                let idx = next % endpoints.len();
                Some(endpoints[idx].clone())
            }
            LoadBalancingStrategy::LeastConnections => {
                let conns = self.connections.lock();
                let mut best_idx = 0usize;
                let mut best_count = u64::MAX;
                for (i, ep) in endpoints.iter().enumerate() {
                    let count = *conns.get(ep).unwrap_or(&0);
                    if count < best_count {
                        best_count = count;
                        best_idx = i;
                    }
                }
                Some(endpoints[best_idx].clone())
            }
            LoadBalancingStrategy::ConsistentHash => {
                // 无 key 时退化为轮询，保证可用性。
                let idx = self.cursor.fetch_add(1, Ordering::Relaxed) % endpoints.len();
                Some(endpoints[idx].clone())
            }
        }
    }

    /// 一致性哈希选择：按 key 路由到固定节点。
    pub fn select_by_key(&self, key: &str) -> Option<String> {
        let endpoints = self.endpoints.lock();
        if endpoints.is_empty() {
            return None;
        }
        let idx = (simple_hash(key) as usize) % endpoints.len();
        Some(endpoints[idx].clone())
    }

    /// 记录连接建立（LeastConnections 计数 +1）。
    pub fn on_connect(&self, endpoint: &str) {
        if self.strategy == LoadBalancingStrategy::LeastConnections {
            *self
                .connections
                .lock()
                .entry(endpoint.to_string())
                .or_insert(0) += 1;
        }
    }

    /// 记录连接释放（LeastConnections 计数 -1，下限 0）。
    pub fn on_disconnect(&self, endpoint: &str) {
        if self.strategy == LoadBalancingStrategy::LeastConnections {
            if let Some(count) = self.connections.lock().get_mut(endpoint) {
                *count = count.saturating_sub(1);
            }
        }
    }

    /// 当前某节点的活跃连接数。
    pub fn connection_count(&self, endpoint: &str) -> u64 {
        *self.connections.lock().get(endpoint).unwrap_or(&0)
    }
}

/// FNV-1a 32bit 字符串哈希，避免引入 `std::hash::Hasher` 复杂度。
fn simple_hash(s: &str) -> u32 {
    let mut hash: u32 = 2166136261;
    for byte in s.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

// =========================================================================
// ServiceDiscovery — 服务发现 trait + 实现
// =========================================================================

/// 服务发现 trait：按服务名解析出可用节点列表。
pub trait ServiceDiscovery: Send + Sync {
    /// 解析服务名，返回节点地址列表（可能为空）。
    fn resolve(&self, service_name: &str) -> Vec<String>;

    /// 服务是否存在（至少一个节点）。
    fn exists(&self, service_name: &str) -> bool {
        !self.resolve(service_name).is_empty()
    }
}

/// 静态服务发现：基于预置映射表。
///
/// 适用于测试、固定拓扑、配置文件加载场景。
#[derive(Default)]
pub struct StaticDiscovery {
    services: Mutex<HashMap<String, Vec<String>>>,
}

impl StaticDiscovery {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个服务的节点列表（覆盖）。
    pub fn register(&self, service_name: impl Into<String>, endpoints: Vec<String>) {
        self.services.lock().insert(service_name.into(), endpoints);
    }

    /// 追加一个节点到已有服务。
    pub fn add_endpoint(&self, service_name: &str, endpoint: impl Into<String>) {
        self.services
            .lock()
            .entry(service_name.to_string())
            .or_default()
            .push(endpoint.into());
    }

    /// 移除一个节点。返回是否实际移除。
    pub fn remove_endpoint(&self, service_name: &str, endpoint: &str) -> bool {
        let mut services = self.services.lock();
        if let Some(list) = services.get_mut(service_name) {
            let before = list.len();
            list.retain(|e| e != endpoint);
            return list.len() < before;
        }
        false
    }

    /// 已注册服务数量。
    pub fn service_count(&self) -> usize {
        self.services.lock().len()
    }

    /// 某服务的节点数量。
    pub fn endpoint_count(&self, service_name: &str) -> usize {
        self.services
            .lock()
            .get(service_name)
            .map(|v| v.len())
            .unwrap_or(0)
    }
}

impl ServiceDiscovery for StaticDiscovery {
    fn resolve(&self, service_name: &str) -> Vec<String> {
        self.services
            .lock()
            .get(service_name)
            .cloned()
            .unwrap_or_default()
    }
}

/// 带缓存的装饰器：包装另一个 [`ServiceDiscovery`]，缓存解析结果。
///
/// 适用于下游解析昂贵（DNS、Consul、etcd）的场景。
/// 缓存按"代际"失效：调用 [`CachedDiscovery::invalidate`] 清空全部缓存。
pub struct CachedDiscovery<D: ServiceDiscovery> {
    inner: D,
    cache: Mutex<HashMap<String, Vec<String>>>,
    generation: AtomicUsize,
}

impl<D: ServiceDiscovery> CachedDiscovery<D> {
    pub fn new(inner: D) -> Self {
        Self {
            inner,
            cache: Mutex::new(HashMap::new()),
            generation: AtomicUsize::new(0),
        }
    }

    /// 当前缓存代数。
    pub fn generation(&self) -> usize {
        self.generation.load(Ordering::Relaxed)
    }

    /// 失效全部缓存（代数 +1）。
    pub fn invalidate(&self) {
        self.cache.lock().clear();
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    /// 缓存中服务数量。
    pub fn cached_count(&self) -> usize {
        self.cache.lock().len()
    }
}

impl<D: ServiceDiscovery> ServiceDiscovery for CachedDiscovery<D> {
    fn resolve(&self, service_name: &str) -> Vec<String> {
        if let Some(hit) = self.cache.lock().get(service_name).cloned() {
            return hit;
        }
        let fresh = self.inner.resolve(service_name);
        self.cache
            .lock()
            .insert(service_name.to_string(), fresh.clone());
        fresh
    }
}

// =========================================================================
// 测试
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuthInterceptor, LoggingInterceptor};
    use std::collections::HashMap;

    fn req() -> InterceptorRequest {
        InterceptorRequest {
            method: "GetUser".to_string(),
            service_name: "UserService".to_string(),
            metadata: HashMap::new(),
        }
    }

    // ---- InterceptorChain ----

    #[test]
    fn chain_empty_executes_ok() {
        let chain = InterceptorChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
        assert!(chain.execute(&req()).is_ok());
    }

    #[test]
    fn chain_push_increments_len() {
        let chain = InterceptorChain::new()
            .push(Arc::new(LoggingInterceptor))
            .push(Arc::new(LoggingInterceptor));
        assert_eq!(chain.len(), 2);
        assert!(!chain.is_empty());
    }

    #[test]
    fn chain_executes_all_interceptors() {
        let chain = InterceptorChain::new()
            .push(Arc::new(LoggingInterceptor))
            .push(Arc::new(AuthInterceptor::new("Bearer x")));
        assert!(matches!(
            chain.execute(&req()),
            Err(GrpcError::Unauthorized(_))
        ));
    }

    #[test]
    fn chain_short_circuits_on_failure() {
        use parking_lot::Mutex;
        struct CountInterceptor {
            count: Arc<Mutex<u32>>,
        }
        impl Interceptor for CountInterceptor {
            fn call(&self, _: &InterceptorRequest) -> Result<(), GrpcError> {
                *self.count.lock() += 1;
                Ok(())
            }
        }
        let count = Arc::new(Mutex::new(0));
        let chain = InterceptorChain::new()
            .push(Arc::new(AuthInterceptor::new("Bearer x")))
            .push(Arc::new(CountInterceptor {
                count: count.clone(),
            }));
        assert!(chain.execute(&req()).is_err());
        assert_eq!(*count.lock(), 0, "后续拦截器不应执行");
    }

    #[test]
    fn chain_from_vec_preserves_order() {
        let interceptors: Vec<Arc<dyn Interceptor>> =
            vec![Arc::new(LoggingInterceptor), Arc::new(LoggingInterceptor)];
        let chain = InterceptorChain::from_vec(interceptors);
        assert_eq!(chain.len(), 2);
        assert!(chain.execute(&req()).is_ok());
    }

    #[test]
    fn chain_debug_shows_len() {
        let chain = InterceptorChain::new().push(Arc::new(LoggingInterceptor));
        let debug = format!("{:?}", chain);
        assert!(debug.contains("InterceptorChain"));
        assert!(debug.contains("len"));
    }

    #[test]
    fn chain_interceptors_slice_access() {
        let chain = InterceptorChain::new().push(Arc::new(LoggingInterceptor));
        assert_eq!(chain.interceptors().len(), 1);
    }

    // ---- TimeoutConfig ----

    #[test]
    fn timeout_config_default_values() {
        let cfg = TimeoutConfig::default();
        assert_eq!(cfg.connect_timeout, Duration::from_secs(5));
        assert_eq!(cfg.read_timeout, Duration::from_secs(10));
        assert_eq!(cfg.write_timeout, Duration::from_secs(10));
        assert_eq!(cfg.rpc_deadline, Duration::from_secs(30));
    }

    #[test]
    fn timeout_config_builder() {
        let cfg = TimeoutConfig::new()
            .with_connect_timeout(Duration::from_millis(100))
            .with_read_timeout(Duration::from_millis(200))
            .with_write_timeout(Duration::from_millis(300))
            .with_rpc_deadline(Duration::from_millis(400));
        assert_eq!(cfg.connect_timeout, Duration::from_millis(100));
        assert_eq!(cfg.read_timeout, Duration::from_millis(200));
        assert_eq!(cfg.write_timeout, Duration::from_millis(300));
        assert_eq!(cfg.rpc_deadline, Duration::from_millis(400));
    }

    #[test]
    fn timeout_config_check_passes() {
        let cfg = TimeoutConfig::default();
        assert!(cfg
            .check(TimeoutPhase::Connect, Duration::from_secs(1))
            .is_ok());
        assert!(cfg
            .check(TimeoutPhase::Read, Duration::from_secs(5))
            .is_ok());
        assert!(cfg
            .check(TimeoutPhase::Write, Duration::from_secs(5))
            .is_ok());
        assert!(cfg
            .check(TimeoutPhase::Rpc, Duration::from_secs(20))
            .is_ok());
    }

    #[test]
    fn timeout_config_check_fails() {
        let cfg = TimeoutConfig::default();
        assert!(matches!(
            cfg.check(TimeoutPhase::Connect, Duration::from_secs(10)),
            Err(GrpcError::Timeout(_))
        ));
        assert!(matches!(
            cfg.check(TimeoutPhase::Read, Duration::from_secs(20)),
            Err(GrpcError::Timeout(_))
        ));
    }

    #[test]
    fn timeout_config_effective_deadline_is_min() {
        let cfg = TimeoutConfig::new()
            .with_connect_timeout(Duration::from_secs(5))
            .with_read_timeout(Duration::from_secs(3))
            .with_write_timeout(Duration::from_secs(7))
            .with_rpc_deadline(Duration::from_secs(10));
        assert_eq!(cfg.effective_deadline(), Duration::from_secs(3));
    }

    #[test]
    fn timeout_phase_equality() {
        assert_eq!(TimeoutPhase::Connect, TimeoutPhase::Connect);
        assert_ne!(TimeoutPhase::Connect, TimeoutPhase::Read);
    }

    // ---- RetryPolicyBuilder ----

    #[test]
    fn retry_builder_default_matches_policy_default() {
        let built = RetryPolicyBuilder::new().build();
        let default = crate::RetryPolicy::default();
        assert_eq!(built.max_retries, default.max_retries);
        assert_eq!(built.initial_delay_ms, default.initial_delay_ms);
        assert_eq!(built.max_delay_ms, default.max_delay_ms);
        assert_eq!(built.multiplier, default.multiplier);
        assert_eq!(built.retryable_errors, default.retryable_errors);
    }

    #[test]
    fn retry_builder_custom() {
        let policy = RetryPolicyBuilder::new()
            .max_retries(5)
            .initial_delay(Duration::from_millis(100))
            .max_delay(Duration::from_secs(10))
            .multiplier(3.15)
            .retryable(vec![RetryableErrorKind::Transport])
            .build();
        assert_eq!(policy.max_retries, 5);
        assert_eq!(policy.initial_delay_ms, 100);
        assert_eq!(policy.max_delay_ms, 10_000);
        assert_eq!(policy.multiplier, 3.15);
        assert_eq!(policy.retryable_errors, vec![RetryableErrorKind::Transport]);
    }

    // ---- LoadBalancingStrategy ----

    #[test]
    fn lb_strategy_as_str() {
        assert_eq!(LoadBalancingStrategy::RoundRobin.as_str(), "round-robin");
        assert_eq!(LoadBalancingStrategy::Random.as_str(), "random");
        assert_eq!(
            LoadBalancingStrategy::LeastConnections.as_str(),
            "least-connections"
        );
        assert_eq!(
            LoadBalancingStrategy::ConsistentHash.as_str(),
            "consistent-hash"
        );
    }

    #[test]
    fn lb_round_robin_cycles() {
        let lb = LoadBalancer::new(
            LoadBalancingStrategy::RoundRobin,
            vec!["a".into(), "b".into(), "c".into()],
        );
        assert_eq!(lb.select().unwrap(), "a");
        assert_eq!(lb.select().unwrap(), "b");
        assert_eq!(lb.select().unwrap(), "c");
        assert_eq!(lb.select().unwrap(), "a");
    }

    #[test]
    fn lb_empty_returns_none() {
        let lb = LoadBalancer::new(LoadBalancingStrategy::RoundRobin, vec![]);
        assert!(lb.select().is_none());
    }

    #[test]
    fn lb_random_returns_valid_endpoint() {
        let endpoints = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let lb = LoadBalancer::new(LoadBalancingStrategy::Random, endpoints.clone());
        for _ in 0..20 {
            let chosen = lb.select().unwrap();
            assert!(endpoints.contains(&chosen));
        }
    }

    #[test]
    fn lb_least_connections_picks_least() {
        let lb = LoadBalancer::new(
            LoadBalancingStrategy::LeastConnections,
            vec!["a".into(), "b".into(), "c".into()],
        );
        lb.on_connect("a");
        lb.on_connect("a");
        lb.on_connect("b");
        assert_eq!(lb.select().unwrap(), "c");
        assert_eq!(lb.connection_count("a"), 2);
        assert_eq!(lb.connection_count("b"), 1);
        assert_eq!(lb.connection_count("c"), 0);
    }

    #[test]
    fn lb_on_disconnect_decrements() {
        let lb = LoadBalancer::new(LoadBalancingStrategy::LeastConnections, vec!["a".into()]);
        lb.on_connect("a");
        lb.on_connect("a");
        lb.on_disconnect("a");
        assert_eq!(lb.connection_count("a"), 1);
    }

    #[test]
    fn lb_disconnect_below_zero_clamped() {
        let lb = LoadBalancer::new(LoadBalancingStrategy::LeastConnections, vec!["a".into()]);
        lb.on_disconnect("a");
        assert_eq!(lb.connection_count("a"), 0);
    }

    #[test]
    fn lb_consistent_hash_key_routes_stably() {
        let endpoints = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let lb = LoadBalancer::new(LoadBalancingStrategy::ConsistentHash, endpoints);
        let first = lb.select_by_key("user:42").unwrap();
        let second = lb.select_by_key("user:42").unwrap();
        assert_eq!(first, second, "相同 key 应路由到相同节点");
    }

    #[test]
    fn lb_consistent_hash_empty_returns_none() {
        let lb = LoadBalancer::new(LoadBalancingStrategy::ConsistentHash, vec![]);
        assert!(lb.select_by_key("k").is_none());
    }

    #[test]
    fn lb_consistent_hash_no_key_degrades_to_round_robin() {
        let lb = LoadBalancer::new(
            LoadBalancingStrategy::ConsistentHash,
            vec!["a".into(), "b".into()],
        );
        let first = lb.select().unwrap();
        let second = lb.select().unwrap();
        assert_ne!(first, second, "无 key 时应轮询");
    }

    #[test]
    fn lb_update_endpoints_hot() {
        let lb = LoadBalancer::new(LoadBalancingStrategy::RoundRobin, vec!["a".into()]);
        assert_eq!(lb.endpoints().len(), 1);
        lb.update_endpoints(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(lb.endpoints().len(), 3);
    }

    #[test]
    fn lb_strategy_getter() {
        let lb = LoadBalancer::new(LoadBalancingStrategy::Random, vec!["a".into()]);
        assert_eq!(lb.strategy(), LoadBalancingStrategy::Random);
    }

    #[test]
    fn simple_hash_deterministic() {
        assert_eq!(simple_hash("abc"), simple_hash("abc"));
        assert_ne!(simple_hash("abc"), simple_hash("abcd"));
    }

    // ---- ServiceDiscovery ----

    #[test]
    fn static_discovery_empty() {
        let d = StaticDiscovery::new();
        assert_eq!(d.service_count(), 0);
        assert!(d.resolve("any").is_empty());
        assert!(!d.exists("any"));
    }

    #[test]
    fn static_discovery_register_and_resolve() {
        let d = StaticDiscovery::new();
        d.register("svc", vec!["a:80".into(), "b:80".into()]);
        assert_eq!(d.service_count(), 1);
        assert_eq!(d.endpoint_count("svc"), 2);
        assert!(d.exists("svc"));
        assert_eq!(
            d.resolve("svc"),
            vec!["a:80".to_string(), "b:80".to_string()]
        );
    }

    #[test]
    fn static_discovery_add_and_remove_endpoint() {
        let d = StaticDiscovery::new();
        d.register("svc", vec!["a:80".into()]);
        d.add_endpoint("svc", "b:80");
        assert_eq!(d.endpoint_count("svc"), 2);
        assert!(d.remove_endpoint("svc", "a:80"));
        assert_eq!(d.endpoint_count("svc"), 1);
        assert!(!d.remove_endpoint("svc", "nonexistent"));
    }

    #[test]
    fn static_discovery_add_to_new_service() {
        let d = StaticDiscovery::new();
        d.add_endpoint("new", "x:80");
        assert_eq!(d.endpoint_count("new"), 1);
    }

    #[test]
    fn cached_discovery_caches() {
        let inner = StaticDiscovery::new();
        inner.register("svc", vec!["a:80".into()]);
        let cached = CachedDiscovery::new(inner);
        assert_eq!(cached.cached_count(), 0);
        let _ = cached.resolve("svc");
        assert_eq!(cached.cached_count(), 1);
        let _ = cached.resolve("svc");
        assert_eq!(cached.cached_count(), 1);
    }

    #[test]
    fn cached_discovery_invalidate() {
        let inner = StaticDiscovery::new();
        inner.register("svc", vec!["a:80".into()]);
        let cached = CachedDiscovery::new(inner);
        assert_eq!(cached.generation(), 0);
        let _ = cached.resolve("svc");
        cached.invalidate();
        assert_eq!(cached.generation(), 1);
        assert_eq!(cached.cached_count(), 0);
    }

    #[test]
    fn cached_discovery_exists() {
        let inner = StaticDiscovery::new();
        inner.register("svc", vec!["a:80".into()]);
        let cached = CachedDiscovery::new(inner);
        assert!(cached.exists("svc"));
        assert!(!cached.exists("missing"));
    }

    #[test]
    fn cached_discovery_empty_inner_caches_empty() {
        let inner = StaticDiscovery::new();
        let cached = CachedDiscovery::new(inner);
        assert!(cached.resolve("any").is_empty());
        assert_eq!(cached.cached_count(), 1);
    }
}
