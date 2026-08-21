//! 响应包装与辅助：统一 API 响应格式、分页响应、响应头配置。
//!
//! - [`ResponseWrapper`] — 统一响应包装（data + message + code）
//! - [`ApiResponse`] — API 响应（状态 + 数据 + 错误）
//! - [`PaginatedResponse`] — 分页响应（items + 分页元信息）
//! - [`PaginationMeta`] — 分页元信息
//! - [`PaginationExtractor`] — 从查询参数提取分页
//! - [`ResponseHeaders`] — 响应头配置
//! - [`CacheControlConfig`] — 缓存控制配置
//! - [`RateLimitConfig`] — 速率限制配置
//! - [`MiddlewareChainConfig`] — 中间件链配置

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ============================================================================
// ResponseWrapper — 统一响应包装
// ============================================================================

/// 统一响应包装：`{ code, message, data }`
///
/// 适用于大多数 API 返回场景，`code` 为业务码（非 HTTP 状态码），
/// `message` 为人类可读消息，`data` 为实际负载。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseWrapper<T> {
    code: u16,
    message: String,
    data: T,
}

impl<T> ResponseWrapper<T> {
    /// 创建成功响应（code=0, message="success"）
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

    /// 创建错误响应（code != 0）
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

    /// 是否成功（code == 0）
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

// ============================================================================
// ApiResponse — API 响应
// ============================================================================

/// API 响应状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ApiStatus {
    /// 成功
    #[default]
    Ok,
    /// 失败
    Error,
    /// 部分成功
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

/// API 响应：`{ status, data, errors, meta }`
///
/// `data` 为可选（失败时可能无数据），`errors` 为错误详情列表，
/// `meta` 为附加元信息（如耗时、请求 ID 等）。
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

    /// 添加元信息（链式）
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

    /// 转换为 JSON 字符串
    pub fn to_json_string(&self) -> String
    where
        T: Serialize,
    {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

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
// PaginationMeta — 分页元信息
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

    /// 起始序号（1-based）
    pub fn from_index(&self) -> u64 {
        if self.total == 0 {
            0
        } else {
            self.offset() + 1
        }
    }

    /// 结束序号
    pub fn to_index(&self) -> u64 {
        if self.total == 0 {
            0
        } else {
            (self.offset() + self.per_page).min(self.total)
        }
    }
}

// ============================================================================
// PaginatedResponse — 分页响应
// ============================================================================

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

    /// 数据条数（当前页）
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

    /// 转换为 JSON 字符串
    pub fn to_json_string(&self) -> String
    where
        T: Serialize,
    {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

// ============================================================================
// PaginationExtractor — 从查询参数提取分页
// ============================================================================

/// 分页提取结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaginationExtractor {
    page: u64,
    per_page: u64,
    max_per_page: u64,
}

impl Default for PaginationExtractor {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 20,
            max_per_page: 100,
        }
    }
}

impl PaginationExtractor {
    /// 创建分页提取器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置默认每页条数（链式）
    pub fn with_default_per_page(mut self, n: u64) -> Self {
        self.per_page = n.max(1);
        self
    }

    /// 设置最大每页条数（链式）
    pub fn with_max_per_page(mut self, n: u64) -> Self {
        self.max_per_page = n.max(1);
        self
    }

    /// 默认每页条数
    pub fn default_per_page(&self) -> u64 {
        self.per_page
    }

    /// 最大每页条数
    pub fn max_per_page(&self) -> u64 {
        self.max_per_page
    }

    /// 从 HashMap 查询参数提取分页
    ///
    /// 支持 `page` 和 `per_page` 键。值无效时使用默认值。
    pub fn extract(&self, params: &HashMap<String, String>) -> (u64, u64) {
        let page = params
            .get("page")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(1)
            .max(1);
        let per_page = params
            .get("per_page")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(self.per_page)
            .clamp(1, self.max_per_page);
        (page, per_page)
    }

    /// 从键值对列表提取分页
    pub fn extract_pairs(&self, pairs: &[(&str, &str)]) -> (u64, u64) {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        self.extract(&map)
    }

    /// 从单个参数提取分页
    pub fn extract_raw(&self, page: Option<&str>, per_page: Option<&str>) -> (u64, u64) {
        let p = page.and_then(|s| s.parse::<u64>().ok()).unwrap_or(1).max(1);
        let pp = per_page
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(self.per_page)
            .clamp(1, self.max_per_page);
        (p, pp)
    }
}

// ============================================================================
// ResponseHeaders — 响应头配置
// ============================================================================

/// 响应头配置：自定义头 + 安全头
#[derive(Debug, Clone, Default)]
pub struct ResponseHeaders {
    custom: HashMap<String, String>,
    security: SecurityHeaders,
}

/// 安全响应头
#[derive(Debug, Clone, Default)]
pub struct SecurityHeaders {
    x_content_type_options: bool,
    x_frame_options: Option<String>,
    x_xss_protection: bool,
    strict_transport_security: Option<String>,
    content_security_policy: Option<String>,
    referrer_policy: Option<String>,
}

