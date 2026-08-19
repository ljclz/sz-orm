//! # SZ-ORM actix-web Framework Integration
//!
//! Provides:
//! - [`PoolState`] — actix-web application data wrapper for connection pool (implements `FromRequest`)
//! - [`JsonRows`] — Wraps `QueryRows` implementing `Responder`
//! - [`JsonResp<T>`] — Generic JSON response wrapper
//! - [`TransactionMiddleware`] — Transaction middleware (commit on success, rollback on failure)
//! - [`Pagination`] — Pagination parameter helper
//! - [`ErrorResponse`] — Standard error response
//! - [`HealthCheck`] — Health check
//! - [`RouteInfo`] — Route info
//!
//! Due to Rust's orphan rule, we cannot directly implement
//! actix-web's `FromRequest`/`Responder` for `Arc<Pool>` or `QueryRows`, so we use `PoolState` / `JsonRows`
//! wrapper types (consistent with sz-orm-axum style).

mod auth;
mod cors;
mod response;
mod validator;

pub use auth::{AuthConfig, AuthResult, RoleChecker, TokenValidator};
pub use cors::{CorsConfig, CorsOrigin};
pub use response::{
    ApiError, ApiResponse, ApiStatus, CacheControlConfig, CacheDirective, MiddlewareChainConfig,
    MiddlewareEntry, MiddlewarePriority, PaginatedResponse, PaginationExtractor, PaginationMeta,
    RateLimitConfig, RateLimitKey, RateLimitStrategy, ResponseHeaders, ResponseWrapper,
    SecurityHeaders,
};
pub use validator::{
    FieldValidator, RequestValidator, RuleType, ValidationError, ValidationResult, ValidationRule,
};

use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    web, FromRequest, HttpMessage, HttpRequest, HttpResponse, Responder,
};
use serde::Serialize;
use std::{
    future::{ready, Ready},
    rc::Rc,
    sync::Arc,
};
use sz_orm_core::{Pool, PooledConnection, QueryRows, Value};
use tokio::sync::{Mutex, MutexGuard};

// ============================================================================
// PoolState — 连接池的 actix-web 应用数据包装
// ============================================================================

/// actix-web application data wrapper for connection pool
///
/// `Pool` uses `Arc` internally for sharing, `PoolState` provides lightweight `Clone`,
/// for easy registration in `App::app_data`, and implements `FromRequest` for extraction in handlers.
#[derive(Clone)]
pub struct PoolState {
    pool: Arc<Pool>,
}

impl PoolState {
    /// Create PoolState
    pub fn new(pool: Pool) -> Self {
        Self {
            pool: Arc::new(pool),
        }
    }

    /// Create from `Arc<Pool>` (avoids duplicate Arc wrapping)
    pub fn from_arc(pool: Arc<Pool>) -> Self {
        Self { pool }
    }

    /// Get connection pool reference
    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    /// Get `Arc<Pool>` clone (for passing directly to other components)
    pub fn pool_arc(&self) -> Arc<Pool> {
        self.pool.clone()
    }

    /// Consume PoolState and return `Arc<Pool>`
    pub fn into_arc(self) -> Arc<Pool> {
        self.pool
    }
}

impl FromRequest for PoolState {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        // 优先从 app_data 中提取 PoolState
        // 注意：`web::Data<T>` deref 到 `Arc<T>`，因此 `**state` 是 `Arc<PoolState>`
        // 而非 `PoolState`。使用 `get_ref()` 直接获取 `&T` 后再 clone 更清晰。
        if let Some(state) = req.app_data::<web::Data<PoolState>>() {
            return ready(Ok(state.get_ref().clone()));
        }
        // 兼容直接注册 Arc<Pool> 的场景
        if let Some(pool) = req.app_data::<web::Data<Arc<Pool>>>() {
            return ready(Ok(PoolState::from_arc(pool.get_ref().clone())));
        }
        ready(Err(actix_web::error::ErrorInternalServerError(
            "PoolState not found in app data",
        )))
    }
}

// ============================================================================
// JsonRows — 查询结果的 JSON 响应
// ============================================================================

