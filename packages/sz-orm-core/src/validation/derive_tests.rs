//! # `#[derive(Validate)]` 端到端测试

#![cfg(feature = "data-validation")]

use crate::Validate;
use crate::validation::Validate as _;
use crate::validation::ValidationError;


#[derive(Validate)]
struct User {
    #[validate(email)]
    email: String,
    #[validate(length(min = 2, max = 50))]
    name: String,
    #[validate(range(min = 0, max = 150))]
    age: i64,
}

#[test]
fn test_derive_validate_all_pass() {
    let user = User {
        email: "test@example.com".to_string(),
        name: "Alice".to_string(),
        age: 30,
    };
    assert!(user.validate().is_ok());
}

#[test]
fn test_derive_validate_email_fail() {
    let user = User {
        email: "not-an-email".to_string(),
        name: "Alice".to_string(),
        age: 30,
    };
    assert!(matches!(
        user.validate(),
        Err(ValidationError::Email { .. })
    ));
}

#[test]
fn test_derive_validate_length_fail() {
    let user = User {
        email: "test@example.com".to_string(),
        name: "A".to_string(),
        age: 30,
    };
    assert!(matches!(
        user.validate(),
        Err(ValidationError::Length { .. })
    ));
}

#[test]
fn test_derive_validate_range_fail() {
    let user = User {
        email: "test@example.com".to_string(),
        name: "Alice".to_string(),
        age: 200,
    };
    assert!(matches!(
        user.validate(),
        Err(ValidationError::Range { .. })
    ));
}

#[test]
fn test_derive_validate_multiple_fail() {
    let user = User {
        email: "bad".to_string(),
        name: "A".to_string(),
        age: 200,
    };
    let result = user.validate();
    match result {
        Err(ValidationError::Aggregate { count, .. }) => {
            assert_eq!(count, 3);
        }
        Err(ValidationError::Email { .. }) => {
            // 单错误时 aggregate 返回该错误本身
        }
        other => panic!("expected validation error, got {:?}", other),
    }
}

#[derive(Validate)]
struct Product {
    #[validate(required)]
    sku: String,
    #[validate(contains(value = "-"))]
    code: String,
    #[validate(does_not_contain(value = "drop"))]
    description: String,
}

#[test]
fn test_derive_validate_required_and_contains() {
    let product = Product {
        sku: "ABC123".to_string(),
        code: "PROD-001".to_string(),
        description: "A good product".to_string(),
    };
    assert!(product.validate().is_ok());
}

#[test]
fn test_derive_validate_required_fail() {
    let product = Product {
        sku: "".to_string(),
        code: "PROD-001".to_string(),
        description: "A good product".to_string(),
    };
    assert!(matches!(
        product.validate(),
        Err(ValidationError::Required { .. })
    ));
}

#[test]
fn test_derive_validate_contains_fail() {
    let product = Product {
        sku: "ABC123".to_string(),
        code: "PROD001".to_string(),
        description: "A good product".to_string(),
    };
    assert!(matches!(
        product.validate(),
        Err(ValidationError::Contains { .. })
    ));
}

#[test]
fn test_derive_validate_does_not_contain_fail() {
    let product = Product {
        sku: "ABC123".to_string(),
        code: "PROD-001".to_string(),
        description: "drop table users".to_string(),
    };
    assert!(matches!(
        product.validate(),
        Err(ValidationError::DoesNotContain { .. })
    ));
}

#[derive(Validate)]
struct OptionalValidation {
    #[validate(email, when = "self.enabled")]
    email: String,
    enabled: bool,
}

#[test]
fn test_derive_validate_conditional_skip() {
    let data = OptionalValidation {
        email: "not-an-email".to_string(),
        enabled: false,
    };
    assert!(data.validate().is_ok());
}

#[test]
fn test_derive_validate_conditional_apply() {
    let data = OptionalValidation {
        email: "not-an-email".to_string(),
        enabled: true,
    };
    assert!(matches!(
        data.validate(),
        Err(ValidationError::Email { .. })
    ));
}
