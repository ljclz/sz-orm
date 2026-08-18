//! CORS 配置：跨域资源共享配置管理。
//!
//! - [`CorsConfig`] — CORS 配置（来源、方法、头、凭证）
//! - [`CorsOrigin`] — 来源匹配策略
//! - 生成 CORS 响应头、检查来源是否允许

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

// ============================================================================
// CORS 来源匹配策略
// ============================================================================

/// CORS 来源匹配策略
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CorsOrigin {
    /// 允许所有来源（`*`）
    Any,
    /// 允许指定来源列表
    Specific(Vec<String>),
    /// 允许匹配通配符的来源（如 `https://*.example.com`）
    Pattern(String),
}

impl Default for CorsOrigin {
    fn default() -> Self {
        Self::Any
    }
}

impl CorsOrigin {
    /// 检查给定 origin 是否允许
    pub fn allows(&self, origin: &str) -> bool {
        match self {
            CorsOrigin::Any => true,
            CorsOrigin::Specific(allowed) => allowed.iter().any(|o| o == origin),
            CorsOrigin::Pattern(pattern) => wildcard_origin_match(pattern, origin),
        }
    }

    /// 是否允许凭证
    pub fn is_any(&self) -> bool {
        matches!(self, CorsOrigin::Any)
    }
}

/// 通配符 origin 匹配：`*` 匹配任意子域名。
///
/// 例如 `https://*.example.com` 匹配 `https://api.example.com`。
fn wildcard_origin_match(pattern: &str, origin: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == origin;
    }
    let parts: Vec<&str> = pattern.splitn(2, '*').collect();
    let prefix = parts[0];
    let suffix = if parts.len() > 1 { parts[1] } else { "" };
    origin.starts_with(prefix)
        && origin.ends_with(suffix)
        && origin.len() >= prefix.len() + suffix.len()
}

// ============================================================================
// CORS 配置
// ============================================================================

/// CORS 配置：管理跨域资源共享规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    origin: CorsOrigin,
    allowed_methods: HashSet<String>,
    allowed_headers: HashSet<String>,
    expose_headers: HashSet<String>,
    allow_credentials: bool,
    max_age_secs: u64,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            origin: CorsOrigin::Any,
            allowed_methods: ["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            allowed_headers: ["Content-Type", "Authorization", "Accept", "Origin"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            expose_headers: HashSet::new(),
            allow_credentials: false,
            max_age_secs: 86400,
        }
    }
}

impl CorsConfig {
    /// 创建默认 CORS 配置（允许所有来源）
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置允许的来源（链式）
    pub fn with_origin(mut self, origin: CorsOrigin) -> Self {
        self.origin = origin;
        self
    }

    /// 设置允许指定来源列表（链式）
    pub fn with_origins(mut self, origins: Vec<String>) -> Self {
        self.origin = CorsOrigin::Specific(origins);
        self
    }

    /// 设置允许所有来源（链式）
    pub fn allow_any_origin(mut self) -> Self {
        self.origin = CorsOrigin::Any;
        self
    }

    /// 添加允许的 HTTP 方法（链式）
    pub fn with_method(mut self, method: &str) -> Self {
        self.allowed_methods.insert(method.to_uppercase());
        self
    }

    /// 设置允许的 HTTP 方法列表（链式）
    pub fn with_methods(mut self, methods: Vec<String>) -> Self {
        self.allowed_methods = methods.into_iter().map(|m| m.to_uppercase()).collect();
        self
    }

    /// 添加允许的请求头（链式）
    pub fn with_header(mut self, header: &str) -> Self {
        self.allowed_headers.insert(header.to_string());
        self
    }

    /// 添加暴露的响应头（链式）
    pub fn with_exposed_header(mut self, header: &str) -> Self {
        self.expose_headers.insert(header.to_string());
        self
    }

    /// 允许凭证（链式）
    pub fn allow_credentials(mut self) -> Self {
        self.allow_credentials = true;
        self
    }

    /// 设置预检缓存时间（秒，链式）
    pub fn with_max_age(mut self, secs: u64) -> Self {
        self.max_age_secs = secs;
        self
    }

