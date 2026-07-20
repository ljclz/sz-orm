//! 真实 Elasticsearch 实现（feature = "real-es"）
//!
//! 通过 elasticsearch crate 连接真实 ES 集群。

use crate::error::SearchError;
use crate::search::{ElasticsearchConfig, SearchExt};
use crate::types::{SearchHit, SearchQuery, SearchResult};
use async_trait::async_trait;
use elasticsearch::{auth::Credentials, http::transport::Transport, ElasticSearch};
use serde_json::Value;

/// Elasticsearch 真实实现
pub struct ElasticsearchProvider {
    client: ElasticSearch,
}

impl ElasticsearchProvider {
    pub fn new(config: ElasticsearchConfig) -> Result<Self, SearchError> {
        let transport = if let (Some(user), Some(pass)) = (&config.username, &config.password) {
            let credentials = Credentials::Basic(user.clone(), pass.clone());
            let transport_builder = Transport::single_node(&config.url);
            let mut transport_builder = transport_builder;
            transport_builder = transport_builder.auth(credentials);
            transport_builder
                .build()
                .map_err(|e| SearchError::Connection(e.to_string()))?
        } else {
            Transport::single_node(&config.url)
                .build()
                .map_err(|e| SearchError::Connection(e.to_string()))?
        };
        let client = ElasticSearch::new(transport);
        Ok(Self { client })
    }
}

#[async_trait]
impl SearchExt for ElasticsearchProvider {
    async fn create_index(&self, index: &str, mappings: &Value) -> Result<(), SearchError> {
        use elasticsearch::indices::IndicesCreateParts;
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
        use elasticsearch::indices::IndicesDeleteParts;
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
        use elasticsearch::IndexParts;
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
        use elasticsearch::GetParts;
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
        use elasticsearch::DeleteParts;
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
        use elasticsearch::SearchParts;
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
        use elasticsearch::CountParts;
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
        use elasticsearch::indices::IndicesRefreshParts;
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
