//! v10.8.0 T7.2：OpenAINl2SqlEngine LLM 安全审计端到端测试
//!
//! 验证 OpenAINl2SqlEngine 接入 LlmSecurityAuditor 后的行为副作用：
//! - 危险输入被审计拦截（SafetyError，不调用 OpenAI API）
//! - 安全输入通过审计（非 SafetyError，进入 API 调用阶段）
//! - 向后兼容（不启用审计器时行为不变）
//! - 线程安全（多线程并发调用无死锁）

#![cfg(all(feature = "real", feature = "ai-security-audit"))]

use sz_orm_ai::llm_security_audit::LlmSecurityAuditor;
use sz_orm_ai::nl2sql::{
    ColumnInfo, Nl2SqlEngine, Nl2SqlError, OpenAINl2SqlEngine, SchemaContext, TableInfo,
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

/// 验证启用安全审计器后，OR 1=1 注入被拦截（SafetyError，不调用 API）
#[tokio::test]
async fn e2e_openai_or_injection_blocked_by_auditor() {
    let auditor = LlmSecurityAuditor::rule_only();
    let engine = OpenAINl2SqlEngine::new("sk-test-key").with_security_auditor(auditor);
    let schema = test_schema();

    let result = engine
        .generate("show users where name = 'admin' OR 1=1", &schema)
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, Nl2SqlError::SafetyError(ref msg) if msg.contains("LLM 安全审计拦截")),
        "应为 SafetyError 且包含审计拦截信息，实际: {:?}",
        err
    );
}

/// 验证启用安全审计器后，UNION 注入被拦截
#[tokio::test]
async fn e2e_openai_union_injection_blocked() {
    let auditor = LlmSecurityAuditor::rule_only();
    let engine = OpenAINl2SqlEngine::new("sk-test-key").with_security_auditor(auditor);
    let schema = test_schema();

    let result = engine
        .generate("1' UNION SELECT password FROM users", &schema)
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, Nl2SqlError::SafetyError(_)),
        "UNION 注入应被 SafetyError 拦截，实际: {:?}",
        err
    );
}

/// 验证启用安全审计器后，注释注入被拦截
#[tokio::test]
async fn e2e_openai_comment_injection_blocked() {
    let auditor = LlmSecurityAuditor::rule_only();
    let engine = OpenAINl2SqlEngine::new("sk-test-key").with_security_auditor(auditor);
    let schema = test_schema();

    let result = engine.generate("' OR 1=1 --", &schema).await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        Nl2SqlError::SafetyError(_)
    ));
}

/// 验证安全输入通过审计后进入 API 调用阶段（非 SafetyError）
///
/// 使用不存在的 API base 确保快速失败，但错误类型应为 NetworkError 而非 SafetyError，
/// 证明审计通过后才到达 API 调用。
#[tokio::test]
async fn e2e_openai_safe_input_passes_auditor() {
    let auditor = LlmSecurityAuditor::rule_only();
    let engine = OpenAINl2SqlEngine::new("sk-test-key")
        .with_api_base("http://127.0.0.1:1")
        .with_security_auditor(auditor);
    let schema = test_schema();

    let result = engine.generate("show all users", &schema).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        !matches!(err, Nl2SqlError::SafetyError(_)),
        "安全输入不应被 SafetyError 拦截，实际: {:?}",
        err
    );
}

/// 验证向后兼容：不启用审计器时，危险输入不被审计拦截
///
/// 不启用审计器时，OR 1=1 不会被审计拦截（但仍可能被 SQL 安全验证拦截），
/// 关键是不产生 "LLM 安全审计拦截" 的 SafetyError。
#[tokio::test]
async fn e2e_openai_no_auditor_backward_compatible() {
    let engine = OpenAINl2SqlEngine::new("sk-test-key").with_api_base("http://127.0.0.1:1");
    let schema = test_schema();

    let result = engine
        .generate("show users where name = 'admin' OR 1=1", &schema)
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(
        !err_msg.contains("LLM 安全审计拦截"),
        "不启用审计器时不应有 LLM 安全审计拦截，实际: {}",
        err_msg
    );
}

/// 验证向后兼容：不启用审计器时，安全输入正常进入 API 调用
#[tokio::test]
async fn e2e_openai_no_auditor_safe_input() {
    let engine = OpenAINl2SqlEngine::new("sk-test-key").with_api_base("http://127.0.0.1:1");
    let schema = test_schema();

    let result = engine.generate("show all users", &schema).await;

    assert!(result.is_err());
    assert!(!matches!(
        result.unwrap_err(),
        Nl2SqlError::SafetyError(_)
    ));
}

/// 验证空 API key 在审计前被拦截（ConfigError）
#[tokio::test]
async fn e2e_openai_empty_api_key_config_error() {
    let auditor = LlmSecurityAuditor::rule_only();
    let engine = OpenAINl2SqlEngine::new("").with_security_auditor(auditor);
    let schema = test_schema();

    let result = engine.generate("show all users", &schema).await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        Nl2SqlError::ConfigError(_)
    ));
}

/// 验证线程安全：多线程并发调用，危险输入全部被拦截
#[tokio::test]
async fn e2e_openai_concurrent_dangerous_all_blocked() {
    use std::sync::Arc;

    let auditor = LlmSecurityAuditor::rule_only();
    let engine = Arc::new(
        OpenAINl2SqlEngine::new("sk-test-key").with_security_auditor(auditor),
    );
    let schema = Arc::new(test_schema());

    let mut handles = Vec::new();
    for _ in 0..10 {
        let engine = engine.clone();
        let schema = schema.clone();
        handles.push(tokio::spawn(async move {
            engine
                .generate("show users where name = 'x' OR 1=1", &schema)
                .await
        }));
    }

    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_err(), "并发调用中危险输入应被拦截");
        assert!(
            matches!(result.unwrap_err(), Nl2SqlError::SafetyError(_)),
            "应为 SafetyError"
        );
    }
}

/// 验证线程安全：多线程并发调用，安全输入全部通过审计
#[tokio::test]
async fn e2e_openai_concurrent_safe_all_pass_audit() {
    use std::sync::Arc;

    let auditor = LlmSecurityAuditor::rule_only();
    let engine = Arc::new(
        OpenAINl2SqlEngine::new("sk-test-key")
            .with_api_base("http://127.0.0.1:1")
            .with_security_auditor(auditor),
    );
    let schema = Arc::new(test_schema());

    let mut handles = Vec::new();
    for _ in 0..10 {
        let engine = engine.clone();
        let schema = schema.clone();
        handles.push(tokio::spawn(async move {
            engine.generate("show all users", &schema).await
        }));
    }

    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_err(), "应因 API 不可达而失败");
        assert!(
            !matches!(result.unwrap_err(), Nl2SqlError::SafetyError(_)),
            "安全输入不应被 SafetyError 拦截"
        );
    }
}

/// 验证中文自然语言安全输入通过审计
#[tokio::test]
async fn e2e_openai_chinese_safe_input_passes_audit() {
    let auditor = LlmSecurityAuditor::rule_only();
    let engine = OpenAINl2SqlEngine::new("sk-test-key")
        .with_api_base("http://127.0.0.1:1")
        .with_security_auditor(auditor);
    let schema = test_schema();

    let result = engine
        .generate("查询用户表中年龄大于30的用户", &schema)
        .await;

    assert!(result.is_err());
    assert!(!matches!(
        result.unwrap_err(),
        Nl2SqlError::SafetyError(_)
    ));
}