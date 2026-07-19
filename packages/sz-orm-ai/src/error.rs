use thiserror::Error;

/// AI 模块统一错误类型
///
/// 保留原有变体（Embedding / Vector / RAG / Config / ModelNotFound / NotSupported）
/// 以维持向后兼容，并新增用于真实 HTTP API 调用的变体：
/// - `ApiError`：HTTP 非 2xx 响应（状态码 + 错误消息）
/// - `NetworkError`：网络层错误（连接失败 / DNS / 超时等）
/// - `ConfigError`：配置错误（例如 API key 缺失）
#[derive(Debug, Error)]
pub enum AiError {
    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error("Vector store error: {0}")]
    Vector(String),

    #[error("RAG error: {0}")]
    RAG(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Not supported: {0}")]
    NotSupported(String),

    /// HTTP 非 2xx 响应（状态码 + 错误消息）
    #[error("API error (status {0}): {1}")]
    ApiError(u16, String),

    /// 网络层错误（连接失败 / DNS / 超时等）
    #[error("Network error: {0}")]
    NetworkError(String),

    /// 配置错误（例如 API key 缺失、参数非法）
    #[error("Config error: {0}")]
    ConfigError(String),
}

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
