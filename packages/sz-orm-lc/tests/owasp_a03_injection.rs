#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A03: 注入深化渗透测试（lc 包）
//!
//! 对应 REQ-V49-003（OWASP A03 深化）
//!
//! 渗透测试向量：
//! - 模板注入转义：`{{7*7}}` / `${7*7}` / `<%= 7*7 %>` 被 validate_identifier 拒绝
//! - 表名验证（FIND-001 修复）：恶意表名 `users" DROP TABLE users; --` 被拒绝

use sz_orm_lc::ModelDefinition;

/// A03-1：模板注入被转义/拒绝
///
/// 构造模板注入向量 `{{7*7}}` / `${7*7}` / `<%= 7*7 %>`，
/// 断言 `validate_identifier` 拒绝（含非字母/数字/下划线字符）。
#[test]
fn a03_template_injection_escaped() {
    let template_injections = [
        "{{7*7}}",
        "${7*7}",
        "<%= 7*7 %>",
        "{{constructor.constructor('return process')().exit()}}",
        "${#rt}",
        "<#assign x = 7*7>",
    ];

    for injection in &template_injections {
        let result = ModelDefinition::validate_identifier(injection);
        assert!(
            result.is_err(),
            "模板注入向量 `{}` 必须被 validate_identifier 拒绝",
            injection
        );
    }

    let valid_names = ["users", "order_items", "table_123", "a"];
    for name in &valid_names {
        let result = ModelDefinition::validate_identifier(name);
        assert!(
            result.is_ok(),
            "合法标识符 `{}` 必须通过 validate_identifier",
            name
        );
    }
}

/// A03-2：表名验证（FIND-001 修复）
///
/// 构造恶意表名 `users" DROP TABLE users; --`，
/// 断言 `validate_identifier` 拒绝（仅允许字母/数字/下划线）。
#[test]
fn a03_model_name_validation_finds_001() {
    let malicious_names = [
        "users\" DROP TABLE users; --",
        "users'; DROP TABLE users; --",
        "users; DROP TABLE users",
        "users UNION SELECT * FROM passwords",
        "users--",
        "users/*",
        "users OR 1=1",
        "users` DROP TABLE users",
        "users|rm -rf /",
        "users$(whoami)",
        "users&calc",
        "users\ncmd",
    ];

    for name in &malicious_names {
        let result = ModelDefinition::validate_identifier(name);
        assert!(
            result.is_err(),
            "恶意表名 `{}` 必须被 validate_identifier 拒绝（FIND-001 修复）",
            name
        );
    }

    let _valid_model = ModelDefinition::new("users");
    let valid_result = ModelDefinition::validate_identifier("users");
    assert!(valid_result.is_ok(), "合法表名必须通过验证");
}
