//! v10.7.0 T7.2：NL2SQL 三重安全检查管线端到端测试
//!
//! 验证管线串联：NL → validate_no_injection → SqlSanitizer::sanitize → validate_select_only
//! 端到端行为副作用，非单个组件单元测试。

#![cfg(feature = "ai-nl2sql-enhanced")]

use sz_orm_ai::nl2sql::{
    ColumnInfo, MultiTurnContext, Nl2SqlEngine, Nl2SqlError, SchemaContext, SimpleNl2SqlEngine,
    TableInfo,
};
use sz_orm_ai::safety;

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

fn multi_turn_ctx(schema: &SchemaContext) -> MultiTurnContext {
    MultiTurnContext {
        schema: schema.clone(),
        history: vec![],
    }
}

// ==================== 第一重：自然语言注入防护 ====================

/// 验证含注入向量的自然语言被 validate_no_injection 拦截
#[tokio::test]
async fn e2e_nl_injection_blocked_at_gate() {
    let engine = SimpleNl2SqlEngine::new();
    let schema = users_schema();
    let ctx = multi_turn_ctx(&schema);

    let result = engine.nl2sql("' OR 1=1 --", &ctx).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, Nl2SqlError::SafetyError(_)),
        "注入输入应在 nl2sql 入口被 SafetyError 拦截"
    );
}

/// 验证 UNION 注入在自然语言入口被拦截
#[tokio::test]
async fn e2e_nl_union_injection_blocked() {
    let engine = SimpleNl2SqlEngine::new();
    let schema = users_schema();
    let ctx = multi_turn_ctx(&schema);

    let result = engine
        .nl2sql("1' UNION SELECT password FROM users --", &ctx)
        .await;

    assert!(result.is_err());
}

// ==================== 第二重：SQL 清洗 ====================

/// 验证 Nl2sqlResult::enforce_injection_filter 对含注入的 SQL 标记并清洗敏感字面量
#[tokio::test]
async fn e2e_sanitizer_cleans_injection_in_result() {
    use sz_orm_ai::nl2sql::{Nl2sqlResult, SqlQuery};

    let schema = users_schema();
    let mut result = Nl2sqlResult {
        sql: SqlQuery {
            sql: "SELECT * FROM users WHERE password = 'secret123' OR 1=1".into(),
            explanation: String::new(),
            confidence: 1.0,
            dialect: None,
            cache_hit: false,
        },
        intent: sz_orm_ai::nl2sql::IntentAnalysis::from_query("show users", &schema),
        latency_ms: 0,
        injection_filtered: false,
    };

    result.enforce_injection_filter();

    assert!(
        result.injection_filtered,
        "含 OR 1=1 注入的 SQL 应标记 injection_filtered = true"
    );
    assert!(
        !result.sql.sql.contains("secret123"),
        "清洗后敏感字面量应被脱敏"
    );
    assert!(result.sql.sql.contains("***"), "脱敏后应包含 *** 占位");
}

/// 验证安全 SQL 不被清洗（injection_filtered 保持 false）
#[tokio::test]
async fn e2e_sanitizer_leaves_safe_sql_untouched() {
    use sz_orm_ai::nl2sql::{Nl2sqlResult, SqlQuery};

    let schema = users_schema();
    let mut result = Nl2sqlResult {
        sql: SqlQuery {
            sql: "SELECT * FROM users WHERE age > $1".into(),
            explanation: String::new(),
            confidence: 1.0,
            dialect: None,
            cache_hit: false,
        },
        intent: sz_orm_ai::nl2sql::IntentAnalysis::from_query("show users where age > 30", &schema),
        latency_ms: 0,
        injection_filtered: false,
    };

    result.enforce_injection_filter();

    assert!(
        !result.injection_filtered,
        "安全 SQL 不应触发 injection_filtered"
    );
    assert_eq!(
        result.sql.sql, "SELECT * FROM users WHERE age > $1",
        "安全 SQL 不应被修改"
    );
}

// ==================== 第三重：SELECT-only 验证 ====================

/// 验证 safety::validate_select_only 拒绝 DROP
#[test]
fn e2e_select_only_rejects_drop() {
    assert!(!safety::validate_select_only("DROP TABLE users"));
}

/// 验证 safety::validate_select_only 拒绝 DELETE
#[test]
fn e2e_select_only_rejects_delete() {
    assert!(!safety::validate_select_only("DELETE FROM users"));
}

/// 验证 safety::validate_select_only 拒绝 TRUNCATE
#[test]
fn e2e_select_only_rejects_truncate() {
    assert!(!safety::validate_select_only("TRUNCATE TABLE users"));
}

/// 验证 safety::validate_select_only 接受 SELECT
#[test]
fn e2e_select_only_accepts_select() {
    assert!(safety::validate_select_only(
        "SELECT * FROM users WHERE id = $1"
    ));
}

// ==================== 管线串联：正常查询端到端 ====================

/// 验证正常自然语言通过三重检查生成有效 SQL
#[tokio::test]
async fn e2e_normal_query_passes_all_checks() {
    let engine = SimpleNl2SqlEngine::new();
    let schema = users_schema();
    let ctx = multi_turn_ctx(&schema);

    let result = engine.nl2sql("show all users", &ctx).await;

    assert!(result.is_ok(), "正常查询应通过三重检查");
    let nl2sql_result = result.unwrap();
    assert!(
        nl2sql_result.sql.sql.to_uppercase().contains("SELECT"),
        "生成的 SQL 应为 SELECT 语句"
    );
    assert!(
        safety::validate_select_only(&nl2sql_result.sql.sql),
        "生成的 SQL 应通过 select_only 验证"
    );
}

/// 验证含 WHERE 条件的正常查询通过三重检查
#[tokio::test]
async fn e2e_conditional_query_passes_all_checks() {
    let engine = SimpleNl2SqlEngine::new();
    let schema = users_schema();
    let ctx = multi_turn_ctx(&schema);

    let result = engine.nl2sql("show users where age > 30", &ctx).await;

    assert!(result.is_ok(), "条件查询应通过三重检查");
    let nl2sql_result = result.unwrap();
    assert!(
        nl2sql_result.sql.sql.to_uppercase().contains("WHERE"),
        "生成的 SQL 应包含 WHERE 子句"
    );
}
