//! TASK-008 验证测试：NL 查询闭环

use sz_orm_nl_query::pipeline::NlQueryPipeline;

#[tokio::test]
async fn test_pipeline_query() {
    let pipeline = NlQueryPipeline::new();
    let result = pipeline.query("查询所有用户").await;
    assert!(result.is_ok(), "查询应成功");
    let resp = result.unwrap();
    assert!(resp.sql.contains("SELECT"));
}
