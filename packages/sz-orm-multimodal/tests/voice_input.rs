//! TASK-013 验证测试：语音输入

use sz_orm_multimodal::voice::VoiceInputHandler;

#[tokio::test]
async fn test_voice_input_handler() {
    let handler = VoiceInputHandler::new("http://localhost:9000/stt");
    assert_eq!(handler.stt_endpoint, "http://localhost:9000/stt");
    let result = handler.transcribe(&[]).await;
    assert!(result.is_err(), "未连接时返回错误");
}
