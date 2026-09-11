//! CDC 搜索索引刷新 Sink（v6.8.0 CDC-SEARCH-01）
//!
//! 接收变更事件，将变更转化为搜索索引刷新指令：
//! - Insert/Update → `SearchExt::index_doc`
//! - Delete → `SearchExt::delete_doc`

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use super::super::checkpoint::CdcError;
use super::super::dispatcher::CdcSink;
use super::super::event::{ChangeEvent, ChangeEventType};
use sz_orm_search::search::SearchExt;

/// 索引名解析策略：将源表名映射到搜索索引名
#[derive(Debug, Clone)]
pub struct IndexNameResolver {
    mapping: HashMap<String, String>,
    default_suffix: String,
}

impl IndexNameResolver {
    /// 创建解析器，默认索引名 = 表名 + `_idx`
    pub fn new() -> Self {
        Self {
            mapping: HashMap::new(),
            default_suffix: "_idx".to_string(),
        }
    }

    /// 注册显式映射（表名 → 索引名）
    pub fn with_mapping(mut self, table: &str, index: &str) -> Self {
        self.mapping.insert(table.to_string(), index.to_string());
        self
    }

    /// 解析索引名
    pub fn resolve(&self, table: &str) -> String {
        if let Some(name) = self.mapping.get(table) {
            name.clone()
        } else {
            format!("{}{}", table, self.default_suffix)
        }
    }
}

impl Default for IndexNameResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// 文档 ID 提取策略：从行数据 JSON 中提取主键字段
#[derive(Debug, Clone)]
pub struct DocIdExtractor {
    id_field: String,
}

impl DocIdExtractor {
    /// 创建提取器，指定 ID 字段名
    pub fn new(id_field: &str) -> Self {
        Self {
            id_field: id_field.to_string(),
        }
    }

