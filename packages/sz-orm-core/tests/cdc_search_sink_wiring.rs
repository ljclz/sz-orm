//! W2-9 CDC-SEARCH-01 接线验证：下游搜索索引刷新
//!
//! 更新某行 + 该行已被搜索索引收录 → 断言索引文档被更新 + 下次检索命中新值。

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use sz_orm_core::cdc::dispatcher::CdcSink;
use sz_orm_core::cdc::event::{ChangeEvent, ChangeEventType, ChangePosition};
use sz_orm_core::cdc::sinks::search::{
    IndexNameResolver, SearchIndexSink, TableFilteredSearchIndexSink,
};
use sz_orm_search::error::SearchError;
use sz_orm_search::search::SearchExt;
use sz_orm_search::types::{SearchHit, SearchQuery, SearchResult};

struct InMemorySearchProvider {
    docs: parking_lot::RwLock<std::collections::HashMap<(String, String), Value>>,
}

impl InMemorySearchProvider {
    fn new() -> Self {
        Self {
            docs: parking_lot::RwLock::new(std::collections::HashMap::new()),
        }
    }

    fn get(&self, index: &str, id: &str) -> Option<Value> {
        self.docs
            .read()
            .get(&(index.to_string(), id.to_string()))
            .cloned()
    }

    fn contains(&self, index: &str, id: &str) -> bool {
        self.docs
            .read()
            .contains_key(&(index.to_string(), id.to_string()))
    }
}

#[async_trait]
impl SearchExt for InMemorySearchProvider {
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
        Ok(self.get(index, id))
    }

    async fn delete_doc(&self, index: &str, id: &str) -> Result<(), SearchError> {
        self.docs
            .write()
            .remove(&(index.to_string(), id.to_string()));
        Ok(())
    }

    async fn search(&self, index: &str, query: &SearchQuery) -> Result<SearchResult, SearchError> {
        let docs = self.docs.read();
        let mut hits = Vec::new();
        for ((idx, id), doc) in docs.iter() {
            if idx == index && !query.query.is_empty() {
                if let Some(name) = doc.get("name").and_then(|v| v.as_str()) {
                    if name.contains(query.query.as_str()) {
                        hits.push(SearchHit::new(id.clone(), 1.0, doc.clone()));
                    }
                }
            }
        }
        let total = hits.len() as u64;
        Ok(SearchResult::new(total, hits, 1))
    }

    async fn count(&self, _index: &str, _query: &SearchQuery) -> Result<u64, SearchError> {
        Ok(self.docs.read().len() as u64)
    }

    async fn refresh(&self, _index: &str) -> Result<(), SearchError> {
        Ok(())
    }
}

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
async fn wiring_insert_then_search_finds_document() {
    let provider = Arc::new(InMemorySearchProvider::new());
    let sink = SearchIndexSink::new(provider.clone());

    let event = make_event(
        ChangeEventType::Insert,
        "users",
        serde_json::json!({"id": "u1", "name": "Alice"}),
    );
    sink.handle(&event).await.unwrap();

    let query = SearchQuery::new("Alice");
    let result = provider.search("users_idx", &query).await.unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.hits[0].id, "u1");
}

#[tokio::test]
async fn wiring_update_then_search_finds_new_value() {
    let provider = Arc::new(InMemorySearchProvider::new());
    let sink = SearchIndexSink::new(provider.clone());

    sink.handle(&make_event(
        ChangeEventType::Insert,
        "users",
        serde_json::json!({"id": "u1", "name": "Alice"}),
    ))
    .await
    .unwrap();

    sink.handle(&make_event(
        ChangeEventType::Update,
        "users",
        serde_json::json!({"id": "u1", "name": "Bob"}),
    ))
    .await
    .unwrap();

    let alice_result = provider
        .search("users_idx", &SearchQuery::new("Alice"))
        .await
        .unwrap();
    let bob_result = provider
        .search("users_idx", &SearchQuery::new("Bob"))
        .await
        .unwrap();

    assert_eq!(alice_result.total, 0);
    assert_eq!(bob_result.total, 1);
    assert_eq!(bob_result.hits[0].id, "u1");
}

#[tokio::test]
async fn wiring_delete_then_search_does_not_find_document() {
    let provider = Arc::new(InMemorySearchProvider::new());
    let sink = SearchIndexSink::new(provider.clone());

    sink.handle(&make_event(
        ChangeEventType::Insert,
        "users",
        serde_json::json!({"id": "u1", "name": "Alice"}),
    ))
    .await
    .unwrap();

    sink.handle(&make_event(
        ChangeEventType::Delete,
        "users",
        serde_json::json!({"id": "u1"}),
    ))
    .await
    .unwrap();

    assert!(!provider.contains("users_idx", "u1"));

    let result = provider
        .search("users_idx", &SearchQuery::new("Alice"))
        .await
        .unwrap();
    assert_eq!(result.total, 0);
}

#[tokio::test]
async fn wiring_custom_index_name_used_in_search() {
    let provider = Arc::new(InMemorySearchProvider::new());
    let resolver = IndexNameResolver::new().with_mapping("products", "product_search");
    let sink = SearchIndexSink::new(provider.clone()).with_index_resolver(resolver);

    sink.handle(&make_event(
        ChangeEventType::Insert,
        "products",
        serde_json::json!({"id": "p1", "name": "Widget"}),
    ))
    .await
    .unwrap();

    assert!(provider.contains("product_search", "p1"));
    assert!(!provider.contains("products_idx", "p1"));
}

#[tokio::test]
async fn wiring_table_filtered_sink_only_indexes_watched_tables() {
    let provider = Arc::new(InMemorySearchProvider::new());
    let sink = TableFilteredSearchIndexSink::new(provider.clone(), vec!["users".to_string()]);

    sink.handle(&make_event(
        ChangeEventType::Insert,
        "users",
        serde_json::json!({"id": "u1", "name": "Alice"}),
    ))
    .await
    .unwrap();

    sink.handle(&make_event(
        ChangeEventType::Insert,
        "orders",
        serde_json::json!({"id": "o1", "amount": 100}),
    ))
    .await
    .unwrap();

    assert!(provider.contains("users_idx", "u1"));
    assert!(!provider.contains("orders_idx", "o1"));
    assert_eq!(sink.indexed_total(), 1);
}

#[tokio::test]
async fn wiring_multiple_crud_operations_consistent_state() {
    let provider = Arc::new(InMemorySearchProvider::new());
    let sink = SearchIndexSink::new(provider.clone());

    sink.handle(&make_event(
        ChangeEventType::Insert,
        "users",
        serde_json::json!({"id": "u1", "name": "Alice"}),
    ))
    .await
    .unwrap();
    sink.handle(&make_event(
        ChangeEventType::Insert,
        "users",
        serde_json::json!({"id": "u2", "name": "Bob"}),
    ))
    .await
    .unwrap();
    sink.handle(&make_event(
        ChangeEventType::Update,
        "users",
        serde_json::json!({"id": "u1", "name": "Charlie"}),
    ))
    .await
    .unwrap();
    sink.handle(&make_event(
        ChangeEventType::Delete,
        "users",
        serde_json::json!({"id": "u2"}),
    ))
    .await
    .unwrap();

    assert_eq!(sink.indexed_total(), 3);
    assert_eq!(sink.deleted_total(), 1);

    let u1 = provider.get("users_idx", "u1").unwrap();
    assert_eq!(u1["name"], "Charlie");
    assert!(!provider.contains("users_idx", "u2"));
}
