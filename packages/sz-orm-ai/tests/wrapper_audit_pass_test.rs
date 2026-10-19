//! v10.8.0 T7.3：泛型包装器审计传递端到端测试
//!
//! 验证 MultiTurnNl2SqlEngine<E> 和 ValidatedNl2SqlEngine<E, V> 通过委托
//! 内部引擎 E::generate 自动传递审计能力。
//!
//! 当内部引擎 E（如 SimpleNl2SqlEngine）已接入 LlmSecurityAuditor 时，
//! 包装器的 generate 方法委托 E::generate，审计在 E 内部触发。

#![cfg(feature = "ai-security-audit")]

use sz_orm_ai::llm_security_audit::LlmSecurityAuditor;
use sz_orm_ai::multi_turn::MultiTurnNl2SqlEngine;
use sz_orm_ai::nl2sql::{
    ColumnInfo, Nl2SqlError, SchemaContext, SimpleNl2SqlEngine, TableInfo,
};

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

// ==================== MultiTurnNl2SqlEngine 审计传递 ====================

/// 验证 MultiTurnNl2SqlEngine<SimpleNl2SqlEngine> 危险输入通过委托被审计拦截
#[tokio::test]
async fn e2e_multi_turn_dangerous_input_blocked() {
    let auditor = LlmSecurityAuditor::rule_only();
    let inner = SimpleNl2SqlEngine::new().with_security_auditor(auditor);
    let mut engine = MultiTurnNl2SqlEngine::new(inner);
    let schema = test_schema();

    let result = engine
        .generate_with_context("show users where name = 'admin' OR 1=1", &schema)
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, Nl2SqlError::SafetyError(ref msg) if msg.contains("LLM 安全审计拦截")),
        "MultiTurn 包装器应通过委托触发审计拦截，实际: {:?}",
        err
    );
}

/// 验证 MultiTurnNl2SqlEngine<SimpleNl2SqlEngine> 安全输入通过审计正常生成
#[tokio::test]
async fn e2e_multi_turn_safe_input_passes() {
    let auditor = LlmSecurityAuditor::rule_only();
    let inner = SimpleNl2SqlEngine::new().with_security_auditor(auditor);
    let mut engine = MultiTurnNl2SqlEngine::new(inner);
    let schema = test_schema();

    let result = engine.generate_with_context("show all users", &schema).await;

    assert!(result.is_ok(), "安全输入应通过审计正常生成 SQL");
    let sql = result.unwrap();
    assert!(sql.sql.to_uppercase().contains("SELECT"));
}

/// 验证 MultiTurnNl2SqlEngine<SimpleNl2SqlEngine> UNION 注入通过委托被拦截
#[tokio::test]
async fn e2e_multi_turn_union_injection_blocked() {
    let auditor = LlmSecurityAuditor::rule_only();
    let inner = SimpleNl2SqlEngine::new().with_security_auditor(auditor);
    let mut engine = MultiTurnNl2SqlEngine::new(inner);
    let schema = test_schema();

    let result = engine
        .generate_with_context("1' UNION SELECT password FROM users", &schema)
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        Nl2SqlError::SafetyError(_)
    ));
}

/// 验证 MultiTurnNl2SqlEngine 不启用审计器时向后兼容
#[tokio::test]
async fn e2e_multi_turn_no_auditor_backward_compatible() {
    let inner = SimpleNl2SqlEngine::new();
    let mut engine = MultiTurnNl2SqlEngine::new(inner);
    let schema = test_schema();

    let result = engine.generate_with_context("show all users", &schema).await;

    assert!(result.is_ok(), "不启用审计器时应正常工作");
}

/// 验证 MultiTurnNl2SqlEngine 多轮对话中每轮都触发审计
#[tokio::test]
async fn e2e_multi_turn_audit_every_turn() {
    let auditor = LlmSecurityAuditor::rule_only();
    let inner = SimpleNl2SqlEngine::new().with_security_auditor(auditor);
    let mut engine = MultiTurnNl2SqlEngine::new(inner);
    let schema = test_schema();

    // 第一轮：安全输入通过
    let r1 = engine.generate_with_context("show all users", &schema).await;
    assert!(r1.is_ok(), "第一轮安全输入应通过");

    // 第二轮：危险输入被拦截（即使有历史上下文）
    let r2 = engine
        .generate_with_context("show users where name = 'x' OR 1=1", &schema)
        .await;
    assert!(r2.is_err());
    assert!(matches!(
        r2.unwrap_err(),
        Nl2SqlError::SafetyError(_)
    ));
}

// ==================== ValidatedNl2SqlEngine 审计传递 ====================

#[cfg(feature = "ai-nl2sql-enhanced")]
mod validated_wrapper_tests {
    use super::*;
    use sz_orm_ai::sql_validator::{StaticSqlValidator, ValidatedNl2SqlEngine};

    /// 验证 ValidatedNl2SqlEngine<SimpleNl2SqlEngine, V> 危险输入通过委托被审计拦截
    #[tokio::test]
    async fn e2e_validated_dangerous_input_blocked() {
        let auditor = LlmSecurityAuditor::rule_only();
        let inner = SimpleNl2SqlEngine::new().with_security_auditor(auditor);
        let validator = StaticSqlValidator::new();
        let engine = ValidatedNl2SqlEngine::new(inner, validator);
        let schema = test_schema();

        let result = engine
            .generate_validated("show users where name = 'admin' OR 1=1", &schema, None)
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, Nl2SqlError::SafetyError(ref msg) if msg.contains("LLM 安全审计拦截")),
            "Validated 包装器应通过委托触发审计拦截，实际: {:?}",
            err
        );
    }

    /// 验证 ValidatedNl2SqlEngine<SimpleNl2SqlEngine, V> 安全输入通过审计
    #[tokio::test]
    async fn e2e_validated_safe_input_passes_audit() {
        let auditor = LlmSecurityAuditor::rule_only();
        let inner = SimpleNl2SqlEngine::new().with_security_auditor(auditor);
        let validator = StaticSqlValidator::new();
        let engine = ValidatedNl2SqlEngine::new(inner, validator);
        let schema = test_schema();

        let result = engine
            .generate_validated("show all users", &schema, None)
            .await;

        assert!(result.is_ok(), "安全输入应通过审计和验证");
    }

    /// 验证 ValidatedNl2SqlEngine 不启用审计器时向后兼容
    #[tokio::test]
    async fn e2e_validated_no_auditor_backward_compatible() {
        let inner = SimpleNl2SqlEngine::new();
        let validator = StaticSqlValidator::new();
        let engine = ValidatedNl2SqlEngine::new(inner, validator);
        let schema = test_schema();

        let result = engine.generate_validated("show all users", &schema, None).await;

        assert!(result.is_ok(), "不启用审计器时应正常工作");
    }

    /// 验证 ValidatedNl2SqlEngine UNION 注入通过委托被拦截
    #[tokio::test]
    async fn e2e_validated_union_injection_blocked() {
        let auditor = LlmSecurityAuditor::rule_only();
        let inner = SimpleNl2SqlEngine::new().with_security_auditor(auditor);
        let validator = StaticSqlValidator::new();
        let engine = ValidatedNl2SqlEngine::new(inner, validator);
        let schema = test_schema();

        let result = engine
            .generate_validated("1' UNION SELECT password FROM users", &schema, None)
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Nl2SqlError::SafetyError(_)
        ));
    }
}