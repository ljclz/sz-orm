//! ClaudeProvider — Anthropic Claude API 实现

use super::types::{LlmRequestConfig, LlmResponse, LlmUsage};
use super::{LlmConfig, LlmError, LlmProvider};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct ClaudeMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ClaudeRequestBody {
    model: String,
    max_tokens: u32,
    temperature: f32,
    messages: Vec<ClaudeMessage>,
}

#[derive(Debug, Deserialize)]
struct ClaudeContent {
    text: String,
}

#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    content: Vec<ClaudeContent>,
    usage: Option<ClaudeUsage>,
}

#[derive(Debug, Deserialize)]
struct ClaudeUsage {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
}

/// Anthropic Claude provider
pub struct ClaudeProvider {
    config: LlmConfig,
    client: reqwest::Client,
}

impl ClaudeProvider {
    /// 构造 ClaudeProvider（需要 api_key）
    pub fn new(config: LlmConfig) -> Result<Self, LlmError> {
        if config.api_key.is_none() {
            return Err(LlmError::Config(
                "Claude provider requires api_key".to_string(),
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| LlmError::Config(format!("reqwest client build failed: {e}")))?;
        Ok(Self { config, client })
    }
}

#[async_trait]
impl LlmProvider for ClaudeProvider {
    async fn complete(
        &self,
        prompt: &str,
        req_config: &LlmRequestConfig,
    ) -> Result<LlmResponse, LlmError> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| LlmError::Config("Claude provider requires api_key".to_string()))?;

        let body = ClaudeRequestBody {
            model: self.config.model.clone(),
            max_tokens: req_config.max_tokens,
            temperature: req_config.temperature,
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
        };

        let url = format!("{}/messages", self.config.api_base);
        let resp = self
            .client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout(self.config.timeout)
                } else if e.is_connect() {
                    LlmError::ConnectionRefused {
                        endpoint: self.config.api_base.clone(),
                    }
                } else {
                    LlmError::ApiError {
                        status: 0,
                        message: e.to_string(),
                    }
                }
            })?;

        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            let msg = resp.text().await.unwrap_or_default();
            return Err(LlmError::Auth {
                reason: format!("status {status}: {msg}"),
            });
        }
        if !status.is_success() {
            let msg = resp.text().await.unwrap_or_default();
            return Err(LlmError::ApiError {
                status: status.as_u16(),
                message: msg,
            });
        }

        let claude_resp: ClaudeResponse = resp.json().await.map_err(|e| LlmError::ApiError {
            status: status.as_u16(),
            message: format!("deserialize failed: {e}"),
        })?;

        let text = claude_resp
            .content
            .into_iter()
            .next()
            .map(|c| c.text)
            .unwrap_or_default();

        let usage = claude_resp.usage.unwrap_or(ClaudeUsage {
            input_tokens: None,
            output_tokens: None,
        });
        let usage = LlmUsage {
            prompt_tokens: usage.input_tokens.unwrap_or(0),
            completion_tokens: usage.output_tokens.unwrap_or(0),
        };

        Ok(LlmResponse { text, usage })
    }

    async fn embed(&self, _text: &str) -> Result<Vec<f32>, LlmError> {
        Err(LlmError::ApiError {
            status: 501,
            message: "Claude does not support embed, use OpenAI/Gemini for embedding".to_string(),
        })
    }

    fn provider_name(&self) -> &'static str {
        "claude"
    }

    fn model(&self) -> &str {
        &self.config.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_provider_name() {
        let config = LlmConfig {
            provider: super::super::LlmProviderKind::Claude,
            model: "claude-sonnet-4-20250514".to_string(),
            api_key: Some("test-key".to_string()),
            api_base: LlmConfig::api_base_for(super::super::LlmProviderKind::Claude),
            timeout: std::time::Duration::from_secs(30),
            max_tokens: 2000,
            fallback: None,
        };
        let provider = ClaudeProvider::new(config).unwrap();
        assert_eq!(provider.provider_name(), "claude");
        assert_eq!(provider.model(), "claude-sonnet-4-20250514");
    }

    #[test]
    fn test_claude_provider_no_api_key() {
        let config = LlmConfig::for_provider(super::super::LlmProviderKind::Claude);
        let result = ClaudeProvider::new(config);
        assert!(matches!(result, Err(LlmError::Config(_))));
    }
}
