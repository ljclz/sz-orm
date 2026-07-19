use std::fmt;

#[derive(Debug)]
pub enum AiError {
    Embedding(String),
    Vector(String),
    RAG(String),
    Config(String),
    ModelNotFound(String),
    NotSupported(String),
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AiError::Embedding(msg) => write!(f, "Embedding error: {}", msg),
            AiError::Vector(msg) => write!(f, "Vector store error: {}", msg),
            AiError::RAG(msg) => write!(f, "RAG error: {}", msg),
            AiError::Config(msg) => write!(f, "Config error: {}", msg),
            AiError::ModelNotFound(name) => write!(f, "Model not found: {}", name),
            AiError::NotSupported(msg) => write!(f, "Not supported: {}", msg),
        }
    }
}

impl std::error::Error for AiError {}

impl From<std::io::Error> for AiError {
    fn from(err: std::io::Error) -> Self {
        AiError::Config(err.to_string())
    }
}

impl From<serde_json::Error> for AiError {
    fn from(err: serde_json::Error) -> Self {
        AiError::Config(err.to_string())
    }
}
