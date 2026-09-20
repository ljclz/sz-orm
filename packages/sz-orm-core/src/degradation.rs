//! v7.5.0 降级路径执行器（feature gate: `circuit-breaker`，默认关闭）
//!
//! 当 `CircuitBreaker::can_execute() == false` 时调用 `DegradationHandler::handle()`，
//! 提供缓存降级 / 默认值降级 / 快速失败三种策略。

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DegradationStrategy {
    ReturnCache,
    ReturnDefault,
    FastFail,
    /// v7.7.0: 缓存降级（返回缓存值，未命中则返回空集合）
    Cache,
    /// v7.7.0: 默认值降级（返回空集合）
    DefaultValue,
    /// v7.7.0: 简化结果降级（返回精简结构，剥离非必要字段）
    SimplifiedResult,
}

impl DegradationStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            DegradationStrategy::ReturnCache => "return_cache",
            DegradationStrategy::ReturnDefault => "return_default",
            DegradationStrategy::FastFail => "fast_fail",
            DegradationStrategy::Cache => "cache",
            DegradationStrategy::DefaultValue => "default_value",
            DegradationStrategy::SimplifiedResult => "simplified_result",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DegradationResult {
    pub data: Option<Vec<HashMap<String, String>>>,
    pub is_degraded: bool,
    pub degradation_strategy: DegradationStrategy,
    pub mismatch_warning: bool,
}

impl DegradationResult {
    pub fn from_cache(data: Vec<HashMap<String, String>>) -> Self {
        Self {
            data: Some(data),
            is_degraded: true,
            degradation_strategy: DegradationStrategy::ReturnCache,
            mismatch_warning: false,
        }
    }

    pub fn from_default() -> Self {
        Self {
            data: Some(Vec::new()),
            is_degraded: true,
            degradation_strategy: DegradationStrategy::ReturnDefault,
            mismatch_warning: false,
        }
    }

    pub fn fast_fail() -> Self {
        Self {
            data: None,
            is_degraded: true,
            degradation_strategy: DegradationStrategy::FastFail,
            mismatch_warning: false,
        }
    }

