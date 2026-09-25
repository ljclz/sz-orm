use sz_orm_mqtt::MqttError;
use std::error::Error;

#[test]
fn test_connection() { assert_eq!(MqttError::Connection("refused".into()).to_string(), "MQTT connection error: refused"); }

#[test]
fn test_publish() { assert_eq!(MqttError::Publish("fail".into()).to_string(), "MQTT publish error: fail"); }

#[test]
fn test_subscribe() { assert_eq!(MqttError::Subscribe("fail".into()).to_string(), "MQTT subscribe error: fail"); }

#[test]
fn test_topic() { assert_eq!(MqttError::Topic("bad".into()).to_string(), "MQTT topic error: bad"); }

#[test]
fn test_protocol() { assert_eq!(MqttError::Protocol("bad".into()).to_string(), "MQTT protocol error: bad"); }

#[test]
fn test_from_io_error() {
    let io_err = std::io::Error::new(std::io::ErrorKind::Other, "test");
    let err: MqttError = io_err.into();
    assert!(err.to_string().contains("MQTT connection error"));
}

#[test]
fn test_error_trait() { let err = MqttError::Connection("x".into()); assert!(err.source().is_none()); }