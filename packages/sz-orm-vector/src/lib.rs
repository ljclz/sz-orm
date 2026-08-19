//! SZ-ORM pgvector Extension
//!
//! Provides PostgreSQL pgvector vector similarity search capabilities, supporting three implementations:
//!
//! - **In-memory implementation** (`InMemoryVectorStore`): Pure Rust vector computation, no database connection, suitable for testing and benchmarking
//! - **Stub implementation** (`StubVectorStore`): All methods return Unsupported, suitable for debug placeholder
//! - **Real implementation** (`RealPgVectorStore`, requires `real-pg` feature): Connects to PostgreSQL + pgvector via tokio-postgres
//!
//! # Quick Start
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
pub mod extensions;
pub mod memory;
pub mod stub;

#[cfg(feature = "real-pg")]
pub mod real_pg;

// v4.0.0 M3：混合搜索（hybrid-search feature gate 隔离）
#[cfg(feature = "hybrid-search")]
pub mod hybrid_search;

pub use error::VectorError;
pub use extensions::{
    AnnIndexDef, AnnIndexRegistry, AnnIndexType, BatchOpsExt, DimensionValidator, HnswParams,
    IvfflatParams, MemoryBatchOps, SimilarityAlgorithms, VectorNormalizer, MAX_VECTOR_DIMENSION,
    MIN_VECTOR_DIMENSION,
};
pub use memory::InMemoryVectorStore;
pub use stub::StubVectorStore;

#[cfg(feature = "real-pg")]
pub use real_pg::{RealPgConfig, RealPgVectorStore};

use async_trait::async_trait;
use std::collections::HashMap;
use std::str::FromStr;

/// M-16 fix: top_k maximum limit
///
/// Limit top_k upper bound to prevent:
/// - Large k values causing memory explosion (each SearchResult contains full vector)
/// - Performance issues with database/vector engine executing huge k queries
/// - Malicious caller triggering OOM via top_k=usize::MAX
pub const MAX_TOP_K: usize = 10_000;

/// M-16 fix: Validate whether top_k is within reasonable range
///
/// - `top_k = 0`: Returns `TopKExceeded` error (meaningless query)
/// - `top_k > MAX_TOP_K`: Returns `TopKExceeded` error
/// - `1 <= top_k <= MAX_TOP_K`: Returns Ok
pub fn validate_top_k(top_k: usize) -> Result<usize, VectorError> {
    if top_k == 0 {
        return Err(VectorError::TopKExceeded {
            requested: top_k,
            max: MAX_TOP_K,
        });
    }
    if top_k > MAX_TOP_K {
        return Err(VectorError::TopKExceeded {
            requested: top_k,
            max: MAX_TOP_K,
        });
    }
    Ok(top_k)
}

/// Vector record
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

/// Search result
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

/// Vector distance metric
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum VectorMetric {
    #[default]
    Cosine,
    Euclidean,
    DotProduct,
}

impl VectorMetric {
    /// pgvector operator mapping
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

/// Vector Store core trait
///
/// Provides CRUD and similarity search capabilities for vector collections.
/// All methods are async, suitable for real database I/O.
#[async_trait]
pub trait PgVectorStore: Send + Sync {
    /// Create collection
    async fn create_collection(
        &self,
        name: &str,
        dimension: usize,
        metric: Option<VectorMetric>,
    ) -> Result<(), VectorError>;

    /// Delete collection
    async fn delete_collection(&self, name: &str) -> Result<(), VectorError>;

    /// Insert vector record (upsert semantics: same id overwrites)
    async fn insert(&self, collection: &str, records: Vec<VectorRecord>)
        -> Result<(), VectorError>;

    /// Similarity search
    ///
    /// M-16 fix: `top_k` must be in `[1, MAX_TOP_K]` range.
    /// Implementations should call `validate_top_k(top_k)?` before executing search.
    async fn search(
        &self,
        collection: &str,
        query: &[f32],
        top_k: usize,
    ) -> Result<Vec<SearchResult>, VectorError>;

    /// Get single record
    async fn get(&self, collection: &str, id: &str) -> Result<Option<VectorRecord>, VectorError>;

    /// Delete record
    async fn delete(&self, collection: &str, ids: Vec<String>) -> Result<u64, VectorError>;

    /// Count records
    async fn count(&self, collection: &str) -> Result<usize, VectorError>;
}