    pub fn with_mismatch_warning(mut self) -> Self {
        self.mismatch_warning = true;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DegradationError {
    #[error("cache miss for query fingerprint: {0}")]
    CacheMiss(String),
    #[error("fast fail: circuit breaker open")]
    FastFail,
    #[error("degradation data mismatch for query fingerprint: {0}")]
    DataMismatch(String),
}

pub trait DegradationHandler: Send + Sync {
    fn handle(&self, query_fingerprint: &str) -> Result<DegradationResult, DegradationError>;
    fn strategy(&self) -> DegradationStrategy;
}

pub struct CacheDegradation {
    cache: HashMap<String, Vec<HashMap<String, String>>>,
}

impl CacheDegradation {
    /// 创建空的缓存降级处理器。
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    pub fn with_cache(cache: HashMap<String, Vec<HashMap<String, String>>>) -> Self {
        Self { cache }
    }

    pub fn insert(&mut self, fingerprint: String, data: Vec<HashMap<String, String>>) {
        self.cache.insert(fingerprint, data);
    }
}

impl Default for CacheDegradation {
    fn default() -> Self {
        Self::new()
    }
}

impl DegradationHandler for CacheDegradation {
    fn handle(&self, query_fingerprint: &str) -> Result<DegradationResult, DegradationError> {
        match self.cache.get(query_fingerprint) {
            Some(data) => Ok(DegradationResult::from_cache(data.clone())),
            None => Err(DegradationError::CacheMiss(query_fingerprint.to_string())),
        }
    }

    fn strategy(&self) -> DegradationStrategy {
        DegradationStrategy::ReturnCache
    }
}

pub struct DefaultDegradation;

impl DefaultDegradation {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultDegradation {
    fn default() -> Self {
        Self
    }
}

impl DegradationHandler for DefaultDegradation {
    fn handle(&self, _query_fingerprint: &str) -> Result<DegradationResult, DegradationError> {
        Ok(DegradationResult::from_default())
    }

    fn strategy(&self) -> DegradationStrategy {
        DegradationStrategy::ReturnDefault
    }
}

pub struct FastFailDegradation;

impl FastFailDegradation {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FastFailDegradation {
    fn default() -> Self {
        Self
    }
}

impl DegradationHandler for FastFailDegradation {
    fn handle(&self, _query_fingerprint: &str) -> Result<DegradationResult, DegradationError> {
        Err(DegradationError::FastFail)
    }

    fn strategy(&self) -> DegradationStrategy {
        DegradationStrategy::FastFail
    }
}

/// 在断路器拦截时执行降级路径，正常时执行原始查询。
pub fn execute_with_degradation(
    can_execute: bool,
    handler: &dyn DegradationHandler,
    query_fingerprint: &str,
    normal_query: impl FnOnce() -> Result<Vec<HashMap<String, String>>, String>,
) -> Result<DegradationResult, DegradationError> {
    if can_execute {
        match normal_query() {
            Ok(data) => Ok(DegradationResult {
                data: Some(data),
                is_degraded: false,
                degradation_strategy: handler.strategy(),
                mismatch_warning: false,
            }),
            Err(_) => handler.handle(query_fingerprint),
        }
    } else {
        handler.handle(query_fingerprint)
    }
}
// ============================================================================
// v7.7.0 核心链路保护与降级策略
// ============================================================================

use crate::circuit_breaker::{CircuitBreaker, CircuitState, DefaultCircuitBreaker};
use std::sync::Mutex;
use std::time::Duration;

/// v7.7.0 核心链路降级错误
///
/// 仅在核心链路请求被误传入 [`CoreLinkProtector::degrade_non_core`] 时产生；
/// 非核心请求的降级路径始终返回合理值，不返回错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DegradeError {
    /// 核心链路请求不可降级
    #[error("core link request cannot be degraded: {0}")]
    CoreLinkNotDegradable(String),
}

/// v7.7.0 核心链路保护器
///
/// 识别核心请求（支付/订单/事务等）不被降级，对非核心请求执行降级策略。
/// 内部复用 [`DefaultCircuitBreaker`] 驱动状态机，支持半开探测恢复。
///
/// # 核心链路识别
/// `request` 包含任一核心关键字即视为核心链路：`payment`、`order`、`transaction`、
/// `checkout`、`auth`、`login`。可通过 [`CoreLinkProtector::with_core_links`] 自定义。
///
/// # 降级策略
/// - [`DegradationStrategy::Cache`]：返回缓存值，未命中则返回空集合
/// - [`DegradationStrategy::DefaultValue`]：返回空集合
/// - [`DegradationStrategy::SimplifiedResult`]：返回精简结构
///
/// 非核心请求的降级路径禁止返回错误（任务约束：降级返回须合理）。
pub struct CoreLinkProtector {
    /// 核心链路关键字（request 包含任一关键字即视为核心链路）
    core_link_patterns: Vec<String>,
    /// 断路器（保护非核心链路的后端调用，驱动半开探测恢复）
    breaker: Mutex<DefaultCircuitBreaker>,
    /// 降级缓存（request -> 降级值）
    cache: Mutex<HashMap<String, serde_json::Value>>,
}

impl CoreLinkProtector {
    /// 创建核心链路保护器，默认核心链路关键字：
    /// `payment`、`order`、`transaction`、`checkout`、`auth`、`login`。
    ///
    /// 默认断路器：连续失败 5 次熔断，30 秒后进入半开探测。
    pub fn new() -> Self {
        Self {
            core_link_patterns: vec![
                "payment".to_string(),
                "order".to_string(),
                "transaction".to_string(),
                "checkout".to_string(),
                "auth".to_string(),
                "login".to_string(),
            ],
            breaker: Mutex::new(DefaultCircuitBreaker::new(5, Duration::from_secs(30))),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// 自定义核心链路关键字（覆盖默认）。
    pub fn with_core_links(mut self, patterns: Vec<String>) -> Self {
        self.core_link_patterns = patterns;
        self
    }

    /// 自定义断路器（覆盖默认）。
    pub fn with_circuit_breaker(self, breaker: DefaultCircuitBreaker) -> Self {
        Self {
            breaker: Mutex::new(breaker),
            ..self
        }
    }

    /// 写入降级缓存值，供 [`DegradationStrategy::Cache`] 命中返回。
    pub fn set_cache(&self, request: String, value: serde_json::Value) {
        self.cache
            .lock()
            .expect("cache lock poisoned")
            .insert(request, value);
    }

    /// 判断请求是否属于核心链路。
    ///
    /// 核心链路请求不被降级，应走原始执行路径。
    pub fn is_core_link(&self, request: &str) -> bool {
        self.core_link_patterns
            .iter()
            .any(|p| request.contains(p.as_str()))
    }

    /// 对非核心请求执行降级，返回合理的降级值。
    ///
    /// - 核心链路请求返回 `Err(DegradeError::CoreLinkNotDegradable)`，调用方应走原始路径；
    /// - 非核心请求根据 `strategy` 返回 `Ok(value)`，禁止返回错误。
    ///
    /// # 策略映射
    /// | 策略 | 返回值 |
    /// |------|--------|
    /// | `Cache` / `ReturnCache` | 缓存命中返回缓存值，未命中返回空数组 |
    /// | `DefaultValue` / `ReturnDefault` | 空数组 |
    /// | `SimplifiedResult` | `{"simplified": true, "request": <request>}` |
    /// | `FastFail` | `{"simplified": true, "fast_fail": true, "request": <request>}` |
    pub async fn degrade_non_core(
        &self,
        request: &str,
        strategy: DegradationStrategy,
    ) -> Result<serde_json::Value, DegradeError> {
        if self.is_core_link(request) {
            return Err(DegradeError::CoreLinkNotDegradable(request.to_string()));
        }
        let value = match strategy {
            DegradationStrategy::Cache | DegradationStrategy::ReturnCache => {
                let cache = self.cache.lock().expect("cache lock poisoned");
                cache
                    .get(request)
                    .cloned()
                    .unwrap_or_else(Self::empty_result)
            }
            DegradationStrategy::DefaultValue | DegradationStrategy::ReturnDefault => {
                Self::empty_result()
            }
            DegradationStrategy::SimplifiedResult => {
                serde_json::json!({"simplified": true, "request": request})
            }
            DegradationStrategy::FastFail => {
                serde_json::json!({"simplified": true, "fast_fail": true, "request": request})
            }
        };
        Ok(value)
    }

    /// 空集合（默认降级值）
    fn empty_result() -> serde_json::Value {
        serde_json::Value::Array(Vec::new())
    }

    /// 断路器是否可执行（代理 [`DefaultCircuitBreaker::can_execute`]）。
    ///
    /// `Open` 且熔断超时后自动转为 `HalfOpen` 并放行探测请求。
    pub fn can_execute(&self) -> bool {
        self.breaker
            .lock()
            .expect("breaker lock poisoned")
            .can_execute()
    }

    /// 记录一次成功（代理断路器，复位失败计数，回到 `Closed`）。
    pub fn record_success(&self) {
        self.breaker
            .lock()
            .expect("breaker lock poisoned")
            .record_success();
    }

    /// 记录一次失败（代理断路器，达到阈值后熔断为 `Open`）。
    pub fn record_failure(&self) {
        self.breaker
            .lock()
            .expect("breaker lock poisoned")
            .record_failure();
    }

    /// 断路器当前状态（代理）。
    pub fn state(&self) -> CircuitState {
        self.breaker.lock().expect("breaker lock poisoned").state()
    }
}

impl Default for CoreLinkProtector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod core_link_protector_tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_core_link_identified() {
        let protector = CoreLinkProtector::new();
        assert!(protector.is_core_link("payment/charge"));
        assert!(protector.is_core_link("order/create"));
        assert!(protector.is_core_link("transaction/commit"));
        assert!(protector.is_core_link("checkout/submit"));
        assert!(protector.is_core_link("auth/verify"));
        assert!(protector.is_core_link("user/login"));
    }

    #[tokio::test]
    async fn test_non_core_link_identified() {
        let protector = CoreLinkProtector::new();
        assert!(!protector.is_core_link("report/list"));
        assert!(!protector.is_core_link("stats/summary"));
        assert!(!protector.is_core_link("recommend/feed"));
        assert!(!protector.is_core_link("analytics/track"));
        assert!(!protector.is_core_link("log/append"));
    }

    #[tokio::test]
    async fn test_core_link_rejects_degradation() {
        let protector = CoreLinkProtector::new();
        let result = protector
            .degrade_non_core("payment/charge", DegradationStrategy::Cache)
            .await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            DegradeError::CoreLinkNotDegradable("payment/charge".to_string())
        );
    }

    #[tokio::test]
    async fn test_non_core_cache_miss_returns_empty() {
        let protector = CoreLinkProtector::new();
        let result = protector
            .degrade_non_core("report/list", DegradationStrategy::Cache)
            .await;
        assert_eq!(result.unwrap(), serde_json::Value::Array(vec![]));
    }

    #[tokio::test]
    async fn test_non_core_cache_hit_returns_cached() {
        let protector = CoreLinkProtector::new();
        protector.set_cache(
            "report/list".to_string(),
            serde_json::json!({"items": [1, 2, 3]}),
        );
        let result = protector
            .degrade_non_core("report/list", DegradationStrategy::Cache)
            .await;
        assert_eq!(result.unwrap(), serde_json::json!({"items": [1, 2, 3]}));
    }

    #[tokio::test]
    async fn test_non_core_default_value_returns_empty() {
        let protector = CoreLinkProtector::new();
        let result = protector
            .degrade_non_core("stats/summary", DegradationStrategy::DefaultValue)
            .await;
        assert_eq!(result.unwrap(), serde_json::Value::Array(vec![]));
    }

    #[tokio::test]
    async fn test_non_core_simplified_result() {
        let protector = CoreLinkProtector::new();
        let result = protector
            .degrade_non_core("recommend/feed", DegradationStrategy::SimplifiedResult)
            .await;
        let value = result.unwrap();
        assert_eq!(value["simplified"], true);
        assert_eq!(value["request"], "recommend/feed");
    }

    #[tokio::test]
    async fn test_non_core_never_errors_for_all_strategies() {
        let protector = CoreLinkProtector::new();
        let strategies = [
            DegradationStrategy::Cache,
            DegradationStrategy::DefaultValue,
            DegradationStrategy::SimplifiedResult,
            DegradationStrategy::ReturnCache,
            DegradationStrategy::ReturnDefault,
            DegradationStrategy::FastFail,
        ];
        for strategy in strategies {
            let result = protector
                .degrade_non_core("analytics/report", strategy)
                .await;
            assert!(result.is_ok(), "strategy {:?} should not error", strategy);
        }
    }

    #[tokio::test]
    async fn test_custom_core_links_override_default() {
        let protector =
            CoreLinkProtector::new().with_core_links(vec!["pay".to_string(), "settle".to_string()]);
        assert!(protector.is_core_link("pay/charge"));
        assert!(protector.is_core_link("settle/daily"));
        // 默认关键字不再视为核心
        assert!(!protector.is_core_link("order/create"));
        assert!(!protector.is_core_link("login"));
    }

    #[tokio::test]
    async fn test_half_open_probe_recovery() {
        // 断路器阈值 3，超时 10ms
        let breaker = DefaultCircuitBreaker::new(3, Duration::from_millis(10));
        let protector = CoreLinkProtector::new().with_circuit_breaker(breaker);

        // 累积失败打开断路器
        assert!(protector.can_execute());
        protector.record_failure();
        protector.record_failure();
        assert_eq!(protector.state(), CircuitState::Closed);
        protector.record_failure();
        assert_eq!(protector.state(), CircuitState::Open);
        assert!(!protector.can_execute());

        // 超时后进入半开，放行探测
        std::thread::sleep(Duration::from_millis(20));
        assert!(protector.can_execute());
        assert_eq!(protector.state(), CircuitState::HalfOpen);

        // 探测成功，恢复 Closed
        protector.record_success();
        assert_eq!(protector.state(), CircuitState::Closed);
        assert!(protector.can_execute());
    }

    #[tokio::test]
    async fn test_half_open_probe_failure_reopens() {
        let breaker = DefaultCircuitBreaker::new(1, Duration::from_millis(10));
        let protector = CoreLinkProtector::new().with_circuit_breaker(breaker);

        protector.record_failure();
        assert_eq!(protector.state(), CircuitState::Open);

        std::thread::sleep(Duration::from_millis(20));
        assert!(protector.can_execute()); // HalfOpen
                                          // 探测失败，重回 Open
        protector.record_failure();
        assert_eq!(protector.state(), CircuitState::Open);
        assert!(!protector.can_execute());
    }

    #[tokio::test]
    async fn test_degrade_non_core_with_legacy_strategies() {
        let protector = CoreLinkProtector::new();
        // ReturnCache 等价于 Cache
        let result = protector
            .degrade_non_core("report/list", DegradationStrategy::ReturnCache)
            .await;
        assert_eq!(result.unwrap(), serde_json::Value::Array(vec![]));

        // ReturnDefault 等价于 DefaultValue
        let result = protector
            .degrade_non_core("report/list", DegradationStrategy::ReturnDefault)
            .await;
        assert_eq!(result.unwrap(), serde_json::Value::Array(vec![]));

        // FastFail 在保护器语境下返回简化结果而非错误
        let result = protector
            .degrade_non_core("report/list", DegradationStrategy::FastFail)
            .await;
        let value = result.unwrap();
        assert_eq!(value["simplified"], true);
        assert_eq!(value["fast_fail"], true);
    }

    #[test]
    fn test_default_impl() {
        let protector = CoreLinkProtector::default();
        assert!(protector.is_core_link("payment/charge"));
        assert!(!protector.is_core_link("report/list"));
        assert_eq!(protector.state(), CircuitState::Closed);
    }

    #[test]
    fn test_new_strategy_as_str() {
        assert_eq!(DegradationStrategy::Cache.as_str(), "cache");
        assert_eq!(DegradationStrategy::DefaultValue.as_str(), "default_value");
        assert_eq!(
            DegradationStrategy::SimplifiedResult.as_str(),
            "simplified_result"
        );
    }
}
