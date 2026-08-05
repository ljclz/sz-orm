//! SqlType derive 宏端到端集成测试
//!
//! 验证 `#[derive(SqlType)]` 可正确将枚举映射到 `Value::String`，
//! 覆盖：基本序列化、rename_all、变体级 rename、反序列化。

use sz_orm_core::FromQueryResult;
use sz_orm_core::Value;
use sz_orm_macros::SqlType;

/// 基本枚举（默认 snake_case）
#[derive(Debug, PartialEq, SqlType)]
enum Status {
    Active,
    Inactive,
    PendingReview,
}

/// rename_all = "UPPERCASE"
#[derive(Debug, PartialEq, SqlType)]
#[sql_type(rename_all = "UPPERCASE")]
enum Priority {
    Low,
    Medium,
    High,
}

/// 变体级 rename
#[derive(Debug, PartialEq, SqlType)]
enum Role {
    #[sql_type(rename = "admin_user")]
    Admin,
    User,
    Guest,
}

#[test]
fn sql_type_to_value_basic() {
    assert_eq!(
        Status::Active.to_value(),
        Value::String("active".to_string())
    );
    assert_eq!(
        Status::PendingReview.to_value(),
        Value::String("pending_review".to_string())
    );
    assert_eq!(
        Status::Inactive.to_value(),
        Value::String("inactive".to_string())
    );
}

#[test]
fn sql_type_to_value_uppercase() {
    assert_eq!(Priority::Low.to_value(), Value::String("LOW".to_string()));
    assert_eq!(
        Priority::Medium.to_value(),
        Value::String("MEDIUM".to_string())
    );
    assert_eq!(Priority::High.to_value(), Value::String("HIGH".to_string()));
}

#[test]
fn sql_type_to_value_variant_rename() {
    assert_eq!(
        Role::Admin.to_value(),
        Value::String("admin_user".to_string())
    );
    assert_eq!(Role::User.to_value(), Value::String("user".to_string()));
    assert_eq!(Role::Guest.to_value(), Value::String("guest".to_string()));
}

#[test]
fn sql_type_from_value_success() {
    let v = Value::String("active".to_string());
    let status = Status::from_value(&v).unwrap();
    assert_eq!(status, Status::Active);

    let v2 = Value::String("pending_review".to_string());
    let status2 = Status::from_value(&v2).unwrap();
    assert_eq!(status2, Status::PendingReview);
}

#[test]
fn sql_type_from_value_variant_rename() {
    let v = Value::String("admin_user".to_string());
    let role = Role::from_value(&v).unwrap();
    assert_eq!(role, Role::Admin);
}

#[test]
fn sql_type_from_value_unknown_variant_errors() {
    let v = Value::String("unknown_status".to_string());
    let result = Status::from_value(&v);
    assert!(result.is_err(), "未知变体应返回 Err");
    let err = result.unwrap_err();
    assert!(err.contains("unknown_status"), "错误应包含未知值: {}", err);
}

#[test]
fn sql_type_from_value_null_errors() {
    let v = Value::Null;
    let result = Status::from_value(&v);
    assert!(result.is_err(), "NULL 应返回 Err");
}

#[test]
fn sql_type_from_value_wrong_type_errors() {
    let v = Value::I64(42);
    let result = Status::from_value(&v);
    assert!(result.is_err(), "非 String 类型应返回 Err");
}
