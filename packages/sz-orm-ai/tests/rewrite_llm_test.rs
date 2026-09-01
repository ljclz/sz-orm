//! RewriteAdvisor LLM fallback 单元测试

#![cfg(all(feature = "ai-rewrite-advisor", feature = "multi-llm"))]

use sz_orm_ai::nl2sql::{ColumnInfo, SchemaContext, TableInfo};
use sz_orm_ai::rewrite_advisor::RewriteAdvisor;

fn test_schema() -> SchemaContext {
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
            ],
        }],
    }
}

#[tokio::test]
async fn test_advise_with_llm_no_router_returns_empty() {
    let advisor = RewriteAdvisor::new();
    let schema = test_schema();
    let result = advisor
        .advise_with_llm("SELECT * FROM users", &schema)
        .await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[tokio::test]
async fn test_advise_with_llm_rule_suggestions_returned_directly() {
    let advisor = RewriteAdvisor::new().with_llm();
    let schema = test_schema();
    let result = advisor
        .advise_with_llm(
            "SELECT * FROM users WHERE id IN (SELECT user_id FROM orders)",
            &schema,
        )
        .await;
    assert!(result.is_ok());
    let suggestions = result.unwrap();
    // 规则引擎应识别子查询展开建议
    assert!(!suggestions.is_empty());
}

#[tokio::test]
async fn test_advise_with_llm_empty_sql_returns_error() {
    let advisor = RewriteAdvisor::new();
    let schema = test_schema();
    let result = advisor.advise_with_llm("", &schema).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_advise_with_llm_simple_query_no_suggestions() {
    let advisor = RewriteAdvisor::new();
    let schema = test_schema();
    let result = advisor
        .advise_with_llm("SELECT id, name FROM users", &schema)
        .await;
    assert!(result.is_ok());
    // 简单查询无可优化模式，无 router 时返回空
    assert!(result.unwrap().is_empty());
}
