//! Search 核心 trait 与 Builder/Wrapper/Provider

use crate::error::SearchError;
use crate::types::{SearchQuery, SearchResult};
use async_trait::async_trait;
use serde_json::Value;

/// 全文搜索扩展 trait
///
/// 统一抽象 Elasticsearch / OpenSearch / Meilisearch 的核心操作。
#[async_trait]
pub trait SearchExt: Send + Sync {
    /// 创建索引
    ///
    /// ES 等价：`PUT /index` with mappings
    /// Meilisearch 等价：`POST /indexes` with settings
    async fn create_index(&self, index: &str, mappings: &Value) -> Result<(), SearchError>;

    /// 删除索引
    async fn delete_index(&self, index: &str) -> Result<(), SearchError>;

    /// 索引单个文档
    ///
    /// ES 等价：`POST /index/_doc/id` with body
    async fn index_doc(&self, index: &str, id: &str, doc: &Value) -> Result<(), SearchError>;

    /// 批量索引文档
    async fn bulk_index(&self, index: &str, docs: &[(String, Value)]) -> Result<(), SearchError> {
        for (id, doc) in docs {
            self.index_doc(index, id, doc).await?;
        }
        Ok(())
    }

    /// 获取单个文档
    ///
    /// ES 等价：`GET /index/_doc/id`
    async fn get_doc(&self, index: &str, id: &str) -> Result<Option<Value>, SearchError>;

    /// 删除单个文档
    ///
    /// ES 等价：`DELETE /index/_doc/id`
    async fn delete_doc(&self, index: &str, id: &str) -> Result<(), SearchError>;

    /// 搜索
    ///
    /// ES 等价：`POST /index/_search` with body
    async fn search(&self, index: &str, query: &SearchQuery) -> Result<SearchResult, SearchError>;

    /// 统计文档数
    async fn count(&self, index: &str, query: &SearchQuery) -> Result<u64, SearchError>;

    /// 刷新索引（强制刷新，使最近写入的文档可搜索）
    ///
    /// ES 等价：`POST /index/_refresh`
    async fn refresh(&self, index: &str) -> Result<(), SearchError>;
}

/// Provider 类型
#[derive(Debug, Clone)]
pub enum SearchProvider {
    /// 内存实现（简易倒排索引）
    Memory,
    /// Stub 实现（生成查询 JSON 但不执行）
    Stub,
    /// 真实 Elasticsearch（需启用 `real-es` feature）
    #[cfg(feature = "real-es")]
    Elasticsearch(ElasticsearchConfig),
    /// 真实 OpenSearch（需启用 `real-opensearch` feature）
    #[cfg(feature = "real-opensearch")]
    OpenSearch(OpenSearchConfig),
    /// 真实 Meilisearch（需启用 `real-meilisearch` feature）
    #[cfg(feature = "real-meilisearch")]
    Meilisearch(MeilisearchConfig),
}

/// Elasticsearch 配置
#[cfg(feature = "real-es")]
#[derive(Debug, Clone, Default)]
pub struct ElasticsearchConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// OpenSearch 配置
#[cfg(feature = "real-opensearch")]
#[derive(Debug, Clone, Default)]
pub struct OpenSearchConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// Meilisearch 配置
#[cfg(feature = "real-meilisearch")]
#[derive(Debug, Clone, Default)]
pub struct MeilisearchConfig {
    pub url: String,
    pub api_key: Option<String>,
}

/// Wrapper enum
pub enum SearchWrapper {
    Memory(crate::memory::MemorySearch),
    Stub(crate::stub::StubSearch),
    #[cfg(feature = "real-es")]
    Elasticsearch(crate::elasticsearch_provider::ElasticsearchProvider),
    #[cfg(feature = "real-opensearch")]
    OpenSearch(crate::opensearch_provider::OpensearchProvider),
    #[cfg(feature = "real-meilisearch")]
    Meilisearch(crate::meilisearch_provider::MeilisearchProvider),
}

#[async_trait]
impl SearchExt for SearchWrapper {
    async fn create_index(&self, index: &str, mappings: &Value) -> Result<(), SearchError> {
        match self {
            SearchWrapper::Memory(p) => p.create_index(index, mappings).await,
            SearchWrapper::Stub(p) => p.create_index(index, mappings).await,
            #[cfg(feature = "real-es")]
            SearchWrapper::Elasticsearch(p) => p.create_index(index, mappings).await,
            #[cfg(feature = "real-opensearch")]
            SearchWrapper::OpenSearch(p) => p.create_index(index, mappings).await,
            #[cfg(feature = "real-meilisearch")]
            SearchWrapper::Meilisearch(p) => p.create_index(index, mappings).await,
        }
    }

    async fn delete_index(&self, index: &str) -> Result<(), SearchError> {
        match self {
            SearchWrapper::Memory(p) => p.delete_index(index).await,
            SearchWrapper::Stub(p) => p.delete_index(index).await,
            #[cfg(feature = "real-es")]
            SearchWrapper::Elasticsearch(p) => p.delete_index(index).await,
            #[cfg(feature = "real-opensearch")]
            SearchWrapper::OpenSearch(p) => p.delete_index(index).await,
            #[cfg(feature = "real-meilisearch")]
            SearchWrapper::Meilisearch(p) => p.delete_index(index).await,
        }
    }

