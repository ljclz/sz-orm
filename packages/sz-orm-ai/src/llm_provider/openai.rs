//! OpenAIProvider — OpenAI Chat Completions API 实现

use super::types::{LlmRequestConfig, LlmResponse, LlmUsage};
use super::{LlmConfig, LlmError, LlmProvider};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct OpenAiRequestBody {
    model: String,
    messages: Vec<OpenAiMessage>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoiceMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbedResponse {
    data: Vec<OpenAiEmbedData>,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbedData {
    embedding: Vec<f32>,
}

/// OpenAI 兼容 provider（包装既有 OptimizerConfig::with_llm 的 HTTP 调用）
pub struct OpenAIProvider {
    config: LlmConfig,
    client: reqwest::Client,
}

impl OpenAIProvider {
    /// 构造 OpenAIProvider（需要 api_key）
    pub fn new(config: LlmConfig) -> Result<Self, LlmError> {
        if config.api_key.is_none() {
            return Err(LlmError::Config(
                "OpenAI provider requires api_key".to_string(),
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
impl LlmProvider for OpenAIProvider {
    async fn complete(
        &self,
        prompt: &str,
        req_config: &LlmRequestConfig,
    ) -> Result<LlmResponse, LlmError> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| LlmError::Config("OpenAI provider requires api_key".to_string()))?;

        let body = OpenAiRequestBody {
            model: self.config.model.clone(),
            messages: vec![OpenAiMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            max_tokens: req_config.max_tokens,
            temperature: req_config.temperature,
        };

        let url = format!("{}/chat/completions", self.config.api_base);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {api_key}"))
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

        let openai_resp: OpenAiResponse = resp.json().await.map_err(|e| LlmError::ApiError {
            status: status.as_u16(),
            message: format!("deserialize failed: {e}"),
        })?;

        let text = openai_resp
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .unwrap_or_default();

        let usage_data = openai_resp.usage.unwrap_or(OpenAiUsage {
            prompt_tokens: None,
            completion_tokens: None,
        });
        let usage = LlmUsage {
            prompt_tokens: usage_data.prompt_tokens.unwrap_or(0),
            completion_tokens: usage_data.completion_tokens.unwrap_or(0),
        };

        Ok(LlmResponse { text, usage })
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, LlmError> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| LlmError::Config("OpenAI provider requires api_key".to_string()))?;

        let body = serde_json::json!({
            "model": "text-embedding-3-small",
            "input": text,
        });

        let url = format!("{}/embeddings", self.config.api_base);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout(self.config.timeout)
                } else {
                    LlmError::ApiError {
                        status: 0,
                        message: e.to_string(),
                    }
                }
            })?;

        let status = resp.status();
        if !status.is_success() {
            let msg = resp.text().await.unwrap_or_default();
            return Err(LlmError::ApiError {
                status: status.as_u16(),
                message: msg,
            });
        }

        let embed_resp: OpenAiEmbedResponse =
            resp.json().await.map_err(|e| LlmError::ApiError {
                status: status.as_u16(),
                message: format!("deserialize failed: {e}"),
            })?;

        embed_resp
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| LlmError::ApiError {
                status: 200,
                message: "empty embedding response".to_string(),
            })
    }

    fn provider_name(&self) -> &'static str {
        "openai"
    }

    fn model(&self) -> &str {
        &self.config.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_name() {
        let config = LlmConfig {
            provider: super::super::LlmProviderKind::OpenAI,
            model: "gpt-4o".to_string(),
            api_key: Some("test-key".to_string()),
            api_base: LlmConfig::api_base_for(super::super::LlmProviderKind::OpenAI),
            timeout: std::time::Duration::from_secs(30),
            max_tokens: 2000,
            fallback: None,
        };
        let provider = OpenAIProvider::new(config).unwrap();
        assert_eq!(provider.provider_name(), "openai");
        assert_eq!(provider.model(), "gpt-4o");
    }

    #[test]
    fn test_openai_provider_no_api_key() {
        let config = LlmConfig::for_provider(super::super::LlmProviderKind::OpenAI);
        let result = OpenAIProvider::new(config);
        assert!(matches!(result, Err(LlmError::Config(_))));
    }
}
