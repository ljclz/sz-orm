//! GeminiProvider — Google Gemini API 实现

use super::types::{LlmRequestConfig, LlmResponse, LlmUsage};
use super::{LlmConfig, LlmError, LlmProvider};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct GeminiPart {
    text: String,
}

#[derive(Debug, Serialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize)]
struct GeminiRequestBody {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    temperature: f32,
    max_output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiContentResponse,
}

#[derive(Debug, Deserialize)]
struct GeminiContentResponse {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Deserialize)]
struct GeminiUsageMetadata {
    prompt_token_count: Option<u32>,
    candidates_token_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct GeminiEmbedResponse {
    embedding: GeminiEmbedding,
}

#[derive(Debug, Deserialize)]
struct GeminiEmbedding {
    values: Vec<f32>,
}

/// Google Gemini provider
pub struct GeminiProvider {
    config: LlmConfig,
    client: reqwest::Client,
}

impl GeminiProvider {
    /// 构造 GeminiProvider（需要 api_key）
    pub fn new(config: LlmConfig) -> Result<Self, LlmError> {
        if config.api_key.is_none() {
            return Err(LlmError::Config(
                "Gemini provider requires api_key".to_string(),
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
impl LlmProvider for GeminiProvider {
    async fn complete(
        &self,
        prompt: &str,
        req_config: &LlmRequestConfig,
    ) -> Result<LlmResponse, LlmError> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| LlmError::Config("Gemini provider requires api_key".to_string()))?;

        let body = GeminiRequestBody {
            contents: vec![GeminiContent {
                parts: vec![GeminiPart {
                    text: prompt.to_string(),
                }],
            }],
            generation_config: Some(GeminiGenerationConfig {
                temperature: req_config.temperature,
                max_output_tokens: req_config.max_tokens,
            }),
        };

        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.config.api_base, self.config.model, api_key
        );
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

        let gemini_resp: GeminiResponse = resp.json().await.map_err(|e| LlmError::ApiError {
            status: status.as_u16(),
            message: format!("deserialize failed: {e}"),
        })?;

        let text = gemini_resp
            .candidates
            .into_iter()
            .next()
            .and_then(|c| c.content.parts.into_iter().next())
            .map(|p| p.text)
            .unwrap_or_default();

        let usage_meta = gemini_resp.usage_metadata.unwrap_or(GeminiUsageMetadata {
            prompt_token_count: None,
            candidates_token_count: None,
        });
        let usage = LlmUsage {
            prompt_tokens: usage_meta.prompt_token_count.unwrap_or(0),
            completion_tokens: usage_meta.candidates_token_count.unwrap_or(0),
        };

        Ok(LlmResponse { text, usage })
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, LlmError> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| LlmError::Config("Gemini provider requires api_key".to_string()))?;

        let url = format!(
            "{}/models/text-embedding-004:embedContent?key={}",
            self.config.api_base, api_key
        );
        let body = serde_json::json!({
            "content": {
                "parts": [{ "text": text }]
            }
        });

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

        let embed_resp: GeminiEmbedResponse =
            resp.json().await.map_err(|e| LlmError::ApiError {
                status: status.as_u16(),
                message: format!("deserialize failed: {e}"),
            })?;

        Ok(embed_resp.embedding.values)
    }

    fn provider_name(&self) -> &'static str {
        "gemini"
    }

    fn model(&self) -> &str {
        &self.config.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_provider_name() {
        let config = LlmConfig {
            provider: super::super::LlmProviderKind::Gemini,
            model: "gemini-1.5-pro".to_string(),
            api_key: Some("test-key".to_string()),
            api_base: LlmConfig::api_base_for(super::super::LlmProviderKind::Gemini),
            timeout: std::time::Duration::from_secs(30),
            max_tokens: 2000,
            fallback: None,
        };
        let provider = GeminiProvider::new(config).unwrap();
        assert_eq!(provider.provider_name(), "gemini");
        assert_eq!(provider.model(), "gemini-1.5-pro");
    }

    #[test]
    fn test_gemini_provider_no_api_key() {
        let config = LlmConfig::for_provider(super::super::LlmProviderKind::Gemini);
        let result = GeminiProvider::new(config);
        assert!(matches!(result, Err(LlmError::Config(_))));
    }
}
