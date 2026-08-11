//! 多 LLM 模型支持（v4.0.0 M1）
//!
//! 提供 `LlmProvider` trait 统一接口，支持 Claude / Gemini / Ollama / OpenAI 四种 provider，
//! 通过 `LlmRouter` 实现运行时热切换、按能力路由和 fallback 链。

pub mod types;

pub use types::{LlmCapability, LlmRequestConfig, LlmResponse, LlmUsage};

pub mod claude;
pub mod gemini;
pub mod ollama;
pub mod openai;
pub mod router;

pub use claude::ClaudeProvider;
pub use gemini::GeminiProvider;
pub use ollama::LocalLlamaProvider;
pub use openai::OpenAIProvider;
pub use router::LlmRouter;

use async_trait::async_trait;
use std::time::Duration;

/// LLM provider 种类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProviderKind {
    /// Anthropic Claude
    Claude,
    /// Google Gemini
    Gemini,
    /// 本地 Ollama（Llama 等）
    Ollama,
    /// OpenAI 兼容 API
    OpenAI,
}

impl LlmProviderKind {
    /// 返回 provider 名称字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::Ollama => "ollama",
            Self::OpenAI => "openai",
        }
    }
}

/// LLM 配置（含 fallback 链）
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// provider 种类
    pub provider: LlmProviderKind,
    /// 模型名称（如 "gpt-4o"、"claude-sonnet-4-20250514"）
    pub model: String,
    /// API Key（Ollama 无需）
    pub api_key: Option<String>,
    /// API base URL
    pub api_base: String,
    /// 请求超时
    pub timeout: Duration,
    /// 最大 token 数
    pub max_tokens: u32,
    /// fallback 配置（当前 provider 不可用时尝试）
    pub fallback: Option<Box<LlmConfig>>,
}

impl LlmConfig {
    /// 按 provider 种类推断默认 API base
    pub fn api_base_for(provider: LlmProviderKind) -> String {
        match provider {
            LlmProviderKind::Claude => "https://api.anthropic.com/v1".to_string(),
            LlmProviderKind::Gemini => "https://generativelanguage.googleapis.com/v1".to_string(),
            LlmProviderKind::Ollama => "http://localhost:11434".to_string(),
            LlmProviderKind::OpenAI => "https://api.openai.com/v1".to_string(),
        }
    }

    /// 构造指定 provider 的配置（api_key 需后续设置）
    pub fn for_provider(provider: LlmProviderKind) -> Self {
        let model = match provider {
            LlmProviderKind::Claude => "claude-sonnet-4-20250514",
            LlmProviderKind::Gemini => "gemini-1.5-pro",
            LlmProviderKind::Ollama => "llama3.1",
            LlmProviderKind::OpenAI => "gpt-4o",
        };
        Self {
            provider,
            model: model.to_string(),
            api_key: None,
            api_base: Self::api_base_for(provider),
            timeout: Duration::from_secs(30),
            max_tokens: 2000,
            fallback: None,
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProviderKind::OpenAI,
            model: "gpt-4o".to_string(),
            api_key: None,
            api_base: "https://api.openai.com/v1".to_string(),
            timeout: Duration::from_secs(30),
            max_tokens: 2000,
            fallback: None,
        }
    }
}

/// LLM 错误类型
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// 请求超时
    #[error("LLM request timeout after {0:?}")]
    Timeout(Duration),

    /// 认证错误（API Key 无效或缺失）
    #[error("LLM auth error: {reason}")]
    Auth { reason: String },

    /// 连接被拒绝（如 Ollama 未启动）
    #[error("LLM connection refused: {endpoint}")]
    ConnectionRefused { endpoint: String },

    /// API 返回错误
    #[error("LLM API error (status {status}): {message}")]
    ApiError { status: u16, message: String },

    /// 配置错误
    #[error("LLM config error: {0}")]
    Config(String),

    /// fallback 链全部耗尽
    #[error("LLM fallback exhausted: all providers failed")]
    FallbackExhausted,
}

