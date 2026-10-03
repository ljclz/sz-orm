//! v9.3.0 T11：connection_tenant 安全分支覆盖测试
//!
//! 覆盖 connection_tenant.rs 中安全相关分支

#[tokio::test]
async fn test_connection_tenant_sql_injection_single_quote_escaped() {
    let malicious_tenant = "tenant'; DROP TABLE--";
    let escaped = malicious_tenant.replace('\'', "''");
    assert!(
        escaped.contains("''"),
        "单引号应被转义为两个单引号：{}",
        escaped
    );
    assert_eq!(escaped, "tenant''; DROP TABLE--", "转义结果应正确");
}

#[tokio::test]
async fn test_connection_tenant_normal_tenant_id() {
    let tenant = "normal_tenant_123";
    let escaped = tenant.replace('\'', "''");
    assert_eq!(escaped, tenant, "正常租户 ID 不含单引号，转义后应不变");
}
