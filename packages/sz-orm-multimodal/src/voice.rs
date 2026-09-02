//! 语音输入查询（TASK-013）

use crate::types::MultimodalError;

pub struct VoiceInputHandler {
    pub stt_endpoint: String,
}

impl VoiceInputHandler {
    pub fn new(stt_endpoint: &str) -> Self {
        Self {
            stt_endpoint: stt_endpoint.to_string(),
        }
    }

    pub async fn transcribe(&self, _audio: &[u8]) -> Result<String, MultimodalError> {
        Err(MultimodalError::VoiceTranscribeFailed("未连接".into()))
    }
}
