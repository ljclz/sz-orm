//! Multimodal 核心类型

use serde::{Deserialize, Serialize};

/// 模态类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Modality {
    Text,
    Voice,
    ErDiagram,
    Screenshot,
    Sketch,
}

/// 图表规格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartSpec {
    pub chart_type: String,
    pub data: serde_json::Value,
}

/// Multimodal 错误
#[derive(Debug, thiserror::Error)]
pub enum MultimodalError {
    #[error("语音转写失败: {0}")]
    VoiceTranscribeFailed(String),
    #[error("渲染失败，降级为表格")]
    RenderFallback,
    #[error("截图识别失败")]
    ScreenshotRecognizeFailed,
}
