//! CORS 配置：跨域资源共享配置管理。
//!
//! - [`CorsConfig`] — CORS 配置
//! - [`CorsOrigin`] — 来源匹配策略

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

// ============================================================================
// CorsOrigin — 来源匹配策略
// ============================================================================

/// CORS 来源匹配策略
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[derive(Default)]
pub enum CorsOrigin {
    #[default]
    Any,
    Specific(Vec<String>),
    Pattern(String),
}


impl CorsOrigin {
    /// 检查给定 origin 是否允许
    pub fn allows(&self, origin: &str) -> bool {
        match self {
            CorsOrigin::Any => true,
            CorsOrigin::Specific(allowed) => allowed.iter().any(|o| o == origin),
            CorsOrigin::Pattern(pattern) => wildcard_match(pattern, origin),
        }
    }

    /// 是否为 Any
    pub fn is_any(&self) -> bool {
        matches!(self, CorsOrigin::Any)
    }
}

/// 通配符匹配
fn wildcard_match(pattern: &str, input: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == input;
    }
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !input[pos..].starts_with(part) {
                return false;
            }
            pos += part.len();
        } else if i == parts.len() - 1 {
            return input[pos..].ends_with(part);
        } else if let Some(idx) = input[pos..].find(part) {
            pos += idx + part.len();
        } else {
            return false;
        }
    }
    true
}

// ============================================================================
// CorsConfig — CORS 配置
// ============================================================================

/// CORS 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    origin: CorsOrigin,
    methods: HashSet<String>,
    headers: HashSet<String>,
    expose_headers: HashSet<String>,
    max_age: Option<u64>,
    allow_credentials: bool,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            origin: CorsOrigin::Any,
            methods: ["GET", "POST", "PUT", "DELETE", "OPTIONS"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            headers: ["Content-Type", "Authorization"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            expose_headers: HashSet::new(),
            max_age: None,
            allow_credentials: false,
        }
    }
}

impl CorsConfig {
    /// 创建默认配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置允许所有来源（链式）
    pub fn allow_any_origin(mut self) -> Self {
        self.origin = CorsOrigin::Any;
        self
    }

    /// 设置指定来源（链式）
    pub fn with_specific_origins(mut self, origins: Vec<String>) -> Self {
        self.origin = CorsOrigin::Specific(origins);
        self
    }

    /// 设置通配符来源（链式）
    pub fn with_pattern_origin(mut self, pattern: &str) -> Self {
        self.origin = CorsOrigin::Pattern(pattern.to_string());
        self
    }

    /// 添加允许方法（链式）
    pub fn with_method(mut self, method: &str) -> Self {
        self.methods.insert(method.to_string());
        self
    }

    /// 添加允许头（链式）
    pub fn with_header(mut self, header: &str) -> Self {
        self.headers.insert(header.to_string());
        self
    }

    /// 添加暴露头（链式）
    pub fn with_exposed_header(mut self, header: &str) -> Self {
        self.expose_headers.insert(header.to_string());
        self
    }

    /// 设置 max-age（链式）
    pub fn with_max_age(mut self, seconds: u64) -> Self {
        self.max_age = Some(seconds);
        self
    }

    /// 启用凭证（链式）
    pub fn allow_credentials(mut self) -> Self {
        self.allow_credentials = true;
        self
    }

    /// 来源
    pub fn origin_value(&self) -> &CorsOrigin {
        &self.origin
    }

    /// 是否允许来源
    pub fn allows_origin(&self, origin: &str) -> bool {
        self.origin.allows(origin)
    }

    /// 允许方法值
    pub fn allow_methods_value(&self) -> String {
        let mut methods: Vec<String> = self.methods.iter().cloned().collect();
        methods.sort();
        methods.join(", ")
    }

    /// 允许头值
    pub fn allow_headers_value(&self) -> String {
        let mut headers: Vec<String> = self.headers.iter().cloned().collect();
        headers.sort();
        headers.join(", ")
    }

    /// 暴露头值
    pub fn expose_headers_value(&self) -> String {
        let mut headers: Vec<String> = self.expose_headers.iter().cloned().collect();
        headers.sort();
        headers.join(", ")
    }

    /// max-age
    pub fn max_age_value(&self) -> Option<u64> {
        self.max_age
    }