    /// 从行数据中提取文档 ID
    pub fn extract(&self, row_data: &Value) -> Option<String> {
        if let Value::Object(map) = row_data {
            if let Some(id_val) = map.get(&self.id_field) {
                match id_val {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                }
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl Default for DocIdExtractor {
    fn default() -> Self {
        Self::new("id")
    }
}

/// CDC 搜索索引刷新 Sink
///
/// 将变更事件转化为搜索索引刷新指令。
pub struct SearchIndexSink {
    provider: Arc<dyn SearchExt>,
    index_resolver: IndexNameResolver,
    id_extractor: DocIdExtractor,
    indexed_count: AtomicU64,
    deleted_count: AtomicU64,
    error_count: AtomicU64,
}

impl SearchIndexSink {
    /// 创建搜索索引 Sink
    pub fn new(provider: Arc<dyn SearchExt>) -> Self {
        Self {
            provider,
            index_resolver: IndexNameResolver::new(),
            id_extractor: DocIdExtractor::new("id"),
            indexed_count: AtomicU64::new(0),
            deleted_count: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
        }
    }

    /// 设置索引名解析器
    pub fn with_index_resolver(mut self, resolver: IndexNameResolver) -> Self {
        self.index_resolver = resolver;
        self
    }

    /// 设置文档 ID 提取器
    pub fn with_id_extractor(mut self, extractor: DocIdExtractor) -> Self {
        self.id_extractor = extractor;
        self
    }

    /// 返回已索引的文档数（Insert/Update）
    pub fn indexed_total(&self) -> u64 {
        self.indexed_count.load(Ordering::Relaxed)
    }

    /// 返回已删除的文档数（Delete）
    pub fn deleted_total(&self) -> u64 {
        self.deleted_count.load(Ordering::Relaxed)
    }

    /// 返回处理失败的事件数
    pub fn error_total(&self) -> u64 {
        self.error_count.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl CdcSink for SearchIndexSink {
    async fn handle(&self, event: &ChangeEvent) -> Result<(), CdcError> {
        let index_name = self.index_resolver.resolve(&event.source_table);
        let doc_id = match self.id_extractor.extract(&event.row_data) {
            Some(id) => id,
            None => {
                self.error_count.fetch_add(1, Ordering::Relaxed);
                return Err(CdcError::SinkError(format!(
                    "无法从事件 {} 提取文档 ID（字段 '{}' 不存在）",
                    event.event_id, self.id_extractor.id_field
                )));
            }
        };

        match event.event_type {
            ChangeEventType::Insert | ChangeEventType::Update => {
                match self
                    .provider
                    .index_doc(&index_name, &doc_id, &event.row_data)
                    .await
                {
                    Ok(()) => {
                        self.indexed_count.fetch_add(1, Ordering::Relaxed);
                        Ok(())
                    }
                    Err(e) => {
                        self.error_count.fetch_add(1, Ordering::Relaxed);
                        Err(CdcError::SinkError(format!(
                            "索引文档失败 (index={}, id={}): {}",
                            index_name, doc_id, e
                        )))
                    }
                }
            }
            ChangeEventType::Delete => match self.provider.delete_doc(&index_name, &doc_id).await {
                Ok(()) => {
                    self.deleted_count.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }
                Err(e) => {
                    self.error_count.fetch_add(1, Ordering::Relaxed);
                    Err(CdcError::SinkError(format!(
                        "删除索引文档失败 (index={}, id={}): {}",
                        index_name, doc_id, e
                    )))
                }
            },
        }
    }
}

/// 仅对指定表执行索引刷新的过滤型 Sink
pub struct TableFilteredSearchIndexSink {
    inner: SearchIndexSink,
    watched_tables: std::collections::HashSet<String>,
}

impl TableFilteredSearchIndexSink {
    /// 创建表过滤搜索索引 Sink
    pub fn new(provider: Arc<dyn SearchExt>, watched_tables: Vec<String>) -> Self {
        Self {
            inner: SearchIndexSink::new(provider),
            watched_tables: watched_tables.into_iter().collect(),
        }
    }

    /// 返回已索引的文档数
    pub fn indexed_total(&self) -> u64 {
        self.inner.indexed_total()
    }

    /// 返回已删除的文档数
    pub fn deleted_total(&self) -> u64 {
        self.inner.deleted_total()
    }
}

#[async_trait]
impl CdcSink for TableFilteredSearchIndexSink {
    async fn handle(&self, event: &ChangeEvent) -> Result<(), CdcError> {
        if !self.watched_tables.contains(&event.source_table) {
            return Ok(());
        }
        self.inner.handle(event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_search::error::SearchError;
    use sz_orm_search::types::{SearchQuery, SearchResult};

    struct MockSearchProvider {
        docs: parking_lot::RwLock<HashMap<(String, String), Value>>,
    }

    impl MockSearchProvider {
        fn new() -> Self {
            Self {
                docs: parking_lot::RwLock::new(HashMap::new()),
            }
        }

        fn get_doc(&self, index: &str, id: &str) -> Option<Value> {
            self.docs
                .read()
                .get(&(index.to_string(), id.to_string()))
                .cloned()
        }

        fn doc_count(&self) -> usize {
            self.docs.read().len()
        }
    }

    #[async_trait]
    impl SearchExt for MockSearchProvider {
        async fn create_index(&self, _index: &str, _mappings: &Value) -> Result<(), SearchError> {
            Ok(())
        }

        async fn delete_index(&self, index: &str) -> Result<(), SearchError> {
            self.docs.write().retain(|(idx, _), _| idx != index);
            Ok(())
        }

        async fn index_doc(&self, index: &str, id: &str, doc: &Value) -> Result<(), SearchError> {
            self.docs
                .write()
                .insert((index.to_string(), id.to_string()), doc.clone());
            Ok(())
        }

        async fn get_doc(&self, index: &str, id: &str) -> Result<Option<Value>, SearchError> {
            Ok(self.get_doc(index, id))
        }

        async fn delete_doc(&self, index: &str, id: &str) -> Result<(), SearchError> {
            self.docs
                .write()
                .remove(&(index.to_string(), id.to_string()));
            Ok(())
        }

        async fn search(
            &self,
            _index: &str,
            _query: &SearchQuery,
        ) -> Result<SearchResult, SearchError> {
            Ok(SearchResult {
                total: 0,
                hits: vec![],
                took_ms: 0,
            })
        }

        async fn count(&self, _index: &str, _query: &SearchQuery) -> Result<u64, SearchError> {
            Ok(self.doc_count() as u64)
        }

        async fn refresh(&self, _index: &str) -> Result<(), SearchError> {
            Ok(())
        }
    }

    use crate::cdc::event::ChangePosition;

    fn make_event(event_type: ChangeEventType, table: &str, row_data: Value) -> ChangeEvent {
        ChangeEvent::new(
            event_type,
            "test_db",
            table,
            row_data,
            ChangePosition::SqliteHook { seq: 1 },
            1000,
        )
    }

    #[tokio::test]
    async fn insert_event_indexes_document() {
        let provider = Arc::new(MockSearchProvider::new());
        let sink = SearchIndexSink::new(provider.clone());
        let event = make_event(
            ChangeEventType::Insert,
            "users",
            serde_json::json!({"id": "u1", "name": "Alice"}),
        );
        sink.handle(&event).await.unwrap();
        assert_eq!(sink.indexed_total(), 1);
        assert!(provider.get_doc("users_idx", "u1").is_some());
    }

    #[tokio::test]
    async fn update_event_reindexes_document() {
        let provider = Arc::new(MockSearchProvider::new());
        let sink = SearchIndexSink::new(provider.clone());
        let insert = make_event(
            ChangeEventType::Insert,
            "users",
            serde_json::json!({"id": "u1", "name": "Alice"}),
        );
        let update = make_event(
            ChangeEventType::Update,
            "users",
            serde_json::json!({"id": "u1", "name": "Bob"}),
        );
        sink.handle(&insert).await.unwrap();
        sink.handle(&update).await.unwrap();
        assert_eq!(sink.indexed_total(), 2);
        let doc = provider.get_doc("users_idx", "u1").unwrap();
        assert_eq!(doc["name"], "Bob");
    }

    #[tokio::test]
    async fn delete_event_removes_document() {
        let provider = Arc::new(MockSearchProvider::new());
        let sink = SearchIndexSink::new(provider.clone());
        let insert = make_event(
            ChangeEventType::Insert,
            "users",
            serde_json::json!({"id": "u1", "name": "Alice"}),
        );
        let delete = make_event(
            ChangeEventType::Delete,
            "users",
            serde_json::json!({"id": "u1"}),
        );
        sink.handle(&insert).await.unwrap();
        sink.handle(&delete).await.unwrap();
        assert_eq!(sink.indexed_total(), 1);
        assert_eq!(sink.deleted_total(), 1);
        assert!(provider.get_doc("users_idx", "u1").is_none());
    }

    #[tokio::test]
    async fn custom_index_name_resolver() {
        let provider = Arc::new(MockSearchProvider::new());
        let resolver = IndexNameResolver::new().with_mapping("users", "user_search");
        let sink = SearchIndexSink::new(provider.clone()).with_index_resolver(resolver);
        let event = make_event(
            ChangeEventType::Insert,
            "users",
            serde_json::json!({"id": "u1", "name": "Alice"}),
        );
        sink.handle(&event).await.unwrap();
        assert!(provider.get_doc("user_search", "u1").is_some());
        assert!(provider.get_doc("users_idx", "u1").is_none());
    }

    #[tokio::test]
    async fn custom_id_field_extractor() {
        let provider = Arc::new(MockSearchProvider::new());
        let extractor = DocIdExtractor::new("user_id");
        let sink = SearchIndexSink::new(provider.clone()).with_id_extractor(extractor);
        let event = make_event(
            ChangeEventType::Insert,
            "orders",
            serde_json::json!({"user_id": "o1", "amount": 100}),
        );
        sink.handle(&event).await.unwrap();
        assert!(provider.get_doc("orders_idx", "o1").is_some());
    }

    #[tokio::test]
    async fn missing_id_field_returns_error() {
        let provider = Arc::new(MockSearchProvider::new());
        let sink = SearchIndexSink::new(provider);
        let event = make_event(
            ChangeEventType::Insert,
            "users",
            serde_json::json!({"name": "Alice"}),
        );
        let result = sink.handle(&event).await;
        assert!(result.is_err());
        assert_eq!(sink.error_total(), 1);
        assert_eq!(sink.indexed_total(), 0);
    }

    #[tokio::test]
    async fn numeric_id_extracted_correctly() {
        let provider = Arc::new(MockSearchProvider::new());
        let sink = SearchIndexSink::new(provider.clone());
        let event = make_event(
            ChangeEventType::Insert,
            "users",
            serde_json::json!({"id": 42, "name": "Alice"}),
        );
        sink.handle(&event).await.unwrap();
        assert!(provider.get_doc("users_idx", "42").is_some());
    }

    #[tokio::test]
    async fn table_filtered_sink_skips_unwatched_tables() {
        let provider = Arc::new(MockSearchProvider::new());
        let sink = TableFilteredSearchIndexSink::new(provider.clone(), vec!["users".to_string()]);
        let users_event = make_event(
            ChangeEventType::Insert,
            "users",
            serde_json::json!({"id": "u1", "name": "Alice"}),
        );
        let orders_event = make_event(
            ChangeEventType::Insert,
            "orders",
            serde_json::json!({"id": "o1", "amount": 100}),
        );
        sink.handle(&users_event).await.unwrap();
        sink.handle(&orders_event).await.unwrap();
        assert_eq!(sink.indexed_total(), 1);
        assert!(provider.get_doc("users_idx", "u1").is_some());
        assert!(provider.get_doc("orders_idx", "o1").is_none());
    }
}