impl SecurityHeaders {
    /// 创建默认安全头配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 启用 X-Content-Type-Options: nosniff（链式）
    pub fn nosniff(mut self) -> Self {
        self.x_content_type_options = true;
        self
    }

    /// 设置 X-Frame-Options（链式）
    pub fn frame_options(mut self, value: &str) -> Self {
        self.x_frame_options = Some(value.to_string());
        self
    }

    /// 启用 X-XSS-Protection: 1; mode=block（链式）
    pub fn xss_protection(mut self) -> Self {
        self.x_xss_protection = true;
        self
    }

    /// 设置 Strict-Transport-Security（链式）
    pub fn hsts(mut self, value: &str) -> Self {
        self.strict_transport_security = Some(value.to_string());
        self
    }

    /// 设置 Content-Security-Policy（链式）
    pub fn csp(mut self, policy: &str) -> Self {
        self.content_security_policy = Some(policy.to_string());
        self
    }

    /// 设置 Referrer-Policy（链式）
    pub fn referrer_policy(mut self, policy: &str) -> Self {
        self.referrer_policy = Some(policy.to_string());
        self
    }

    /// 是否启用 nosniff
    pub fn has_nosniff(&self) -> bool {
        self.x_content_type_options
    }

    /// X-Frame-Options 值
    pub fn frame_options_value(&self) -> Option<&str> {
        self.x_frame_options.as_deref()
    }

    /// 是否启用 XSS 保护
    pub fn has_xss_protection(&self) -> bool {
        self.x_xss_protection
    }

    /// HSTS 值
    pub fn hsts_value(&self) -> Option<&str> {
        self.strict_transport_security.as_deref()
    }

    /// CSP 值
    pub fn csp_value(&self) -> Option<&str> {
        self.content_security_policy.as_deref()
    }

    /// Referrer-Policy 值
    pub fn referrer_policy_value(&self) -> Option<&str> {
        self.referrer_policy.as_deref()
    }

    /// 已配置的安全头数量
    pub fn header_count(&self) -> usize {
        let mut count = 0;
        if self.x_content_type_options {
            count += 1;
        }
        if self.x_frame_options.is_some() {
            count += 1;
        }
        if self.x_xss_protection {
            count += 1;
        }
        if self.strict_transport_security.is_some() {
            count += 1;
        }
        if self.content_security_policy.is_some() {
            count += 1;
        }
        if self.referrer_policy.is_some() {
            count += 1;
        }
        count
    }

    /// 转换为 header 键值对列表
    pub fn to_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![];
        if self.x_content_type_options {
            headers.push(("X-Content-Type-Options".to_string(), "nosniff".to_string()));
        }
        if let Some(ref v) = self.x_frame_options {
            headers.push(("X-Frame-Options".to_string(), v.clone()));
        }
        if self.x_xss_protection {
            headers.push(("X-XSS-Protection".to_string(), "1; mode=block".to_string()));
        }
        if let Some(ref v) = self.strict_transport_security {
            headers.push(("Strict-Transport-Security".to_string(), v.clone()));
        }
        if let Some(ref v) = self.content_security_policy {
            headers.push(("Content-Security-Policy".to_string(), v.clone()));
        }
        if let Some(ref v) = self.referrer_policy {
            headers.push(("Referrer-Policy".to_string(), v.clone()));
        }
        headers
    }
}

impl ResponseHeaders {
    /// 创建空响应头配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置安全头（链式）
    pub fn with_security(mut self, security: SecurityHeaders) -> Self {
        self.security = security;
        self
    }

    /// 添加自定义头（链式）
    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.custom.insert(key.to_string(), value.to_string());
        self
    }

    /// 获取自定义头
    pub fn get(&self, key: &str) -> Option<&str> {
        self.custom.get(key).map(|s| s.as_str())
    }

    /// 自定义头数量
    pub fn custom_count(&self) -> usize {
        self.custom.len()
    }

    /// 安全头引用
    pub fn security(&self) -> &SecurityHeaders {
        &self.security
    }

    /// 总头数量（自定义 + 安全）
    pub fn total_count(&self) -> usize {
        self.custom.len() + self.security.header_count()
    }

    /// 转换为所有 header 键值对列表
    pub fn to_all_headers(&self) -> Vec<(String, String)> {
        let mut headers: Vec<(String, String)> = self
            .custom
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        headers.extend(self.security.to_headers());
        headers
    }
}

// ============================================================================
// CacheControlConfig — 缓存控制配置
// ============================================================================

/// 缓存策略
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheDirective {
    /// 不缓存
    NoCache,
    /// 不存储
    NoStore,
    /// 公共缓存，指定 max-age（秒）
    Public(u64),
    /// 私有缓存，指定 max-age（秒）
    Private(u64),
    /// 仅 max-age
    MaxAge(u64),
}

