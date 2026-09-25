use sz_orm_postgis::PostgisError;
use std::error::Error;

#[test]
fn test_invalid_geometry() {
    let err = PostgisError::InvalidGeometry("bad wkt".into());
    assert_eq!(err.to_string(), "invalid geometry: bad wkt");
}

#[test]
fn test_srid_mismatch() {
    let err = PostgisError::SridMismatch { expected: 4326, actual: 3857 };
    assert_eq!(err.to_string(), "SRID mismatch: expected 4326, got 3857");
}

#[test]
fn test_unsupported() {
    let err = PostgisError::Unsupported("3D".into());
    assert_eq!(err.to_string(), "unsupported operation: 3D");
}

#[test]
fn test_query_error() {
    let err = PostgisError::Query("timeout".into());
    assert_eq!(err.to_string(), "query error: timeout");
}

#[test]
fn test_connection_error() {
    let err = PostgisError::Connection("refused".into());
    assert_eq!(err.to_string(), "connection error: refused");
}

#[test]
fn test_invalid_config() {
    let err = PostgisError::InvalidConfig("no host".into());
    assert_eq!(err.to_string(), "invalid config: no host");
}

#[test]
fn test_type_mismatch() {
    let err = PostgisError::TypeMismatch { expected: "Point", actual: "LineString" };
    assert_eq!(err.to_string(), "geometry type mismatch: expected Point, got LineString");
}

#[test]
fn test_from_io_error() {
    let io_err = std::io::Error::new(std::io::ErrorKind::Other, "test");
    let err: PostgisError = io_err.into();
    assert!(err.to_string().contains("query error"));
}

#[test]
fn test_error_trait_impl() {
    let err = PostgisError::InvalidGeometry("x".into());
    assert!(err.source().is_none());
}