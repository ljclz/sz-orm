//! # SZ-ORM AI — AI 能力包
//!
//! 提供自然语言转 SQL（NL2SQL）、检索增强生成（RAG）、文本 embedding 与向量搜索能力，
//! 内置安全防护与 OpenAI 兼容 API 客户端（启用 `real` feature 时编译）。
//!
//! ## 主要模块
//!
//! - [`embedding`] — 文本向量化接口
//! - [`nl2sql`] — 自然语言到 SQL 的转换
//! - [`rag`] — 检索增强生成
//! - [`vector`] — 向量存储与相似度检索
//! - [`safety`] — 输入安全检查

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
