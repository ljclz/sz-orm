//! 语音输入查询（TASK-013）
//!
//! 通过 HTTP 调用语音转写（STT）服务，将音频转为文本。
//! 未连接或服务不可达时返回包含 endpoint 的错误信息。

use crate::types::MultimodalError;

/// 语音输入处理器
///
/// 通过 HTTP 调用外部 STT 服务（如 Whisper API）进行语音转写。
pub struct VoiceInputHandler {
    pub stt_endpoint: String,
    timeout: std::time::Duration,
}

impl VoiceInputHandler {
    pub fn new(stt_endpoint: &str) -> Self {
        Self {
            stt_endpoint: stt_endpoint.to_string(),
            timeout: std::time::Duration::from_secs(30),
        }
    }

    /// 设置请求超时时间
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// 将音频字节流转写为文本
    ///
    /// 向 `stt_endpoint` 发送 POST 请求（body 为原始音频字节），
    /// 期望返回 JSON `{"text": "..."}`。服务不可达时返回错误。
    pub async fn transcribe(&self, audio: &[u8]) -> Result<String, MultimodalError> {
        if audio.is_empty() {
            return Err(MultimodalError::VoiceTranscribeFailed(
                "音频数据为空".into(),
            ));
        }

        let client = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| MultimodalError::VoiceTranscribeFailed(format!("客户端构建失败: {e}")))?;

        let response = client
            .post(&self.stt_endpoint)
            .header("Content-Type", "application/octet-stream")
            .body(audio.to_vec())
            .send()
            .await
            .map_err(|e| {
                MultimodalError::VoiceTranscribeFailed(format!(
                    "连接 {} 失败: {e}",
                    self.stt_endpoint
                ))
            })?;

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| MultimodalError::VoiceTranscribeFailed(format!("解析响应失败: {e}")))?;

        json["text"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| MultimodalError::VoiceTranscribeFailed("响应格式无效: 缺少 text".into()))
    }
}
