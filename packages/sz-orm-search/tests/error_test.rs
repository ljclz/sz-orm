use std::error::Error;
use sz_orm_search::SearchError;

#[test]
fn test_not_found() {
    let err = SearchError::NotFound("idx".into());
    assert_eq!(err.to_string(), "not found: idx");
}

#[test]
fn test_doc_not_found() {
    let err = SearchError::DocNotFound {
        index: "users".into(),
        id: "42".into(),
    };
    assert_eq!(err.to_string(), "document not found: users/42");
}

#[test]
fn test_invalid_query() {
    let err = SearchError::InvalidQuery("syntax".into());
    assert_eq!(err.to_string(), "invalid query: syntax");
}

#[test]
fn test_index_already_exists() {
    let err = SearchError::IndexAlreadyExists("idx".into());
    assert_eq!(err.to_string(), "index already exists: idx");
}

#[test]
fn test_invalid_config() {
    let err = SearchError::InvalidConfig("bad".into());
    assert_eq!(err.to_string(), "invalid config: bad");
}

#[test]
fn test_serialization() {
    let err = SearchError::Serialization("json".into());
    assert_eq!(err.to_string(), "serialization error: json");
}

#[test]
fn test_query_error() {
    let err = SearchError::Query("fail".into());
    assert_eq!(err.to_string(), "query error: fail");
}

#[test]
fn test_connection_error() {
    let err = SearchError::Connection("down".into());
    assert_eq!(err.to_string(), "connection error: down");
}

#[test]
fn test_from_serde_json_error() {
    let json_err = serde_json::from_str::<serde_json::Value>("bad").unwrap_err();
    let err: SearchError = json_err.into();
    assert!(err.to_string().contains("serialization error"));
}

#[test]
fn test_from_io_error() {
    let io_err = std::io::Error::other("test");
    let err: SearchError = io_err.into();
    assert!(err.to_string().contains("query error"));
}

#[test]
fn test_error_trait_impl() {
    let err = SearchError::NotFound("x".into());
    assert!(err.source().is_none());
}
