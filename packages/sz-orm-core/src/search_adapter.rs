//! # Search Adapter — sz-orm-core 搜索引擎适配层
//!
//! v5.0.0 M4：将 sz-orm-search 的 MemorySearch 接入 sz-orm-core，
//! 提供 `search_index_doc` / `search_query` / `search_query_count` 入口。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use parking_lot::RwLock;
use sz_orm_search::{MemorySearch, SearchExt, SearchQuery, SearchResult};

static SEARCH: OnceLock<RwLock<MemorySearch>> = OnceLock::new();
static QUERY_COUNT: AtomicU64 = AtomicU64::new(0);
static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn search() -> &'static RwLock<MemorySearch> {
    SEARCH.get_or_init(|| RwLock::new(MemorySearch::new()))
}

fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
    })
}

/// 索引文档
pub fn search_index_doc(
    index: &str,
    id: &str,
    doc: &serde_json::Value,
) -> Result<(), sz_orm_search::SearchError> {
    QUERY_COUNT.fetch_add(1, Ordering::Relaxed);
    let s = search().read();
    runtime().block_on(s.index_doc(index, id, doc))
}

/// 执行搜索
pub fn search_query(
    index: &str,
    query: &SearchQuery,
) -> Result<SearchResult, sz_orm_search::SearchError> {
    QUERY_COUNT.fetch_add(1, Ordering::Relaxed);
    let s = search().read();
    runtime().block_on(s.search(index, query))
}

/// 获取查询计数
pub fn search_query_count() -> u64 {
    QUERY_COUNT.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
