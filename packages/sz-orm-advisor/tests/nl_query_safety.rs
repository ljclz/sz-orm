//! NL 查询安全校验接线测试

use sz_orm_advisor::*;

#[test]
fn test_safety_pass_parameterized_select() {
    let gate = NlQuerySafetyGate::new();
    assert_eq!(
        gate.validate("SELECT id, name FROM users WHERE id = ? AND status = ?"),
        SafetyVerdict::Pass
    );
}

#[test]
fn test_safety_block_union_injection() {
    let gate = NlQuerySafetyGate::new();
    let verdict = gate.validate("SELECT * FROM users UNION SELECT * FROM passwords");
    assert!(matches!(verdict, SafetyVerdict::InjectionDetected(_)));
}

#[test]
fn test_safety_block_drop() {
    let gate = NlQuerySafetyGate::new();
    let verdict = gate.validate("DROP TABLE users");
    assert!(matches!(verdict, SafetyVerdict::DangerousKeyword(_)));
}

#[test]
fn test_safety_block_non_parameterized() {
    let gate = NlQuerySafetyGate::new();
    let verdict = gate.validate("SELECT * FROM users WHERE name = 'alice'");
    assert!(matches!(verdict, SafetyVerdict::NonParameterized(_)));
}

#[test]
fn test_safety_allow_ddl_when_explicit() {
    let gate = NlQuerySafetyGate::new().allow_ddl();
    assert_eq!(
        gate.validate("CREATE TABLE logs (id INT, msg TEXT)"),
        SafetyVerdict::Pass
    );
}

#[test]
fn test_safety_block_load_file() {
    let gate = NlQuerySafetyGate::new();
    let verdict = gate.validate("SELECT LOAD_FILE('/etc/passwd')");
    assert!(matches!(verdict, SafetyVerdict::DangerousKeyword(_)));
}

#[test]
fn test_safety_block_into_outfile() {
    let gate = NlQuerySafetyGate::new();
    let verdict = gate.validate("SELECT * INTO OUTFILE '/tmp/evil' FROM users");
    assert!(matches!(verdict, SafetyVerdict::DangerousKeyword(_)));
}

#[test]
fn test_safety_block_truncate() {
    let gate = NlQuerySafetyGate::new();
    let verdict = gate.validate("TRUNCATE TABLE users");
    assert!(matches!(verdict, SafetyVerdict::DangerousKeyword(_)));
}
