use std::fmt;

#[derive(Debug)]
pub enum MqError {
    Publish(String),
    Subscribe(String),
    Connection(String),
    NotSupported(String),
    Json(String),
}

impl fmt::Display for MqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MqError::Publish(msg) => write!(f, "Publish error: {}", msg),
            MqError::Subscribe(msg) => write!(f, "Subscribe error: {}", msg),
            MqError::Connection(msg) => write!(f, "Connection error: {}", msg),
            MqError::NotSupported(msg) => write!(f, "Not supported: {}", msg),
            MqError::Json(msg) => write!(f, "JSON error: {}", msg),
        }
    }
}

impl std::error::Error for MqError {}

impl From<serde_json::Error> for MqError {
    fn from(err: serde_json::Error) -> Self {
        MqError::Json(err.to_string())
    }
}
