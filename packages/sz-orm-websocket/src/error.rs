use std::fmt;

#[derive(Debug)]
pub enum WsError {
    Connection(String),
    Message(String),
    Authentication(String),
    Channel(String),
    Protocol(String),
}

impl fmt::Display for WsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WsError::Connection(msg) => write!(f, "Connection error: {}", msg),
            WsError::Message(msg) => write!(f, "Message error: {}", msg),
            WsError::Authentication(msg) => write!(f, "Authentication error: {}", msg),
            WsError::Channel(msg) => write!(f, "Channel error: {}", msg),
            WsError::Protocol(msg) => write!(f, "Protocol error: {}", msg),
        }
    }
}

impl std::error::Error for WsError {}

impl From<std::io::Error> for WsError {
    fn from(err: std::io::Error) -> Self {
        WsError::Connection(err.to_string())
    }
}

impl From<serde_json::Error> for WsError {
    fn from(err: serde_json::Error) -> Self {
        WsError::Message(err.to_string())
    }
}
