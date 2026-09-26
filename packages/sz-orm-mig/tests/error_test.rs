use std::error::Error;
use sz_orm_mig::MigError;

#[test]
fn test_connection() {
    assert_eq!(
        MigError::Connection("fail".into()).to_string(),
        "Connection error: fail"
    );
}

#[test]
fn test_migration() {
    assert_eq!(
        MigError::Migration("fail".into()).to_string(),
        "Migration error: fail"
    );
}

#[test]
fn test_transform() {
    assert_eq!(
        MigError::Transform("fail".into()).to_string(),
        "Transform error: fail"
    );
}

#[test]
fn test_validation() {
    assert_eq!(
        MigError::Validation("bad".into()).to_string(),
        "Validation error: bad"
    );
}

#[test]
fn test_not_supported() {
    assert_eq!(
        MigError::NotSupported("x".into()).to_string(),
        "Not supported: x"
    );
}

#[test]
fn test_table_not_found() {
    assert_eq!(
        MigError::TableNotFound("users".into()).to_string(),
        "Table not found: users"
    );
}

#[test]
fn test_batch_size_error() {
    assert_eq!(
        MigError::BatchSizeError("too large".into()).to_string(),
        "Batch size error: too large"
    );
}

#[test]
fn test_error_trait() {
    let err = MigError::Migration("x".into());
    assert!(err.source().is_none());
}
#[test]
fn test_from_io_error() {
    let io_err = std::io::Error::other("fail");
    let mig: MigError = io_err.into();
    assert!(matches!(mig, MigError::Connection(_)));
}

#[test]
fn test_from_serde_json_error() {
    let json_err = serde_json::from_str::<serde_json::Value>("bad").unwrap_err();
    let mig: MigError = json_err.into();
    assert!(matches!(mig, MigError::Validation(_)));
}
