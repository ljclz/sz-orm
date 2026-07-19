//! 真实 OpenAI 兼容 API 嵌入客户端
//!
//! 仅在启用 `real` feature 时编译。
//!
//! 通过 HTTP 调用 OpenAI 兼容的 `/v1/embeddings` 接口获取真实向量，
//! 与内存实现 [`crate::SimpleEmbeddingModel`] 共享同一 [`EmbeddingModel`] trait，
//! 业务代码可无感切换。
//!
//! # 用法
//!
//! ```ignore
//! use sz_orm_ai::{EmbeddingModel, OpenAIEmbeddingClient};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let client = OpenAIEmbeddingClient::new("sk-xxxx")
//!     .with_model("text-embedding-3-small")
//!     .with_dimension(1536);
//! let v = client.embed("hello world").await?;
//! # Ok(())
//! # }
//! ```

use crate::embedding::EmbeddingModel;
use crate::error::AiError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// OpenAI 兼容的嵌入客户端
///
/// 调用 `{api_base}/embeddings` 接口获取真实向量。通过 builder 风格的链式方法
/// 配置 `api_base` / `model` / `dimension`。
///
/// 与 [`crate::SimpleEmbeddingModel`] 实现同一 trait，可在 RAG 引擎中互换使用。
pub struct OpenAIEmbeddingClient {
    /// API 基础地址（不含 `/embeddings` 后缀），默认 `https://api.openai.com/v1`
    api_base: String,
    /// API Key（Bearer token）
    api_key: String,
    /// 嵌入模型名称，默认 `text-embedding-3-small`
    model: String,
    /// 输出向量维度，默认 1536
    dimension: usize,
    /// HTTP 客户端
    http_client: reqwest::Client,
}

impl OpenAIEmbeddingClient {
    /// 默认 API 基础地址
    const DEFAULT_API_BASE: &'static str = "https://api.openai.com/v1";
    /// 默认嵌入模型
    const DEFAULT_MODEL: &'static str = "text-embedding-3-small";
    /// 默认向量维度
    const DEFAULT_DIMENSION: usize = 1536;

    /// 创建客户端实例
    ///
    /// 使用默认 api_base / model / dimension，仅传入 API key。
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_base: Self::DEFAULT_API_BASE.to_string(),
            api_key: api_key.into(),
            model: Self::DEFAULT_MODEL.to_string(),
            dimension: Self::DEFAULT_DIMENSION,
            http_client: reqwest::Client::new(),
        }
    }

    /// 设置 API base URL（例如自托管 OpenAI 兼容服务、Azure OpenAI 等）
    pub fn with_api_base(mut self, api_base: impl Into<String>) -> Self {
        self.api_base = api_base.into();
        self
    }

    /// 设置嵌入模型名称
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// 设置输出向量维度
    pub fn with_dimension(mut self, dimension: usize) -> Self {
        self.dimension = dimension;
        self
    }

    /// 校验 API key 非空（避免发出注定失败的请求）
    fn ensure_api_key(&self) -> Result<(), AiError> {
        if self.api_key.is_empty() {
            return Err(AiError::ConfigError(
                "API key is empty, cannot call OpenAI API".to_string(),
            ));
        }
        Ok(())
    }
}

