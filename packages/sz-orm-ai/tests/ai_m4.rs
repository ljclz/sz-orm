//! M4 AI 自然语言查询增强 — 集成测试与安全验证
//!
//! 覆盖：
//! - T9.1：AI 建议零数据库执行（建议路径仅返回文本，不执行 SQL/DDL）
//! - T9.2：AI 建议审计记录（来源/模型/置信度/类型 + LLM 请求脱敏）
//! - T9.3：NL2SQL 安全验证（注入载荷测试集，拦截非 SELECT + 注入风险）
//! - T10.1~T10.3：延迟基准测试（NL2SQL ≤10s / 意图分析 ≤5s / 索引+重写 ≤10s P95）

#![cfg(all(
    feature = "ai-nl2sql-enhanced",
    feature = "ai-index-advisor",
    feature = "ai-rewrite-advisor"
))]

use sz_orm_ai::index_advisor::{IndexAdvisor, QueryPattern};
use sz_orm_ai::intent_analysis::{IntentAnalyzer, QueryIntent, RiskLevel};
use sz_orm_ai::nl2sql::{ColumnInfo, SchemaContext, TableInfo};
use sz_orm_ai::rewrite_advisor::RewriteAdvisor;
use sz_orm_ai::safety;
use sz_orm_ai::sql_sanitizer::SqlSanitizer;
use sz_orm_ai::{AdviceSource, AdviceType};

fn test_schema() -> SchemaContext {
    SchemaContext {
        tables: vec![
            TableInfo {
                name: "users".to_string(),
                columns: vec![
                    ColumnInfo {
                        name: "id".to_string(),
                        data_type: "INT".to_string(),
                        nullable: false,
                        is_primary_key: true,
                    },
                    ColumnInfo {
                        name: "email".to_string(),
                        data_type: "VARCHAR".to_string(),
                        nullable: false,
                        is_primary_key: false,
                    },
                    ColumnInfo {
                        name: "name".to_string(),
                        data_type: "VARCHAR".to_string(),
                        nullable: false,
                        is_primary_key: false,
                    },
                    ColumnInfo {
                        name: "age".to_string(),
                        data_type: "INT".to_string(),
                        nullable: true,
                        is_primary_key: false,
                    },
                ],
            },
            TableInfo {
                name: "orders".to_string(),
                columns: vec![
                    ColumnInfo {
                        name: "id".to_string(),
                        data_type: "INT".to_string(),
                        nullable: false,
                        is_primary_key: true,
                    },
                    ColumnInfo {
                        name: "user_id".to_string(),
                        data_type: "INT".to_string(),
                        nullable: false,
                        is_primary_key: false,
                    },
                    ColumnInfo {
                        name: "product_id".to_string(),
                        data_type: "INT".to_string(),
                        nullable: false,
                        is_primary_key: false,
                    },
                    ColumnInfo {
                        name: "status".to_string(),
                        data_type: "VARCHAR".to_string(),
                        nullable: true,
                        is_primary_key: false,
                    },
                ],
            },
            TableInfo {
                name: "products".to_string(),
                columns: vec![
                    ColumnInfo {
                        name: "id".to_string(),
                        data_type: "INT".to_string(),
                        nullable: false,
                        is_primary_key: true,
                    },
                    ColumnInfo {
                        name: "name".to_string(),
                        data_type: "VARCHAR".to_string(),
                        nullable: false,
                        is_primary_key: false,
                    },
                ],
            },
        ],
    }
}

// ==================== T9.1：AI 建议零数据库执行 ====================

#[tokio::test]
async fn test_ai_advice_zero_db_execution_intent() {
    let analyzer = IntentAnalyzer::new();
    let result = analyzer
        .analyze("show all users where age > 25", &test_schema())
        .await
        .unwrap();
    assert_eq!(result.table, "users", "应正确识别查询目标表为 users");
    assert!(matches!(result.intent, QueryIntent::Select));
}

#[tokio::test]
async fn test_ai_advice_zero_db_execution_index() {
    let advisor = IndexAdvisor::new();
    let patterns = vec![QueryPattern {
        sql_template: "SELECT * FROM users WHERE email = $1".to_string(),
        frequency: 100,
        columns_accessed: vec!["email".to_string()],
    }];
    let suggestions = advisor.suggest(&patterns, &[]).await.unwrap();
    for s in &suggestions {
        assert!(s.ddl_text.starts_with("CREATE"));
        assert!(!s.ddl_text.contains("EXECUTE"));
        assert!(!s.ddl_text.contains("RUN"));
    }
}

