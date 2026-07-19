use std::fmt;

#[derive(Debug)]
pub enum MqttError {
    Connection(String),
    Publish(String),
    Subscribe(String),
    Topic(String),
    Protocol(String),
}

impl fmt::Display for MqttError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MqttError::Connection(msg) => write!(f, "MQTT connection error: {}", msg),
            MqttError::Publish(msg) => write!(f, "MQTT publish error: {}", msg),
            MqttError::Subscribe(msg) => write!(f, "MQTT subscribe error: {}", msg),
            MqttError::Topic(msg) => write!(f, "MQTT topic error: {}", msg),
            MqttError::Protocol(msg) => write!(f, "MQTT protocol error: {}", msg),
        }
    }
}

impl std::error::Error for MqttError {}

impl From<std::io::Error> for MqttError {
    fn from(err: std::io::Error) -> Self {
        MqttError::Connection(err.to_string())
    }
}

impl From<serde_json::Error> for MqttError {
    fn from(err: serde_json::Error) -> Self {
        MqttError::Publish(err.to_string())
    }
}
