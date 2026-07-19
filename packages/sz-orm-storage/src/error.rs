use std::fmt;

#[derive(Debug)]
pub enum StorageError {
    Put(String),
    Get(String),
    Delete(String),
    NotFound(String),
    PermissionDenied(String),
    Connection(String),
    InvalidConfig(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::Put(msg) => write!(f, "Put error: {}", msg),
            StorageError::Get(msg) => write!(f, "Get error: {}", msg),
            StorageError::Delete(msg) => write!(f, "Delete error: {}", msg),
            StorageError::NotFound(key) => write!(f, "Key not found: {}", key),
            StorageError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            StorageError::Connection(msg) => write!(f, "Connection error: {}", msg),
            StorageError::InvalidConfig(msg) => write!(f, "Invalid config: {}", msg),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<std::io::Error> for StorageError {
    fn from(err: std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::NotFound {
            StorageError::NotFound(err.to_string())
        } else if err.kind() == std::io::ErrorKind::PermissionDenied {
            StorageError::PermissionDenied(err.to_string())
        } else {
            StorageError::Connection(err.to_string())
        }
    }
}
