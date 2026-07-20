//! SZ-ORM pgvector 扩展
//!
//! 提供 PostgreSQL pgvector 向量相似度搜索能力，支持三种实现：
//!
//! - **内存实现**（`InMemoryVectorStore`）：纯 Rust 向量计算，不连接数据库，适用于测试和基准
//! - **Stub 实现**（`StubVectorStore`）：所有方法返回 Unsupported，适用于调试占位
//! - **真实实现**（`RealPgVectorStore`，需启用 `real-pg` feature）：通过 tokio-postgres 连接 PostgreSQL + pgvector
//!
//! # 快速入门
//!
//! ```rust
//! use sz_orm_vector::{InMemoryVectorStore, PgVectorStore, VectorRecord, VectorMetric};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let store = InMemoryVectorStore::new();
//! store.create_collection("docs", 3, None).await?;
//!
//! let record = VectorRecord::new("doc1", vec![1.0, 0.0, 0.0]);
//! store.insert("docs", vec![record]).await?;
//!
//! let results = store.search("docs", &[1.0, 0.0, 0.0], 5).await?;
//! println!("found {} results", results.len());
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod memory;
pub mod stub;

#[cfg(feature = "real-pg")]
pub mod real_pg;

pub use error::VectorError;
pub use memory::InMemoryVectorStore;
pub use stub::StubVectorStore;

#[cfg(feature = "real-pg")]
pub use real_pg::{RealPgConfig, RealPgVectorStore};

use async_trait::async_trait;
use std::collections::HashMap;
use std::str::FromStr;

/// 向量记录
#[derive(Debug, Clone)]
pub struct VectorRecord {
    pub id: String,
    pub vector: Vec<f32>,
    pub score: Option<f32>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl VectorRecord {
    pub fn new(id: impl Into<String>, vector: Vec<f32>) -> Self {
        Self {
            id: id.into(),
            vector,
            score: None,
            metadata: None,
        }
    }

    pub fn with_score(mut self, score: f32) -> Self {
        self.score = Some(score);
        self
    }

    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// 搜索结果
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub vector: Vec<f32>,
    pub text: Option<String>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl SearchResult {
    pub fn new(id: impl Into<String>, score: f32, vector: Vec<f32>) -> Self {
        Self {
            id: id.into(),
            score,
            vector,
            text: None,
            metadata: None,
        }
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// 向量距离度量
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum VectorMetric {
    #[default]
    Cosine,
    Euclidean,
    DotProduct,
}

impl VectorMetric {
    /// pgvector 操作符映射
    pub fn pg_operator(&self) -> &'static str {
        match self {
            VectorMetric::Cosine => "<=>",
            VectorMetric::Euclidean => "<->",
            VectorMetric::DotProduct => "<#>",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            VectorMetric::Cosine => "cosine",
            VectorMetric::Euclidean => "euclidean",
            VectorMetric::DotProduct => "dotproduct",
        }
    }
}

impl FromStr for VectorMetric {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cosine" => Ok(VectorMetric::Cosine),
            "euclidean" => Ok(VectorMetric::Euclidean),
            "dotproduct" => Ok(VectorMetric::DotProduct),
            _ => Err(format!("unknown vector metric: {}", s)),
        }
    }
}

/// Vector Store 核心 trait
///
/// 提供向量集合的 CRUD 和相似度搜索能力。
/// 所有方法均为 async，适用于真实数据库 I/O。
#[async_trait]
pub trait PgVectorStore: Send + Sync {
    /// 创建集合
    async fn create_collection(
        &self,
        name: &str,
        dimension: usize,
        metric: Option<VectorMetric>,
    ) -> Result<(), VectorError>;

    /// 删除集合
    async fn delete_collection(&self, name: &str) -> Result<(), VectorError>;

    /// 插入向量记录（upsert 语义：相同 id 会覆盖）
    async fn insert(&self, collection: &str, records: Vec<VectorRecord>)
        -> Result<(), VectorError>;

    /// 相似度搜索
    async fn search(
        &self,
        collection: &str,
        query: &[f32],
        top_k: usize,
    ) -> Result<Vec<SearchResult>, VectorError>;

    /// 获取单个记录
    async fn get(&self, collection: &str, id: &str) -> Result<Option<VectorRecord>, VectorError>;

    /// 删除记录
    async fn delete(&self, collection: &str, ids: Vec<String>) -> Result<u64, VectorError>;

    /// 统计记录数
    async fn count(&self, collection: &str) -> Result<usize, VectorError>;
}
