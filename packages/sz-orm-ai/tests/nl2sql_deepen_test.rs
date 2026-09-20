//! v7.7.0 任务 2.7：NL2SQL 深化端到端测试
//!
//! 验证意图理解 + 复杂查询分解的完整流程：
//! - 意图理解（多意图识别 + 实体提取 + 置信度 + 修正建议）
//! - 复杂查询分解（参数化 SQL + 多轮对话上下文 + 分解步骤）
//! - 单轮延迟 ≤ 1.5s
//! - 参数化验证（禁止字符串拼接）
//! - 可解释性（intent_basis 非空）

#![cfg(feature = "ai-nl2sql-deepen")]

use sz_orm_ai::{ComplexQueryDecomposer, ConversationContext, IntentType, IntentUnderstander};

#[test]
fn e2e_intent_understand_select() {
    let understander = IntentUnderstander::new();
    let result = understander.understand("查询用户").unwrap();
    assert_eq!(result.primary_intent, IntentType::Select);
    assert!(result.confidence > 0.0);
    assert!(!result.intent_basis.is_empty());
}

#[test]
fn e2e_intent_understand_multiple_intents() {
    let understander = IntentUnderstander::new();
    let result = understander
        .understand("关联用户和订单并按销量排序")
        .unwrap();
    assert_eq!(result.primary_intent, IntentType::Join);
    assert!(result.secondary_intents.contains(&IntentType::Sort));
}

#[test]
fn e2e_intent_understand_low_confidence_suggestion() {
    let understander = IntentUnderstander::new();
    let result = understander.understand("查询").unwrap();
    assert!(result.correction_suggestion.is_some());
}

#[tokio::test]
async fn e2e_complex_query_decompose_user() {
    let decomposer = ComplexQueryDecomposer::new();
    let ctx = ConversationContext::new();
    let result = decomposer.decompose("查询用户", &ctx).await.unwrap();
    assert!(result.is_parameterized);
    assert!(result.intent_understood);
    assert!(result.complex_query_decomposed);
    assert!(result.generation_latency_ms <= 1500.0);
    assert!(result.generated_sql.contains("$1"));
}

#[tokio::test]
async fn e2e_complex_query_decompose_with_context() {
    let decomposer = ComplexQueryDecomposer::new();
    let mut ctx = ConversationContext::new();
    ctx.add_turn("查询用户", "SELECT * FROM users");
    let result = decomposer.decompose("排序", &ctx).await.unwrap();
    assert_eq!(result.dialog_round, 2);
    assert!(result.dialog_context_preserved);
}

#[tokio::test]
async fn e2e_complex_query_decompose_steps() {
    let decomposer = ComplexQueryDecomposer::new();
    let ctx = ConversationContext::new();
    let result = decomposer.decompose("查询用户", &ctx).await.unwrap();
    assert!(result.decomposition_steps.len() >= 3);
    assert!(result.decomposition_steps[0].contains("意图理解"));
}

#[tokio::test]
async fn e2e_complex_query_decompose_parameterized() {
    let decomposer = ComplexQueryDecomposer::new();
    let ctx = ConversationContext::new();
    let result = decomposer.decompose("查询订单", &ctx).await.unwrap();
    assert!(result.is_parameterized);
    assert!(result.generated_sql.contains("$1"));
    assert!(!result.generated_sql.contains("''"));
}
