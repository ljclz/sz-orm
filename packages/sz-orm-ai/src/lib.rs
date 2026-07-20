pub mod embedding;
pub mod error;
pub mod nl2sql;
pub mod rag;
pub mod safety;
pub mod vector;

pub use embedding::*;
pub use error::AiError;
pub use nl2sql::*;
pub use rag::*;
pub use safety::*;
pub use vector::*;

// 仅在启用 `real` feature 时编译真实 OpenAI 兼容 API 客户端
#[cfg(feature = "real")]
pub mod real_embedding;
#[cfg(feature = "real")]
pub use real_embedding::OpenAIEmbeddingClient;
