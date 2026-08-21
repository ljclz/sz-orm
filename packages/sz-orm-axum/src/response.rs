//! 响应包装与辅助：统一 API 响应格式、分页响应、错误响应。
//!
//! - [`ResponseWrapper`] — 统一响应包装
//! - [`ApiResponse`] — API 响应
//! - [`PaginatedResponse`] — 分页响应
//! - [`ErrorResponse`] — 标准错误响应
//! - [`ApiError`] — API 错误详情

use std::collections::HashMap;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

// ============================================================================
// ResponseWrapper — 统一响应包装
// ============================================================================

/// 统一响应包装：`{ code, message, data }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseWrapper<T> {
    code: u16,
    message: String,
    data: T,
}

impl<T> ResponseWrapper<T> {
    /// 创建成功响应
    pub fn success(data: T) -> Self {
        Self {
            code: 0,
            message: "success".to_string(),
            data,
        }
    }

    /// 创建带消息的成功响应
    pub fn success_with_message(data: T, message: &str) -> Self {
        Self {
            code: 0,
            message: message.to_string(),
            data,
        }
    }

    /// 创建错误响应
    pub fn error(code: u16, message: &str, data: T) -> Self {
        Self {
            code,
            message: message.to_string(),
            data,
        }
    }

    /// 业务码
    pub fn code(&self) -> u16 {
        self.code
    }

    /// 消息
    pub fn message(&self) -> &str {
        &self.message
    }

    /// 数据引用
    pub fn data(&self) -> &T {
        &self.data
    }

    /// 消费包装，返回内部数据
    pub fn into_data(self) -> T {
        self.data
    }

    /// 是否成功
    pub fn is_success(&self) -> bool {
        self.code == 0
    }

    /// 映射数据类型
    pub fn map<U, F>(self, f: F) -> ResponseWrapper<U>
    where
        F: FnOnce(T) -> U,
    {
        ResponseWrapper {
            code: self.code,
            message: self.message,
            data: f(self.data),
        }
    }

    /// 设置业务码（链式）
    pub fn with_code(mut self, code: u16) -> Self {
        self.code = code;
        self
    }

    /// 设置消息（链式）
    pub fn with_message(mut self, message: &str) -> Self {
        self.message = message.to_string();
        self
    }

    /// 转换为 JSON 字符串
    pub fn to_json_string(&self) -> String
    where
        T: Serialize,
    {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

impl<T: Serialize> IntoResponse for ResponseWrapper<T> {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

// ============================================================================
// ApiStatus — API 响应状态
// ============================================================================

/// API 响应状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ApiStatus {
    #[default]
    Ok,
    Error,
    Partial,
}

impl ApiStatus {
    /// 是否成功
    pub fn is_ok(&self) -> bool {
        matches!(self, ApiStatus::Ok)
    }

    /// 是否失败
    pub fn is_error(&self) -> bool {
        matches!(self, ApiStatus::Error)
    }

    /// 转为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            ApiStatus::Ok => "ok",
            ApiStatus::Error => "error",
            ApiStatus::Partial => "partial",
        }
    }
}

// ============================================================================
// ApiError — API 错误详情
// ============================================================================

/// API 错误详情
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    code: String,
    message: String,
    field: Option<String>,
}

impl ApiError {
    /// 创建错误
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            field: None,
        }
    }

    /// 创建带字段名的错误
    pub fn with_field(code: &str, message: &str, field: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            field: Some(field.to_string()),
        }
    }

    /// 错误码
    pub fn code(&self) -> &str {
        &self.code
    }

    /// 错误消息
    pub fn message(&self) -> &str {
        &self.message
    }

    /// 关联字段
    pub fn field(&self) -> Option<&str> {
        self.field.as_deref()
    }
}

// ============================================================================
// ApiResponse — API 响应
// ============================================================================

/// API response with status, data, errors, and meta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    status: ApiStatus,
    data: Option<T>,
    errors: Vec<ApiError>,
    meta: HashMap<String, String>,
}

impl<T> ApiResponse<T> {
    /// 创建成功响应
    pub fn ok(data: T) -> Self {
        Self {
            status: ApiStatus::Ok,
            data: Some(data),
            errors: vec![],
            meta: HashMap::new(),
        }
    }

    /// 创建失败响应
    pub fn error(errors: Vec<ApiError>) -> Self {
        Self {
            status: ApiStatus::Error,
            data: None,
            errors,
            meta: HashMap::new(),
        }
    }

