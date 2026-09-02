//! TASK-031 集成测试：模态降级端到端验证

use sz_orm_multimodal::fallback::{FallbackResult, FallbackStrategy, ModalFallback};
use sz_orm_multimodal::types::Modality;
use sz_orm_multimodal::types::MultimodalError;

#[test]
fn test_voice_fallback_to_text() {
    let fallback = ModalFallback::default();
    let result: FallbackResult = fallback
        .fallback(
            &Modality::Voice,
            &MultimodalError::VoiceTranscribeFailed("音频质量差".to_string()),
            0,
        )
        .unwrap();

    assert_eq!(result.original_modality, Modality::Voice);
    assert_eq!(result.fallback_modality, Modality::Text);
    assert_eq!(result.strategy, FallbackStrategy::DirectToText);
    assert!(result.message.contains("语音"));
}

#[test]
fn test_er_diagram_fallback_to_text_table() {
    let fallback = ModalFallback::default();
    let result = fallback
        .fallback(&Modality::ErDiagram, &MultimodalError::RenderFallback, 0)
        .unwrap();

    assert_eq!(result.fallback_modality, Modality::Text);
    assert_eq!(result.strategy, FallbackStrategy::ToTextThenTable);
}

#[test]
fn test_screenshot_fallback_retry() {
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
fn test_sketch_fallback() {
    let fallback = ModalFallback::default();
    let result = fallback
        .fallback(&Modality::Sketch, &MultimodalError::RenderFallback, 0)
        .unwrap();

    assert_eq!(result.fallback_modality, Modality::Text);
    assert_eq!(result.strategy, FallbackStrategy::ToTextThenTable);
}

#[test]
fn test_text_no_fallback_needed() {
    let fallback = ModalFallback::default();
    let result = fallback
        .fallback(&Modality::Text, &MultimodalError::RenderFallback, 0)
        .unwrap();

    assert_eq!(result.fallback_modality, Modality::Text);
    assert_eq!(result.strategy, FallbackStrategy::DirectToText);
}

#[test]
fn test_max_retries_exceeded() {
    let fallback = ModalFallback::new(3);

    assert!(fallback
        .fallback(&Modality::Voice, &MultimodalError::RenderFallback, 2)
        .is_ok());
    assert!(fallback
        .fallback(&Modality::Voice, &MultimodalError::RenderFallback, 3)
        .is_err());
}

#[test]
fn test_fallback_chain_all_modalities() {
    let fallback = ModalFallback::default();

    assert_eq!(
        fallback.fallback_chain(&Modality::Text),
        vec![Modality::Text]
    );
    assert_eq!(
        fallback.fallback_chain(&Modality::Voice),
        vec![Modality::Voice, Modality::Text]
    );
    assert_eq!(
        fallback.fallback_chain(&Modality::ErDiagram),
        vec![Modality::ErDiagram, Modality::Text]
    );
    assert_eq!(
        fallback.fallback_chain(&Modality::Screenshot),
        vec![Modality::Screenshot, Modality::Text]
    );
    assert_eq!(
        fallback.fallback_chain(&Modality::Sketch),
        vec![Modality::Sketch, Modality::Text]
    );
}

#[test]
fn test_fallback_result_serialization() {
    let result = FallbackResult {
        original_modality: Modality::Voice,
        fallback_modality: Modality::Text,
        strategy: FallbackStrategy::DirectToText,
        message: "降级".to_string(),
    };
    let json = serde_json::to_string(&result).unwrap();
    let restored: FallbackResult = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.original_modality, Modality::Voice);
    assert_eq!(restored.fallback_modality, Modality::Text);
}
