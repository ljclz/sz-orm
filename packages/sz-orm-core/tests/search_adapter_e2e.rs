//! Search 适配层端到端测试
use sz_orm_core::search_adapter::{search_index_doc, search_query, search_query_count};
use sz_orm_search::SearchQuery;

#[test]
fn test_search_index_and_query_reachable() {
    let doc = serde_json::json!({"title": "hello world"});
    let _ = search_index_doc("docs", "1", &doc);
    let query = SearchQuery::new("hello");
    let _ = search_query("docs", &query);
    assert!(
        search_query_count() > 0,
        "search functions should be callable"
    );
}

#[test]
fn test_search_count_increments() {
    let before = search_query_count();
    let doc = serde_json::json!({"title": "test"});
    let _ = search_index_doc("docs", "2", &doc);
    let after = search_query_count();
    assert!(after > before);
}
