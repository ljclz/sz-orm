//! 真实 OpenSearch 实现（feature = "real-opensearch"）
//!
//! 通过 opensearch crate 连接真实 OpenSearch 集群。
//! OpenSearch 是 Elasticsearch 的开源分支，API 高度兼容。

use crate::error::SearchError;
use crate::search::{OpenSearchConfig, SearchExt};
use crate::types::{SearchHit, SearchQuery, SearchResult};
use async_trait::async_trait;
use opensearch::http::transport::{SingleNodeConnectionPool, TransportBuilder};
use opensearch::{auth::Credentials, OpenSearch as OpenSearchClient};
use serde_json::Value;
use url::Url;

/// OpenSearch 真实实现
pub struct OpensearchProvider {
    client: OpenSearchClient,
}

impl OpensearchProvider {
    pub fn new(config: OpenSearchConfig) -> Result<Self, SearchError> {
        let url = Url::parse(&config.url)
            .map_err(|e| SearchError::InvalidConfig(format!("invalid url: {}", e)))?;
        let pool = SingleNodeConnectionPool::new(url);
        let mut transport_builder = TransportBuilder::new(pool);
        if let (Some(user), Some(pass)) = (&config.username, &config.password) {
            transport_builder =
                transport_builder.auth(Credentials::Basic(user.clone(), pass.clone()));
        }
        let transport = transport_builder
            .build()
            .map_err(|e| SearchError::Connection(e.to_string()))?;
        Ok(Self {
            client: OpenSearchClient::new(transport),
        })
    }
}

#[async_trait]
impl SearchExt for OpensearchProvider {
    async fn create_index(&self, index: &str, mappings: &Value) -> Result<(), SearchError> {
        use opensearch::indices::IndicesCreateParts;
        let response = self
            .client
            .indices()
            .create(IndicesCreateParts::Index(index))
            .body(mappings.clone())
            .send()
            .await
            .map_err(|e| SearchError::Query(e.to_string()))?;
        if !response.status_code().is_success() {
            return Err(SearchError::Query(format!(
                "create index failed: {}",
                response.status_code()
            )));
        }
        Ok(())
    }

    async fn delete_index(&self, index: &str) -> Result<(), SearchError> {
        use opensearch::indices::IndicesDeleteParts;
        let response = self
            .client
            .indices()
            .delete(IndicesDeleteParts::Index(&[index]))
            .send()
            .await
            .map_err(|e| SearchError::Query(e.to_string()))?;
        if !response.status_code().is_success() && response.status_code() != 404 {
            return Err(SearchError::Query(format!(
                "delete index failed: {}",
                response.status_code()
            )));
        }
        Ok(())
    }

    async fn index_doc(&self, index: &str, id: &str, doc: &Value) -> Result<(), SearchError> {
        use opensearch::IndexParts;
        let response = self
            .client
            .index(IndexParts::IndexId(index, id))
            .body(doc.clone())
            .send()
            .await
            .map_err(|e| SearchError::Query(e.to_string()))?;
        if !response.status_code().is_success() {
            return Err(SearchError::Query(format!(
                "index doc failed: {}",
                response.status_code()
            )));
        }
        Ok(())
    }

    async fn get_doc(&self, index: &str, id: &str) -> Result<Option<Value>, SearchError> {
        use opensearch::GetParts;
        let response = self
            .client
            .get(GetParts::IndexId(index, id))
            .send()
            .await
            .map_err(|e| SearchError::Query(e.to_string()))?;
        if response.status_code() == 404 {
            return Ok(None);
        }
        if !response.status_code().is_success() {
            return Err(SearchError::Query(format!(
                "get doc failed: {}",
                response.status_code()
            )));
        }
        let response_body: Value = response
            .json()
            .await
            .map_err(|e| SearchError::Query(e.to_string()))?;
        Ok(response_body.get("source").cloned())
    }

    async fn delete_doc(&self, index: &str, id: &str) -> Result<(), SearchError> {
        use opensearch::DeleteParts;
        let response = self
            .client
            .delete(DeleteParts::IndexId(index, id))
            .send()
            .await
            .map_err(|e| SearchError::Query(e.to_string()))?;
        if !response.status_code().is_success() && response.status_code() != 404 {
            return Err(SearchError::Query(format!(
                "delete doc failed: {}",
                response.status_code()
            )));
        }
        Ok(())
    }

    async fn search(&self, index: &str, query: &SearchQuery) -> Result<SearchResult, SearchError> {
        use opensearch::SearchParts;
        let dsl = query.to_es_dsl();
        let response = self
            .client
            .search(SearchParts::Index(&[index]))
            .body(dsl)
            .send()
            .await
            .map_err(|e| SearchError::Query(e.to_string()))?;
        if !response.status_code().is_success() {
            return Err(SearchError::Query(format!(
                "search failed: {}",
                response.status_code()
            )));
        }
        let response_body: Value = response
            .json()
            .await
            .map_err(|e| SearchError::Query(e.to_string()))?;
        let took_ms = response_body
            .get("took")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let total = response_body
            .get("hits")
            .and_then(|h| h.get("total"))
            .and_then(|t| t.get("value"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let hits: Vec<SearchHit> = response_body
            .get("hits")
            .and_then(|h| h.get("hits"))
            .and_then(|h| h.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|hit| {
                        let id = hit.get("_id")?.as_str()?.to_string();
                        let score = hit.get("_score").and_then(|s| s.as_f64()).unwrap_or(0.0);
                        let source = hit.get("_source")?.clone();
                        Some(SearchHit::new(id, score, source))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(SearchResult::new(total, hits, took_ms))
    }

    async fn count(&self, index: &str, query: &SearchQuery) -> Result<u64, SearchError> {
        use opensearch::CountParts;
        let dsl = serde_json::json!({ "query": query.to_es_dsl()["query"] });
        let response = self
            .client
            .count(CountParts::Index(&[index]))
            .body(dsl)
            .send()
            .await
            .map_err(|e| SearchError::Query(e.to_string()))?;
        if !response.status_code().is_success() {
            return Err(SearchError::Query(format!(
                "count failed: {}",
                response.status_code()
            )));
        }
        let response_body: Value = response
            .json()
            .await
            .map_err(|e| SearchError::Query(e.to_string()))?;
        Ok(response_body
            .get("count")
            .and_then(|c| c.as_u64())
            .unwrap_or(0))
    }

    async fn refresh(&self, index: &str) -> Result<(), SearchError> {
        use opensearch::indices::IndicesRefreshParts;
        let response = self
            .client
            .indices()
            .refresh(IndicesRefreshParts::Index(&[index]))
            .send()
            .await
            .map_err(|e| SearchError::Query(e.to_string()))?;
        if !response.status_code().is_success() {
            return Err(SearchError::Query(format!(
                "refresh failed: {}",
                response.status_code()
            )));
        }
        Ok(())
    }
}
