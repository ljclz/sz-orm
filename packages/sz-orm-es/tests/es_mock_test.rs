use serde_json::json;
use sz_orm_es::*;

#[test]
fn test_es_document_new() {
    let doc = EsDocument::new("index1", json!({"field": "value"}));
    assert_eq!(doc.index, "index1");
    assert_eq!(doc.source["field"], "value");
    assert!(doc.id.is_none());
}

#[test]
fn test_es_document_with_id() {
    let doc = EsDocument::new("index1", json!({})).with_id("doc123");
    assert_eq!(doc.id, Some("doc123".to_string()));
}

#[test]
fn test_es_document_timestamp() {
    let doc = EsDocument::new("index", json!({}));
    assert!(doc.timestamp > 0);
}

#[test]
fn test_es_query_match_all() {
    let query = EsQuery::match_all();
    let req = EsSearchRequest::new("users", query);
    assert_eq!(req.index, "users");
    assert_eq!(req.from, 0);
    assert_eq!(req.size, 10);
}

#[test]
fn test_es_query_term() {
    let query = EsQuery::term("name", json!("Alice"));
    let req = EsSearchRequest::new("users", query);
    assert_eq!(req.index, "users");
}

#[test]
fn test_es_search_request_with_pagination() {
    let req = EsSearchRequest::new("users", EsQuery::match_all()).with_pagination(10, 20);
    assert_eq!(req.from, 10);
    assert_eq!(req.size, 20);
}

#[test]
fn test_es_search_request_with_sort() {
    let req =
        EsSearchRequest::new("users", EsQuery::match_all()).with_sort("age", EsSortOrder::Desc);
    assert_eq!(req.sort.len(), 1);
    assert_eq!(req.sort[0].field, "age");
    assert_eq!(req.sort[0].order, EsSortOrder::Desc);
}

#[test]
fn test_es_sync_manager_new() {
    let manager = EsSyncManager::new();
    let docs = vec![EsDocument::new("index", json!({})).with_id("1")];
    let result = manager.sync_to_es(docs);
    assert!(result.is_ok());
}

#[test]
fn test_es_sync_manager_with_backend() {
    let backend = Box::new(InMemoryEsSync::new());
    let manager = EsSyncManager::with_backend(backend);
    let docs = vec![EsDocument::new("index", json!({"data": "test"})).with_id("1")];
    let result = manager.sync_to_es(docs);
    assert!(result.is_ok());
}

#[test]
fn test_es_sync_manager_search() {
    let manager = EsSyncManager::new();
    let docs = vec![EsDocument::new("users", json!({"name": "Alice"})).with_id("1")];
    manager.sync_to_es(docs).unwrap();
    let req = EsSearchRequest::new("users", EsQuery::match_all());
    let result = manager.search(req);
    assert!(result.is_ok());
}

#[test]
fn test_es_sync_manager_delete() {
    let manager = EsSyncManager::new();
    let docs = vec![EsDocument::new("users", json!({})).with_id("1")];
    manager.sync_to_es(docs).unwrap();
    let result = manager.delete_from_es("users", vec!["1".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn test_in_memory_es_sync_new() {
    let backend = InMemoryEsSync::new();
    let _ = backend;
}

#[test]
fn test_es_sort_order_equality() {
    assert_eq!(EsSortOrder::Asc, EsSortOrder::Asc);
    assert_ne!(EsSortOrder::Asc, EsSortOrder::Desc);
}