impl CacheDirective {
    /// 转换为 Cache-Control 头值
    pub fn to_header_value(&self) -> String {
        match self {
            CacheDirective::NoCache => "no-cache".to_string(),
            CacheDirective::NoStore => "no-store".to_string(),
            CacheDirective::Public(max_age) => format!("public, max-age={}", max_age),
            CacheDirective::Private(max_age) => format!("private, max-age={}", max_age),
            CacheDirective::MaxAge(max_age) => format!("max-age={}", max_age),
        }
    }

    /// 是否禁止缓存
    pub fn is_no_cache(&self) -> bool {
        matches!(self, CacheDirective::NoCache | CacheDirective::NoStore)
    }

    /// max-age 值（若存在）
    pub fn max_age(&self) -> Option<u64> {
        match self {
            CacheDirective::Public(n) | CacheDirective::Private(n) | CacheDirective::MaxAge(n) => {
                Some(*n)
            }
            _ => None,
        }
    }
}

/// 缓存控制配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControlConfig {
    directive: CacheDirective,
    s_max_age: Option<u64>,
    stale_while_revalidate: Option<u64>,
    must_revalidate: bool,
    immutable: bool,
}

impl Default for CacheControlConfig {
    fn default() -> Self {
        Self {
            directive: CacheDirective::NoCache,
            s_max_age: None,
            stale_while_revalidate: None,
            must_revalidate: false,
            immutable: false,
        }
    }
}

impl CacheControlConfig {
    /// 创建默认配置（no-cache）
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置缓存指令（链式）
    pub fn directive(mut self, directive: CacheDirective) -> Self {
        self.directive = directive;
        self
    }

    /// 设置共享缓存 max-age（链式）
    pub fn s_max_age(mut self, n: u64) -> Self {
        self.s_max_age = Some(n);
        self
    }

    /// 设置 stale-while-revalidate（链式）
    pub fn stale_while_revalidate(mut self, n: u64) -> Self {
        self.stale_while_revalidate = Some(n);
        self
    }

    /// 启用 must-revalidate（链式）
    pub fn must_revalidate(mut self) -> Self {
        self.must_revalidate = true;
        self
    }

    /// 启用 immutable（链式）
    pub fn immutable(mut self) -> Self {
        self.immutable = true;
        self
    }

    /// 缓存指令
    pub fn directive_value(&self) -> &CacheDirective {
        &self.directive
    }

    /// 是否禁止缓存
    pub fn is_no_cache(&self) -> bool {
        self.directive.is_no_cache()
    }

    /// 生成 Cache-Control 头值
    pub fn to_header_value(&self) -> String {
        let mut parts = vec![self.directive.to_header_value()];
        if let Some(n) = self.s_max_age {
            parts.push(format!("s-maxage={}", n));
        }
        if let Some(n) = self.stale_while_revalidate {
            parts.push(format!("stale-while-revalidate={}", n));
        }
        if self.must_revalidate {
            parts.push("must-revalidate".to_string());
        }
        if self.immutable {
            parts.push("immutable".to_string());
        }
        parts.join(", ")
    }
}

// ============================================================================
// RateLimitConfig — 速率限制配置
// ============================================================================

/// 速率限制策略
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RateLimitStrategy {
    /// 固定窗口
    FixedWindow,
    /// 滑动窗口
    SlidingWindow,
    /// 令牌桶
    TokenBucket,
    /// 漏桶
    LeakyBucket,
}

impl RateLimitStrategy {
    /// 策略名
    pub fn name(&self) -> &'static str {
        match self {
            RateLimitStrategy::FixedWindow => "fixed_window",
            RateLimitStrategy::SlidingWindow => "sliding_window",
            RateLimitStrategy::TokenBucket => "token_bucket",
            RateLimitStrategy::LeakyBucket => "leaky_bucket",
        }
    }
}

/// 速率限制配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    strategy: RateLimitStrategy,
    max_requests: u64,
    window_seconds: u64,
    key: RateLimitKey,
    message: String,
}

/// 限流键类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RateLimitKey {
    /// 按 IP
    Ip,
    /// 按用户 ID
    UserId,
    /// 按 API Key
    ApiKey,
    /// 自定义
    Custom(String),
}

impl RateLimitKey {
    /// 键名
    pub fn name(&self) -> String {
        match self {
            RateLimitKey::Ip => "ip".to_string(),
            RateLimitKey::UserId => "user_id".to_string(),
            RateLimitKey::ApiKey => "api_key".to_string(),
            RateLimitKey::Custom(s) => s.clone(),
        }
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            strategy: RateLimitStrategy::FixedWindow,
            max_requests: 100,
            window_seconds: 60,
            key: RateLimitKey::Ip,
            message: "Rate limit exceeded".to_string(),
        }
    }
}