/// Wrap `QueryRows` implementing `Responder`
///
/// `QueryRows = Vec<HashMap<String, Value>>`, converted field-by-field to JSON object array.
pub struct JsonRows(pub QueryRows);

impl Responder for JsonRows {
    type Body = actix_web::body::BoxBody;

    fn respond_to(self, _: &HttpRequest) -> HttpResponse {
        let json: Vec<serde_json::Value> = self
            .0
            .iter()
            .map(|row| {
                let mut map = serde_json::Map::new();
                for (k, v) in row {
                    map.insert(k.clone(), value_to_json(v));
                }
                serde_json::Value::Object(map)
            })
            .collect();
        HttpResponse::Ok().json(json)
    }
}

/// Convert `Value` to `serde_json::Value`
///
/// Manual mapping so `Bytes` outputs hex string, `Decimal`/`Date` etc. kept as strings,
/// avoiding default serialization expanding `Vec<u8>` to numeric array.
fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::I8(n) => (*n).into(),
        Value::I16(n) => (*n).into(),
        Value::I32(n) => (*n).into(),
        Value::I64(n) => (*n).into(),
        Value::U8(n) => (*n).into(),
        Value::U16(n) => (*n).into(),
        Value::U32(n) => (*n).into(),
        Value::U64(n) => (*n).into(),
        Value::F32(f) => serde_json::Value::from(*f),
        Value::F64(f) => serde_json::Value::from(*f),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Decimal(s) => serde_json::Value::String(s.clone()),
        // 字节序列以十六进制字符串表示，避免默认序列化为数字数组
        Value::Bytes(b) => {
            serde_json::Value::String(b.iter().map(|byte| format!("{:02x}", byte)).collect())
        }
        Value::Date(s) | Value::DateTime(s) | Value::Time(s) => {
            serde_json::Value::String(s.clone())
        }
        Value::Json(s) => {
            serde_json::from_str(s).unwrap_or_else(|_| serde_json::Value::String(s.clone()))
        }
        Value::Uuid(s) => serde_json::Value::String(s.clone()),
        Value::Array(arr) => serde_json::Value::Array(arr.iter().map(value_to_json).collect()),
        Value::Object(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k.clone(), value_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
        // Value 标记为 #[non_exhaustive]，预留兜底
        _ => serde_json::Value::Null,
    }
}

// ============================================================================
// JsonResp<T> — 通用 JSON 响应包装
// ============================================================================

/// Generic JSON response wrapper
///
/// Provides `Responder` implementation for any type implementing `Serialize`.
pub struct JsonResp<T: Serialize>(pub T);

impl<T: Serialize> JsonResp<T> {
    /// Create JSON response wrapper
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Unwrap inner value
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Serialize> Responder for JsonResp<T> {
    type Body = actix_web::body::BoxBody;

    fn respond_to(self, _: &HttpRequest) -> HttpResponse {
        match serde_json::to_value(&self.0) {
            Ok(v) => HttpResponse::Ok().json(v),
            Err(e) => HttpResponse::InternalServerError().body(format!("JSON 序列化失败: {}", e)),
        }
    }
}

// ============================================================================
// TransactionMiddleware — 事务中间件
// ============================================================================

/// Transaction connection holder
///
/// Created in [`TransactionMiddleware`], injected into request extensions for handler
/// to reuse the same connection for queries. Handler extracts via `web::ReqData<TransactionConn>`.
///
/// Connection wrapped in `Arc<Mutex<Option<PooledConnection>>>`:
/// - `Some(conn)`: Transaction in progress, handler can acquire connection for queries
/// - `None`: Transaction ended (middleware took back connection for commit/rollback)
///
/// # Usage
///
/// ```ignore
/// use actix_web::web::ReqData;
/// use sz_orm_actix::TransactionConn;
///
/// async fn handler(tx: ReqData<TransactionConn>) -> impl Responder {
///     if let Some(mut guard) = tx.conn().await {
///         if let Some(conn) = guard.as_mut() {
///             conn.execute("INSERT INTO users (name) VALUES ('Alice')").await?;
///         }
///     }
///     HttpResponse::Ok()
/// }
/// ```
pub struct TransactionConn {
    inner: Arc<Mutex<Option<PooledConnection>>>,
}

impl TransactionConn {
    /// Create holder (middleware internal use only)
    fn new(conn: PooledConnection) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some(conn))),
        }
    }

    /// Get mutable reference to connection
    ///
    /// Returns `MutexGuard<Option<PooledConnection>>`, caller via `guard.as_mut()`
    /// gets `&mut Option<PooledConnection>`, then `.as_mut().unwrap()` to get
    /// `&mut PooledConnection`。
    ///
    /// If middleware has taken back connection (transaction ended), returns `None`.
    pub async fn conn(&self) -> Option<MutexGuard<'_, Option<PooledConnection>>> {
        let guard = self.inner.lock().await;
        if guard.is_none() {
            return None;
        }
        Some(guard)
    }
}

