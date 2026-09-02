//! TASK-008 验证测试：NL 查询闭环

use sz_orm_nl_query::pipeline::NlQueryPipeline;

#[tokio::test]
async fn test_pipeline_query() {
    let pipeline = NlQueryPipeline::new();
    let result = pipeline.query("查询所有用户").await;
    assert!(result.is_err(), "骨架实现返回错误");
}