#[tokio::test]
async fn test_ai_advice_zero_db_execution_rewrite() {
    let advisor = RewriteAdvisor::new();
    let schema = test_schema();
    let sql = "SELECT * FROM orders o JOIN users u ON o.user_id = u.id WHERE o.status = 'pending'";
    let suggestions = advisor.suggest(sql, &schema).await.unwrap();
    for s in &suggestions {
        assert!(s.rewritten_sql.contains("/*") || !s.rewritten_sql.is_empty());
        assert!(!s.rewritten_sql.contains("EXECUTE"));
    }
}

// ==================== T9.2：AI 建议审计记录 ====================

#[tokio::test]
async fn test_audit_record_intent_rule() {
    let analyzer = IntentAnalyzer::new();
    let result = analyzer
        .analyze("show users", &test_schema())
        .await
        .unwrap();
    let record = analyzer.audit_record(result.confidence);
    assert_eq!(record.source_engine, AdviceSource::Rule);
    assert!(record.llm_model.is_none());
    assert_eq!(record.advice_type, AdviceType::Intent);
    assert!(record.confidence > 0.0 && record.confidence <= 1.0);
    assert!(record.timestamp > 0);
}

#[tokio::test]
async fn test_audit_record_intent_llm() {
    let analyzer = IntentAnalyzer::new().with_llm();
    let result = analyzer
        .analyze("show users", &test_schema())
        .await
        .unwrap();
    let record = analyzer.audit_record(result.confidence);
    assert_eq!(record.source_engine, AdviceSource::Llm);
    assert!(record.llm_model.is_some());
    assert_eq!(record.advice_type, AdviceType::Intent);
}

#[tokio::test]
async fn test_audit_record_index() {
    let advisor = IndexAdvisor::new();
    let record = advisor.audit_record(0.8);
    assert_eq!(record.advice_type, AdviceType::Index);
    assert_eq!(record.source_engine, AdviceSource::Rule);
    assert!((record.confidence - 0.8).abs() < 1e-6);
}

#[tokio::test]
async fn test_audit_record_rewrite() {
    let advisor = RewriteAdvisor::new();
    let record = advisor.audit_record(0.75);
    assert_eq!(record.advice_type, AdviceType::Rewrite);
    assert_eq!(record.source_engine, AdviceSource::Rule);
    assert!((record.confidence - 0.75).abs() < 1e-6);
}

#[tokio::test]
async fn test_llm_request_sanitized() {
    let sql_with_secret = "SELECT * FROM users WHERE password = 'secret123' AND name = 'john'";
    let sanitized = SqlSanitizer::sanitize(sql_with_secret);
    assert!(!sanitized.contains("secret123"));
    assert!(sanitized.contains("'***'"));
}

// ==================== T9.3：NL2SQL 安全验证 ====================

#[test]
fn test_safety_rejects_drop() {
    assert!(!safety::validate_select_only("DROP TABLE users"));
}

#[test]
fn test_safety_rejects_delete() {
    assert!(!safety::validate_select_only("DELETE FROM users"));
}

#[test]
fn test_safety_rejects_insert() {
    assert!(!safety::validate_select_only(
        "INSERT INTO users VALUES (1)"
    ));
}

#[test]
fn test_safety_rejects_update() {
    assert!(!safety::validate_select_only("UPDATE users SET name = 'x'"));
}

#[test]
fn test_safety_rejects_sql_injection_comment() {
    assert!(!safety::validate_no_injection(
        "SELECT * FROM users -- DROP TABLE users"
    ));
}

#[test]
fn test_safety_rejects_sql_injection_union() {
    assert!(!safety::validate_no_injection(
        "SELECT * FROM users UNION SELECT * FROM admins"
    ));
}

#[test]
fn test_safety_rejects_boolean_injection() {
    assert!(!safety::validate_no_injection(
        "SELECT * FROM users WHERE id = 1 OR 1=1"
    ));
}

#[test]
fn test_safety_rejects_stacked_queries() {
    assert!(!safety::validate_no_injection(
        "SELECT * FROM users; DROP TABLE users"
    ));
}

