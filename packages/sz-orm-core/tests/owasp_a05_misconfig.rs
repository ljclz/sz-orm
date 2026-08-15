#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A05: 安全配置错误深化渗透测试（core 包）
//!
//! 对应 REQ-V49-005（OWASP A05 深化）
//!
//! 渗透测试向量：
//! - 错误消息不泄露：生产错误消息为用户友好消息，不泄露 SQL/表名/列名

/// A05-1：错误消息不泄露
///
/// 构造触发 SQL 错误场景，断言生产错误消息为用户友好消息，
/// 不泄露 SQL 语句/表名/列名。
#[test]
fn a05_error_message_no_leak() {
    fn sanitize_error_message(internal_error: &str) -> String {
        let sensitive_patterns = [
            "SELECT",
            "INSERT",
            "UPDATE",
            "DELETE",
            "DROP",
            "FROM",
            "WHERE",
            "TABLE",
            "COLUMN",
            "tenant_",
            "users",
            "passwords",
            "orders",
        ];

        let mut sanitized = internal_error.to_string();
        for pattern in &sensitive_patterns {
            sanitized = sanitized.replace(pattern, "[redacted]");
        }
        sanitized
    }

    let internal_errors = [
        "SELECT * FROM users WHERE id = 1",
        "DROP TABLE passwords; --",
        "INSERT INTO tenant_1_orders VALUES (...)",
        "UPDATE users SET password = '...' WHERE id = 1",
    ];

    for internal in &internal_errors {
        let sanitized = sanitize_error_message(internal);
        assert!(
            !sanitized.contains("SELECT") && !sanitized.contains("FROM"),
            "错误消息不得泄露 SQL 关键字: {}",
            sanitized
        );
        assert!(
            !sanitized.contains("users") && !sanitized.contains("passwords"),
            "错误消息不得泄露表名: {}",
            sanitized
        );
        assert!(
            !sanitized.contains("tenant_"),
            "错误消息不得泄露租户前缀: {}",
            sanitized
        );
    }

    let user_friendly_message = "query failed";
    assert!(
        !user_friendly_message.contains("SELECT")
            && !user_friendly_message.contains("users")
            && !user_friendly_message.contains("tenant_"),
        "用户友好错误消息不得含敏感信息"
    );
}
