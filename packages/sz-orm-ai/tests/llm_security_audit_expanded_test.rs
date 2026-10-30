//! LLM 安全审计扩展规则端到端测试（v10.9.0 T3.1）
//!
//! 验证新增 10 个 InjectionPattern 的检测能力和 false positive 控制：
//! - 每个新规则检测到对应攻击向量并返回正确 RiskLevel
//! - 安全输入不被新规则误判

#![cfg(feature = "ai-security-audit")]

use sz_orm_ai::llm_security_audit::{LlmSecurityAuditor, RiskLevel};

#[tokio::test]
async fn test_llm_audit_or_1_eq_1_variant_detected() {
    let mut auditor = LlmSecurityAuditor::rule_only();
    let result = auditor.audit("' or 1=1 --").await.unwrap();
    assert!(
        result.risk_level.is_dangerous(),
        "OR 注入变体应被检测为危险: {:?}",
        result.risk_level
    );
}

#[tokio::test]
async fn test_llm_audit_union_injection_encoded_detected() {
    let mut auditor = LlmSecurityAuditor::rule_only();
    let result = auditor
        .audit("%27%20UNION SELECT password FROM users")
        .await
        .unwrap();
    assert!(
        result.risk_level.is_dangerous(),
        "URL 编码 UNION 注入应被检测为危险: {:?}",
        result.risk_level
    );
    assert!(
        result
            .detected_patterns
            .iter()
            .any(|p| p == "union_injection_encoded"),
        "应匹配 union_injection_encoded 规则: {:?}",
        result.detected_patterns
    );
}

#[tokio::test]
async fn test_llm_audit_stacked_injection_variant_detected() {
    let mut auditor = LlmSecurityAuditor::rule_only();
    let result = auditor.audit("1;DROP TABLE users").await.unwrap();
    assert!(
        result.risk_level.is_dangerous(),
        "无空格堆叠注入应被检测为危险: {:?}",
        result.risk_level
    );
}

#[tokio::test]
async fn test_llm_audit_comment_injection_nested_detected() {
    let mut auditor = LlmSecurityAuditor::rule_only();
    let result = auditor
        .audit("SELECT /* comment */ * FROM users")
        .await
        .unwrap();
    assert!(
        result.risk_level == RiskLevel::Medium || result.risk_level.is_dangerous(),
        "嵌套注释应被检测: {:?}",
        result.risk_level
    );
}

#[tokio::test]
async fn test_llm_audit_sensitive_data_password_detected() {
    let mut auditor = LlmSecurityAuditor::rule_only();
    let result = auditor
        .audit("SELECT * FROM users WHERE password = 'abc'")
        .await
        .unwrap();
    assert!(
        result.risk_level.is_dangerous(),
        "password 敏感数据泄露应被检测为危险: {:?}",
        result.risk_level
    );
    assert!(
        result
            .detected_patterns
            .iter()
            .any(|p| p == "sensitive_data_password"),
        "应匹配 sensitive_data_password 规则: {:?}",
        result.detected_patterns
    );
}

#[tokio::test]
async fn test_llm_audit_sensitive_data_secret_detected() {
    let mut auditor = LlmSecurityAuditor::rule_only();
    let result = auditor
        .audit("SELECT * FROM config WHERE secret = 'xyz'")
        .await
        .unwrap();
    assert!(
        result.risk_level.is_dangerous(),
        "secret 敏感数据泄露应被检测为危险: {:?}",
        result.risk_level
    );
}

#[tokio::test]
async fn test_llm_audit_sensitive_data_token_detected() {
    let mut auditor = LlmSecurityAuditor::rule_only();
    let result = auditor
        .audit("SELECT * FROM sessions WHERE token = 'abc123'")
        .await
        .unwrap();
    assert!(
        result.risk_level.is_dangerous(),
        "token 敏感数据泄露应被检测为危险: {:?}",
        result.risk_level
    );
}

#[tokio::test]
async fn test_llm_audit_privilege_escalation_drop_detected() {
    let mut auditor = LlmSecurityAuditor::rule_only();
    let result = auditor.audit("DROP TABLE users").await.unwrap();
    assert!(
        result.risk_level.is_dangerous(),
        "DROP TABLE 越权操作应被检测为危险: {:?}",
        result.risk_level
    );
}

#[tokio::test]
async fn test_llm_audit_privilege_escalation_alter_detected() {
    let mut auditor = LlmSecurityAuditor::rule_only();
    let result = auditor
        .audit("ALTER TABLE users ADD COLUMN email TEXT")
        .await
        .unwrap();
    assert!(
        result.risk_level.is_dangerous(),
        "ALTER TABLE 越权操作应被检测为危险: {:?}",
        result.risk_level
    );
}

#[tokio::test]
async fn test_llm_audit_privilege_escalation_grant_detected() {
    let mut auditor = LlmSecurityAuditor::rule_only();
    let result = auditor
        .audit("GRANT SELECT ON users TO public")
        .await
        .unwrap();
    assert!(
        result.risk_level.is_dangerous(),
        "GRANT 越权操作应被检测为危险: {:?}",
        result.risk_level
    );
}

#[tokio::test]
async fn test_llm_audit_safe_input_not_flagged() {
    let mut auditor = LlmSecurityAuditor::rule_only();
    let safe_inputs = vec![
        "SELECT id, name FROM users WHERE age > 18",
        "INSERT INTO products (name, price) VALUES ('apple', 5.99)",
        "UPDATE orders SET status = 'shipped' WHERE id = 100",
        "SELECT COUNT(*) FROM orders WHERE created_at > '2026-01-01'",
        "SELECT u.name, o.total FROM users u JOIN orders o ON u.id = o.user_id",
    ];
    for input in safe_inputs {
        let result = auditor.audit(input).await.unwrap();
        assert!(
            !result.risk_level.is_dangerous(),
            "安全输入不应被标记为危险: {:?} -> {:?}",
            input,
            result.risk_level
        );
    }
}

#[tokio::test]
async fn test_llm_audit_builtin_pattern_count_is_14() {
    let auditor = LlmSecurityAuditor::rule_only();
    let count = auditor.pattern_store().len();
    assert_eq!(
        count, 14,
        "内置规则数应为 14（4 既有 + 10 新增）: 实际 {}",
        count
    );
}
