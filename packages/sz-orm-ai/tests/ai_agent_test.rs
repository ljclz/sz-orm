//! TASK-023: AiAgent + AnalysisAgent 单元测试
//!
//! 验证 Agent 执行多步（查销售数据 → 查用户行为 → LLM 分析 → 生成报告）。

use std::sync::Arc;

use sz_orm_ai::semantic_query::{
    AgentError, AiAgent, AnalysisAgent, Nl2SqlConverter, SemanticQueryError,
};

struct MockNl2Sql;
#[async_trait::async_trait]
impl Nl2SqlConverter for MockNl2Sql {
    async fn convert(&self, query: &str) -> Result<String, SemanticQueryError> {
        Ok(format!("SELECT * FROM data WHERE task = '{}'", query))
    }
}

#[tokio::test]
async fn test_agent_execute_analysis_task() {
    let agent = AnalysisAgent::new(Arc::new(MockNl2Sql));
    let report = agent.execute_task("分析上月销售下降原因").await.unwrap();

    assert!(!report.steps.is_empty());
    assert!(report.conclusion.contains("子任务"));
    assert!(report.confidence > 0.0);
}

#[tokio::test]
async fn test_agent_multiple_steps() {
    let agent = AnalysisAgent::new(Arc::new(MockNl2Sql));
    let report = agent.execute_task("分析上月销售下降原因").await.unwrap();

    assert!(report.steps.len() >= 2);
    for (i, step) in report.steps.iter().enumerate() {
        assert_eq!(step.step_number, (i + 1) as u32);
        assert!(!step.description.is_empty());
    }
}

#[tokio::test]
async fn test_agent_max_steps() {
    let agent = AnalysisAgent::new(Arc::new(MockNl2Sql)).with_max_steps(5);
    let report = agent.execute_task("分析上月销售下降原因").await.unwrap();

    assert!(report.steps.len() <= 5);
    assert_eq!(report.steps.len(), 3);
}

#[tokio::test]
async fn test_agent_exceeds_max_steps() {
    let agent = AnalysisAgent::new(Arc::new(MockNl2Sql)).with_max_steps(1);
    let result = agent.execute_task("分析上月销售下降原因").await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_agent_report_structure() {
    let agent = AnalysisAgent::new(Arc::new(MockNl2Sql));
    let report = agent.execute_task("分析销售原因").await.unwrap();

    assert!(!report.conclusion.is_empty());
    assert!(report.confidence >= 0.0 && report.confidence <= 1.0);
    for step in &report.steps {
        assert!(step.step_number > 0);
        assert!(!step.description.is_empty());
        assert!(!step.result.is_empty());
    }
}

#[tokio::test]
async fn test_agent_steps_success() {
    let agent = AnalysisAgent::new(Arc::new(MockNl2Sql));
    let report = agent.execute_task("分析销售原因").await.unwrap();

    let success_count = report.steps.iter().filter(|s| s.success).count();
    assert!(success_count > 0);
}

#[tokio::test]
async fn test_agent_confidence_calculation() {
    let agent = AnalysisAgent::new(Arc::new(MockNl2Sql));
    let report = agent.execute_task("分析销售原因").await.unwrap();

    let success_count = report.steps.iter().filter(|s| s.success).count();
    let expected_confidence = success_count as f64 / report.steps.len() as f64;
    assert!((report.confidence - expected_confidence).abs() < 1e-6);
}

#[tokio::test]
async fn test_agent_simple_task() {
    let agent = AnalysisAgent::new(Arc::new(MockNl2Sql));
    let report = agent.execute_task("查询用户数据").await.unwrap();

    assert_eq!(report.steps.len(), 1);
    assert!(report.steps[0].success);
}

#[test]
fn test_agent_error_max_steps_exceeded() {
    let err = AgentError::MaxStepsExceeded(10);
    assert!(err.to_string().contains("10"));
}