    /// 创建单错误响应
    pub fn error_one(code: &str, message: &str) -> Self {
        Self::error(vec![ApiError::new(code, message)])
    }

    /// 创建部分成功响应
    pub fn partial(data: T, errors: Vec<ApiError>) -> Self {
        Self {
            status: ApiStatus::Partial,
            data: Some(data),
            errors,
            meta: HashMap::new(),
        }
    }

    /// 状态
    pub fn status(&self) -> &ApiStatus {
        &self.status
    }

    /// 数据引用
    pub fn data(&self) -> Option<&T> {
        self.data.as_ref()
    }

    /// 错误列表
    pub fn errors(&self) -> &[ApiError] {
        &self.errors
    }

    /// 错误数
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// 是否有错误
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// 添加元-信息（链式）
    pub fn with_meta(mut self, key: &str, value: &str) -> Self {
        self.meta.insert(key.to_string(), value.to_string());
        self
    }

    /// 获取元信息
    pub fn meta(&self, key: &str) -> Option<&str> {
        self.meta.get(key).map(|s| s.as_str())
    }

    /// 元信息条数
    pub fn meta_count(&self) -> usize {
        self.meta.len()
    }

    /// 映射数据类型
    pub fn map<U, F>(self, f: F) -> ApiResponse<U>
    where
        F: FnOnce(T) -> U,
    {
        ApiResponse {
            status: self.status,
            data: self.data.map(f),
            errors: self.errors,
            meta: self.meta,
        }
    }
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> Response {
        match serde_json::to_value(&self) {
            Ok(v) => Json(v).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("ApiResponse serialization failed: {}", e),
            )
                .into_response(),
        }
    }
}

// ============================================================================
// ErrorResponse — 标准错误响应
// ============================================================================

/// 标准错误响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    code: u16,
    message: String,
    details: Option<String>,
}

impl Default for ErrorResponse {
    fn default() -> Self {
        Self {
            code: 500,
            message: "Internal Server Error".to_string(),
            details: None,
        }
    }
}

impl ErrorResponse {
    /// 创建默认错误响应
    pub fn new() -> Self {
        Self::default()
    }

    ///* 设置 HTTP 状态码（链式）
    pub fn with_code(mut self, code: u16) -> Self {
        self.code = code;
        self
    }

    /// 设置错误消息（链式）
    pub fn with_message(mut self, msg: &str) -> Self {
        self.message = msg.to_string();
        self
    }

    /// 设置详情（链式）
    pub fn with_details(mut self, details: &str) -> Self {
        self.details = Some(details.to_string());
        self
    }

    /// 状态码
    pub fn code(&self) -> u16 {
        self.code
    }

    /// 错误消息
    pub fn message(&self) -> &str {
        &self.message
    }

    /// 详情
    pub fn details(&self) -> Option<&str> {
        self.details.as_deref()
    }

    /// 转换为 StatusCode
    pub fn status_code(&self) -> StatusCode {
        StatusCode::from_u16(self.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }

    /// JSON 字符串表示
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        let status = self.status_code();
        (status, Json(self)).into_response()
    }
}

// ============================================================================
// PaginatedResponse — 分页响应
// ============================================================================

/// 分页元信息
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaginationMeta {
    page: u64,
    per_page: u64,
    total: u64,
    total_pages: u64,
}

impl PaginationMeta {
    /// 创建分页元信息
    pub fn new(page: u64, per_page: u64, total: u64) -> Self {
        let page = page.max(1);
        let per_page = per_page.max(1);
        let total_pages = if total == 0 {
            0
        } else {
            total.div_ceil(per_page)
        };
        Self {
            page,
            per_page,
            total,
            total_pages,
        }
    }

    /// 当前页
    pub fn page(&self) -> u64 {
        self.page
    }

    /// 每页条数
    pub fn per_page(&self) -> u64 {
        self.per_page
    }

    /// 总条数
    pub fn total(&self) -> u64 {
        self.total
    }

    /// 总页数
    pub fn total_pages(&self) -> u64 {
        self.total_pages
    }

    /// 是否有下一页
    pub fn has_next(&self) -> bool {
        self.page < self.total_pages
    }

    /// 是否有上一页
    pub fn has_prev(&self) -> bool {
        self.page > 1
    }

    /// SQL OFFSET
    pub fn offset(&self) -> u64 {
        (self.page - 1) * self.per_page
    }

