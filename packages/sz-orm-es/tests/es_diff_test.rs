//! sz-orm-es Mock 与真实 ES 差分测试（M3-T4.1）
//!
//! 验证 `InMemoryEsSync`（Mock）与 `RealEsSync`（真实 ES）在相同输入下输出语义一致。
//!
//! - 不带 `#[ignore]` 的测试：验证 Mock 行为符合预期语义（始终运行）
//! - 带 `#[ignore]` 的测试：同时运行 Mock 和真实 ES，对比结果（需 `--features real -- --ignored`）
//!
//! 运行方式：`cargo test -p sz-orm-es --features real --test es_diff_test -- --ignored`
//!
//! 前置条件：本地 ES 服务运行于 `http://localhost:9200`

use serde_json::json;
use sz_orm_es::*;

fn make_doc(index: &str, id: &str, source: serde_json::Value) -> EsDocument {
    EsDocument::new(index, source).with_id(id.to_string())
}

// ============================================================================
// Mock 行为验证（始终运行，不依赖真实 ES）
// ============================================================================

#[test]
fn diff_mock_match_all_total() {
    let mock = InMemoryEsSync::new();
    let docs = vec![
        make_doc("diff_test", "1", json!({"name": "Alice", "age": 30})),
        make_doc("diff_test", "2", json!({"name": "Bob", "age": 25})),
        make_doc("diff_test", "3", json!({"name": "Charlie", "age": 35})),
    ];
    mock.sync_to_es(docs).unwrap();

    let req = EsSearchRequest::new("diff_test", EsQuery::match_all());
    let result = mock.search(req).unwrap();
    assert_eq!(result.total, 3);
    assert_eq!(result.hits.len(), 3);
}

#[test]
fn diff_mock_term_query_filter() {
    let mock = InMemoryEsSync::new();
    let docs = vec![
        make_doc("diff_test", "1", json!({"status": "active", "name": "A"})),
        make_doc("diff_test", "2", json!({"status": "inactive", "name": "B"})),
        make_doc("diff_test", "3", json!({"status": "active", "name": "C"})),
    ];
    mock.sync_to_es(docs).unwrap();

    let req = EsSearchRequest::new("diff_test", EsQuery::term("status", json!("active")));
    let result = mock.search(req).unwrap();
    assert_eq!(result.total, 2);
    for hit in &result.hits {
        assert_eq!(hit.source["status"], json!("active"));
    }
}

#[test]
fn diff_mock_range_query_filter() {
    let mock = InMemoryEsSync::new();
    let docs: Vec<EsDocument> = (1..=5)
        .map(|i| make_doc("diff_test", &i.to_string(), json!({"age": 20 + i})))
        .collect();
    mock.sync_to_es(docs).unwrap();

    let range = EsRangeQuery::new().gte(json!(22)).lte(json!(24));
    let req = EsSearchRequest::new("diff_test", EsQuery::range("age", range));
    let result = mock.search(req).unwrap();
    assert_eq!(result.total, 3);
}

#[test]
fn diff_mock_bool_must_query() {
    let mock = InMemoryEsSync::new();
    let docs = vec![
        make_doc(
            "diff_test",
            "1",
            json!({"name": "Alice", "age": 30, "active": true}),
        ),
        make_doc(
            "diff_test",
            "2",
            json!({"name": "Bob", "age": 25, "active": false}),
        ),
        make_doc(
            "diff_test",
            "3",
            json!({"name": "Alice", "age": 22, "active": true}),
        ),
    ];
    mock.sync_to_es(docs).unwrap();

    let query = EsQuery::must(vec![
        EsQuery::term("name", json!("Alice")),
        EsQuery::term("active", json!(true)),
    ]);
    let req = EsSearchRequest::new("diff_test", query);
    let result = mock.search(req).unwrap();
    assert_eq!(result.total, 2);
}

#[test]
fn diff_mock_delete_semantics() {
    let mock = InMemoryEsSync::new();
    let docs = vec![
        make_doc("diff_test", "1", json!({"name": "Alice"})),
        make_doc("diff_test", "2", json!({"name": "Bob"})),
    ];
    mock.sync_to_es(docs).unwrap();
    assert_eq!(mock.count("diff_test").unwrap(), 2);

    let result = mock
        .delete_from_es("diff_test", vec!["1".to_string()])
        .unwrap();
    assert_eq!(result.indexed, 1);
    assert_eq!(mock.count("diff_test").unwrap(), 1);
}

