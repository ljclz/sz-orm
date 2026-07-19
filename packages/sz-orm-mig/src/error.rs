use std::fmt;

#[derive(Debug)]
pub enum MigError {
    Connection(String),
    Migration(String),
    Transform(String),
    Validation(String),
    NotSupported(String),
    TableNotFound(String),
    BatchSizeError(String),
}

impl fmt::Display for MigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MigError::Connection(msg) => write!(f, "Connection error: {}", msg),
            MigError::Migration(msg) => write!(f, "Migration error: {}", msg),
            MigError::Transform(msg) => write!(f, "Transform error: {}", msg),
            MigError::Validation(msg) => write!(f, "Validation error: {}", msg),
            MigError::NotSupported(msg) => write!(f, "Not supported: {}", msg),
            MigError::TableNotFound(name) => write!(f, "Table not found: {}", name),
            MigError::BatchSizeError(msg) => write!(f, "Batch size error: {}", msg),
        }
    }
}

impl std::error::Error for MigError {}

impl From<std::io::Error> for MigError {
    fn from(err: std::io::Error) -> Self {
        MigError::Connection(err.to_string())
    }
}

impl From<serde_json::Error> for MigError {
    fn from(err: serde_json::Error) -> Self {
        MigError::Validation(err.to_string())
    }
}