    /// 检查 origin 是否允许
    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        self.origin.allows(origin)
    }

    /// 检查方法是否允许
    pub fn is_method_allowed(&self, method: &str) -> bool {
        self.allowed_methods.contains(&method.to_uppercase())
    }

    /// 检查请求头是否允许
    pub fn is_header_allowed(&self, header: &str) -> bool {
        self.allowed_headers.contains(header)
    }

    /// 是否允许凭证
    pub fn credentials_allowed(&self) -> bool {
        self.allow_credentials
    }

    /// 预检缓存时间
    pub fn max_age_secs(&self) -> u64 {
        self.max_age_secs
    }

    /// 允许的方法列表
    pub fn allowed_methods(&self) -> Vec<&str> {
        self.allowed_methods.iter().map(|s| s.as_str()).collect()
    }

    /// 允许的头列表
    pub fn allowed_headers(&self) -> Vec<&str> {
        self.allowed_headers.iter().map(|s| s.as_str()).collect()
    }

    /// 暴露的头列表
    pub fn expose_headers(&self) -> Vec<&str> {
        self.expose_headers.iter().map(|s| s.as_str()).collect()
    }

    /// 生成 Access-Control-Allow-Origin 头值
    pub fn allow_origin_value(&self, request_origin: Option<&str>) -> String {
        match &self.origin {
            CorsOrigin::Any => {
                if self.allow_credentials {
                    request_origin.unwrap_or("*").to_string()
                } else {
                    "*".to_string()
                }
            }
            CorsOrigin::Specific(origins) => {
                if let Some(origin) = request_origin {
                    if origins.iter().any(|o| o == origin) {
                        origin.to_string()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            }
            CorsOrigin::Pattern(_) => {
                if let Some(origin) = request_origin {
                    if self.origin.allows(origin) {
                        origin.to_string()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            }
        }
    }

    /// 生成 Access-Control-Allow-Methods 头值
    pub fn allow_methods_value(&self) -> String {
        let mut methods: Vec<String> = self.allowed_methods.iter().cloned().collect();
        methods.sort();
        methods.join(", ")
    }

    /// 生成 Access-Control-Allow-Headers 头值
    pub fn allow_headers_value(&self) -> String {
        let mut headers: Vec<String> = self.allowed_headers.iter().cloned().collect();
        headers.sort();
        headers.join(", ")
    }

    /// 生成 Access-Control-Expose-Headers 头值
    pub fn expose_headers_value(&self) -> String {
        let mut headers: Vec<String> = self.expose_headers.iter().cloned().collect();
        headers.sort();
        headers.join(", ")
    }

    /// 生成所有 CORS 响应头
    pub fn response_headers(&self, request_origin: Option<&str>) -> Vec<(String, String)> {
        let mut headers = vec![(
            "Access-Control-Allow-Origin".to_string(),
            self.allow_origin_value(request_origin),
        )];
        let methods = self.allow_methods_value();
        if !methods.is_empty() {
            headers.push(("Access-Control-Allow-Methods".to_string(), methods));
        }
        let allow_headers = self.allow_headers_value();
        if !allow_headers.is_empty() {
            headers.push(("Access-Control-Allow-Headers".to_string(), allow_headers));
        }
        let expose = self.expose_headers_value();
        if !expose.is_empty() {
            headers.push(("Access-Control-Expose-Headers".to_string(), expose));
        }
        if self.allow_credentials {
            headers.push((
                "Access-Control-Allow-Credentials".to_string(),
                "true".to_string(),
            ));
        }
        headers.push((
            "Access-Control-Max-Age".to_string(),
            self.max_age_secs.to_string(),
        ));
        headers
    }

    /// 是否为预检请求（OPTIONS 方法 + Origin 头）
    pub fn is_preflight(method: &str, has_origin: bool) -> bool {
        method.eq_ignore_ascii_case("OPTIONS") && has_origin
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- CorsOrigin -----

    #[test]
    fn cors_origin_any_allows_all() {
        let origin = CorsOrigin::Any;
        assert!(origin.allows("https://example.com"));
        assert!(origin.allows("http://localhost:3000"));
        assert!(origin.is_any());
    }

    #[test]
    fn cors_origin_specific_allows_listed() {
        let origin = CorsOrigin::Specific(vec![
            "https://example.com".to_string(),
            "https://api.example.com".to_string(),
        ]);
        assert!(origin.allows("https://example.com"));
        assert!(origin.allows("https://api.example.com"));
        assert!(!origin.allows("https://evil.com"));
        assert!(!origin.is_any());
    }

    #[test]
    fn cors_origin_pattern_allows_wildcard() {
        let origin = CorsOrigin::Pattern("https://*.example.com".to_string());
        assert!(origin.allows("https://api.example.com"));
        assert!(origin.allows("https://sub.api.example.com"));
        assert!(!origin.allows("http://api.example.com"));
        assert!(!origin.allows("https://evil.com"));
    }

    #[test]
    fn cors_origin_pattern_no_wildcard() {
        let origin = CorsOrigin::Pattern("https://example.com".to_string());
        assert!(origin.allows("https://example.com"));
        assert!(!origin.allows("https://other.com"));
    }

    #[test]
    fn cors_origin_default_is_any() {
        assert_eq!(CorsOrigin::default(), CorsOrigin::Any);
    }

    // ----- CorsConfig -----

    #[test]
    fn cors_config_default() {
        let config = CorsConfig::new();
        assert!(config.is_origin_allowed("https://anything.com"));
        assert!(config.is_method_allowed("GET"));
        assert!(config.is_method_allowed("POST"));
        assert!(!config.credentials_allowed());
    }

    #[test]
    fn cors_config_with_specific_origins() {
        let config = CorsConfig::new().with_origins(vec!["https://example.com".to_string()]);
        assert!(config.is_origin_allowed("https://example.com"));
        assert!(!config.is_origin_allowed("https://evil.com"));
    }

    #[test]
    fn cors_config_allow_any_origin() {
        let config = CorsConfig::new()
            .with_origins(vec!["https://example.com".to_string()])
            .allow_any_origin();
        assert!(config.is_origin_allowed("https://anything.com"));
    }

    #[test]
    fn cors_config_with_method() {
        let config = CorsConfig::new().with_method("TRACE");
        assert!(config.is_method_allowed("TRACE"));
        assert!(config.is_method_allowed("trace"));
    }

    #[test]
    fn cors_config_with_methods() {
        let config = CorsConfig::new().with_methods(vec!["get".to_string(), "post".to_string()]);
        assert!(config.is_method_allowed("GET"));
        assert!(config.is_method_allowed("POST"));
        assert!(!config.is_method_allowed("DELETE"));
    }

    #[test]
    fn cors_config_with_header() {
        let config = CorsConfig::new().with_header("X-Custom-Header");
        assert!(config.is_header_allowed("X-Custom-Header"));
    }

    #[test]
    fn cors_config_with_exposed_header() {
        let config = CorsConfig::new().with_exposed_header("X-Total-Count");
        assert!(config
            .expose_headers()
            .iter()
            .any(|h| *h == "X-Total-Count"));
    }

    #[test]
    fn cors_config_allow_credentials() {
        let config = CorsConfig::new().allow_credentials();
        assert!(config.credentials_allowed());
    }

    #[test]
    fn cors_config_with_max_age() {
        let config = CorsConfig::new().with_max_age(3600);
        assert_eq!(config.max_age_secs(), 3600);
    }

    #[test]
    fn cors_config_allow_origin_value_any() {
        let config = CorsConfig::new();
        assert_eq!(config.allow_origin_value(Some("https://example.com")), "*");
    }

    #[test]
    fn cors_config_allow_origin_value_specific() {
        let config = CorsConfig::new().with_origins(vec!["https://example.com".to_string()]);
        assert_eq!(
            config.allow_origin_value(Some("https://example.com")),
            "https://example.com"
        );
        assert_eq!(config.allow_origin_value(Some("https://evil.com")), "");
    }

    #[test]
    fn cors_config_allow_origin_value_with_credentials() {
        let config = CorsConfig::new().allow_credentials();
        assert_eq!(
            config.allow_origin_value(Some("https://example.com")),
            "https://example.com"
        );
    }

    #[test]
    fn cors_config_allow_methods_value() {
        let config = CorsConfig::new();
        let value = config.allow_methods_value();
        assert!(value.contains("GET"));
        assert!(value.contains("POST"));
    }

    #[test]
    fn cors_config_allow_headers_value() {
        let config = CorsConfig::new();
        let value = config.allow_headers_value();
        assert!(value.contains("Content-Type"));
        assert!(value.contains("Authorization"));
    }

    #[test]
    fn cors_config_expose_headers_value() {
        let config = CorsConfig::new()
            .with_exposed_header("X-Total-Count")
            .with_exposed_header("X-Page");
        let value = config.expose_headers_value();
        assert!(value.contains("X-Total-Count"));
        assert!(value.contains("X-Page"));
    }

    #[test]
    fn cors_config_response_headers() {
        let config = CorsConfig::new();
        let headers = config.response_headers(Some("https://example.com"));
        assert!(headers
            .iter()
            .any(|(k, _)| k == "Access-Control-Allow-Origin"));
        assert!(headers
            .iter()
            .any(|(k, _)| k == "Access-Control-Allow-Methods"));
        assert!(headers.iter().any(|(k, _)| k == "Access-Control-Max-Age"));
    }

    #[test]
    fn cors_config_response_headers_with_credentials() {
        let config = CorsConfig::new().allow_credentials();
        let headers = config.response_headers(Some("https://example.com"));
        assert!(headers
            .iter()
            .any(|(k, v)| k == "Access-Control-Allow-Credentials" && v == "true"));
    }

    #[test]
    fn cors_config_is_preflight() {
        assert!(CorsConfig::is_preflight("OPTIONS", true));
        assert!(CorsConfig::is_preflight("options", true));
        assert!(!CorsConfig::is_preflight("GET", true));
        assert!(!CorsConfig::is_preflight("OPTIONS", false));
    }

    #[test]
    fn cors_config_pattern_origin() {
        let config =
            CorsConfig::new().with_origin(CorsOrigin::Pattern("https://*.app.com".to_string()));
        assert!(config.is_origin_allowed("https://my.app.com"));
        assert!(!config.is_origin_allowed("https://evil.com"));
    }
}
