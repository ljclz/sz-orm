//! TASK-010 验证测试：vLLM 后端

use sz_orm_model_ops::vllm::VllmProvider;

#[tokio::test]
async fn test_vllm_provider_interface() {
    let provider = VllmProvider::new("http://localhost:8000");
    assert_eq!(provider.endpoint, "http://localhost:8000");
    let result = provider.complete("hello").await;
    assert!(result.is_err());
}
