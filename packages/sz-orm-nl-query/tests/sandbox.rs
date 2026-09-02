//! TASK-009 验证测试：安全执行沙箱

use sz_orm_nl_query::types::NlQueryError;

#[test]
fn test_sandbox_error_types() {
    let injection = NlQueryError::SqlInjectionDetected;
    assert!(injection.to_string().contains("注入"));

    let timeout = NlQueryError::Timeout;
    assert!(timeout.to_string().contains("超时"));

    let dml = NlQueryError::DmlDenied;
    assert!(dml.to_string().contains("DML"));
}
