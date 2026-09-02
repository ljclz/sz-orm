//! llama.cpp 推理后端（TASK-010）
//!
//! 通过 HTTP 调用 llama.cpp server 的 `/completion` 端点。
//! 未连接或服务不可达时返回包含 endpoint 的错误信息。

use crate::types::ModelOpsError;

/// llama.cpp 推理提供者
///
/// 通过 HTTP 调用 llama.cpp server，支持其原生 `/completion` 接口。
pub struct LlamaCppProvider {
    pub endpoint: String,
    timeout: std::time::Duration,
}

impl LlamaCppProvider {
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            timeout: std::time::Duration::from_secs(30),
        }
    }

    /// 设置请求超时时间
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// 调用 llama.cpp server 完成推理
    ///
    /// 向 `{endpoint}/completion` 发送 POST 请求，
    /// 返回模型生成的文本。服务不可达时返回错误。
    pub async fn complete(&self, prompt: &str) -> Result<String, ModelOpsError> {
        let client = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| ModelOpsError::InferenceFailed(format!("客户端构建失败: {e}")))?;

        let url = format!("{}/completion", self.endpoint);
        let body = serde_json::json!({
            "prompt": prompt,
            "n_predict": 256,
            "temperature": 0.7,
        });

        let response = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ModelOpsError::InferenceFailed(format!("连接 {url} 失败: {e}")))?;

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ModelOpsError::InferenceFailed(format!("解析响应失败: {e}")))?;

        json["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| ModelOpsError::InferenceFailed("响应格式无效: 缺少 content".into()))
    }
}
