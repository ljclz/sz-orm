//! WasmDbAuthValidator — Token/Session 鉴权
//!
//! 管理 WASM 端 Token 和会话的有效性。

use std::collections::HashSet;

/// WASM DB 鉴权验证器
///
/// 维护有效 Token 集合和活跃会话集合。
/// Token 用于请求认证，Session 用于跟踪连接状态。
#[derive(Debug, Clone)]
pub struct WasmDbAuthValidator {
    valid_tokens: HashSet<String>,
    active_sessions: HashSet<String>,
}

impl WasmDbAuthValidator {
    /// 创建空验证器
    pub fn new() -> Self {
        Self {
            valid_tokens: HashSet::new(),
            active_sessions: HashSet::new(),
        }
    }

    /// 添加有效 Token
    pub fn add_token(&mut self, token: &str) {
        self.valid_tokens.insert(token.to_string());
    }

    /// 撤销 Token
    pub fn revoke_token(&mut self, token: &str) -> bool {
        self.valid_tokens.remove(token)
    }

    /// 验证 Token 是否有效
    pub fn validate_token(&self, token: &str) -> bool {
        self.valid_tokens.contains(token)
    }

    /// 创建新会话
    pub fn create_session(&mut self, session_id: &str) -> bool {
        self.active_sessions.insert(session_id.to_string())
    }

    /// 撤销会话
    pub fn revoke_session(&mut self, session_id: &str) -> bool {
        self.active_sessions.remove(session_id)
    }

    /// 验证会话是否活跃
    pub fn validate_session(&self, session_id: &str) -> bool {
        self.active_sessions.contains(session_id)
    }

    /// 有效 Token 数量
    pub fn token_count(&self) -> usize {
        self.valid_tokens.len()
    }

    /// 活跃会话数量
    pub fn session_count(&self) -> usize {
        self.active_sessions.len()
    }

    /// 清除所有 Token 和会话
    pub fn clear(&mut self) {
        self.valid_tokens.clear();
        self.active_sessions.clear();
    }
}

impl Default for WasmDbAuthValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_empty() {
        let auth = WasmDbAuthValidator::new();
        assert_eq!(auth.token_count(), 0);
        assert_eq!(auth.session_count(), 0);
    }

    #[test]
    fn test_add_and_validate_token() {
        let mut auth = WasmDbAuthValidator::new();
        auth.add_token("token-1");
        assert!(auth.validate_token("token-1"));
        assert!(!auth.validate_token("token-2"));
        assert_eq!(auth.token_count(), 1);
    }

    #[test]
    fn test_revoke_token() {
        let mut auth = WasmDbAuthValidator::new();
        auth.add_token("token-1");
        assert!(auth.revoke_token("token-1"));
        assert!(!auth.validate_token("token-1"));
        assert!(!auth.revoke_token("token-1"));
    }

    #[test]
    fn test_create_and_validate_session() {
        let mut auth = WasmDbAuthValidator::new();
        assert!(auth.create_session("sess-1"));
        assert!(auth.validate_session("sess-1"));
        assert!(!auth.validate_session("sess-2"));
        assert_eq!(auth.session_count(), 1);
    }

    #[test]
    fn test_duplicate_session() {
        let mut auth = WasmDbAuthValidator::new();
        assert!(auth.create_session("sess-1"));
        assert!(!auth.create_session("sess-1"));
        assert_eq!(auth.session_count(), 1);
    }

    #[test]
    fn test_revoke_session() {
        let mut auth = WasmDbAuthValidator::new();
        auth.create_session("sess-1");
        assert!(auth.revoke_session("sess-1"));
        assert!(!auth.validate_session("sess-1"));
        assert!(!auth.revoke_session("sess-1"));
    }

    #[test]
    fn test_clear() {
        let mut auth = WasmDbAuthValidator::new();
        auth.add_token("token-1");
        auth.add_token("token-2");
        auth.create_session("sess-1");
        auth.clear();
        assert_eq!(auth.token_count(), 0);
        assert_eq!(auth.session_count(), 0);
    }

    #[test]
    fn test_default() {
        let auth = WasmDbAuthValidator::default();
        assert_eq!(auth.token_count(), 0);
    }

    #[test]
    fn test_multiple_tokens() {
        let mut auth = WasmDbAuthValidator::new();
        for i in 0..10 {
            auth.add_token(&format!("token-{}", i));
        }
        assert_eq!(auth.token_count(), 10);
        for i in 0..10 {
            assert!(auth.validate_token(&format!("token-{}", i)));
        }
    }
}