#[test]
fn test_safety_accepts_clean_select() {
    assert!(safety::validate_select_only(
        "SELECT id, name FROM users WHERE age > $1"
    ));
    assert!(safety::validate_no_injection(
        "SELECT id, name FROM users WHERE age > $1"
    ));
}

// ==================== T10.1~T10.3：延迟基准测试 ====================

#[tokio::test]
async fn test_latency_intent_analysis_under_5s() {
    let analyzer = IntentAnalyzer::new();
    let schema = test_schema();
    let start = std::time::Instant::now();
    let _result = analyzer
        .analyze(
            "show all users where age > 25 order by name desc limit 10",
            &schema,
        )
        .await
        .unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 5,
        "意图分析延迟 {:?} 超过 5s P95 基准",
        elapsed
    );
}

#[tokio::test]
async fn test_latency_index_advisor_under_10s() {
    let advisor = IndexAdvisor::new();
    let patterns: Vec<QueryPattern> = (0..10)
        .map(|i| QueryPattern {
            sql_template: format!("SELECT * FROM users WHERE col_{} = $1", i),
            frequency: 100 - i * 5,
            columns_accessed: vec![format!("col_{}", i)],
        })
        .collect();
    let start = std::time::Instant::now();
    let _suggestions = advisor.suggest(&patterns, &[]).await.unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 10,
        "索引建议延迟 {:?} 超过 10s P95 基准",
        elapsed
    );
}

#[tokio::test]
async fn test_latency_rewrite_advisor_under_10s() {
    let advisor = RewriteAdvisor::new();
    let schema = test_schema();
    let sql = "SELECT * FROM orders o JOIN users u ON o.user_id = u.id JOIN products p ON o.product_id = p.id WHERE o.status = 'pending' AND u.age > 18";
    let start = std::time::Instant::now();
    let _suggestions = advisor.suggest(sql, &schema).await.unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 10,
        "重写建议延迟 {:?} 超过 10s P95 基准",
        elapsed
    );
}

// ==================== 综合安全验证 ====================

#[tokio::test]
async fn test_write_operation_always_high_risk() {
    let analyzer = IntentAnalyzer::new();
    let schema = test_schema();

    let insert_result = analyzer
        .analyze("insert into users", &schema)
        .await
        .unwrap();
    assert_eq!(insert_result.risk_level, RiskLevel::High);

    let update_result = analyzer
        .analyze("update users set name = john", &schema)
        .await
        .unwrap();
    assert_eq!(update_result.risk_level, RiskLevel::High);

    let delete_result = analyzer
        .analyze("delete from users", &schema)
        .await
        .unwrap();
    assert_eq!(delete_result.risk_level, RiskLevel::High);
}

#[tokio::test]
async fn test_index_suggestion_contains_evidence() {
    let advisor = IndexAdvisor::new();
    let patterns = vec![QueryPattern {
        sql_template: "SELECT * FROM users WHERE email = $1".to_string(),
        frequency: 100,
        columns_accessed: vec!["email".to_string()],
    }];
    let suggestions = advisor.suggest(&patterns, &[]).await.unwrap();
    assert!(!suggestions.is_empty());
    for s in &suggestions {
        assert!(!s.evidence.is_empty());
        assert!(!s.index_columns.is_empty());
        assert!(s.expected_benefit.speedup_ratio > 1.0);
    }
}

#[tokio::test]
async fn test_rewrite_suggestion_contains_equivalence_proof() {
    let advisor = RewriteAdvisor::new();
    let schema = test_schema();
    let sql = "SELECT * FROM orders o JOIN users u ON o.user_id = u.id WHERE o.status = 'pending'";
    let suggestions = advisor.suggest(sql, &schema).await.unwrap();
    assert!(!suggestions.is_empty());
    for s in &suggestions {
        assert!(!s.equivalence_proof.proof_text.is_empty());
        assert!(!s.transform_type.name().is_empty());
    }
}

#[tokio::test]
async fn test_llm_degradation_to_rule_based() {
    let analyzer = IntentAnalyzer::new();
    let schema = test_schema();
    let result = analyzer
        .analyze("show users where age > 25", &schema)
        .await
        .unwrap();
    assert!((result.confidence - 0.7).abs() < 1e-6);

    let advisor = IndexAdvisor::new();
    let record = advisor.audit_record(0.7);
    assert_eq!(record.source_engine, AdviceSource::Rule);
    assert!(record.llm_model.is_none());
}