impl RateLimitConfig {
    /// 创建默认配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置策略（链式）
    pub fn strategy(mut self, strategy: RateLimitStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// 设置最大请求数（链式）
    pub fn max_requests(mut self, n: u64) -> Self {
        self.max_requests = n;
        self
    }

    /// 设置窗口大小（秒）（链式）
    pub fn window_seconds(mut self, n: u64) -> Self {
        self.window_seconds = n;
        self
    }

    /// 设置限流键（链式）
    pub fn key(mut self, key: RateLimitKey) -> Self {
        self.key = key;
        self
    }

    /// 设置错误消息（链式）
    pub fn message(mut self, msg: &str) -> Self {
        self.message = msg.to_string();
        self
    }

    /// 策略
    pub fn strategy_value(&self) -> &RateLimitStrategy {
        &self.strategy
    }

    /// 最大请求数
    pub fn max_requests_value(&self) -> u64 {
        self.max_requests
    }

    /// 窗口大小（秒）
    pub fn window_seconds_value(&self) -> u64 {
        self.window_seconds
    }

    /// 限流键
    pub fn key_value(&self) -> &RateLimitKey {
        &self.key
    }

    /// 错误消息
    pub fn message_value(&self) -> &str {
        &self.message
    }

    /// 每秒请求数
    pub fn requests_per_second(&self) -> f64 {
        if self.window_seconds == 0 {
            0.0
        } else {
            self.max_requests as f64 / self.window_seconds as f64
        }
    }
}

// ============================================================================
// MiddlewareChainConfig — 中间件链配置
// ============================================================================

/// 中间件优先级
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum MiddlewarePriority {
    /// 最低（最先执行）
    Lowest,
    /// 低
    Low,
    /// 普通
    #[default]
    Normal,
    /// 高
    High,
    /// 最高（最后执行）
    Highest,
}

impl MiddlewarePriority {
    /// 数值（用于排序）
    pub fn value(&self) -> u8 {
        match self {
            MiddlewarePriority::Lowest => 0,
            MiddlewarePriority::Low => 1,
            MiddlewarePriority::Normal => 2,
            MiddlewarePriority::High => 3,
            MiddlewarePriority::Highest => 4,
        }
    }
}

/// 中间件配置项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiddlewareEntry {
    name: String,
    priority: MiddlewarePriority,
    enabled: bool,
    config: HashMap<String, String>,
}

impl MiddlewareEntry {
    /// 创建中间件配置项
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            priority: MiddlewarePriority::default(),
            enabled: true,
            config: HashMap::new(),
        }
    }

    /// 设置优先级（链式）
    pub fn priority(mut self, priority: MiddlewarePriority) -> Self {
        self.priority = priority;
        self
    }

    /// 禁用（链式）
    pub fn disable(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// 启用（链式）
    pub fn enable(mut self) -> Self {
        self.enabled = true;
        self
    }

    /// 添加配置项（链式）
    pub fn config(mut self, key: &str, value: &str) -> Self {
        self.config.insert(key.to_string(), value.to_string());
        self
    }

    /// 名称
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 优先级
    pub fn priority_value(&self) -> &MiddlewarePriority {
        &self.priority
    }

    /// 是否启用
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// 配置项
    pub fn config_value(&self, key: &str) -> Option<&str> {
        self.config.get(key).map(|s| s.as_str())
    }

    /// 配置项数
    pub fn config_count(&self) -> usize {
        self.config.len()
    }
}

/// 中间件链配置
#[derive(Debug, Clone, Default)]
pub struct MiddlewareChainConfig {
    entries: Vec<MiddlewareEntry>,
}

