pub mod embedding;
pub mod error;
pub mod rag;
pub mod vector;

pub use embedding::*;
pub use error::AiError;
pub use rag::*;
pub use vector::*;

// 仅在启用 `real` feature 时编译真实 OpenAI 兼容 API 客户端
#[cfg(feature = "real")]
pub mod real_embedding;
#[cfg(feature = "real")]
pub use real_embedding::OpenAIEmbeddingClient;
