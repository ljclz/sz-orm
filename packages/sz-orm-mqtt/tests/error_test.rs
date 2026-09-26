use std::error::Error;
use sz_orm_mqtt::MqttError;

#[test]
fn test_connection() {
    assert_eq!(
        MqttError::Connection("refused".into()).to_string(),
        "MQTT connection error: refused"
    );
}

#[test]
fn test_publish() {
    assert_eq!(
        MqttError::Publish("fail".into()).to_string(),
        "MQTT publish error: fail"
    );
}

#[test]
fn test_subscribe() {
    assert_eq!(
        MqttError::Subscribe("fail".into()).to_string(),
        "MQTT subscribe error: fail"
    );
}

#[test]
fn test_topic() {
    assert_eq!(
        MqttError::Topic("bad".into()).to_string(),
        "MQTT topic error: bad"
    );
}

#[test]
fn test_protocol() {
    assert_eq!(
        MqttError::Protocol("bad".into()).to_string(),
        "MQTT protocol error: bad"
    );
}

#[test]
fn test_from_io_error() {
    let io_err = std::io::Error::other("test");
    let err: MqttError = io_err.into();
    assert!(err.to_string().contains("MQTT connection error"));
}

#[test]
fn test_from_serde_json_error() {
    let json_err = serde_json::from_str::<serde_json::Value>("bad").unwrap_err();
    let err: MqttError = json_err.into();
    assert!(err.to_string().contains("MQTT publish error"));
}

#[test]
fn test_error_trait() {
    let err = MqttError::Connection("x".into());
    assert!(err.source().is_none());
}