// ============ 请求 / 响应结构 ============

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: serde_json::Value,
    dimensions: usize,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbeddingModel for OpenAIEmbeddingClient {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, AiError> {
        self.ensure_api_key()?;

        let body = EmbeddingRequest {
            model: &self.model,
            input: serde_json::Value::String(text.to_string()),
            dimensions: self.dimension,
        };

        let url = format!("{}/embeddings", self.api_base);
        let resp = self
            .http_client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(AiError::ApiError(status, message));
        }

        let parsed: EmbeddingResponse = resp
            .json()
            .await
            .map_err(|e| AiError::NetworkError(e.to_string()))?;

        parsed
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| AiError::ApiError(status, "empty data array in response".to_string()))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
        self.ensure_api_key()?;

        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // 用 serde_json::Value 直接构造数组，避免借用问题
        let input_array: Vec<serde_json::Value> = texts
            .iter()
            .map(|t| serde_json::Value::String(t.clone()))
            .collect();

        let body = EmbeddingRequest {
            model: &self.model,
            input: serde_json::Value::Array(input_array),
            dimensions: self.dimension,
        };

        let url = format!("{}/embeddings", self.api_base);
        let resp = self
            .http_client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(AiError::ApiError(status, message));
        }

        let parsed: EmbeddingResponse = resp
            .json()
            .await
            .map_err(|e| AiError::NetworkError(e.to_string()))?;

        // OpenAI 批量返回顺序与输入顺序一致
        Ok(parsed.data.into_iter().map(|d| d.embedding).collect())
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_client_new() {
        // 验证 new() 的默认值
        let client = OpenAIEmbeddingClient::new("sk-test-key");
        assert_eq!(client.api_base, "https://api.openai.com/v1");
        assert_eq!(client.api_key, "sk-test-key");
        assert_eq!(client.model, "text-embedding-3-small");
        assert_eq!(client.dimension, 1536);
    }

    #[test]
    fn test_openai_client_with_options() {
        // 验证 builder 链式方法
        let client = OpenAIEmbeddingClient::new("sk-test")
            .with_api_base("https://api.deepseek.com/v1")
            .with_model("text-embedding-3-large")
            .with_dimension(3072);
        assert_eq!(client.api_base, "https://api.deepseek.com/v1");
        assert_eq!(client.model, "text-embedding-3-large");
        assert_eq!(client.dimension, 3072);
    }

    #[test]
    fn test_openai_client_dimension_and_name() {
        // 验证 trait 方法 dimension() / model_name()
        let client = OpenAIEmbeddingClient::new("k")
            .with_model("custom-embed")
            .with_dimension(768);
        assert_eq!(client.dimension(), 768);
        assert_eq!(client.model_name(), "custom-embed");
    }

    #[tokio::test]
    async fn test_openai_client_missing_api_key() {
        // 空 API key 应返回 ConfigError（仅校验，不发请求）
        let client = OpenAIEmbeddingClient::new("");
        let result = client.embed("hello").await;
        match result {
            Err(AiError::ConfigError(_)) => {}
            other => panic!("expected AiError::ConfigError, got {:?}", other),
        }

        // embed_batch 同样应校验
        let client = OpenAIEmbeddingClient::new("");
        let result = client.embed_batch(&["a".to_string()]).await;
        match result {
            Err(AiError::ConfigError(_)) => {}
            other => panic!("expected AiError::ConfigError, got {:?}", other),
        }
    }

    // ===================== 真实 API 集成测试（CI 不跑） =====================

    #[tokio::test]
    #[ignore = "需要真实 OPENAI_API_KEY，CI 跳过"]
    async fn test_real_openai_embed() {
        let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY 未设置");
        let client = OpenAIEmbeddingClient::new(api_key);
        let v = client.embed("hello world").await.expect("embed 失败");
        assert!(!v.is_empty(), "嵌入向量不应为空");
        assert_eq!(v.len(), client.dimension(), "向量维度应与配置一致");
    }

    #[tokio::test]
    #[ignore = "需要真实 OPENAI_API_KEY，CI 跳过"]
    async fn test_real_openai_embed_batch() {
        let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY 未设置");
        let client = OpenAIEmbeddingClient::new(api_key);
        let texts = vec!["hello".to_string(), "world".to_string()];
        let vecs = client.embed_batch(&texts).await.expect("batch embed 失败");
        assert_eq!(vecs.len(), 2, "批量返回数量应匹配");
        for v in &vecs {
            assert_eq!(v.len(), client.dimension(), "每条向量维度应一致");
        }
    }
}
