//! 模态降级策略（TASK-031）

use crate::types::{Modality, MultimodalError};
use serde::{Deserialize, Serialize};

/// 降级策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FallbackStrategy {
    DirectToText,
    ToTextThenTable,
    RetryWithSimplerModality,
}

/// 降级结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackResult {
    pub original_modality: Modality,
    pub fallback_modality: Modality,
    pub strategy: FallbackStrategy,
    pub message: String,
}

/// 模态降级器
pub struct ModalFallback {
    max_retries: usize,
}

impl ModalFallback {
    pub fn new(max_retries: usize) -> Self {
        Self { max_retries }
    }

    /// 根据错误决定降级策略
    pub fn fallback(
        &self,
        original_modality: &Modality,
        error: &MultimodalError,
        retry_count: usize,
    ) -> Result<FallbackResult, MultimodalError> {
        if retry_count >= self.max_retries {
            return Err(MultimodalError::RenderFallback);
        }

        match original_modality {
            Modality::Voice => self.fallback_from_voice(error, retry_count),
            Modality::ErDiagram => self.fallback_from_er(error, retry_count),
            Modality::Screenshot => self.fallback_from_screenshot(error, retry_count),
            Modality::Sketch => self.fallback_from_sketch(error, retry_count),
            Modality::Text => Ok(FallbackResult {
                original_modality: original_modality.clone(),
                fallback_modality: Modality::Text,
                strategy: FallbackStrategy::DirectToText,
                message: "文本模态无需降级".to_string(),
            }),
        }
    }

    fn fallback_from_voice(
        &self,
        error: &MultimodalError,
        retry_count: usize,
    ) -> Result<FallbackResult, MultimodalError> {
        match error {
            MultimodalError::VoiceTranscribeFailed(_) => Ok(FallbackResult {
                original_modality: Modality::Voice,
                fallback_modality: Modality::Text,
                strategy: FallbackStrategy::DirectToText,
                message: format!(
                    "语音转写失败（重试 {}/{}），降级为文本输入",
                    retry_count + 1,
                    self.max_retries
                ),
            }),
            _ => Ok(FallbackResult {
                original_modality: Modality::Voice,
                fallback_modality: Modality::Text,
                strategy: FallbackStrategy::DirectToText,
                message: "语音模态降级为文本".to_string(),
            }),
        }
    }

    fn fallback_from_er(
        &self,
        _error: &MultimodalError,
        _retry_count: usize,
    ) -> Result<FallbackResult, MultimodalError> {
        Ok(FallbackResult {
            original_modality: Modality::ErDiagram,
            fallback_modality: Modality::Text,
            strategy: FallbackStrategy::ToTextThenTable,
            message: "ER 图渲染失败，降级为文本+表格描述".to_string(),
        })
    }

    fn fallback_from_screenshot(
        &self,
        error: &MultimodalError,
        _retry_count: usize,
    ) -> Result<FallbackResult, MultimodalError> {
        match error {
            MultimodalError::ScreenshotRecognizeFailed => Ok(FallbackResult {
                original_modality: Modality::Screenshot,
                fallback_modality: Modality::Text,
                strategy: FallbackStrategy::RetryWithSimplerModality,
                message: "截图识别失败，请改用文本描述".to_string(),
            }),
            _ => Ok(FallbackResult {
                original_modality: Modality::Screenshot,
                fallback_modality: Modality::Text,
                strategy: FallbackStrategy::DirectToText,
                message: "截图模态降级为文本".to_string(),
            }),
        }
    }

    fn fallback_from_sketch(
        &self,
        _error: &MultimodalError,
        _retry_count: usize,
    ) -> Result<FallbackResult, MultimodalError> {
        Ok(FallbackResult {
            original_modality: Modality::Sketch,
            fallback_modality: Modality::Text,
            strategy: FallbackStrategy::ToTextThenTable,
            message: "草图识别失败，降级为文本描述".to_string(),
        })
    }

    /// 获取降级链
    pub fn fallback_chain(&self, modality: &Modality) -> Vec<Modality> {
        match modality {
            Modality::Text => vec![Modality::Text],
            Modality::Voice => vec![Modality::Voice, Modality::Text],
            Modality::ErDiagram => vec![Modality::ErDiagram, Modality::Text],
            Modality::Screenshot => vec![Modality::Screenshot, Modality::Text],
            Modality::Sketch => vec![Modality::Sketch, Modality::Text],
        }
    }
}

impl Default for ModalFallback {
    fn default() -> Self {
        Self::new(3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_voice_to_text() {
        let fallback = ModalFallback::default();
        let result = fallback
            .fallback(
                &Modality::Voice,
                &MultimodalError::VoiceTranscribeFailed("音频质量差".to_string()),
                0,
            )
            .unwrap();

        assert_eq!(result.original_modality, Modality::Voice);
        assert_eq!(result.fallback_modality, Modality::Text);
        assert_eq!(result.strategy, FallbackStrategy::DirectToText);
    }

    #[test]
    fn test_fallback_er_to_text_table() {
        let fallback = ModalFallback::default();
        let result = fallback
            .fallback(&Modality::ErDiagram, &MultimodalError::RenderFallback, 0)
            .unwrap();

        assert_eq!(result.fallback_modality, Modality::Text);
        assert_eq!(result.strategy, FallbackStrategy::ToTextThenTable);
    }

    #[test]
    fn test_fallback_screenshot_retry() {
        let fallback = ModalFallback::default();
        let result = fallback
            .fallback(
                &Modality::Screenshot,
                &MultimodalError::ScreenshotRecognizeFailed,
                0,
            )
            .unwrap();

        assert_eq!(result.strategy, FallbackStrategy::RetryWithSimplerModality);
    }

    #[test]
    fn test_fallback_text_no_degradation() {
        let fallback = ModalFallback::default();
        let result = fallback
            .fallback(&Modality::Text, &MultimodalError::RenderFallback, 0)
            .unwrap();

        assert_eq!(result.fallback_modality, Modality::Text);
        assert_eq!(result.strategy, FallbackStrategy::DirectToText);
    }

    #[test]
    fn test_max_retries_exceeded() {
        let fallback = ModalFallback::new(2);
        assert!(fallback
            .fallback(&Modality::Voice, &MultimodalError::RenderFallback, 2)
            .is_err());
        assert!(fallback
            .fallback(&Modality::Voice, &MultimodalError::RenderFallback, 3)
            .is_err());
    }

    #[test]
    fn test_fallback_chain() {
        let fallback = ModalFallback::default();

        let voice_chain = fallback.fallback_chain(&Modality::Voice);
        assert_eq!(voice_chain, vec![Modality::Voice, Modality::Text]);

        let text_chain = fallback.fallback_chain(&Modality::Text);
        assert_eq!(text_chain, vec![Modality::Text]);

        let sketch_chain = fallback.fallback_chain(&Modality::Sketch);
        assert_eq!(sketch_chain, vec![Modality::Sketch, Modality::Text]);
    }

    #[test]
    fn test_fallback_sketch() {
        let fallback = ModalFallback::default();
        let result = fallback
            .fallback(&Modality::Sketch, &MultimodalError::RenderFallback, 0)
            .unwrap();

        assert_eq!(result.fallback_modality, Modality::Text);
        assert_eq!(result.strategy, FallbackStrategy::ToTextThenTable);
    }
}
