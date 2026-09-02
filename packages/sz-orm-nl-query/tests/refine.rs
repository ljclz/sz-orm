//! TASK-025 集成测试：追问+对话上下文端到端验证

use sz_orm_nl_query::pipeline::{ConversationContext, ConversationTurn, NlQueryPipeline};

#[tokio::test]
async fn test_refine_single_turn() {
    let mut pipeline = NlQueryPipeline::new();
    let result = pipeline.refine("查询所有订单").await.unwrap();
    assert!(result.sql.contains("SELECT"));
    assert_eq!(pipeline.context().history.len(), 1);
}

#[tokio::test]
async fn test_refine_multi_turn_accumulates() {
    let mut pipeline = NlQueryPipeline::new();

    pipeline.refine("查询用户").await.unwrap();
    pipeline.refine("只看活跃的").await.unwrap();
    pipeline.refine("按注册时间排序").await.unwrap();
    pipeline.refine("取前 10 条").await.unwrap();

    assert_eq!(pipeline.context().history.len(), 4);
}

#[tokio::test]
async fn test_refine_context_includes_previous_sql() {
    let mut pipeline = NlQueryPipeline::new();
    pipeline.set_tables(vec!["users".to_string(), "orders".to_string()]);

    pipeline.refine("查询用户").await.unwrap();
    let second = pipeline.refine("关联订单").await.unwrap();

    assert!(second.sql.contains("SELECT"));
    assert!(!pipeline.context().last_tables.is_empty());
}

#[test]
fn test_conversation_context_serialization() {
    let mut ctx = ConversationContext::new();
    ctx.add_turn("查询用户", "SELECT * FROM users");
    ctx.last_tables = vec!["users".to_string()];

    let json = serde_json::to_string(&ctx).unwrap();
    let restored: ConversationContext = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.history.len(), 1);
    assert_eq!(restored.last_tables, vec!["users".to_string()]);
}

#[test]
fn test_build_refine_prompt_with_context() {
    let mut ctx = ConversationContext::new();
    ctx.add_turn("查询订单", "SELECT * FROM orders");
    ctx.last_tables = vec!["orders".to_string()];

    let prompt = ctx.build_refine_prompt("按金额降序");
    assert!(prompt.contains("SELECT * FROM orders"));
    assert!(prompt.contains("orders"));
    assert!(prompt.contains("按金额降序"));
}

#[test]
fn test_conversation_turn_structure() {
    let turn = ConversationTurn {
        user_query: "测试查询".to_string(),
        generated_sql: "SELECT 1".to_string(),
        timestamp: "2024-01-01T00:00:00Z".to_string(),
    };
    assert_eq!(turn.user_query, "测试查询");
    assert_eq!(turn.generated_sql, "SELECT 1");
}
