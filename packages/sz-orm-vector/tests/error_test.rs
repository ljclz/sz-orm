use std::error::Error;
use sz_orm_vector::VectorError;

#[test]
fn test_collection_not_found() {
    let err = VectorError::CollectionNotFound("vec".into());
    assert_eq!(err.to_string(), "collection not found: vec");
}

#[test]
fn test_dimension_mismatch() {
    let err = VectorError::DimensionMismatch {
        expected: 128,
        actual: 256,
    };
    assert_eq!(err.to_string(), "dimension mismatch: expected 128, got 256");
}

#[test]
fn test_unsupported() {
    let err = VectorError::Unsupported("stub".into());
    assert_eq!(err.to_string(), "unsupported operation: stub");
}

#[test]
fn test_query_error() {
    let err = VectorError::Query("fail".into());
    assert_eq!(err.to_string(), "query error: fail");
}

#[test]
fn test_connection_error() {
    let err = VectorError::Connection("refused".into());
    assert_eq!(err.to_string(), "connection error: refused");
}

#[test]
fn test_invalid_config() {
    let err = VectorError::InvalidConfig("bad".into());
    assert_eq!(err.to_string(), "invalid config: bad");
}

#[test]
fn test_invalid_identifier() {
    let err = VectorError::InvalidIdentifier("bad!name".into());
    assert_eq!(err.to_string(), "invalid identifier: bad!name");
}

#[test]
fn test_top_k_exceeded() {
    let err = VectorError::TopKExceeded {
        requested: 200,
        max: 100,
    };
    assert_eq!(err.to_string(), "top_k 200 exceeds maximum allowed 100");
}

#[test]
fn test_backend_unreachable() {
    let err = VectorError::BackendUnreachable {
        reason: "timeout".into(),
    };
    assert_eq!(err.to_string(), "backend unreachable: timeout");
}

#[test]
fn test_error_trait_impl() {
    let err = VectorError::CollectionNotFound("x".into());
    assert!(err.source().is_none());
}