impl MiddlewareChainConfig {
    /// 创建空配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加中间件（链式）
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, entry: MiddlewareEntry) -> Self {
        self.entries.push(entry);
        self
    }

    /// 按名称移除中间件
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.name() != name);
        self.entries.len() != before
    }

    /// 按名称查找
    pub fn find(&self, name: &str) -> Option<&MiddlewareEntry> {
        self.entries.iter().find(|e| e.name() == name)
    }

    /// 启用指定中间件
    pub fn enable(&mut self, name: &str) -> bool {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.name() == name) {
            entry.enabled = true;
            true
        } else {
            false
        }
    }

    /// 禁用指定中间件
    pub fn disable(&mut self, name: &str) -> bool {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.name() == name) {
            entry.enabled = false;
            true
        } else {
            false
        }
    }

    /// 中间件总数
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// 启用的中间件数
    pub fn enabled_count(&self) -> usize {
        self.entries.iter().filter(|e| e.is_enabled()).count()
    }

    /// 按优先级排序的启用中间件名称列表
    ///
    /// 优先级越高越靠后（越接近 handler）。
    pub fn ordered_names(&self) -> Vec<String> {
        let mut enabled: Vec<&MiddlewareEntry> =
            self.entries.iter().filter(|e| e.is_enabled()).collect();
        enabled.sort_by_key(|e| e.priority_value().value());
        enabled.iter().map(|e| e.name().to_string()).collect()
    }

    /// 按优先级排序的启用中间件（从高到低）
    pub fn ordered_desc(&self) -> Vec<&MiddlewareEntry> {
        let mut enabled: Vec<&MiddlewareEntry> =
            self.entries.iter().filter(|e| e.is_enabled()).collect();
        enabled.sort_by_key(|e| std::cmp::Reverse(e.priority_value().value()));
        enabled
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
        assert!(json.contains("success"));
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
        assert_eq!(r.error_count(), 1);
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
        assert!(r.has_errors());
    }

    #[test]
    fn api_response_with_meta() {
        let r = ApiResponse::ok(1).with_meta("duration", "50ms");
        assert_eq!(r.meta("duration"), Some("50ms"));
        assert_eq!(r.meta_count(), 1);
    }

    #[test]
    fn api_response_meta_missing() {
        let r = ApiResponse::ok(1);
        assert!(r.meta("missing").is_none());
    }

    #[test]
    fn api_response_map() {
        let r = ApiResponse::ok(5);
        let r2 = r.map(|x| x + 1);
        assert_eq!(r2.data(), Some(&6));
    }

    #[test]
    fn api_response_to_json_string() {
        let r = ApiResponse::ok(42);
        let json = r.to_json_string();
        assert!(json.contains("Ok"));
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
        assert_eq!(m.from_index(), 0);
        assert_eq!(m.to_index(), 0);
    }

    #[test]
    fn pagination_meta_has_next() {
        let m = PaginationMeta::new(1, 10, 100);
        assert!(m.has_next());
        let m2 = PaginationMeta::new(10, 10, 100);
        assert!(!m2.has_next());
    }

    #[test]
    fn pagination_meta_has_prev() {
        let m = PaginationMeta::new(2, 10, 100);
        assert!(m.has_prev());
        let m2 = PaginationMeta::new(1, 10, 100);
        assert!(!m2.has_prev());
    }

    #[test]
    fn pagination_meta_offset_limit() {
        let m = PaginationMeta::new(3, 20, 100);
        assert_eq!(m.offset(), 40);
        assert_eq!(m.limit(), 20);
    }

    #[test]
    fn pagination_meta_from_to_index() {
        let m = PaginationMeta::new(2, 10, 25);
        assert_eq!(m.from_index(), 11);
        assert_eq!(m.to_index(), 20);
    }

    #[test]
    fn pagination_meta_from_to_index_last_page() {
        let m = PaginationMeta::new(3, 10, 25);
        assert_eq!(m.from_index(), 21);
        assert_eq!(m.to_index(), 25);
    }

    #[test]
    fn pagination_meta_page_zero_clamped() {
        let m = PaginationMeta::new(0, 10, 100);
        assert_eq!(m.page(), 1);
    }

    #[test]
    fn pagination_meta_per_page_zero_clamped() {
        let m = PaginationMeta::new(1, 0, 100);
        assert_eq!(m.per_page(), 1);
    }

    // ----- PaginatedResponse -----

    #[test]
    fn paginated_response_new() {
        let r = PaginatedResponse::new(vec![1, 2, 3], 1, 10, 3);
        assert_eq!(r.item_count(), 3);
        assert!(!r.is_empty());
        assert_eq!(r.pagination().total(), 3);
    }

    #[test]
    fn paginated_response_from_all() {
        let all = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let r = PaginatedResponse::from_all(all, 2, 3);
        assert_eq!(r.item_count(), 3);
        assert_eq!(r.items(), &[4, 5, 6]);
        assert_eq!(r.pagination().total(), 10);
        assert_eq!(r.pagination().total_pages(), 4);
    }

    #[test]
    fn paginated_response_from_all_last_page() {
        let all = vec![1, 2, 3, 4, 5];
        let r = PaginatedResponse::from_all(all, 2, 3);
        assert_eq!(r.item_count(), 2);
        assert_eq!(r.items(), &[4, 5]);
    }

    #[test]
    fn paginated_response_from_all_empty() {
        let all: Vec<i32> = vec![];
        let r = PaginatedResponse::from_all(all, 1, 10);
        assert!(r.is_empty());
        assert_eq!(r.pagination().total(), 0);
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
    fn paginated_response_to_json_string() {
        let r = PaginatedResponse::new(vec![1, 2], 1, 10, 2);
        let json = r.to_json_string();
        assert!(json.contains("items"));
    }

    // ----- PaginationExtractor -----

    #[test]
    fn pagination_extractor_default() {
        let e = PaginationExtractor::new();
        assert_eq!(e.default_per_page(), 20);
        assert_eq!(e.max_per_page(), 100);
    }

    #[test]
    fn pagination_extractor_custom_defaults() {
        let e = PaginationExtractor::new()
            .with_default_per_page(50)
            .with_max_per_page(200);
        assert_eq!(e.default_per_page(), 50);
        assert_eq!(e.max_per_page(), 200);
    }

    #[test]
    fn pagination_extractor_extract_from_map() {
        let e = PaginationExtractor::new();
        let mut params = HashMap::new();
        params.insert("page".to_string(), "3".to_string());
        params.insert("per_page".to_string(), "50".to_string());
        let (page, per_page) = e.extract(&params);
        assert_eq!(page, 3);
        assert_eq!(per_page, 50);
    }

    #[test]
    fn pagination_extractor_extract_defaults() {
        let e = PaginationExtractor::new();
        let params = HashMap::new();
        let (page, per_page) = e.extract(&params);
        assert_eq!(page, 1);
        assert_eq!(per_page, 20);
    }

    #[test]
    fn pagination_extractor_extract_clamp_max() {
        let e = PaginationExtractor::new().with_max_per_page(50);
        let mut params = HashMap::new();
        params.insert("per_page".to_string(), "1000".to_string());
        let (_, per_page) = e.extract(&params);
        assert_eq!(per_page, 50);
    }

    #[test]
    fn pagination_extractor_extract_invalid() {
        let e = PaginationExtractor::new();
        let mut params = HashMap::new();
        params.insert("page".to_string(), "abc".to_string());
        let (page, _) = e.extract(&params);
        assert_eq!(page, 1);
    }

    #[test]
    fn pagination_extractor_extract_pairs() {
        let e = PaginationExtractor::new();
        let (page, per_page) = e.extract_pairs(&[("page", "2"), ("per_page", "30")]);
        assert_eq!(page, 2);
        assert_eq!(per_page, 30);
    }

    #[test]
    fn pagination_extractor_extract_raw() {
        let e = PaginationExtractor::new();
        let (page, per_page) = e.extract_raw(Some("5"), Some("15"));
        assert_eq!(page, 5);
        assert_eq!(per_page, 15);
    }

    #[test]
    fn pagination_extractor_extract_raw_none() {
        let e = PaginationExtractor::new();
        let (page, per_page) = e.extract_raw(None, None);
        assert_eq!(page, 1);
        assert_eq!(per_page, 20);
    }

    // ----- SecurityHeaders -----

    #[test]
    fn security_headers_default() {
        let s = SecurityHeaders::new();
        assert_eq!(s.header_count(), 0);
    }

    #[test]
    fn security_headers_nosniff() {
        let s = SecurityHeaders::new().nosniff();
        assert!(s.has_nosniff());
        assert_eq!(s.header_count(), 1);
    }

    #[test]
    fn security_headers_frame_options() {
        let s = SecurityHeaders::new().frame_options("DENY");
        assert_eq!(s.frame_options_value(), Some("DENY"));
    }

    #[test]
    fn security_headers_xss_protection() {
        let s = SecurityHeaders::new().xss_protection();
        assert!(s.has_xss_protection());
    }

    #[test]
    fn security_headers_hsts() {
        let s = SecurityHeaders::new().hsts("max-age=31536000");
        assert_eq!(s.hsts_value(), Some("max-age=31536000"));
    }

    #[test]
    fn security_headers_csp() {
        let s = SecurityHeaders::new().csp("default-src 'self'");
        assert_eq!(s.csp_value(), Some("default-src 'self'"));
    }

    #[test]
    fn security_headers_referrer_policy() {
        let s = SecurityHeaders::new().referrer_policy("no-referrer");
        assert_eq!(s.referrer_policy_value(), Some("no-referrer"));
    }

    #[test]
    fn security_headers_all() {
        let s = SecurityHeaders::new()
            .nosniff()
            .frame_options("DENY")
            .xss_protection()
            .hsts("max-age=31536000")
            .csp("default-src 'self'")
            .referrer_policy("no-referrer");
        assert_eq!(s.header_count(), 6);
    }

    #[test]
    fn security_headers_to_headers() {
        let s = SecurityHeaders::new().nosniff();
        let headers = s.to_headers();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0, "X-Content-Type-Options");
        assert_eq!(headers[0].1, "nosniff");
    }

    // ----- ResponseHeaders -----

    #[test]
    fn response_headers_default() {
        let h = ResponseHeaders::new();
        assert_eq!(h.custom_count(), 0);
        assert_eq!(h.total_count(), 0);
    }

    #[test]
    fn response_headers_custom() {
        let h = ResponseHeaders::new().header("X-Custom", "value");
        assert_eq!(h.get("X-Custom"), Some("value"));
        assert_eq!(h.custom_count(), 1);
    }

    #[test]
    fn response_headers_with_security() {
        let h = ResponseHeaders::new().with_security(SecurityHeaders::new().nosniff());
        assert_eq!(h.total_count(), 1);
    }

    #[test]
    fn response_headers_to_all_headers() {
        let h = ResponseHeaders::new()
            .header("X-Custom", "val")
            .with_security(SecurityHeaders::new().nosniff());
        let all = h.to_all_headers();
        assert_eq!(all.len(), 2);
    }

    // ----- CacheDirective -----

    #[test]
    fn cache_directive_no_cache() {
        let d = CacheDirective::NoCache;
        assert_eq!(d.to_header_value(), "no-cache");
        assert!(d.is_no_cache());
    }

    #[test]
    fn cache_directive_no_store() {
        let d = CacheDirective::NoStore;
        assert_eq!(d.to_header_value(), "no-store");
        assert!(d.is_no_cache());
    }

    #[test]
    fn cache_directive_public() {
        let d = CacheDirective::Public(3600);
        assert_eq!(d.to_header_value(), "public, max-age=3600");
        assert!(!d.is_no_cache());
        assert_eq!(d.max_age(), Some(3600));
    }

    #[test]
    fn cache_directive_private() {
        let d = CacheDirective::Private(600);
        assert_eq!(d.to_header_value(), "private, max-age=600");
        assert_eq!(d.max_age(), Some(600));
    }

    #[test]
    fn cache_directive_max_age() {
        let d = CacheDirective::MaxAge(120);
        assert_eq!(d.to_header_value(), "max-age=120");
        assert_eq!(d.max_age(), Some(120));
    }

    // ----- CacheControlConfig -----

    #[test]
    fn cache_control_default() {
        let c = CacheControlConfig::new();
        assert!(c.is_no_cache());
    }

    #[test]
    fn cache_control_public() {
        let c = CacheControlConfig::new().directive(CacheDirective::Public(3600));
        assert!(!c.is_no_cache());
        let val = c.to_header_value();
        assert!(val.contains("public"));
        assert!(val.contains("max-age=3600"));
    }

    #[test]
    fn cache_control_with_s_max_age() {
        let c = CacheControlConfig::new()
            .directive(CacheDirective::Public(3600))
            .s_max_age(600);
        let val = c.to_header_value();
        assert!(val.contains("s-maxage=600"));
    }

    #[test]
    fn cache_control_with_stale_while_revalidate() {
        let c = CacheControlConfig::new()
            .directive(CacheDirective::Public(3600))
            .stale_while_revalidate(60);
        let val = c.to_header_value();
        assert!(val.contains("stale-while-revalidate=60"));
    }

    #[test]
    fn cache_control_must_revalidate() {
        let c = CacheControlConfig::new()
            .directive(CacheDirective::Public(3600))
            .must_revalidate();
        let val = c.to_header_value();
        assert!(val.contains("must-revalidate"));
    }

    #[test]
    fn cache_control_immutable() {
        let c = CacheControlConfig::new()
            .directive(CacheDirective::Public(3600))
            .immutable();
        let val = c.to_header_value();
        assert!(val.contains("immutable"));
    }

    #[test]
    fn cache_control_full() {
        let c = CacheControlConfig::new()
            .directive(CacheDirective::Public(3600))
            .s_max_age(600)
            .stale_while_revalidate(60)
            .must_revalidate()
            .immutable();
        let val = c.to_header_value();
        assert!(val.contains("public"));
        assert!(val.contains("s-maxage=600"));
        assert!(val.contains("stale-while-revalidate=60"));
        assert!(val.contains("must-revalidate"));
        assert!(val.contains("immutable"));
    }

    // ----- RateLimitStrategy -----

    #[test]
    fn rate_limit_strategy_name() {
        assert_eq!(RateLimitStrategy::FixedWindow.name(), "fixed_window");
        assert_eq!(RateLimitStrategy::SlidingWindow.name(), "sliding_window");
        assert_eq!(RateLimitStrategy::TokenBucket.name(), "token_bucket");
        assert_eq!(RateLimitStrategy::LeakyBucket.name(), "leaky_bucket");
    }

    // ----- RateLimitKey -----

    #[test]
    fn rate_limit_key_name() {
        assert_eq!(RateLimitKey::Ip.name(), "ip");
        assert_eq!(RateLimitKey::UserId.name(), "user_id");
        assert_eq!(RateLimitKey::ApiKey.name(), "api_key");
        assert_eq!(RateLimitKey::Custom("tenant".to_string()).name(), "tenant");
    }

    // ----- RateLimitConfig -----

    #[test]
    fn rate_limit_config_default() {
        let c = RateLimitConfig::new();
        assert_eq!(c.max_requests_value(), 100);
        assert_eq!(c.window_seconds_value(), 60);
        assert_eq!(c.requests_per_second(), 100.0 / 60.0);
    }

    #[test]
    fn rate_limit_config_builder() {
        let c = RateLimitConfig::new()
            .strategy(RateLimitStrategy::TokenBucket)
            .max_requests(1000)
            .window_seconds(120)
            .key(RateLimitKey::UserId)
            .message("too many requests");
        assert_eq!(c.strategy_value(), &RateLimitStrategy::TokenBucket);
        assert_eq!(c.max_requests_value(), 1000);
        assert_eq!(c.window_seconds_value(), 120);
        assert_eq!(c.key_value(), &RateLimitKey::UserId);
        assert_eq!(c.message_value(), "too many requests");
    }

    #[test]
    fn rate_limit_config_requests_per_second() {
        let c = RateLimitConfig::new().max_requests(200).window_seconds(10);
        assert_eq!(c.requests_per_second(), 20.0);
    }

    #[test]
    fn rate_limit_config_zero_window() {
        let c = RateLimitConfig::new().window_seconds(0);
        assert_eq!(c.requests_per_second(), 0.0);
    }

    // ----- MiddlewarePriority -----

    #[test]
    fn middleware_priority_ordering() {
        assert!(MiddlewarePriority::Lowest < MiddlewarePriority::Low);
        assert!(MiddlewarePriority::Low < MiddlewarePriority::Normal);
        assert!(MiddlewarePriority::Normal < MiddlewarePriority::High);
        assert!(MiddlewarePriority::High < MiddlewarePriority::Highest);
    }

    #[test]
    fn middleware_priority_value() {
        assert_eq!(MiddlewarePriority::Lowest.value(), 0);
        assert_eq!(MiddlewarePriority::Low.value(), 1);
        assert_eq!(MiddlewarePriority::Normal.value(), 2);
        assert_eq!(MiddlewarePriority::High.value(), 3);
        assert_eq!(MiddlewarePriority::Highest.value(), 4);
    }

    // ----- MiddlewareEntry -----

    #[test]
    fn middleware_entry_new() {
        let e = MiddlewareEntry::new("cors");
        assert_eq!(e.name(), "cors");
        assert!(e.is_enabled());
        assert_eq!(e.config_count(), 0);
    }

    #[test]
    fn middleware_entry_priority() {
        let e = MiddlewareEntry::new("auth").priority(MiddlewarePriority::High);
        assert_eq!(e.priority_value(), &MiddlewarePriority::High);
    }

    #[test]
    fn middleware_entry_disable() {
        let e = MiddlewareEntry::new("logging").disable();
        assert!(!e.is_enabled());
    }

    #[test]
    fn middleware_entry_config() {
        let e = MiddlewareEntry::new("cors").config("origin", "*");
        assert_eq!(e.config_value("origin"), Some("*"));
        assert_eq!(e.config_count(), 1);
    }

    #[test]
    fn middleware_entry_config_missing() {
        let e = MiddlewareEntry::new("cors");
        assert!(e.config_value("missing").is_none());
    }

    // ----- MiddlewareChainConfig -----

    #[test]
    fn middleware_chain_empty() {
        let c = MiddlewareChainConfig::new();
        assert_eq!(c.count(), 0);
        assert_eq!(c.enabled_count(), 0);
    }

    #[test]
    fn middleware_chain_add() {
        let c = MiddlewareChainConfig::new()
            .add(MiddlewareEntry::new("cors"))
            .add(MiddlewareEntry::new("auth"));
        assert_eq!(c.count(), 2);
        assert_eq!(c.enabled_count(), 2);
    }

    #[test]
    fn middleware_chain_remove() {
        let mut c = MiddlewareChainConfig::new()
            .add(MiddlewareEntry::new("cors"))
            .add(MiddlewareEntry::new("auth"));
        assert!(c.remove("cors"));
        assert_eq!(c.count(), 1);
        assert!(!c.remove("nonexistent"));
    }

    #[test]
    fn middleware_chain_find() {
        let c = MiddlewareChainConfig::new().add(MiddlewareEntry::new("cors"));
        assert!(c.find("cors").is_some());
        assert!(c.find("auth").is_none());
    }

    #[test]
    fn middleware_chain_enable_disable() {
        let mut c = MiddlewareChainConfig::new()
            .add(MiddlewareEntry::new("cors"))
            .add(MiddlewareEntry::new("auth"));
        assert!(c.disable("cors"));
        assert_eq!(c.enabled_count(), 1);
        assert!(c.enable("cors"));
        assert_eq!(c.enabled_count(), 2);
        assert!(!c.disable("nonexistent"));
    }

    #[test]
    fn middleware_chain_ordered_names() {
        let c = MiddlewareChainConfig::new()
            .add(MiddlewareEntry::new("logging").priority(MiddlewarePriority::Lowest))
            .add(MiddlewareEntry::new("auth").priority(MiddlewarePriority::High))
            .add(MiddlewareEntry::new("cors").priority(MiddlewarePriority::Normal));
        let names = c.ordered_names();
        assert_eq!(names, vec!["logging", "cors", "auth"]);
    }

    #[test]
    fn middleware_chain_ordered_names_skip_disabled() {
        let c = MiddlewareChainConfig::new()
            .add(MiddlewareEntry::new("logging").priority(MiddlewarePriority::Lowest))
            .add(MiddlewareEntry::new("auth").disable())
            .add(MiddlewareEntry::new("cors").priority(MiddlewarePriority::Normal));
        let names = c.ordered_names();
        assert_eq!(names, vec!["logging", "cors"]);
    }

    #[test]
    fn middleware_chain_ordered_desc() {
        let c = MiddlewareChainConfig::new()
            .add(MiddlewareEntry::new("logging").priority(MiddlewarePriority::Lowest))
            .add(MiddlewareEntry::new("auth").priority(MiddlewarePriority::High))
            .add(MiddlewareEntry::new("cors").priority(MiddlewarePriority::Normal));
        let desc = c.ordered_desc();
        assert_eq!(desc.len(), 3);
        assert_eq!(desc[0].name(), "auth");
        assert_eq!(desc[1].name(), "cors");
        assert_eq!(desc[2].name(), "logging");
    }

    #[test]
    fn middleware_chain_ordered_desc_empty() {
        let c = MiddlewareChainConfig::new();
        assert!(c.ordered_desc().is_empty());
    }
}
