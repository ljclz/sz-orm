//! v4.3.0 M3-T3/T4：编译期数据治理集成测试
//!
//! 仅当 sz-orm-core 启用 `compile-governance` feature 时编译。
//!
//! 运行方式：
//! ```bash
//! cargo test -p sz-orm-core --features compile-governance --test compile_governance
//! ```
//!
//! 编译期强制（无法在运行时测试，已手动验证）：
//! - `#[pii]` 字段缺 `#[mask]` → compile_error "PII field must declare #[mask(...)]"
//! - `#[mask(strategy = "invalid")]` → compile_error "invalid mask strategy"

#![cfg(feature = "compile-governance")]

use sz_orm_core::governance::{compliance_report, with_retention, GovernedModel};

/// 合法模型：全部 PII 字段均声明 mask 策略
#[derive(sz_orm_macros::Governed)]
struct User {
    id: i64,
    #[pii]
    #[mask(strategy = "partial")]
    email: String,
    #[pii]
    #[mask(strategy = "hash")]
    phone: String,
    name: String, // 非 PII，无需 mask
}

/// 空 PII 模型（无 pii 标注也应编译通过）
#[derive(sz_orm_macros::Governed)]
struct AuditLog {
    id: i64,
    action: String,
}

#[test]
fn derive_generates_pii_fields() {
    let fields = User::pii_fields();
    assert_eq!(fields, vec![("email", "partial"), ("phone", "hash")]);
}

#[test]
fn derive_generates_empty_for_no_pii() {
    assert!(AuditLog::pii_fields().is_empty());
}

#[test]
fn compliance_report_integration() {
    let report = with_retention(
        compliance_report(&[User::pii_fields(), AuditLog::pii_fields()]),
        730,
    );
    assert_eq!(report.pii_field_count(), 2);
    assert_eq!(report.retention_days, Some(730));
    let json = report.to_json().expect("serialize");
    assert!(json.contains("\"field\": \"email\""));
    assert!(json.contains("\"strategy\": \"hash\""));
}
