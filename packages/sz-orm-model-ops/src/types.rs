//! Model Ops 核心类型

use serde::{Deserialize, Serialize};

/// 量化方式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Quantization {
    None,
    Int4,
    Int8,
}

/// 模型路由配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRouterConfig {
    pub small_model: String,
    pub medium_model: String,
    pub large_model: String,
    pub fallback: String,
}

impl Default for ModelRouterConfig {
    fn default() -> Self {
        Self {
            small_model: "qwen-1.8b".to_string(),
            medium_model: "qwen-7b".to_string(),
            large_model: "qwen-14b".to_string(),
            fallback: "qwen-1.8b".to_string(),
        }
    }
}

/// Model Ops 错误
#[derive(Debug, thiserror::Error)]
pub enum ModelOpsError {
    #[error("无可用模型候选")]
    NoCandidate,
    #[error("模型推理失败: {0}")]
    InferenceFailed(String),
    #[error("量化准确率劣化: {0}")]
    QuantizationAccuracyDrop(String),
}
