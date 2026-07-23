//! SZ-ORM Search 扩展
//!
//! 提供多 provider 全文搜索能力，支持三种实现：
//!
//! - **内存实现**（`Memory`）：线性扫描 + 子串匹配（无倒排索引），不连接真实搜索引擎
//! - **Stub 实现**（`Stub`）：生成查询 JSON 但不执行
//! - **真实 Elasticsearch**（需启用 `real-es` feature）：通过 elasticsearch crate 连接 ES
//! - **真实 OpenSearch**（需启用 `real-opensearch` feature）：通过 opensearch crate 连接 OpenSearch
//! - **真实 Meilisearch**（需启用 `real-meilisearch` feature）：通过 meilisearch-sdk crate 连接 Meilisearch
//!
//! # 支持的操作
//!
//! | 方法 | ES 等价 | 说明 |
//! |------|---------|------|
//! | `create_index` | `PUT /index` | 创建索引 |
//! | `delete_index` | `DELETE /index` | 删除索引 |
//! | `index_doc` | `POST /index/_doc/id` | 索引文档 |
//! | `bulk_index` | `_bulk` | 批量索引 |
//! | `get_doc` | `GET /index/_doc/id` | 获取文档 |
//! | `delete_doc` | `DELETE /index/_doc/id` | 删除文档 |
//! | `search` | `POST /index/_search` | 搜索 |
//! | `count` | `POST /index/_count` | 计数 |
//! | `refresh` | `POST /index/_refresh` | 刷新索引 |
//!
//! # 快速入门
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
    BoostScorer, FacetField, FacetResult, FacetedSearchExt, FacetedSearchResult, FacetValue,
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