    /// SQL LIMIT
    pub fn limit(&self) -> u64 {
        self.per_page
    }
}

/// 分页响应：`{ items, pagination }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    items: Vec<T>,
    pagination: PaginationMeta,
}

impl<T> PaginatedResponse<T> {
    /// 创建分页响应
    pub fn new(items: Vec<T>, page: u64, per_page: u64, total: u64) -> Self {
        Self {
            items,
            pagination: PaginationMeta::new(page, per_page, total),
        }
    }

    /// 从全部数据创建分页响应（自动切片）
    pub fn from_all(all: Vec<T>, page: u64, per_page: u64) -> Self {
        let total = all.len() as u64;
        let meta = PaginationMeta::new(page, per_page, total);
        let offset = meta.offset() as usize;
        let limit = meta.limit() as usize;
        let items: Vec<T> = all.into_iter().skip(offset).take(limit).collect();
        Self {
            items,
            pagination: meta,
        }
    }

    /// 数据项
    pub fn items(&self) -> &[T] {
        &self.items
    }

    /// 数据条数
    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    /// 分页元信息
    pub fn pagination(&self) -> &PaginationMeta {
        &self.pagination
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 消费响应，返回 items
    pub fn into_items(self) -> Vec<T> {
        self.items
    }

    /// 映射 items 类型
    pub fn map<U, F>(self, f: F) -> PaginatedResponse<U>
    where
        F: FnMut(T) -> U,
    {
        PaginatedResponse {
            items: self.items.into_iter().map(f).collect(),
            pagination: self.pagination,
        }
    }
}

impl<T: Serialize> IntoResponse for PaginatedResponse<T> {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

// ============================================================================
// HealthStatus — 健康状态
// ============================================================================

/// 健康检查状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
    Degraded,
}

impl HealthStatus {
    /// 是否健康
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy)
    }

    /// 是否不健康
    pub fn is_unhealthy(&self) -> bool {
        matches!(self, HealthStatus::Unhealthy)
    }

    /// 是否降级
    pub fn is_degraded(&self) -> bool {
        matches!(self, HealthStatus::Degraded)
    }

    /// 转为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            HealthStatus::Healthy => "healthy",
            HealthStatus::Unhealthy => "unhealthy",
            HealthStatus::Degraded => "degraded",
        }
    }
}

/// 健康检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    status: HealthStatus,
    checks: Vec<(String, bool)>,
    version: String,
    uptime_seconds: u64,
}

impl Default for HealthCheck {
    fn default() -> Self {
        Self {
            status: HealthStatus::Healthy,
            checks: vec![],
            version: "unknown".to_string(),
            uptime_seconds: 0,
        }
    }
}

impl HealthCheck {
    /// 创建健康检查
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加检查结果
    pub fn add_check(&mut self, name: &str, passed: bool) {
        self.checks.push((name.to_string(), passed));
        self.status = if self.checks.iter().all(|(_, p)| *p) {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy
        };
    }

    /// 设置版本（链式）
    pub fn version(mut self, v: &str) -> Self {
        self.version = v.to_string();
        self
    }

    /// 设置运行时间（链式）
    pub fn uptime(mut self, seconds: u64) -> Self {
        self.uptime_seconds = seconds;
        self
    }

    /// 状态
    pub fn status(&self) -> &HealthStatus {
        &self.status
    }

    /// 检查项数
    pub fn check_count(&self) -> usize {
        self.checks.len()
    }

    /// 是否健康
    pub fn is_healthy(&self) -> bool {
        self.status.is_healthy()
    }

    /// 版本
    pub fn version_value(&self) -> &str {
        &self.version
    }

    /// 运行时间
    pub fn uptime_seconds(&self) -> u64 {
        self.uptime_seconds
    }
}