    async fn index_doc(&self, index: &str, id: &str, doc: &Value) -> Result<(), SearchError> {
        match self {
            SearchWrapper::Memory(p) => p.index_doc(index, id, doc).await,
            SearchWrapper::Stub(p) => p.index_doc(index, id, doc).await,
            #[cfg(feature = "real-es")]
            SearchWrapper::Elasticsearch(p) => p.index_doc(index, id, doc).await,
            #[cfg(feature = "real-opensearch")]
            SearchWrapper::OpenSearch(p) => p.index_doc(index, id, doc).await,
            #[cfg(feature = "real-meilisearch")]
            SearchWrapper::Meilisearch(p) => p.index_doc(index, id, doc).await,
        }
    }

    async fn get_doc(&self, index: &str, id: &str) -> Result<Option<Value>, SearchError> {
        match self {
            SearchWrapper::Memory(p) => p.get_doc(index, id).await,
            SearchWrapper::Stub(p) => p.get_doc(index, id).await,
            #[cfg(feature = "real-es")]
            SearchWrapper::Elasticsearch(p) => p.get_doc(index, id).await,
            #[cfg(feature = "real-opensearch")]
            SearchWrapper::OpenSearch(p) => p.get_doc(index, id).await,
            #[cfg(feature = "real-meilisearch")]
            SearchWrapper::Meilisearch(p) => p.get_doc(index, id).await,
        }
    }

    async fn delete_doc(&self, index: &str, id: &str) -> Result<(), SearchError> {
        match self {
            SearchWrapper::Memory(p) => p.delete_doc(index, id).await,
            SearchWrapper::Stub(p) => p.delete_doc(index, id).await,
            #[cfg(feature = "real-es")]
            SearchWrapper::Elasticsearch(p) => p.delete_doc(index, id).await,
            #[cfg(feature = "real-opensearch")]
            SearchWrapper::OpenSearch(p) => p.delete_doc(index, id).await,
            #[cfg(feature = "real-meilisearch")]
            SearchWrapper::Meilisearch(p) => p.delete_doc(index, id).await,
        }
    }

    async fn search(&self, index: &str, query: &SearchQuery) -> Result<SearchResult, SearchError> {
        match self {
            SearchWrapper::Memory(p) => p.search(index, query).await,
            SearchWrapper::Stub(p) => p.search(index, query).await,
            #[cfg(feature = "real-es")]
            SearchWrapper::Elasticsearch(p) => p.search(index, query).await,
            #[cfg(feature = "real-opensearch")]
            SearchWrapper::OpenSearch(p) => p.search(index, query).await,
            #[cfg(feature = "real-meilisearch")]
            SearchWrapper::Meilisearch(p) => p.search(index, query).await,
        }
    }

    async fn count(&self, index: &str, query: &SearchQuery) -> Result<u64, SearchError> {
        match self {
            SearchWrapper::Memory(p) => p.count(index, query).await,
            SearchWrapper::Stub(p) => p.count(index, query).await,
            #[cfg(feature = "real-es")]
            SearchWrapper::Elasticsearch(p) => p.count(index, query).await,
            #[cfg(feature = "real-opensearch")]
            SearchWrapper::OpenSearch(p) => p.count(index, query).await,
            #[cfg(feature = "real-meilisearch")]
            SearchWrapper::Meilisearch(p) => p.count(index, query).await,
        }
    }

    async fn refresh(&self, index: &str) -> Result<(), SearchError> {
        match self {
            SearchWrapper::Memory(p) => p.refresh(index).await,
            SearchWrapper::Stub(p) => p.refresh(index).await,
            #[cfg(feature = "real-es")]
            SearchWrapper::Elasticsearch(p) => p.refresh(index).await,
            #[cfg(feature = "real-opensearch")]
            SearchWrapper::OpenSearch(p) => p.refresh(index).await,
            #[cfg(feature = "real-meilisearch")]
            SearchWrapper::Meilisearch(p) => p.refresh(index).await,
        }
    }
}

/// Builder
pub struct SearchBuilder {
    provider: SearchProvider,
}

impl SearchBuilder {
    pub fn new(provider: SearchProvider) -> Self {
        Self { provider }
    }

    pub fn build(self) -> Result<SearchWrapper, SearchError> {
        match self.provider {
            SearchProvider::Memory => Ok(SearchWrapper::Memory(crate::memory::MemorySearch::new())),
            SearchProvider::Stub => Ok(SearchWrapper::Stub(crate::stub::StubSearch::new())),
            #[cfg(feature = "real-es")]
            SearchProvider::Elasticsearch(config) => {
                let p = crate::elasticsearch_provider::ElasticsearchProvider::new(config)?;
                Ok(SearchWrapper::Elasticsearch(p))
            }
            #[cfg(feature = "real-opensearch")]
            SearchProvider::OpenSearch(config) => {
                let p = crate::opensearch_provider::OpensearchProvider::new(config)?;
                Ok(SearchWrapper::OpenSearch(p))
            }
            #[cfg(feature = "real-meilisearch")]
            SearchProvider::Meilisearch(config) => {
                let p = crate::meilisearch_provider::MeilisearchProvider::new(config)?;
                Ok(SearchWrapper::Meilisearch(p))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_builder_memory() {
        let wrapper = SearchBuilder::new(SearchProvider::Memory)
            .build()
            .expect("build failed");
        wrapper
            .create_index("test", &serde_json::json!({}))
            .await
            .expect("create_index failed");
    }

    #[tokio::test]
    async fn test_builder_stub() {
        let wrapper = SearchBuilder::new(SearchProvider::Stub)
            .build()
            .expect("build failed");
        wrapper
            .create_index("test", &serde_json::json!({}))
            .await
            .expect("create_index failed");
    }
}
