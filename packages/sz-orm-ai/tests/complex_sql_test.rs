//! v7.6.0 组2.7：ComplexSqlGenerator + ClarificationEngine 端到端测试
//!
//! 验证：
//! - 复杂 SQL 生成（JOIN/子查询/聚合/窗口函数）
//! - 追问澄清
//! - 多轮对话上下文保持
//! - 参数化验证

use sz_orm_ai::{
    Ambiguity, Clarification, ClarificationEngine, ColumnInfo, ComplexSqlGenerator,
    ConversationContext, SchemaContext, SqlComplexity, TableInfo,
};

fn make_schema(n: usize) -> SchemaContext {
    let tables = (0..n)
        .map(|i| TableInfo {
            name: format!("table_{}", i),
            columns: vec![
                ColumnInfo {
                    name: "id".to_string(),
                    data_type: "int".to_string(),
                    nullable: false,
                    is_primary_key: true,
                },
                ColumnInfo {
                    name: "status".to_string(),
                    data_type: "varchar".to_string(),
                    nullable: false,
                    is_primary_key: false,
                },
                ColumnInfo {
                    name: "created_at".to_string(),
                    data_type: "timestamp".to_string(),
                    nullable: false,
                    is_primary_key: false,
                },
            ],
        })
        .collect();
    SchemaContext { tables }
}

#[test]
fn test_e2e_complex_sql_all_types() {
    let schema = make_schema(2);

    for complexity in [
        SqlComplexity::Simple,
        SqlComplexity::Join,
        SqlComplexity::Subquery,
        SqlComplexity::Aggregate,
        SqlComplexity::WindowFunction,
        SqlComplexity::Complex,
    ] {
        let result = ComplexSqlGenerator::generate_complex("test query", &schema, complexity);
        assert!(result.is_ok(), "复杂度 {:?} 应成功", complexity);
        let r = result.unwrap();
        assert!(r.sql.sql.contains("$1"), "SQL 应参数化");
        assert!(r.latency_ms <= 2000, "单轮延迟应 ≤ 2s");
    }
}

#[test]
fn test_e2e_clarification_flow() {
    let nl = "查询最近活跃用户";
    let amb = ClarificationEngine::detect_ambiguity(nl);
    assert!(amb.is_some());

    let amb = amb.unwrap();
    let question = ClarificationEngine::clarify(&amb);
    assert!(!question.options.is_empty());

    let ctx = ConversationContext::new();
    let clarification = Clarification {
        selected_interpretation: question.options[0].clone(),
        user_response: question.options[0].clone(),
    };
    let result = ClarificationEngine::regenerate_with_clarification(nl, &clarification, &ctx);
    assert!(result.is_ok());
    let sql = result.unwrap();
    assert!(sql.contains("$1"), "重新生成的 SQL 应参数化");
}

#[test]
fn test_e2e_multi_turn_context_preserved() {
    let mut ctx = ConversationContext::new().with_max_turns(5);
    ctx.add_turn("查询用户", "SELECT * FROM users LIMIT $1");
    ctx.add_turn("筛选活跃", "SELECT * FROM users WHERE status = $1 LIMIT $2");

    assert_eq!(ctx.history.len(), 2);

    let amb = Ambiguity {
        ambiguous_term: "活跃".to_string(),
        possible_interpretations: vec!["最近登录".to_string(), "有交易记录".to_string()],
    };
    let q = ClarificationEngine::clarify(&amb);
    assert_eq!(q.options.len(), 2);

    let clarification = Clarification {
        selected_interpretation: "最近登录".to_string(),
        user_response: "最近登录".to_string(),
    };
    let result =
        ClarificationEngine::regenerate_with_clarification("查询活跃用户", &clarification, &ctx);
    assert!(result.is_ok());
    let sql = result.unwrap();
    assert!(sql.contains("last_login"));
}

#[test]
fn test_e2e_no_ambiguity() {
    let nl = "查询所有订单";
    let amb = ClarificationEngine::detect_ambiguity(nl);
    assert!(amb.is_none());
}

#[test]
fn test_e2e_complex_sql_join_structure() {
    let schema = make_schema(2);
    let result = ComplexSqlGenerator::generate_complex("JOIN 查询", &schema, SqlComplexity::Join);
    assert!(result.is_ok());
    let r = result.unwrap();
    assert!(r.sql.sql.contains("INNER JOIN"));
    assert!(r.sql.sql.contains("ON"));
    assert!(r.sql.sql.contains("$1"));
}

#[test]
fn test_e2e_complex_sql_window_function() {
    let schema = make_schema(1);
    let result =
        ComplexSqlGenerator::generate_complex("排名查询", &schema, SqlComplexity::WindowFunction);
    assert!(result.is_ok());
    let r = result.unwrap();
    assert!(r.sql.sql.contains("ROW_NUMBER()"));
    assert!(r.sql.sql.contains("OVER"));
}