#[test]
fn diff_mock_pagination_consistency() {
    let mock = InMemoryEsSync::new();
    let docs: Vec<EsDocument> = (1..=10)
        .map(|i| make_doc("diff_test", &i.to_string(), json!({"id": i})))
        .collect();
    mock.sync_to_es(docs).unwrap();

    let req = EsSearchRequest::new("diff_test", EsQuery::match_all()).with_pagination(2, 3);
    let result = mock.search(req).unwrap();
    assert_eq!(result.total, 10);
    assert_eq!(result.hits.len(), 3);
}

// ============================================================================
// Mock vs 真实 ES 差分对比（需真实 ES，标注 #[ignore]）
// ============================================================================

#[cfg(feature = "real")]
mod real_diff {
    use super::*;
    use std::collections::HashMap;
    use sz_orm_es::real_es::RealEsSync;

    const ES_URL: &str = "http://localhost:9200";
    const TEST_INDEX: &str = "sz_orm_es_diff_test";

    async fn es_available(es: &RealEsSync) -> bool {
        use sz_orm_es::EsSync;
        let req = EsSearchRequest::new("sz_orm_health_check_nonexistent", EsQuery::match_all());
        match es.search(req) {
            Ok(_) => true,
            Err(EsError::IndexNotFound(_)) => true,
            Err(EsError::ConnectionFailed(_)) => false,
            Err(_) => true,
        }
    }

    async fn cleanup_index(es: &RealEsSync) {
        let _ = es.delete_index(TEST_INDEX).await;
    }

    async fn setup_index(es: &RealEsSync, mapping: &HashMap<String, EsFieldType>) {
        cleanup_index(es).await;
        es.create_index(TEST_INDEX, mapping).await.unwrap();
    }

    async fn index_docs(es: &RealEsSync, docs: &[EsDocument]) {
        for doc in docs {
            es.index_document(doc).await.unwrap();
        }
        es.refresh(TEST_INDEX).await.unwrap();
    }

    /// 差分测试：match_all 查询 total 一致
    #[tokio::test]
    #[ignore]
    async fn diff_real_match_all_total() {
        let es = RealEsSync::new(ES_URL).unwrap();
        if !es_available(&es).await {
            eprintln!("ES not available, skipping");
            return;
        }

        let mock = InMemoryEsSync::new();
        let docs = vec![
            make_doc(TEST_INDEX, "1", json!({"name": "Alice", "age": 30})),
            make_doc(TEST_INDEX, "2", json!({"name": "Bob", "age": 25})),
            make_doc(TEST_INDEX, "3", json!({"name": "Charlie", "age": 35})),
        ];
        mock.sync_to_es(docs.clone()).unwrap();

        let mut mapping = HashMap::new();
        mapping.insert("name".to_string(), EsFieldType::Keyword);
        mapping.insert("age".to_string(), EsFieldType::Integer);
        setup_index(&es, &mapping).await;
        index_docs(&es, &docs).await;

        let mock_req = EsSearchRequest::new(TEST_INDEX, EsQuery::match_all());
        let mock_result = mock.search(mock_req).unwrap();

        let real_req = EsSearchRequest::new(TEST_INDEX, EsQuery::match_all());
        let real_result = es.search(real_req).unwrap();

        assert_eq!(mock_result.total, real_result.total);
        assert_eq!(mock_result.total, 3);

        cleanup_index(&es).await;
    }

    /// 差分测试：term 查询 total 一致
    #[tokio::test]
    #[ignore]
    async fn diff_real_term_query_total() {
        let es = RealEsSync::new(ES_URL).unwrap();
        if !es_available(&es).await {
            eprintln!("ES not available, skipping");
            return;
        }

        let mock = InMemoryEsSync::new();
        let docs = vec![
            make_doc(TEST_INDEX, "1", json!({"status": "active", "name": "A"})),
            make_doc(TEST_INDEX, "2", json!({"status": "inactive", "name": "B"})),
            make_doc(TEST_INDEX, "3", json!({"status": "active", "name": "C"})),
        ];
        mock.sync_to_es(docs.clone()).unwrap();

        let mut mapping = HashMap::new();
        mapping.insert("status".to_string(), EsFieldType::Keyword);
        mapping.insert("name".to_string(), EsFieldType::Keyword);
        setup_index(&es, &mapping).await;
        index_docs(&es, &docs).await;

        let mock_req = EsSearchRequest::new(TEST_INDEX, EsQuery::term("status", json!("active")));
        let mock_result = mock.search(mock_req).unwrap();

        let real_req = EsSearchRequest::new(TEST_INDEX, EsQuery::term("status", json!("active")));
        let real_result = es.search(real_req).unwrap();

        assert_eq!(mock_result.total, real_result.total);
        assert_eq!(mock_result.total, 2);

        cleanup_index(&es).await;
    }