impl Clone for TransactionConn {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl FromRequest for TransactionConn {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        if let Some(tx) = req.extensions().get::<TransactionConn>() {
            return ready(Ok(tx.clone()));
        }
        ready(Err(actix_web::error::ErrorInternalServerError(
            "TransactionConn not found in request extensions. \
             Is TransactionMiddleware registered?",
        )))
    }
}

/// Transaction middleware
///
/// Acquires connection from pool and `begin_transaction` before request processing, injects connection into
/// `request extensions` for handler reuse; after request processing, based on `ServiceResponse`
/// status code 2xx commit / otherwise rollback.
///
/// **Fallback strategy**: If `PoolState` is not registered in `app_data`, or acquire fails,
/// or `begin_transaction` fails, degrades to passthrough request (no transaction), and
/// logs a warning. This ensures scenarios without transactions still process requests normally.
///
/// # Usage
///
/// ```ignore
/// use actix_web::{web, App};
/// use sz_orm_actix::{PoolState, TransactionMiddleware};
///
/// let app = App::new()
///     .app_data(web::Data::new(PoolState::new(pool)))
///     .wrap(TransactionMiddleware);
/// ```
pub struct TransactionMiddleware;

impl<S, B> Transform<S, ServiceRequest> for TransactionMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Transform = TransactionMiddlewareService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(TransactionMiddlewareService {
            service: Rc::new(service),
        }))
    }
}

/// Transaction middleware service
///
/// Internally uses `Rc<S>` to share downstream service, for calling service in async blocks
/// (actix-web's `Service::Future` does not require `Send`, so uses `Rc` instead of `Arc`).
pub struct TransactionMiddlewareService<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for TransactionMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future =
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // 1. 从 app_data 获取 PoolState（clone 出来，不持有 req 的借用）
        let pool_state = req
            .app_data::<web::Data<PoolState>>()
            .map(|s| s.get_ref().clone());

        // 克隆 Rc<S> 以便在 async 块中调用 service
        let svc = Rc::clone(&self.service);

        Box::pin(async move {
            // 无 PoolState 时退化为透传
            let pool_state = match pool_state {
                Some(state) => state,
                None => {
                    tracing::warn!(
                        target: "sz_orm_actix::transaction",
                        "PoolState not found in app_data, TransactionMiddleware degrades to passthrough"
                    );
                    return svc.call(req).await;
                }
            };

            // 2. acquire 连接
            let mut conn = match pool_state.pool().acquire().await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(
                        target: "sz_orm_actix::transaction",
                        error = %e,
                        "acquire connection failed, TransactionMiddleware degrades to passthrough"
                    );
                    return svc.call(req).await;
                }
            };

            // 3. begin_transaction
            if let Err(e) = conn.begin_transaction().await {
                tracing::warn!(
                    target: "sz_orm_actix::transaction",
                    error = %e,
                    "begin_transaction failed, TransactionMiddleware degrades to passthrough"
                );
                // conn drop 时自动归还池
                return svc.call(req).await;
            }

            // 4. 将连接注入 request extensions 供 handler 复用
            let tx_conn = TransactionConn::new(conn);
            let tx_clone = tx_conn.clone(); // 用于请求结束后取回连接
            req.extensions_mut().insert(tx_conn);

            // 5. 调用下游 service
            let res = svc.call(req).await?;

            // 6. 取回连接，根据响应状态码 commit / rollback
            let mut guard = tx_clone.inner.lock().await;
            if let Some(mut conn) = guard.take() {
                let tx_result = if res.status().is_success() {
                    conn.commit().await
                } else {
                    conn.rollback().await
                };
                if let Err(e) = tx_result {
                    tracing::error!(
                        target: "sz_orm_actix::transaction",
                        error = %e,
                        status = %res.status(),
                        "transaction commit/rollback failed"
                    );
                    // conn drop 时自动归还池
                }
            } else {
                tracing::debug!(
                    target: "sz_orm_actix::transaction",
                    "TransactionConn was None after service call (handler may have dropped it)"
                );
            }

            Ok(res)
        })
    }
}

