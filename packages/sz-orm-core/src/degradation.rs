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

    // ========================================================================
    // T22 新增测试：覆盖 degradation.rs 全部 pub fn/method 核心路径与边界
    // ========================================================================

    #[test]
    fn t22_degradation_strategy_as_str_all_variants() {
        assert_eq!(DegradationStrategy::ReturnCache.as_str(), "return_cache");
        assert_eq!(
            DegradationStrategy::ReturnDefault.as_str(),
            "return_default"
        );
        assert_eq!(DegradationStrategy::FastFail.as_str(), "fast_fail");
        assert_eq!(DegradationStrategy::Cache.as_str(), "cache");
        assert_eq!(DegradationStrategy::DefaultValue.as_str(), "default_value");
        assert_eq!(
            DegradationStrategy::SimplifiedResult.as_str(),
            "simplified_result"
        );
    }

    #[test]
    fn t22_degradation_result_from_cache_basic() {
        let row: HashMap<String, String> =
            [("k".to_string(), "v".to_string())].into_iter().collect();
        let result = DegradationResult::from_cache(vec![row.clone()]);
        assert!(result.is_degraded);
        assert!(!result.mismatch_warning);
        assert_eq!(
            result.degradation_strategy,
            DegradationStrategy::ReturnCache
        );
        let data = result.data.expect("data should be Some");
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].get("k").map(String::as_str), Some("v"));
    }

    #[test]
    fn t22_degradation_result_from_default_basic() {
        let result = DegradationResult::from_default();
        assert!(result.is_degraded);
        assert!(!result.mismatch_warning);
        assert_eq!(
            result.degradation_strategy,
            DegradationStrategy::ReturnDefault
        );
        assert_eq!(result.data.expect("data should be Some").len(), 0);
    }

    #[test]
    fn t22_degradation_result_fast_fail_basic() {
        let result = DegradationResult::fast_fail();
        assert!(result.is_degraded);
        assert!(!result.mismatch_warning);
        assert_eq!(result.degradation_strategy, DegradationStrategy::FastFail);
        assert!(result.data.is_none());
    }

    #[test]
    fn t22_degradation_result_with_mismatch_warning_chain() {
        let result = DegradationResult::from_default().with_mismatch_warning();
        assert!(result.mismatch_warning);
        assert!(result.is_degraded);
        let result = DegradationResult::fast_fail().with_mismatch_warning();
        assert!(result.mismatch_warning);
        assert!(result.data.is_none());
    }

    #[test]
    fn t22_cache_degradation_new_and_default_handle_miss() {
        let handler = CacheDegradation::new();
        assert_eq!(handler.strategy(), DegradationStrategy::ReturnCache);
        let err = handler.handle("fp_missing").unwrap_err();
        assert_eq!(err, DegradationError::CacheMiss("fp_missing".to_string()));

        let default_handler = CacheDegradation::default();
        assert_eq!(
            default_handler.handle("any").unwrap_err(),
            DegradationError::CacheMiss("any".to_string())
        );
    }

    #[test]
    fn t22_cache_degradation_with_cache_prepopulated_hit() {
        let row: HashMap<String, String> =
            [("col".to_string(), "1".to_string())].into_iter().collect();
        let mut map: HashMap<String, Vec<HashMap<String, String>>> = HashMap::new();
        map.insert("fp1".to_string(), vec![row]);
        let handler = CacheDegradation::with_cache(map);
        let result = handler.handle("fp1").unwrap();
        assert!(result.is_degraded);
        assert_eq!(
            result.degradation_strategy,
            DegradationStrategy::ReturnCache
        );
        assert_eq!(
            result.data.as_ref().unwrap()[0]
                .get("col")
                .map(String::as_str),
            Some("1")
        );
        // 未命中分支
        let err = handler.handle("fp2").unwrap_err();
        assert_eq!(err, DegradationError::CacheMiss("fp2".to_string()));
    }

    #[test]
    fn t22_cache_degradation_insert_then_handle_hit() {
        let mut handler = CacheDegradation::new();
        let row: HashMap<String, String> =
            [("a".to_string(), "b".to_string())].into_iter().collect();
        handler.insert("fp_insert".to_string(), vec![row]);
        let result = handler.handle("fp_insert").unwrap();
        assert!(result.is_degraded);
        assert_eq!(
            result.data.as_ref().unwrap()[0]
                .get("a")
                .map(String::as_str),
            Some("b")
        );
    }

    #[test]
    fn t22_default_degradation_handle_and_strategy() {
        let handler = DefaultDegradation::new();
        assert_eq!(handler.strategy(), DegradationStrategy::ReturnDefault);
        let result = handler.handle("any_fp").unwrap();
        assert!(result.is_degraded);
        assert!(result.data.as_ref().unwrap().is_empty());
        assert_eq!(
            result.degradation_strategy,
            DegradationStrategy::ReturnDefault
        );

        let default_handler = DefaultDegradation::new();
        assert_eq!(
            default_handler.strategy(),
            DegradationStrategy::ReturnDefault
        );
    }

    #[test]
    fn t22_fast_fail_degradation_handle_and_strategy() {
        let handler = FastFailDegradation::new();
        assert_eq!(handler.strategy(), DegradationStrategy::FastFail);
        let err = handler.handle("any_fp").unwrap_err();
        assert_eq!(err, DegradationError::FastFail);

        let default_handler = FastFailDegradation::new();
        assert_eq!(default_handler.strategy(), DegradationStrategy::FastFail);
        assert_eq!(
            default_handler.handle("x").unwrap_err(),
            DegradationError::FastFail
        );
    }

    #[test]
    fn t22_execute_with_degradation_normal_success() {
        let handler = DefaultDegradation::new();
        let row: HashMap<String, String> =
            [("id".to_string(), "42".to_string())].into_iter().collect();
        let result = execute_with_degradation(true, &handler, "fp", || Ok(vec![row])).unwrap();
        assert!(!result.is_degraded);
        assert!(!result.mismatch_warning);
        assert_eq!(
            result.degradation_strategy,
            DegradationStrategy::ReturnDefault
        );
        assert_eq!(
            result.data.as_ref().unwrap()[0]
                .get("id")
                .map(String::as_str),
            Some("42")
        );
    }

    #[test]
    fn t22_execute_with_degradation_normal_error_fallback_to_handler() {
        let handler = DefaultDegradation::new();
        let result =
            execute_with_degradation(true, &handler, "fp", || Err("db down".to_string())).unwrap();
        assert!(result.is_degraded);
        assert_eq!(
            result.degradation_strategy,
            DegradationStrategy::ReturnDefault
        );
        assert!(result.data.as_ref().unwrap().is_empty());
    }

    #[test]
    fn t22_execute_with_degradation_cannot_execute_degrades() {
        let handler = FastFailDegradation::new();
        let result = execute_with_degradation(false, &handler, "fp", || Ok(vec![]));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), DegradationError::FastFail);
    }

    #[test]
    fn t22_execute_with_degradation_cannot_execute_with_cache_handler_miss() {
        let handler = CacheDegradation::new();
        let result = execute_with_degradation(false, &handler, "fp_x", || Ok(vec![]));
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            DegradationError::CacheMiss("fp_x".to_string())
        );
    }

    #[test]
    fn t22_degradation_error_variants_display() {
        let cache_miss = DegradationError::CacheMiss("qfp".to_string());
        assert_eq!(
            cache_miss.to_string(),
            "cache miss for query fingerprint: qfp"
        );
        let fast_fail = DegradationError::FastFail;
        assert_eq!(fast_fail.to_string(), "fast fail: circuit breaker open");
        let mismatch = DegradationError::DataMismatch("qfp2".to_string());
        assert_eq!(
            mismatch.to_string(),
            "degradation data mismatch for query fingerprint: qfp2"
        );
    }

    #[test]
    fn t22_degrade_error_core_link_not_degradable_display() {
        let err = DegradeError::CoreLinkNotDegradable("payment/charge".to_string());
        assert_eq!(
            err.to_string(),
            "core link request cannot be degraded: payment/charge"
        );
    }

    /// T22.1 降级策略按优先级关闭非核心功能：触发资源紧张降级，
    /// 断言非核心功能被降级（返回合理降级值），核心功能保持可用（拒绝降级、走原始路径）。
    #[tokio::test]
    async fn t22_priority_degrade_non_core_while_core_preserved() {
        let protector = CoreLinkProtector::new();

        // 资源紧张：断路器熔断（连续失败达阈值）
        let breaker = DefaultCircuitBreaker::new(2, Duration::from_secs(60));
        let protector = protector.with_circuit_breaker(breaker);
        protector.record_failure();
        protector.record_failure();
        assert_eq!(protector.state(), CircuitState::Open);
        assert!(!protector.can_execute());

        // 非核心请求被降级（按优先级选择 Cache 策略，未命中返回空集合）
        let non_core = protector
            .degrade_non_core("report/list", DegradationStrategy::Cache)
            .await
            .unwrap();
        assert_eq!(non_core, serde_json::Value::Array(vec![]));

        // 非核心请求按 DefaultValue 策略降级
        let non_core_default = protector
            .degrade_non_core("stats/summary", DegradationStrategy::DefaultValue)
            .await
            .unwrap();
        assert_eq!(non_core_default, serde_json::Value::Array(vec![]));

        // 非核心请求按 SimplifiedResult 策略降级
        let non_core_simplified = protector
            .degrade_non_core("recommend/feed", DegradationStrategy::SimplifiedResult)
            .await
            .unwrap();
        assert_eq!(non_core_simplified["simplified"], true);

        // 核心功能保持可用：核心链路请求拒绝降级，调用方应走原始执行路径
        let core_payment = protector
            .degrade_non_core("payment/charge", DegradationStrategy::Cache)
            .await;
        assert!(core_payment.is_err());
        assert_eq!(
            core_payment.unwrap_err(),
            DegradeError::CoreLinkNotDegradable("payment/charge".to_string())
        );

        let core_order = protector
            .degrade_non_core("order/create", DegradationStrategy::DefaultValue)
            .await;
        assert!(core_order.is_err());

        // 核心链路识别仍然有效（即使断路器 Open，核心请求也不被降级）
        assert!(protector.is_core_link("payment/charge"));
        assert!(protector.is_core_link("order/create"));
        assert!(!protector.is_core_link("report/list"));
    }

    #[tokio::test]
    async fn t22_priority_degrade_non_core_cache_hit_preserved() {
        // 资源紧张降级时，若缓存命中，非核心功能返回缓存值（优先级最高，避免空响应）
        let protector = CoreLinkProtector::new();
        protector.set_cache(
            "report/list".to_string(),
            serde_json::json!({"items": [1, 2, 3]}),
        );
        let value = protector
            .degrade_non_core("report/list", DegradationStrategy::Cache)
            .await
            .unwrap();
        assert_eq!(value, serde_json::json!({"items": [1, 2, 3]}));

        // ReturnCache 等价策略同样命中
        let value_legacy = protector
            .degrade_non_core("report/list", DegradationStrategy::ReturnCache)
            .await
            .unwrap();
        assert_eq!(value_legacy, serde_json::json!({"items": [1, 2, 3]}));
    }

    #[tokio::test]
    async fn t22_priority_degrade_fast_fail_strategy_returns_simplified() {
        // FastFail 策略在保护器语境下不返回错误，而是返回简化结果
        let protector = CoreLinkProtector::new();
        let value = protector
            .degrade_non_core("analytics/track", DegradationStrategy::FastFail)
            .await
            .unwrap();
        assert_eq!(value["simplified"], true);
        assert_eq!(value["fast_fail"], true);
        assert_eq!(value["request"], "analytics/track");
    }
}
