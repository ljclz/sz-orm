use sz_orm_queue::MqError;
use std::error::Error;

#[test]
fn test_publish() { assert_eq!(MqError::Publish("fail".into()).to_string(), "Publish error: fail"); }

#[test]
fn test_subscribe() { assert_eq!(MqError::Subscribe("fail".into()).to_string(), "Subscribe error: fail"); }

#[test]
fn test_connection() { assert_eq!(MqError::Connection("refused".into()).to_string(), "Connection error: refused"); }

#[test]
fn test_not_supported() { assert_eq!(MqError::NotSupported("x".into()).to_string(), "Not supported: x"); }

#[test]
fn test_json() { assert_eq!(MqError::Json("bad".into()).to_string(), "JSON error: bad"); }

#[test]
fn test_from_serde_json_error() {
    let json_err = serde_json::from_str::<serde_json::Value>("bad").unwrap_err();
    let err: MqError = json_err.into();
    assert!(err.to_string().contains("JSON error"));
}

#[test]
fn test_error_trait() { let err = MqError::Publish("x".into()); assert!(err.source().is_none()); }