// ============================================================================
// Pagination — 分页辅助
// ============================================================================

/// Pagination parameter helper: compute offset/limit, total pages, next/prev page flags
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pagination {
    page: u64,
    per_page: u64,
}

impl Pagination {
    /// Create pagination (page starts from 1, per_page defaults to 20)
    pub fn new(page: u64, per_page: u64) -> Self {
        Self {
            page: page.max(1),
            per_page: per_page.max(1),
        }
    }

    /// Current page number
    pub fn page(&self) -> u64 {
        self.page
    }

    /// Rows per page
    pub fn per_page(&self) -> u64 {
        self.per_page
    }

    /// SQL OFFSET
    pub fn offset(&self) -> u64 {
        (self.page - 1) * self.per_page
    }

    /// SQL LIMIT
    pub fn limit(&self) -> u64 {
        self.per_page
    }

    /// Total pages
    pub fn total_pages(&self, total: u64) -> u64 {
        if total == 0 {
            0
        } else {
            total.div_ceil(self.per_page)
        }
    }

    /// Whether there is a next page
    pub fn has_next(&self, total: u64) -> bool {
        self.page < self.total_pages(total)
    }

    /// Whether there is a previous page
    pub fn has_prev(&self) -> bool {
        self.page > 1
    }
}

// ============================================================================
// ErrorResponse — 标准错误响应
// ============================================================================

/// Standard error response: unified code + message format
#[derive(Debug, Clone)]
pub struct ErrorResponse {
    code: u16,
    message: String,
}

impl Default for ErrorResponse {
    fn default() -> Self {
        Self {
            code: 500,
            message: "Internal Server Error".to_string(),
        }
    }
}

impl ErrorResponse {
    /// Create default error response (500)
    pub fn new() -> Self {
        Self::default()
    }

    /// Set HTTP status code (chainable)
    pub fn with_code(mut self, code: u16) -> Self {
        self.code = code;
        self
    }

    /// Set error message (chainable)
    pub fn with_message(mut self, msg: &str) -> Self {
        self.message = msg.to_string();
        self
    }

    /// Status code
    pub fn code(&self) -> u16 {
        self.code
    }

    /// Error message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// JSON string representation
    pub fn to_json_string(&self) -> String {
        serde_json::json!({
            "code": self.code,
            "message": self.message,
        })
        .to_string()
    }

    /// Convert to actix-web HttpResponse
    pub fn to_http_response(&self) -> HttpResponse {
        let status = actix_web::http::StatusCode::from_u16(self.code)
            .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);
        HttpResponse::build(status).json(serde_json::json!({
            "code": self.code,
            "message": self.message,
        }))
    }
}

// ============================================================================
// HealthCheck — 健康检查
// ============================================================================

/// Health check result aggregation
#[derive(Debug, Clone, Default)]
pub struct HealthCheck {
    checks: Vec<(String, bool)>,
}

impl HealthCheck {
    /// Create empty health check
    pub fn new() -> Self {
        Self::default()
    }

    /// Add check result
    pub fn add_check(&mut self, name: &str, passed: bool) {
        self.checks.push((name.to_string(), passed));
    }

    /// Number of check items
    pub fn check_count(&self) -> usize {
        self.checks.len()
    }

    /// Whether all passed
    pub fn is_healthy(&self) -> bool {
        self.checks.iter().all(|(_, passed)| *passed)
    }
}

