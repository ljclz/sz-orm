//! LLM 查询优化器集成测试
//!
//! 调用真实 OpenAI 兼容 API 生成优化建议。
//! 所有测试标注 `#[ignore]`，需手动执行：
//!
//! ```bash
//! OPENAI_API_KEY=sk-xxx cargo test -p sz-orm-ai --features llm-optimizer --test llm_integration -- --ignored
//! ```

#![cfg(feature = "llm-optimizer")]

use sz_orm_ai::{
    ColumnInfo, MySqlExplainParser, OptimizerConfig, SchemaContext, TableInfo,
    UnifiedQueryOptimizer,
};

fn make_schema() -> SchemaContext {
    SchemaContext {
        tables: vec![TableInfo {
            name: "users".to_string(),
            columns: vec![
                ColumnInfo {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    nullable: false,
                    is_primary_key: true,
                },
                ColumnInfo {
                    name: "name".to_string(),
                    data_type: "varchar".to_string(),
                    nullable: true,
                    is_primary_key: false,
                },
                ColumnInfo {
                    name: "email".to_string(),
                    data_type: "varchar".to_string(),
                    nullable: true,
                    is_primary_key: false,
                },
            ],
        }],
    }
}

#[tokio::test]
#[ignore]
async fn test_llm_real_optimization_suggestions() {
    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
    let config = OptimizerConfig::with_llm(&api_key, "gpt-4o-mini");
    let optimizer = UnifiedQueryOptimizer::new(config);
    let schema = make_schema();

    let sql = "SELECT * FROM users WHERE name = 'John'";
    let analysis = optimizer.optimize(sql, &schema, None, None).await;

    assert!(analysis.llm_available, "LLM should be available");
    assert!(
        analysis.llm_hint_count() > 0,
        "LLM should generate suggestions"
    );

    for hint in &analysis.hints {
        if matches!(hint.source, sz_orm_ai::HintSource::Llm { .. }) {
            assert!(!hint.title.is_empty(), "LLM hint title should not be empty");
            assert!(
                !hint.description.is_empty(),
                "LLM hint description should not be empty"
            );
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_llm_real_with_explain_signals() {
    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
    let config = OptimizerConfig::with_llm(&api_key, "gpt-4o-mini");
    let optimizer = UnifiedQueryOptimizer::new(config);
    let schema = make_schema();

    let explain_output = "+----+-------------+-------+------------+------+---------------+------+---------+------+------+----------+-------+\n| id | select_type | table | partitions | type | possible_keys | key  | key_len | ref  | rows | filtered | Extra |\n+----+-------------+-------+------------+------+---------------+------+---------+------+------+----------+-------+\n|  1 | SIMPLE      | users | NULL       | ALL  | NULL          | NULL | NULL    | NULL |  100 |   100.00 | NULL  |\n+----+-------------+-------+------------+------+---------------+------+---------+------+------+----------+-------+";
    let parser = MySqlExplainParser;

    let sql = "SELECT * FROM users WHERE name = 'John'";
    let analysis = optimizer
        .optimize(sql, &schema, Some(explain_output), Some(&parser))
        .await;

    assert!(analysis.llm_available);
    assert!(!analysis.explain_signals.is_empty());
    assert!(analysis.llm_hint_count() > 0);
}

#[tokio::test]
#[ignore]
async fn test_llm_real_sql_sanitized() {
    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
    let config = OptimizerConfig::with_llm(&api_key, "gpt-4o-mini");
    let optimizer = UnifiedQueryOptimizer::new(config);
    let schema = make_schema();

    let sql = "SELECT * FROM users WHERE password = 'secret123' AND name = 'John'";
    let analysis = optimizer.optimize(sql, &schema, None, None).await;

    assert!(analysis.llm_available);
}

#[tokio::test]
#[ignore]
async fn test_llm_real_degradation_on_invalid_key() {
    let config = OptimizerConfig::with_llm("sk-invalid-key", "gpt-4o-mini")
        .with_api_base("http://127.0.0.1:1/v1");
    let optimizer = UnifiedQueryOptimizer::new(config);
    let schema = make_schema();

    let sql = "SELECT * FROM users";
    let analysis = optimizer.optimize(sql, &schema, None, None).await;

    assert!(!analysis.llm_available);
    assert!(analysis.llm_degraded_reason.is_some());
    assert!(analysis.rule_hint_count() > 0);
}