    /// 差分测试：range 查询 total 一致
    #[tokio::test]
    #[ignore]
    async fn diff_real_range_query_total() {
        let es = RealEsSync::new(ES_URL).unwrap();
        if !es_available(&es).await {
            eprintln!("ES not available, skipping");
            return;
        }

        let mock = InMemoryEsSync::new();
        let docs: Vec<EsDocument> = (1..=5)
            .map(|i| make_doc(TEST_INDEX, &i.to_string(), json!({"age": 20 + i})))
            .collect();
        mock.sync_to_es(docs.clone()).unwrap();

        let mut mapping = HashMap::new();
        mapping.insert("age".to_string(), EsFieldType::Integer);
        setup_index(&es, &mapping).await;
        index_docs(&es, &docs).await;

        let range = EsRangeQuery::new().gte(json!(22)).lte(json!(24));
        let mock_req = EsSearchRequest::new(TEST_INDEX, EsQuery::range("age", range.clone()));
        let mock_result = mock.search(mock_req).unwrap();

        let real_req = EsSearchRequest::new(TEST_INDEX, EsQuery::range("age", range));
        let real_result = es.search(real_req).unwrap();

        assert_eq!(mock_result.total, real_result.total);
        assert_eq!(mock_result.total, 3);

        cleanup_index(&es).await;
    }

    /// 差分测试：bool must 查询 total 一致
    #[tokio::test]
    #[ignore]
    async fn diff_real_bool_must_query_total() {
        let es = RealEsSync::new(ES_URL).unwrap();
        if !es_available(&es).await {
            eprintln!("ES not available, skipping");
            return;
        }

        let mock = InMemoryEsSync::new();
        let docs = vec![
            make_doc(
                TEST_INDEX,
                "1",
                json!({"name": "Alice", "age": 30, "active": true}),
            ),
            make_doc(
                TEST_INDEX,
                "2",
                json!({"name": "Bob", "age": 25, "active": false}),
            ),
            make_doc(
                TEST_INDEX,
                "3",
                json!({"name": "Alice", "age": 22, "active": true}),
            ),
        ];
        mock.sync_to_es(docs.clone()).unwrap();

        let mut mapping = HashMap::new();
        mapping.insert("name".to_string(), EsFieldType::Keyword);
        mapping.insert("age".to_string(), EsFieldType::Integer);
        mapping.insert("active".to_string(), EsFieldType::Boolean);
        setup_index(&es, &mapping).await;
        index_docs(&es, &docs).await;

        let query = EsQuery::must(vec![
            EsQuery::term("name", json!("Alice")),
            EsQuery::term("active", json!(true)),
        ]);
        let mock_req = EsSearchRequest::new(TEST_INDEX, query.clone());
        let mock_result = mock.search(mock_req).unwrap();

        let real_req = EsSearchRequest::new(TEST_INDEX, query);
        let real_result = es.search(real_req).unwrap();

        assert_eq!(mock_result.total, real_result.total);
        assert_eq!(mock_result.total, 2);

        cleanup_index(&es).await;
    }

    /// 差分测试：delete 后 count 一致
    #[tokio::test]
    #[ignore]
    async fn diff_real_delete_count() {
        let es = RealEsSync::new(ES_URL).unwrap();
        if !es_available(&es).await {
            eprintln!("ES not available, skipping");
            return;
        }

        let mock = InMemoryEsSync::new();
        let docs = vec![
            make_doc(TEST_INDEX, "1", json!({"name": "Alice"})),
            make_doc(TEST_INDEX, "2", json!({"name": "Bob"})),
        ];
        mock.sync_to_es(docs.clone()).unwrap();
        mock.delete_from_es(TEST_INDEX, vec!["1".to_string()])
            .unwrap();

        let mut mapping = HashMap::new();
        mapping.insert("name".to_string(), EsFieldType::Keyword);
        setup_index(&es, &mapping).await;
        index_docs(&es, &docs).await;
        es.delete_from_es(TEST_INDEX, vec!["1".to_string()])
            .unwrap();
        es.refresh(TEST_INDEX).await.unwrap();

        let mock_count = mock.count(TEST_INDEX).unwrap();
        let real_req = EsSearchRequest::new(TEST_INDEX, EsQuery::match_all()).with_pagination(0, 0);
        let real_result = es.search(real_req).unwrap();

        assert_eq!(mock_count, real_result.total);
        assert_eq!(mock_count, 1);

        cleanup_index(&es).await;
    }