// ============================================================================
// RouteInfo — 路由信息
// ============================================================================

/// Route metadata: path, method, handler name
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteInfo {
    path: String,
    method: String,
    handler_name: String,
}

impl RouteInfo {
    /// Create route info
    pub fn new(path: &str, method: &str, handler_name: &str) -> Self {
        Self {
            path: path.to_string(),
            method: method.to_string(),
            handler_name: handler_name.to_string(),
        }
    }

    /// Path
    pub fn path(&self) -> &str {
        &self.path
    }

    /// HTTP method
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Handler name
    pub fn handler_name(&self) -> &str {
        &self.handler_name
    }

    /// Description string
    pub fn description(&self) -> String {
        format!("{} {} -> {}", self.method, self.path, self.handler_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_pool_state_clone() {
        fn _assert_clone<T: Clone>() {}
        _assert_clone::<PoolState>();
    }

    #[test]
    fn test_value_to_json_variants() {
        assert_eq!(value_to_json(&Value::Null), serde_json::Value::Null);
        assert_eq!(
            value_to_json(&Value::Bool(true)),
            serde_json::Value::Bool(true)
        );
        assert_eq!(value_to_json(&Value::I64(42)), serde_json::json!(42));
        assert_eq!(
            value_to_json(&Value::String("hi".into())),
            serde_json::json!("hi")
        );
        assert_eq!(
            value_to_json(&Value::Bytes(vec![0x1a, 0x2b])),
            serde_json::json!("1a2b")
        );
        // JSON 字符串解析为对象/数组
        assert_eq!(
            value_to_json(&Value::Json("{\"k\":1}".into())),
            serde_json::json!({"k": 1})
        );
    }

    #[test]
    fn test_json_rows_responder() {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        row.insert("name".to_string(), Value::String("Alice".into()));
        let rows: QueryRows = vec![row];
        // actix-web 4 的 `HttpRequest` 不再实现 `Default`，
        // 通过 `TestRequest::default().to_http_request()` 构造测试请求。
        let req = actix_web::test::TestRequest::default().to_http_request();
        let resp = JsonRows(rows).respond_to(&req);
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    }

    #[test]
    fn test_json_resp_responder() {
        #[derive(Serialize)]
        struct User {
            id: i64,
            name: String,
        }
        let user = User {
            id: 1,
            name: "Bob".into(),
        };
        let req = actix_web::test::TestRequest::default().to_http_request();
        let resp = JsonResp(user).respond_to(&req);
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    }
}

#[cfg(test)]
mod pagination_tests {
    use super::*;

    // --- Pagination tests ---

    #[test]
    fn pagination_new_defaults() {
        let p = Pagination::new(1, 20);
        assert_eq!(p.page(), 1);
        assert_eq!(p.per_page(), 20);
    }

    #[test]
    fn pagination_page_zero_clamped_to_one() {
        let p = Pagination::new(0, 10);
        assert_eq!(p.page(), 1);
    }

    #[test]
    fn pagination_per_page_zero_clamped_to_one() {
        let p = Pagination::new(1, 0);
        assert_eq!(p.per_page(), 1);
    }

    #[test]
    fn pagination_offset() {
        let p = Pagination::new(3, 20);
        assert_eq!(p.offset(), 40);
    }

    #[test]
    fn pagination_limit_equals_per_page() {
        let p = Pagination::new(1, 50);
        assert_eq!(p.limit(), 50);
    }

    #[test]
    fn pagination_total_pages_exact() {
        let p = Pagination::new(1, 10);
        assert_eq!(p.total_pages(100), 10);
    }

    #[test]
    fn pagination_total_pages_with_remainder() {
        let p = Pagination::new(1, 10);
        assert_eq!(p.total_pages(105), 11);
    }

    #[test]
    fn pagination_total_pages_zero() {
        let p = Pagination::new(1, 10);
        assert_eq!(p.total_pages(0), 0);
    }

    #[test]
    fn pagination_has_next_true() {
        let p = Pagination::new(1, 10);
        assert!(p.has_next(100));
    }

    #[test]
    fn pagination_has_next_false() {
        let p = Pagination::new(10, 10);
        assert!(!p.has_next(100));
    }

    #[test]
    fn pagination_has_prev_true() {
        let p = Pagination::new(2, 10);
        assert!(p.has_prev());
    }

    #[test]
    fn pagination_has_prev_false() {
        let p = Pagination::new(1, 10);
        assert!(!p.has_prev());
    }
}

#[cfg(test)]
mod error_response_tests {
    use super::*;

    #[test]
    fn error_response_defaults() {
        let e = ErrorResponse::new();
        assert_eq!(e.code(), 500);
        assert_eq!(e.message(), "Internal Server Error");
    }

    #[test]
    fn error_response_with_code() {
        let e = ErrorResponse::new().with_code(404);
        assert_eq!(e.code(), 404);
    }

    #[test]
    fn error_response_with_message() {
        let e = ErrorResponse::new().with_message("not found");
        assert_eq!(e.message(), "not found");
    }

    #[test]
    fn error_response_builder_chain() {
        let e = ErrorResponse::new()
            .with_code(400)
            .with_message("bad request");
        assert_eq!(e.code(), 400);
        assert_eq!(e.message(), "bad request");
    }

    #[test]
    fn error_response_to_json_string() {
        let e = ErrorResponse::new()
            .with_code(404)
            .with_message("Not Found");
        let json = e.to_json_string();
        assert!(json.contains("404"));
        assert!(json.contains("Not Found"));
    }

    #[test]
    fn error_response_to_http_response_status() {
        let e = ErrorResponse::new().with_code(404);
        let resp = e.to_http_response();
        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
    }

    #[test]
    fn error_response_to_http_response_body() {
        let e = ErrorResponse::new().with_code(400).with_message("bad");
        let resp = e.to_http_response();
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn error_response_invalid_code_fallback() {
        let e = ErrorResponse::new().with_code(99);
        let resp = e.to_http_response();
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

#[cfg(test)]
mod health_check_tests {
    use super::*;

    #[test]
    fn health_check_new_empty() {
        let h = HealthCheck::new();
        assert_eq!(h.check_count(), 0);
        assert!(h.is_healthy());
    }

    #[test]
    fn health_check_add_check() {
        let mut h = HealthCheck::new();
        h.add_check("db", true);
        assert_eq!(h.check_count(), 1);
    }

    #[test]
    fn health_check_all_pass() {
        let mut h = HealthCheck::new();
        h.add_check("db", true);
        h.add_check("cache", true);
        assert!(h.is_healthy());
    }

    #[test]
    fn health_check_some_fail() {
        let mut h = HealthCheck::new();
        h.add_check("db", true);
        h.add_check("cache", false);
        assert!(!h.is_healthy());
    }

    #[test]
    fn health_check_all_fail() {
        let mut h = HealthCheck::new();
        h.add_check("db", false);
        h.add_check("cache", false);
        assert!(!h.is_healthy());
    }
}

#[cfg(test)]
mod route_info_tests {
    use super::*;

    #[test]
    fn route_info_new() {
        let r = RouteInfo::new("/users", "GET", "list_users");
        assert_eq!(r.path(), "/users");
        assert_eq!(r.method(), "GET");
        assert_eq!(r.handler_name(), "list_users");
    }

    #[test]
    fn route_info_path_getter() {
        let r = RouteInfo::new("/api/v1/items", "POST", "create_item");
        assert_eq!(r.path(), "/api/v1/items");
    }

    #[test]
    fn route_info_method_getter() {
        let r = RouteInfo::new("/users", "DELETE", "delete_user");
        assert_eq!(r.method(), "DELETE");
    }

    #[test]
    fn route_info_handler_name_getter() {
        let r = RouteInfo::new("/users", "PUT", "update_user");
        assert_eq!(r.handler_name(), "update_user");
    }

    #[test]
    fn route_info_description() {
        let r = RouteInfo::new("/users", "GET", "list_users");
        let desc = r.description();
        assert!(desc.contains("GET"));
        assert!(desc.contains("/users"));
        assert!(desc.contains("list_users"));
    }
}
