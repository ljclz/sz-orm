use sz_orm_storage::StorageError;
use std::error::Error;

#[test]
fn test_put() {
    let err = StorageError::Put("fail".into());
    assert_eq!(err.to_string(), "Put error: fail");
}

#[test]
fn test_get() {
    let err = StorageError::Get("fail".into());
    assert_eq!(err.to_string(), "Get error: fail");
}

#[test]
fn test_delete() {
    let err = StorageError::Delete("fail".into());
    assert_eq!(err.to_string(), "Delete error: fail");
}

#[test]
fn test_not_found() {
    let err = StorageError::NotFound("key".into());
    assert_eq!(err.to_string(), "Key not found: key");
}

#[test]
fn test_permission_denied() {
    let err = StorageError::PermissionDenied("denied".into());
    assert_eq!(err.to_string(), "Permission denied: denied");
}

#[test]
fn test_connection() {
    let err = StorageError::Connection("refused".into());
    assert_eq!(err.to_string(), "Connection error: refused");
}

#[test]
fn test_invalid_config() {
    let err = StorageError::InvalidConfig("bad".into());
    assert_eq!(err.to_string(), "Invalid config: bad");
}

#[test]
fn test_from_io_not_found() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
    let err: StorageError = io_err.into();
    assert!(err.to_string().contains("Key not found"));
}

#[test]
fn test_from_io_permission_denied() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "forbidden");
    let err: StorageError = io_err.into();
    assert!(err.to_string().contains("Permission denied"));
}

#[test]
fn test_from_io_other() {
    let io_err = std::io::Error::new(std::io::ErrorKind::Other, "test");
    let err: StorageError = io_err.into();
    assert!(err.to_string().contains("Connection error"));
}

#[test]
fn test_error_trait_impl() {
    let err = StorageError::Put("x".into());
    assert!(err.source().is_none());
}