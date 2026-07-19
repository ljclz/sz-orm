use std::fmt;

#[derive(Debug)]
pub enum BkError {
    Backup(String),
    Restore(String),
    Export(String),
    Import(String),
    FileNotFound(String),
    PermissionDenied(String),
    Compression(String),
}

impl fmt::Display for BkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BkError::Backup(msg) => write!(f, "Backup error: {}", msg),
            BkError::Restore(msg) => write!(f, "Restore error: {}", msg),
            BkError::Export(msg) => write!(f, "Export error: {}", msg),
            BkError::Import(msg) => write!(f, "Import error: {}", msg),
            BkError::FileNotFound(path) => write!(f, "File not found: {}", path),
            BkError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            BkError::Compression(msg) => write!(f, "Compression error: {}", msg),
        }
    }
}

impl std::error::Error for BkError {}

impl From<std::io::Error> for BkError {
    fn from(err: std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::NotFound {
            BkError::FileNotFound(err.to_string())
        } else if err.kind() == std::io::ErrorKind::PermissionDenied {
            BkError::PermissionDenied(err.to_string())
        } else {
            BkError::Backup(err.to_string())
        }
    }
}
