//! TASK-010 验证测试：本地推理后端

use sz_orm_model_ops::llamacpp::LlamaCppProvider;
use sz_orm_model_ops::vllm::VllmProvider;

#[tokio::test]
async fn test_llamacpp_provider() {
    let provider = LlamaCppProvider::new("http://localhost:8080");
    let result = provider.complete("test").await;
    assert!(result.is_err(), "未连接时返回错误");
}

#[tokio::test]
async fn test_vllm_provider() {
    let provider = VllmProvider::new("http://localhost:8000");
    let result = provider.complete("test").await;
    assert!(result.is_err(), "未连接时返回错误");
}
