//! Stub 实现：生成查询 JSON 但不执行

use crate::error::SearchError;
use crate::search::SearchExt;
use crate::types::{SearchQuery, SearchResult};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Mutex;

/// Stub Search 实现
pub struct StubSearch {
    pub operation_log: Mutex<Vec<String>>,
}

impl StubSearch {
    pub fn new() -> Self {
        Self {
            operation_log: Mutex::new(Vec::new()),
        }
    }

    pub fn operations(&self) -> Vec<String> {
        self.operation_log.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.operation_log.lock().unwrap().clear();
    }

    fn log(&self, op: String) {
        self.operation_log.lock().unwrap().push(op);
    }
}

impl Default for StubSearch {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchExt for StubSearch {
    async fn create_index(&self, index: &str, mappings: &Value) -> Result<(), SearchError> {
        self.log(format!(
            "CREATE INDEX {} mappings={}",
            index,
            serde_json::to_string(mappings).unwrap_or_default()
        ));
        Ok(())
    }

    async fn delete_index(&self, index: &str) -> Result<(), SearchError> {
        self.log(format!("DELETE INDEX {}", index));
        Ok(())
    }

    async fn index_doc(&self, index: &str, id: &str, doc: &Value) -> Result<(), SearchError> {
        self.log(format!(
            "INDEX {}/{} doc={}",
            index,
            id,
            serde_json::to_string(doc).unwrap_or_default()
        ));
        Ok(())
    }

    async fn get_doc(&self, index: &str, id: &str) -> Result<Option<Value>, SearchError> {
        self.log(format!("GET {}/{}", index, id));
        Ok(None)
    }

    async fn delete_doc(&self, index: &str, id: &str) -> Result<(), SearchError> {
        self.log(format!("DELETE {}/{}", index, id));
        Ok(())
    }

    async fn search(&self, index: &str, query: &SearchQuery) -> Result<SearchResult, SearchError> {
        let es_dsl = query.to_es_dsl();
        self.log(format!(
            "SEARCH {} dsl={}",
            index,
            serde_json::to_string(&es_dsl).unwrap_or_default()
        ));
        Ok(SearchResult::empty())
    }

    async fn count(&self, index: &str, query: &SearchQuery) -> Result<u64, SearchError> {
        self.log(format!(
            "COUNT {} dsl={}",
            index,
            serde_json::to_string(&query.to_es_dsl()).unwrap_or_default()
        ));
        Ok(0)
    }

    async fn refresh(&self, index: &str) -> Result<(), SearchError> {
        self.log(format!("REFRESH {}", index));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stub_create_index() {
        let stub = StubSearch::new();
        stub.create_index("test", &serde_json::json!({"mappings": {}}))
            .await
            .unwrap();
        let ops = stub.operations();
        assert_eq!(ops.len(), 1);
        assert!(ops[0].contains("CREATE INDEX"));
    }

    #[tokio::test]
    async fn test_stub_index_doc() {
        let stub = StubSearch::new();
        stub.index_doc("docs", "1", &serde_json::json!({"title": "test"}))
            .await
            .unwrap();
        let ops = stub.operations();
        assert!(ops[0].contains("INDEX docs/1"));
    }

    #[tokio::test]
    async fn test_stub_search() {
        let stub = StubSearch::new();
        let q = SearchQuery::new("hello");
        let result = stub.search("docs", &q).await.unwrap();
        assert_eq!(result.total, 0);
        let ops = stub.operations();
        assert!(ops[0].contains("SEARCH docs"));
        assert!(ops[0].contains("multi_match"));
    }

    #[tokio::test]
    async fn test_stub_delete() {
        let stub = StubSearch::new();
        stub.delete_doc("docs", "1").await.unwrap();
        stub.delete_index("docs").await.unwrap();
        let ops = stub.operations();
        assert_eq!(ops.len(), 2);
        assert!(ops[0].contains("DELETE docs/1"));
        assert!(ops[1].contains("DELETE INDEX docs"));
    }

    #[tokio::test]
    async fn test_stub_refresh() {
        let stub = StubSearch::new();
        stub.refresh("docs").await.unwrap();
        let ops = stub.operations();
        assert!(ops[0].contains("REFRESH docs"));
    }
}