    /// 是否允许凭证
    pub fn allows_credentials(&self) -> bool {
        self.allow_credentials
    }

    /// 是否为预检请求
    pub fn is_preflight(&self, method: &str) -> bool {
        method.eq_ignore_ascii_case("OPTIONS")
    }

    /// 生成 CORS 响应头
    pub fn response_headers(&self, origin: &str) -> Vec<(String, String)> {
        let mut headers = vec![];
        let allow_origin = if self.origin.is_any() {
            "*".to_string()
        } else {
            origin.to_string()
        };
        headers.push(("Access-Control-Allow-Origin".to_string(), allow_origin));
        headers.push((
            "Access-Control-Allow-Methods".to_string(),
            self.allow_methods_value(),
        ));
        headers.push((
            "Access-Control-Allow-Headers".to_string(),
            self.allow_headers_value(),
        ));
        if !self.expose_headers.is_empty() {
            headers.push((
                "Access-Control-Expose-Headers".to_string(),
                self.expose_headers_value(),
            ));
        }
        if let Some(max_age) = self.max_age {
            headers.push(("Access-Control-Max-Age".to_string(), max_age.to_string()));
        }
        if self.allow_credentials {
            headers.push((
                "Access-Control-Allow-Credentials".to_string(),
                "true".to_string(),
            ));
        }
        headers
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cors_origin_any() {
        assert!(CorsOrigin::Any.allows("https://example.com"));
        assert!(CorsOrigin::Any.is_any());
    }

    #[test]
    fn cors_origin_specific() {
        let o = CorsOrigin::Specific(vec![
            "https://a.com".to_string(),
            "https://b.com".to_string(),
        ]);
        assert!(o.allows("https://a.com"));
        assert!(o.allows("https://b.com"));
        assert!(!o.allows("https://c.com"));
    }

    #[test]
    fn cors_origin_pattern() {
        let o = CorsOrigin::Pattern("https://*.example.com".to_string());
        assert!(o.allows("https://api.example.com"));
        assert!(!o.allows("https://api.other.com"));
    }

    #[test]
    fn cors_config_default() {
        let c = CorsConfig::new();
        assert!(c.origin_value().is_any());
        assert!(!c.allows_credentials());
    }

    #[test]
    fn cors_config_specific_origins() {
        let c = CorsConfig::new().with_specific_origins(vec!["https://a.com".to_string()]);
        assert!(c.allows_origin("https://a.com"));
        assert!(!c.allows_origin("https://b.com"));
    }

    #[test]
    fn cors_config_credentials() {
        let c = CorsConfig::new().allow_credentials();
        assert!(c.allows_credentials());
    }

    #[test]
    fn cors_config_max_age() {
        let c = CorsConfig::new().with_max_age(3600);
        assert_eq!(c.max_age_value(), Some(3600));
    }

    #[test]
    fn cors_config_response_headers() {
        let c = CorsConfig::new();
        let headers = c.response_headers("https://example.com");
        assert!(headers
            .iter()
            .any(|(k, _)| k == "Access-Control-Allow-Origin"));
        assert!(headers
            .iter()
            .any(|(k, _)| k == "Access-Control-Allow-Methods"));
    }

    #[test]
    fn cors_config_is_preflight() {
        let c = CorsConfig::new();
        assert!(c.is_preflight("OPTIONS"));
        assert!(!c.is_preflight("GET"));
    }

    #[test]
    fn cors_config_methods() {
        let c = CorsConfig::new().with_method("PATCH");
        assert!(c.allow_methods_value().contains("PATCH"));
    }

    #[test]
    fn cors_config_headers() {
        let c = CorsConfig::new().with_header("X-Custom");
        assert!(c.allow_headers_value().contains("X-Custom"));
    }

    #[test]
    fn cors_config_expose_headers() {
        let c = CorsConfig::new().with_exposed_header("X-Total-Count");
        assert!(c.expose_headers_value().contains("X-Total-Count"));
    }

    #[test]
    fn wildcard_match_star() {
        assert!(wildcard_match("*", "anything"));
    }

    #[test]
    fn wildcard_match_exact() {
        assert!(wildcard_match("exact", "exact"));
        assert!(!wildcard_match("exact", "other"));
    }
}