/// LLM provider 统一接口
///
/// 实现 `complete`（文本生成）和 `embed`（文本嵌入）两个核心方法，
/// 加上 `provider_name` 和 `model` 元信息方法。
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 文本生成（complete）
    async fn complete(
        &self,
        prompt: &str,
        config: &LlmRequestConfig,
    ) -> Result<LlmResponse, LlmError>;

    /// 文本嵌入（embedding）
    async fn embed(&self, text: &str) -> Result<Vec<f32>, LlmError>;

    /// provider 名称（如 "claude"、"gemini"）
    fn provider_name(&self) -> &'static str;

    /// 当前使用的模型名称
    fn model(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_config_default() {
        let config = LlmConfig::default();
        assert_eq!(config.provider, LlmProviderKind::OpenAI);
        assert_eq!(config.model, "gpt-4o");
        assert_eq!(config.api_base, "https://api.openai.com/v1");
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.max_tokens, 2000);
        assert!(config.api_key.is_none());
        assert!(config.fallback.is_none());
    }

    #[test]
    fn test_api_base_for() {
        assert_eq!(
            LlmConfig::api_base_for(LlmProviderKind::Claude),
            "https://api.anthropic.com/v1"
        );
        assert_eq!(
            LlmConfig::api_base_for(LlmProviderKind::Gemini),
            "https://generativelanguage.googleapis.com/v1"
        );
        assert_eq!(
            LlmConfig::api_base_for(LlmProviderKind::Ollama),
            "http://localhost:11434"
        );
        assert_eq!(
            LlmConfig::api_base_for(LlmProviderKind::OpenAI),
            "https://api.openai.com/v1"
        );
    }

    #[test]
    fn test_for_provider() {
        let claude_config = LlmConfig::for_provider(LlmProviderKind::Claude);
        assert_eq!(claude_config.provider, LlmProviderKind::Claude);
        assert_eq!(claude_config.model, "claude-sonnet-4-20250514");
        assert_eq!(claude_config.api_base, "https://api.anthropic.com/v1");

        let gemini_config = LlmConfig::for_provider(LlmProviderKind::Gemini);
        assert_eq!(gemini_config.provider, LlmProviderKind::Gemini);
        assert_eq!(gemini_config.model, "gemini-1.5-pro");

        let ollama_config = LlmConfig::for_provider(LlmProviderKind::Ollama);
        assert_eq!(ollama_config.provider, LlmProviderKind::Ollama);
        assert_eq!(ollama_config.model, "llama3.1");
        assert_eq!(ollama_config.api_base, "http://localhost:11434");
    }

    #[test]
    fn test_provider_kind_as_str() {
        assert_eq!(LlmProviderKind::Claude.as_str(), "claude");
        assert_eq!(LlmProviderKind::Gemini.as_str(), "gemini");
        assert_eq!(LlmProviderKind::Ollama.as_str(), "ollama");
        assert_eq!(LlmProviderKind::OpenAI.as_str(), "openai");
    }

    #[test]
    fn test_fallback_chain() {
        let mut config_a = LlmConfig::for_provider(LlmProviderKind::OpenAI);
        let mut config_b = LlmConfig::for_provider(LlmProviderKind::Claude);
        let config_c = LlmConfig::for_provider(LlmProviderKind::Gemini);

        config_b.fallback = Some(Box::new(config_c));
        config_a.fallback = Some(Box::new(config_b));

        assert_eq!(config_a.provider, LlmProviderKind::OpenAI);
        let fallback_b = config_a.fallback.as_ref().unwrap();
        assert_eq!(fallback_b.provider, LlmProviderKind::Claude);
        let fallback_c = fallback_b.fallback.as_ref().unwrap();
        assert_eq!(fallback_c.provider, LlmProviderKind::Gemini);
        assert!(fallback_c.fallback.is_none());
    }

    #[test]
    fn test_llm_error_display() {
        let err = LlmError::Timeout(Duration::from_secs(30));
        assert!(err.to_string().contains("timeout"));

        let err = LlmError::Auth {
            reason: "invalid key".to_string(),
        };
        assert!(err.to_string().contains("auth error"));

        let err = LlmError::ConnectionRefused {
            endpoint: "localhost:11434".to_string(),
        };
        assert!(err.to_string().contains("connection refused"));

        let err = LlmError::ApiError {
            status: 401,
            message: "unauthorized".to_string(),
        };
        assert!(err.to_string().contains("API error"));

        let err = LlmError::Config("missing api_key".to_string());
        assert!(err.to_string().contains("config error"));

        let err = LlmError::FallbackExhausted;
        assert!(err.to_string().contains("fallback exhausted"));
    }
}
