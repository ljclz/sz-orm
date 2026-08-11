//! LocalLlamaProvider — 本地 Ollama API 实现（无需 API Key）

use super::types::{LlmRequestConfig, LlmResponse, LlmUsage};
use super::{LlmConfig, LlmError, LlmProvider};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct OllamaRequestBody {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    temperature: f32,
    num_predict: u32,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: OllamaResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OllamaResponseMessage {
    content: String,
}

#[derive(Debug, Serialize)]
struct OllamaEmbedRequest {
    model: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct OllamaEmbedResponse {
    embedding: Vec<f32>,
}

/// 本地 Ollama provider（Llama 等本地模型，无需 API Key）
pub struct LocalLlamaProvider {
    config: LlmConfig,
    client: reqwest::Client,
}

impl LocalLlamaProvider {
    /// 构造 LocalLlamaProvider（无需 api_key，默认 localhost:11434）
    pub fn new(mut config: LlmConfig) -> Result<Self, LlmError> {
        if config.api_base.is_empty() {
            config.api_base = "http://localhost:11434".to_string();
        }
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| LlmError::Config(format!("reqwest client build failed: {e}")))?;
        Ok(Self { config, client })
    }
}

#[async_trait]
impl LlmProvider for LocalLlamaProvider {
    async fn complete(
        &self,
        prompt: &str,
        req_config: &LlmRequestConfig,
    ) -> Result<LlmResponse, LlmError> {
        let body = OllamaRequestBody {
            model: self.config.model.clone(),
            messages: vec![OllamaMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            stream: false,
            options: Some(OllamaOptions {
                temperature: req_config.temperature,
                num_predict: req_config.max_tokens,
            }),
        };

        let url = format!("{}/api/chat", self.config.api_base);
        let resp = self
            .client
            .post(&url)
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
        if !status.is_success() {
            let msg = resp.text().await.unwrap_or_default();
            return Err(LlmError::ApiError {
                status: status.as_u16(),
                message: msg,
            });
        }

        let ollama_resp: OllamaResponse = resp.json().await.map_err(|e| LlmError::ApiError {
            status: status.as_u16(),
            message: format!("deserialize failed: {e}"),
        })?;

        Ok(LlmResponse {
            text: ollama_resp.message.content,
            usage: LlmUsage::default(),
        })
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, LlmError> {
        let body = OllamaEmbedRequest {
            model: self.config.model.clone(),
            prompt: text.to_string(),
        };

        let url = format!("{}/api/embeddings", self.config.api_base);
        let resp = self
            .client
            .post(&url)
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
        if !status.is_success() {
            let msg = resp.text().await.unwrap_or_default();
            return Err(LlmError::ApiError {
                status: status.as_u16(),
                message: msg,
            });
        }

        let embed_resp: OllamaEmbedResponse =
            resp.json().await.map_err(|e| LlmError::ApiError {
                status: status.as_u16(),
                message: format!("deserialize failed: {e}"),
            })?;

        Ok(embed_resp.embedding)
    }

    fn provider_name(&self) -> &'static str {
        "ollama"
    }

    fn model(&self) -> &str {
        &self.config.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_provider_name() {
        let config = LlmConfig::for_provider(super::super::LlmProviderKind::Ollama);
        let provider = LocalLlamaProvider::new(config).unwrap();
        assert_eq!(provider.provider_name(), "ollama");
        assert_eq!(provider.model(), "llama3.1");
    }

    #[test]
    fn test_ollama_provider_no_api_key_ok() {
        let config = LlmConfig::for_provider(super::super::LlmProviderKind::Ollama);
        let result = LocalLlamaProvider::new(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_ollama_provider_default_api_base() {
        let mut config = LlmConfig::for_provider(super::super::LlmProviderKind::Ollama);
        config.api_base = String::new();
        let provider = LocalLlamaProvider::new(config).unwrap();
        assert_eq!(provider.config.api_base, "http://localhost:11434");
    }
}
