use std::error::Error;
use sz_orm_websocket::WsError;

#[test]
fn test_connection() {
    let err = WsError::Connection("refused".into());
    assert_eq!(err.to_string(), "Connection error: refused");
}

#[test]
fn test_message() {
    let err = WsError::Message("too large".into());
    assert_eq!(err.to_string(), "Message error: too large");
}

#[test]
fn test_authentication() {
    let err = WsError::Authentication("bad token".into());
    assert_eq!(err.to_string(), "Authentication error: bad token");
}

#[test]
fn test_channel() {
    let err = WsError::Channel("closed".into());
    assert_eq!(err.to_string(), "Channel error: closed");
}

#[test]
fn test_protocol() {
    let err = WsError::Protocol("bad frame".into());
    assert_eq!(err.to_string(), "Protocol error: bad frame");
}

#[test]
fn test_from_io_error() {
    let io_err = std::io::Error::other("test");
    let err: WsError = io_err.into();
    assert!(err.to_string().contains("Connection error"));
}

#[test]
fn test_from_serde_json_error() {
    let json_err = serde_json::from_str::<serde_json::Value>("bad").unwrap_err();
    let err: WsError = json_err.into();
    assert!(err.to_string().contains("Message error"));
}

#[test]
fn test_error_trait_impl() {
    let err = WsError::Connection("x".into());
    assert!(err.source().is_none());
}
