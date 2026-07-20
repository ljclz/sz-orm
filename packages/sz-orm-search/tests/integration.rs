//! sz-orm-search 集成测试
//!
//! 覆盖：
//! - Memory provider 全流程（CRUD + 搜索 + 过滤 + 排序 + 分页 + 计数 + bulk）
//! - Stub provider 操作日志验证
//! - SearchBuilder + SearchWrapper 多 provider 分发
//! - 错误路径（重复创建索引、删除不存在文档、操作不存在的索引等）
//! - ES/OpenSearch/Meilisearch DSL 生成验证（无需真实服务）

use serde_json::json;
use sz_orm_search::{MemorySearch, StubSearch};

fn build_memory() -> MemorySearch {
    MemorySearch::new()
}

fn build_stub() -> StubSearch {
    StubSearch::new()
}

mod memory_crud {
    use super::*;
    use sz_orm_search::{SearchError, SearchExt, SearchQuery, SortOrder};

    #[tokio::test]
    async fn memory_index_full_lifecycle() {
        let s = build_memory();
        s.create_index("docs", &json!({})).await.unwrap();

        // 索引多个文档
        s.index_doc(
            "docs",
            "1",
            &json!({"title": "rust programming", "lang": "rust"}),
        )
        .await
        .unwrap();
        s.index_doc(
            "docs",
            "2",
            &json!({"title": "python guide", "lang": "python"}),
        )
        .await
        .unwrap();
        s.index_doc(
            "docs",
            "3",
            &json!({"title": "rust advanced", "lang": "rust"}),
        )
        .await
        .unwrap();

        // 全文搜索 "rust" 应命中 2 条
        let result = s.search("docs", &SearchQuery::new("rust")).await.unwrap();
        assert_eq!(result.total, 2);
        assert!(result.took_ms < 1000, "memory search should be fast");

        // 过滤 lang=python
        let q = SearchQuery::new("").with_filter("lang", json!("python"));
        let result = s.search("docs", &q).await.unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.hits[0].id, "2");
    }

    #[tokio::test]
    async fn memory_get_and_delete_doc() {
        let s = build_memory();
        s.create_index("docs", &json!({})).await.unwrap();
        s.index_doc("docs", "1", &json!({"title": "hello"}))
            .await
            .unwrap();

        // 获取存在的文档
        let doc = s.get_doc("docs", "1").await.unwrap();
        assert_eq!(doc, Some(json!({"title": "hello"})));

        // 获取不存在的文档
        let missing = s.get_doc("docs", "999").await.unwrap();
        assert_eq!(missing, None);

        // 删除文档
        s.delete_doc("docs", "1").await.unwrap();
        let after = s.get_doc("docs", "1").await.unwrap();
        assert_eq!(after, None);

        // 再次删除应报错
        let result = s.delete_doc("docs", "1").await;
        assert!(matches!(result, Err(SearchError::DocNotFound { .. })));
    }

    #[tokio::test]
    async fn memory_index_not_found_errors() {
        let s = build_memory();
        // 操作不存在的索引
        let result = s.index_doc("ghost", "1", &json!({})).await;
        assert!(matches!(result, Err(SearchError::NotFound(_))));
        let result = s.get_doc("ghost", "1").await;
        assert!(matches!(result, Err(SearchError::NotFound(_))));
        let result = s.delete_doc("ghost", "1").await;
        assert!(matches!(result, Err(SearchError::NotFound(_))));
        let result = s.search("ghost", &SearchQuery::match_all()).await;
        assert!(matches!(result, Err(SearchError::NotFound(_))));
        let result = s.count("ghost", &SearchQuery::match_all()).await;
        assert!(matches!(result, Err(SearchError::NotFound(_))));
    }

    #[tokio::test]
    async fn memory_duplicate_create_index() {
        let s = build_memory();
        s.create_index("docs", &json!({})).await.unwrap();
        let result = s.create_index("docs", &json!({})).await;
        assert!(matches!(result, Err(SearchError::IndexAlreadyExists(_))));
    }

    #[tokio::test]
    async fn memory_delete_index() {
        let s = build_memory();
        s.create_index("docs", &json!({})).await.unwrap();
        s.delete_index("docs").await.unwrap();
        // 再次删除应报错
        let result = s.delete_index("docs").await;
        assert!(matches!(result, Err(SearchError::NotFound(_))));
    }

    #[tokio::test]
    async fn memory_sort_and_pagination() {
        let s = build_memory();
        s.create_index("docs", &json!({})).await.unwrap();
        for i in 1..=10 {
            s.index_doc(
                "docs",
                &i.to_string(),
                &json!({"score": i * 10, "title": "test"}),
            )
            .await
            .unwrap();
        }

        // 按 score 降序，取前 3
        let q = SearchQuery::new("test")
            .with_sort("score", SortOrder::Desc)
            .with_pagination(0, 3);
        let result = s.search("docs", &q).await.unwrap();
        assert_eq!(result.total, 10);
        assert_eq!(result.hits.len(), 3);
        // score=100 → score=90 → score=80
        assert_eq!(result.hits[0].id, "10");
        assert_eq!(result.hits[1].id, "9");
        assert_eq!(result.hits[2].id, "8");

        // 翻页：从第 5 条开始，取 2 条
        let q = SearchQuery::new("test")
            .with_sort("score", SortOrder::Asc)
            .with_pagination(5, 2);
        let result = s.search("docs", &q).await.unwrap();
        assert_eq!(result.hits.len(), 2);
        // score=60 → score=70
        assert_eq!(result.hits[0].id, "6");
        assert_eq!(result.hits[1].id, "7");
    }

    #[tokio::test]
    async fn memory_count_all_and_filtered() {
        let s = build_memory();
        s.create_index("docs", &json!({})).await.unwrap();
        s.index_doc("docs", "1", &json!({"title": "hello", "status": "active"}))
            .await
            .unwrap();
        s.index_doc(
            "docs",
            "2",
            &json!({"title": "hello", "status": "inactive"}),
        )
        .await
        .unwrap();
        s.index_doc("docs", "3", &json!({"title": "world", "status": "active"}))
            .await
            .unwrap();

        let total = s.count("docs", &SearchQuery::match_all()).await.unwrap();
        assert_eq!(total, 3);

        let q = SearchQuery::new("hello").with_filter("status", json!("active"));
        let count = s.count("docs", &q).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn memory_bulk_index() {
        let s = build_memory();
        s.create_index("docs", &json!({})).await.unwrap();
        let docs: Vec<(String, serde_json::Value)> = (1..=5)
            .map(|i| (i.to_string(), json!({"title": format!("doc_{}", i)})))
            .collect();
        s.bulk_index("docs", &docs).await.unwrap();
        let count = s.count("docs", &SearchQuery::match_all()).await.unwrap();
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn memory_refresh_noop() {
        let s = build_memory();
        s.create_index("docs", &json!({})).await.unwrap();
        // refresh 应为 no-op
        s.refresh("docs").await.unwrap();
    }
}

mod stub_operations {
    use super::*;
    use sz_orm_search::{SearchExt, SearchQuery, SortOrder};

    #[tokio::test]
    async fn stub_records_operations() {
        let s = build_stub();
        s.create_index("docs", &json!({"mappings": {}}))
            .await
            .unwrap();
        s.index_doc("docs", "1", &json!({"title": "test"}))
            .await
            .unwrap();
        s.delete_doc("docs", "1").await.unwrap();
        s.delete_index("docs").await.unwrap();

        let ops = s.operations();
        assert_eq!(ops.len(), 4);
        assert!(ops[0].contains("CREATE INDEX"));
        assert!(ops[1].contains("INDEX docs/1"));
        assert!(ops[2].contains("DELETE docs/1"));
        assert!(ops[3].contains("DELETE INDEX docs"));
    }

    #[tokio::test]
    async fn stub_search_returns_dsl() {
        let s = build_stub();
        let q = SearchQuery::new("hello")
            .with_filter("status", json!("active"))
            .with_sort("ts", SortOrder::Desc);
        let result = s.search("docs", &q).await.unwrap();
        assert_eq!(result.total, 0);
        let ops = s.operations();
        assert!(ops[0].contains("SEARCH docs"));
        assert!(ops[0].contains("multi_match"));
    }

    #[tokio::test]
    async fn stub_count_returns_zero() {
        let s = build_stub();
        let count = s.count("docs", &SearchQuery::new("test")).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn stub_refresh_records() {
        let s = build_stub();
        s.refresh("docs").await.unwrap();
        let ops = s.operations();
        assert_eq!(ops.len(), 1);
        assert!(ops[0].contains("REFRESH docs"));
    }

    #[tokio::test]
    async fn stub_clear_operations() {
        let s = build_stub();
        s.index_doc("docs", "1", &json!({})).await.unwrap();
        assert_eq!(s.operations().len(), 1);
        s.clear();
        assert_eq!(s.operations().len(), 0);
    }
}

mod builder_and_wrapper {
    use sz_orm_search::{SearchBuilder, SearchExt, SearchProvider};

    #[tokio::test]
    async fn builder_memory_provider() {
        let wrapper = SearchBuilder::new(SearchProvider::Memory)
            .build()
            .expect("build failed");
        wrapper
            .create_index("test", &serde_json::json!({}))
            .await
            .expect("create_index failed");
        wrapper.delete_index("test").await.unwrap();
    }

    #[tokio::test]
    async fn builder_stub_provider() {
        let wrapper = SearchBuilder::new(SearchProvider::Stub)
            .build()
            .expect("build failed");
        wrapper
            .create_index("test", &serde_json::json!({}))
            .await
            .expect("create_index failed");
    }
}

mod dsl_generation {
    use sz_orm_search::{SearchQuery, SortOrder};

    #[test]
    fn es_dsl_match_all_when_empty_query() {
        let q = SearchQuery::match_all();
        let dsl = q.to_es_dsl();
        assert!(dsl["query"]["match_all"].is_object());
        assert_eq!(dsl["from"], 0);
        assert_eq!(dsl["size"], 10);
    }

    #[test]
    fn es_dsl_multi_match_with_query() {
        let q = SearchQuery::new("hello world");
        let dsl = q.to_es_dsl();
        assert!(dsl["query"]["multi_match"]["query"].is_string());
        assert_eq!(dsl["query"]["multi_match"]["query"], "hello world");
    }

    #[test]
    fn es_dsl_with_filter_and_sort() {
        let q = SearchQuery::new("test")
            .with_filter("status", serde_json::json!("active"))
            .with_filter("year", serde_json::json!(2026))
            .with_sort("timestamp", SortOrder::Desc)
            .with_sort("score", SortOrder::Asc)
            .with_pagination(20, 50);
        let dsl = q.to_es_dsl();
        assert!(dsl["query"]["bool"].is_object());
        assert_eq!(dsl["query"]["bool"]["must"].as_array().unwrap().len(), 1);
        assert_eq!(dsl["query"]["bool"]["filter"].as_array().unwrap().len(), 2);
        assert_eq!(dsl["sort"].as_array().unwrap().len(), 2);
        assert_eq!(dsl["from"], 20);
        assert_eq!(dsl["size"], 50);
    }

    #[test]
    fn meili_params_basic() {
        let q = SearchQuery::new("hello").with_pagination(10, 20);
        let params = q.to_meili_params();
        assert_eq!(params["q"], "hello");
        assert_eq!(params["offset"], 10);
        assert_eq!(params["limit"], 20);
    }

    #[test]
    fn meili_params_with_filter_and_sort() {
        let q = SearchQuery::new("test")
            .with_filter("status", serde_json::json!("active"))
            .with_sort("ts", SortOrder::Desc);
        let params = q.to_meili_params();
        assert!(params["filter"].is_string());
        assert!(params["filter"].as_str().unwrap().contains("status"));
        assert!(params["sort"].is_array());
        assert_eq!(params["sort"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn meili_params_numeric_filter() {
        let q = SearchQuery::new("test").with_filter("year", serde_json::json!(2026));
        let params = q.to_meili_params();
        let filter = params["filter"].as_str().unwrap();
        assert!(filter.contains("year"));
        assert!(!filter.contains("\"2026\""), "numeric should not be quoted");
    }
}

mod error_paths {
    use sz_orm_search::{MemorySearch, SearchError, SearchExt, StubSearch};

    #[tokio::test]
    async fn memory_get_doc_from_missing_index() {
        let s = MemorySearch::new();
        let result = s.get_doc("ghost", "1").await;
        assert!(matches!(result, Err(SearchError::NotFound(_))));
    }

    #[tokio::test]
    async fn stub_get_doc_returns_none() {
        let s = StubSearch::new();
        let result = s.get_doc("ghost", "1").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn memory_index_doc_to_missing_index() {
        let s = MemorySearch::new();
        let result = s.index_doc("ghost", "1", &serde_json::json!({})).await;
        assert!(matches!(result, Err(SearchError::NotFound(_))));
    }
}
