//! SZ-ORM Search Extension
//!
//! Provides multi-provider full-text search capabilities, supporting three implementations:
//!
//! - **In-memory implementation** (`Memory`): Linear scan + substring matching (no inverted index), no real search engine connection
//! - **Stub implementation** (`Stub`): Generates query JSON but does not execute
//! - **Real Elasticsearch** (requires `real-es` feature): Connects to ES via elasticsearch crate
//! - **Real OpenSearch** (requires `real-opensearch` feature): Connects to OpenSearch via opensearch crate
//! - **Real Meilisearch** (requires `real-meilisearch` feature): Connects to Meilisearch via meilisearch-sdk crate
//!
//! # Supported Operations
//!
//! | Method | ES equivalent | Description |
//! |------|---------|------|
//! | `create_index` | `PUT /index` | Create index |
//! | `delete_index` | `DELETE /index` | Delete index |
//! | `index_doc` | `POST /index/_doc/id` | Index document |
//! | `bulk_index` | `_bulk` | Bulk index |
//! | `get_doc` | `GET /index/_doc/id` | Get document |
//! | `delete_doc` | `DELETE /index/_doc/id` | Delete document |
//! | `search` | `POST /index/_search` | Search |
//! | `count` | `POST /index/_count` | Count |
//! | `refresh` | `POST /index/_refresh` | Refresh index |
//!
//! # Quick Start
//!
//! ```rust
//! use sz_orm_search::{SearchBuilder, SearchExt, SearchProvider, SearchQuery};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let wrapper = SearchBuilder::new(SearchProvider::Memory).build()?;
//! wrapper.create_index("docs", &serde_json::json!({})).await?;
//!
//! wrapper.index_doc("docs", "1", &serde_json::json!({"title": "hello"})).await?;
//!
//! let result = wrapper.search("docs", &SearchQuery::new("hello")).await?;
//! println!("hits: {}", result.hits.len());
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod extensions;
pub mod memory;
pub mod search;
pub mod stub;
pub mod types;

#[cfg(feature = "real-es")]
pub mod elasticsearch_provider;

#[cfg(feature = "real-opensearch")]
pub mod opensearch_provider;

#[cfg(feature = "real-meilisearch")]
pub mod meilisearch_provider;

pub use error::SearchError;
pub use extensions::{
    BoostScorer, FacetField, FacetResult, FacetValue, FacetedSearchExt, FacetedSearchResult,
    FieldBoost, HighlightConfig, HighlightFormat, Highlighter, MemoryFacetedSearch, Tokenizer,
    TokenizerConfig, TokenizerType,
};
pub use memory::MemorySearch;
pub use search::{SearchBuilder, SearchExt, SearchProvider, SearchWrapper};
pub use stub::StubSearch;
pub use types::{SearchHit, SearchQuery, SearchResult, SortField, SortOrder};

#[cfg(feature = "real-es")]
pub use search::ElasticsearchConfig;

#[cfg(feature = "real-es")]
pub use elasticsearch_provider::ElasticsearchProvider;

#[cfg(feature = "real-opensearch")]
pub use search::OpenSearchConfig;

#[cfg(feature = "real-opensearch")]
pub use opensearch_provider::OpensearchProvider;

#[cfg(feature = "real-meilisearch")]
pub use search::MeilisearchConfig;

#[cfg(feature = "real-meilisearch")]
pub use meilisearch_provider::MeilisearchProvider;