    /// 差分测试：sync_to_es（bulk index）indexed 数量一致
    #[tokio::test]
    #[ignore]
    async fn diff_real_sync_to_es_indexed_count() {
        let es = RealEsSync::new(ES_URL).unwrap();
        if !es_available(&es).await {
            eprintln!("ES not available, skipping");
            return;
        }

        let mock = InMemoryEsSync::new();
        let docs = vec![
            make_doc(TEST_INDEX, "1", json!({"name": "A"})),
            make_doc(TEST_INDEX, "2", json!({"name": "B"})),
            make_doc(TEST_INDEX, "3", json!({"name": "C"})),
        ];
        let mock_result = mock.sync_to_es(docs.clone()).unwrap();

        let mut mapping = HashMap::new();
        mapping.insert("name".to_string(), EsFieldType::Keyword);
        setup_index(&es, &mapping).await;
        let real_result = es.sync_to_es(docs).unwrap();
        es.refresh(TEST_INDEX).await.unwrap();

        assert_eq!(mock_result.indexed, real_result.indexed);
        assert_eq!(mock_result.indexed, 3);

        cleanup_index(&es).await;
    }

    /// 差分测试：聚合查询 terms 桶一致
    #[tokio::test]
    #[ignore]
    async fn diff_real_aggregate_terms() {
        let es = RealEsSync::new(ES_URL).unwrap();
        if !es_available(&es).await {
            eprintln!("ES not available, skipping");
            return;
        }

        let mock = InMemoryEsSync::new();
        let docs = vec![
            make_doc(TEST_INDEX, "1", json!({"category": "tech", "price": 100})),
            make_doc(TEST_INDEX, "2", json!({"category": "tech", "price": 200})),
            make_doc(TEST_INDEX, "3", json!({"category": "food", "price": 50})),
        ];
        mock.sync_to_es(docs.clone()).unwrap();

        let mut mapping = HashMap::new();
        mapping.insert("category".to_string(), EsFieldType::Keyword);
        mapping.insert("price".to_string(), EsFieldType::Integer);
        setup_index(&es, &mapping).await;
        index_docs(&es, &docs).await;

        let mock_agg = sz_orm_es::extensions::MemoryAggregator::new(mock.clone());
        let aggs = vec![sz_orm_es::extensions::Aggregation::terms(
            "by_cat", "category",
        )];
        let mock_result = mock_agg
            .aggregate(TEST_INDEX, EsQuery::match_all(), &aggs)
            .unwrap();
        let mock_tech_count = mock_result[0]
            .buckets
            .iter()
            .find(|b| b.key == "tech")
            .map(|b| b.doc_count)
            .unwrap_or(0);

        let aggs_dsl = json!({
            "by_cat": { "terms": { "field": "category" } }
        });
        let real_agg = es
            .aggregate(TEST_INDEX, &EsQuery::match_all(), &aggs_dsl)
            .await
            .unwrap();
        let real_tech_count = real_agg["by_cat"]["buckets"]
            .as_array()
            .and_then(|arr| arr.iter().find(|b| b["key"] == "tech"))
            .and_then(|b| b["doc_count"].as_u64())
            .unwrap_or(0);

        assert_eq!(mock_tech_count, real_tech_count);
        assert_eq!(mock_tech_count, 2);

        cleanup_index(&es).await;
    }

    /// 差分测试：filter 查询 total 一致
    #[tokio::test]
    #[ignore]
    async fn diff_real_filter_total() {
        let es = RealEsSync::new(ES_URL).unwrap();
        if !es_available(&es).await {
            eprintln!("ES not available, skipping");
            return;
        }

        let mock = InMemoryEsSync::new();
        let docs = vec![
            make_doc(TEST_INDEX, "1", json!({"status": "active", "age": 30})),
            make_doc(TEST_INDEX, "2", json!({"status": "inactive", "age": 25})),
            make_doc(TEST_INDEX, "3", json!({"status": "active", "age": 20})),
        ];
        mock.sync_to_es(docs.clone()).unwrap();

        let mut mapping = HashMap::new();
        mapping.insert("status".to_string(), EsFieldType::Keyword);
        mapping.insert("age".to_string(), EsFieldType::Integer);
        setup_index(&es, &mapping).await;
        index_docs(&es, &docs).await;

        let mock_query = EsQuery::must(vec![
            EsQuery::term("status", json!("active")),
            EsQuery::range("age", EsRangeQuery::new().gte(json!(25))),
        ]);
        let mock_req = EsSearchRequest::new(TEST_INDEX, mock_query);
        let mock_result = mock.search(mock_req).unwrap();

        let real_result = es
            .filter(
                TEST_INDEX,
                &[
                    EsQuery::term("status", json!("active")),
                    EsQuery::range("age", EsRangeQuery::new().gte(json!(25))),
                ],
                0,
                10,
            )
            .await
            .unwrap();

        assert_eq!(mock_result.total, real_result.total);
        assert_eq!(mock_result.total, 1);

        cleanup_index(&es).await;
    }
}