impl IntoResponse for HealthCheck {
    fn into_response(self) -> Response {
        let status = if self.is_healthy() {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        };
        (status, Json(self)).into_response()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ----- ResponseWrapper -----

    #[test]
    fn response_wrapper_success() {
        let r = ResponseWrapper::success(42);
        assert!(r.is_success());
        assert_eq!(r.code(), 0);
        assert_eq!(r.message(), "success");
        assert_eq!(*r.data(), 42);
    }

    #[test]
    fn response_wrapper_success_with_message() {
        let r = ResponseWrapper::success_with_message(42, "created");
        assert!(r.is_success());
        assert_eq!(r.message(), "created");
    }

    #[test]
    fn response_wrapper_error() {
        let r = ResponseWrapper::error(1001, "not found", ());
        assert!(!r.is_success());
        assert_eq!(r.code(), 1001);
    }

    #[test]
    fn response_wrapper_with_code() {
        let r = ResponseWrapper::success(1).with_code(100);
        assert_eq!(r.code(), 100);
    }

    #[test]
    fn response_wrapper_with_message() {
        let r = ResponseWrapper::success(1).with_message("ok");
        assert_eq!(r.message(), "ok");
    }

    #[test]
    fn response_wrapper_into_data() {
        let r = ResponseWrapper::success(42);
        assert_eq!(r.into_data(), 42);
    }

    #[test]
    fn response_wrapper_map() {
        let r = ResponseWrapper::success(5);
        let r2 = r.map(|x| x * 2);
        assert_eq!(*r2.data(), 10);
    }

    #[test]
    fn response_wrapper_to_json_string() {
        let r = ResponseWrapper::success(42);
        let json = r.to_json_string();
        assert!(json.contains("42"));
    }

    #[test]
    fn response_wrapper_into_response() {
        let r = ResponseWrapper::success(42);
        let resp = r.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ----- ApiStatus -----

    #[test]
    fn api_status_is_ok() {
        assert!(ApiStatus::Ok.is_ok());
        assert!(!ApiStatus::Error.is_ok());
    }

    #[test]
    fn api_status_is_error() {
        assert!(ApiStatus::Error.is_error());
        assert!(!ApiStatus::Ok.is_error());
    }

    #[test]
    fn api_status_as_str() {
        assert_eq!(ApiStatus::Ok.as_str(), "ok");
        assert_eq!(ApiStatus::Error.as_str(), "error");
        assert_eq!(ApiStatus::Partial.as_str(), "partial");
    }

    // ----- ApiError -----

    #[test]
    fn api_error_new() {
        let e = ApiError::new("NOT_FOUND", "resource not found");
        assert_eq!(e.code(), "NOT_FOUND");
        assert_eq!(e.message(), "resource not found");
        assert!(e.field().is_none());
    }

    #[test]
    fn api_error_with_field() {
        let e = ApiError::with_field("INVALID", "too short", "name");
        assert_eq!(e.field(), Some("name"));
    }

    // ----- ApiResponse -----

    #[test]
    fn api_response_ok() {
        let r = ApiResponse::ok(42);
        assert!(r.status().is_ok());
        assert_eq!(r.data(), Some(&42));
        assert!(!r.has_errors());
    }

    #[test]
    fn api_response_error() {
        let r = ApiResponse::<()>::error(vec![ApiError::new("ERR", "fail")]);
        assert!(r.status().is_error());
        assert!(r.data().is_none());
        assert!(r.has_errors());
    }

    #[test]
    fn api_response_error_one() {
        let r = ApiResponse::<()>::error_one("ERR", "fail");
        assert_eq!(r.error_count(), 1);
    }

    #[test]
    fn api_response_partial() {
        let r = ApiResponse::partial(42, vec![ApiError::new("WARN", "partial")]);
        assert_eq!(r.status(), &ApiStatus::Partial);
        assert_eq!(r.data(), Some(&42));
    }

    #[test]
    fn api_response_with_meta() {
        let r = ApiResponse::ok(1).with_meta("duration", "50ms");
        assert_eq!(r.meta("duration"), Some("50ms"));
    }

    #[test]
    fn api_response_map() {
        let r = ApiResponse::ok(5);
        let r2 = r.map(|x| x + 1);
        assert_eq!(r2.data(), Some(&6));
    }

    #[test]
    fn api_response_into_response() {
        let r = ApiResponse::ok(42);
        let resp = r.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ----- ErrorResponse -----

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
    fn error_response_with_details() {
        let e = ErrorResponse::new().with_details("missing field: name");
        assert_eq!(e.details(), Some("missing field: name"));
    }

    #[test]
    fn error_response_status_code() {
        let e = ErrorResponse::new().with_code(404);
        assert_eq!(e.status_code(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn error_response_invalid_code_fallback() {
        let e = ErrorResponse::new().with_code(99);
        assert_eq!(e.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn error_response_into_response() {
        let e = ErrorResponse::new().with_code(404);
        let resp = e.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn error_response_to_json_string() {
        let e = ErrorResponse::new()
            .with_code(404)
            .with_message("Not Found");
        let json = e.to_json_string();
        assert!(json.contains("404"));
    }

    // ----- PaginationMeta -----

    #[test]
    fn pagination_meta_basic() {
        let m = PaginationMeta::new(1, 10, 100);
        assert_eq!(m.page(), 1);
        assert_eq!(m.per_page(), 10);
        assert_eq!(m.total(), 100);
        assert_eq!(m.total_pages(), 10);
    }

    #[test]
    fn pagination_meta_remainder() {
        let m = PaginationMeta::new(1, 10, 105);
        assert_eq!(m.total_pages(), 11);
    }

    #[test]
    fn pagination_meta_zero_total() {
        let m = PaginationMeta::new(1, 10, 0);
        assert_eq!(m.total_pages(), 0);
    }

    #[test]
    fn pagination_meta_has_next() {
        let m = PaginationMeta::new(1, 10, 100);
        assert!(m.has_next());
    }

    #[test]
    fn pagination_meta_has_prev() {
        let m = PaginationMeta::new(2, 10, 100);
        assert!(m.has_prev());
    }

    #[test]
    fn pagination_meta_offset_limit() {
        let m = PaginationMeta::new(3, 20, 100);
        assert_eq!(m.offset(), 40);
        assert_eq!(m.limit(), 20);
    }

    // ----- PaginatedResponse -----

    #[test]
    fn paginated_response_new() {
        let r = PaginatedResponse::new(vec![1, 2, 3], 1, 10, 3);
        assert_eq!(r.item_count(), 3);
        assert!(!r.is_empty());
    }

    #[test]
    fn paginated_response_from_all() {
        let all = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let r = PaginatedResponse::from_all(all, 2, 3);
        assert_eq!(r.item_count(), 3);
        assert_eq!(r.items(), &[4, 5, 6]);
    }

    #[test]
    fn paginated_response_from_all_last_page() {
        let all = vec![1, 2, 3, 4, 5];
        let r = PaginatedResponse::from_all(all, 2, 3);
        assert_eq!(r.item_count(), 2);
        assert_eq!(r.items(), &[4, 5]);
    }

    #[test]
    fn paginated_response_into_items() {
        let r = PaginatedResponse::new(vec![1, 2], 1, 10, 2);
        assert_eq!(r.into_items(), vec![1, 2]);
    }

    #[test]
    fn paginated_response_map() {
        let r = PaginatedResponse::new(vec![1, 2, 3], 1, 10, 3);
        let r2 = r.map(|x| x * 10);
        assert_eq!(r2.items(), &[10, 20, 30]);
    }

    #[test]
    fn paginated_response_into_response() {
        let r = PaginatedResponse::new(vec![1, 2], 1, 10, 2);
        let resp = r.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ----- HealthStatus -----

    #[test]
    fn health_status_is_healthy() {
        assert!(HealthStatus::Healthy.is_healthy());
        assert!(!HealthStatus::Unhealthy.is_healthy());
    }

    #[test]
    fn health_status_as_str() {
        assert_eq!(HealthStatus::Healthy.as_str(), "healthy");
        assert_eq!(HealthStatus::Unhealthy.as_str(), "unhealthy");
        assert_eq!(HealthStatus::Degraded.as_str(), "degraded");
    }

    // ----- HealthCheck -----

    #[test]
    fn health_check_new() {
        let h = HealthCheck::new();
        assert!(h.is_healthy());
        assert_eq!(h.check_count(), 0);
    }

    #[test]
    fn health_check_add_check_pass() {
        let mut h = HealthCheck::new();
        h.add_check("db", true);
        assert!(h.is_healthy());
    }

    #[test]
    fn health_check_add_check_fail() {
        let mut h = HealthCheck::new();
        h.add_check("db", false);
        assert!(!h.is_healthy());
    }

    #[test]
    fn health_check_version() {
        let h = HealthCheck::new().version("1.0.0");
        assert_eq!(h.version_value(), "1.0.0");
    }

    #[test]
    fn health_check_uptime() {
        let h = HealthCheck::new().uptime(3600);
        assert_eq!(h.uptime_seconds(), 3600);
    }

    #[test]
    fn health_check_into_response_healthy() {
        let h = HealthCheck::new();
        let resp = h.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn health_check_into_response_unhealthy() {
        let mut h = HealthCheck::new();
        h.add_check("db", false);
        let resp = h.into_response();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
