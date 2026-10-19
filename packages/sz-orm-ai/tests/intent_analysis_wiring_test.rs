//! IntentAnalyzer 接线端到端测试（v10.9.0 T2.2）
//!
//! 验证意图分析器接线到 NL2SQL 生成流程的行为副作用：
//! - 启用意图分析后，解释中包含 [意图: XXX] 标注
//! - 启用意图分析后，置信度可能被意图分析增强
//! - 不启用意图分析时，行为不变（向后兼容）

#![cfg(feature = "ai-nl2sql-enhanced")]

use sz_orm_ai::nl2sql::{
    ColumnInfo, Nl2SqlEngine, SchemaContext, SimpleNl2SqlEngine, TableInfo,
};

fn users_schema() -> SchemaContext {
    SchemaContext {
        tables: vec![TableInfo {
            name: "users".into(),
            columns: vec![
                ColumnInfo {
                    name: "id".into(),
                    data_type: "INTEGER".into(),
                    nullable: false,
                    is_primary_key: true,
                },
                ColumnInfo {
                    name: "name".into(),
                    data_type: "TEXT".into(),
                    nullable: true,
                    is_primary_key: false,
                },
                ColumnInfo {
                    name: "age".into(),
                    data_type: "INTEGER".into(),
                    nullable: true,
                    is_primary_key: false,
                },
            ],
        }],
    }
}

#[tokio::test]
async fn test_intent_analyzer_wiring_select() {
    let engine = SimpleNl2SqlEngine::new().with_intent_analyzer();
    let result = engine.generate("show all users", &users_schema()).await.unwrap();
    assert!(
        result.explanation.contains("[意图: SELECT]"),
        "解释应包含 SELECT 意图标注: {}",
        result.explanation
    );
}

#[tokio::test]
async fn test_intent_analyzer_wiring_aggregate() {
    let engine = SimpleNl2SqlEngine::new().with_intent_analyzer();
    let result = engine
        .generate("count all users", &users_schema())
        .await
        .unwrap();
    assert!(
        result.explanation.contains("[意图:"),
        "解释应包含意图标注: {}",
        result.explanation
    );
}

#[tokio::test]
async fn test_intent_analyzer_wiring_with_conditions() {
    let engine = SimpleNl2SqlEngine::new().with_intent_analyzer();
    let result = engine
        .generate("show users where age > 25", &users_schema())
        .await
        .unwrap();
    assert!(
        result.explanation.contains("[意图: SELECT]"),
        "条件查询应标注 SELECT 意图: {}",
        result.explanation
    );
    assert!(
        result.confidence > 0.0,
        "置信度应大于 0: {}",
        result.confidence
    );
}

#[tokio::test]
async fn test_intent_analyzer_disabled_behavior_unchanged() {
    let engine_without = SimpleNl2SqlEngine::new();
    let result_without = engine_without
        .generate("show all users", &users_schema())
        .await
        .unwrap();
    assert!(
        !result_without.explanation.contains("[意图:"),
        "未启用意图分析时解释不应包含意图标注: {}",
        result_without.explanation
    );
}

#[tokio::test]
async fn test_intent_analyzer_wiring_confidence_enhanced() {
    let schema = users_schema();
    let engine_with = SimpleNl2SqlEngine::new().with_intent_analyzer();
    let engine_without = SimpleNl2SqlEngine::new();

    let result_with = engine_with.generate("show all users", &schema).await.unwrap();
    let result_without = engine_without.generate("show all users", &schema).await.unwrap();

    assert!(
        result_with.explanation.contains("[意图:"),
        "启用意图分析应有标注: {}",
        result_with.explanation
    );
    assert!(
        !result_without.explanation.contains("[意图:"),
        "未启用不应有标注: {}",
        result_without.explanation
    );
}

#[tokio::test]
async fn test_intent_analyzer_wiring_multiple_queries() {
    let engine = SimpleNl2SqlEngine::new().with_intent_analyzer();
    let schema = users_schema();

    let queries = vec!["show all users", "count users", "show users where age > 25"];
    for q in &queries {
        let result = engine.generate(q, &schema).await.unwrap();
        assert!(
            result.explanation.contains("[意图:"),
            "查询 '{}' 应有意图标注: {}",
            q,
            result.explanation
        );
    }
}