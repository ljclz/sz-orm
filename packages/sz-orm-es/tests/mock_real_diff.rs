//! Mock 与真实 ES 行为差分测试
//!
//! 验证 `InMemoryEsSync`（Mock）的查询语义与真实 ES REST API 一致。
//!
//! - 不带 `#[ignore]` 的测试：验证 Mock 行为符合预期（始终运行）
//! - 带 `#[ignore]` 的测试：同时运行 Mock 和真实 ES，对比结果（需 `--features real -- --ignored`）

use serde_json::json;
use sz_orm_es::*;

fn make_doc(index: &str, id: &str, source: serde_json::Value) -> EsDocument {
    EsDocument::new(index, source).with_id(id.to_string())
}

// ============================================================================
// Mock 行为验证（始终运行，不依赖真实 ES）
// ============================================================================

#[test]
fn test_mock_match_all_semantics() {
    let mock = InMemoryEsSync::new();
    let docs = vec![
        make_doc("test", "1", json!({"name": "Alice", "age": 30})),
        make_doc("test", "2", json!({"name": "Bob", "age": 25})),
    ];
    let result = mock.sync_to_es(docs).unwrap();
    assert_eq!(result.indexed, 2);

    let search = EsSearchRequest::new("test", EsQuery::match_all());
    let search_result = mock.search(search).unwrap();
    assert_eq!(search_result.total, 2);
}

#[test]
fn test_mock_term_query_semantics() {
    let mock = InMemoryEsSync::new();
    let docs = vec![
        make_doc("test", "1", json!({"name": "Alice", "age": 30})),
        make_doc("test", "2", json!({"name": "Bob", "age": 25})),
        make_doc("test", "3", json!({"name": "Alice", "age": 22})),
    ];
    mock.sync_to_es(docs).unwrap();

    let search = EsSearchRequest::new("test", EsQuery::term("name", json!("Alice")));
    let result = mock.search(search).unwrap();
    assert_eq!(result.total, 2);
    for hit in &result.hits {
        assert_eq!(hit.source["name"], json!("Alice"));
    }
}

#[test]
fn test_mock_terms_query_semantics() {
    let mock = InMemoryEsSync::new();
    let docs = vec![
        make_doc("test", "1", json!({"status": "active"})),
        make_doc("test", "2", json!({"status": "pending"})),
        make_doc("test", "3", json!({"status": "inactive"})),
    ];
    mock.sync_to_es(docs).unwrap();

    let search = EsSearchRequest::new(
        "test",
        EsQuery::terms("status", vec![json!("active"), json!("pending")]),
    );
    let result = mock.search(search).unwrap();
    assert_eq!(result.total, 2);
}

#[test]
fn test_mock_range_query_semantics() {
    let mock = InMemoryEsSync::new();
    let docs: Vec<EsDocument> = (1..=5)
        .map(|i| make_doc("test", &i.to_string(), json!({"age": 20 + i})))
        .collect();
    mock.sync_to_es(docs).unwrap();

    let range = EsRangeQuery::new().gte(json!(22)).lte(json!(24));
    let search = EsSearchRequest::new("test", EsQuery::range("age", range));
    let result = mock.search(search).unwrap();
    assert_eq!(result.total, 3);
}

#[test]
fn test_mock_bool_must_query_semantics() {
    let mock = InMemoryEsSync::new();
    let docs = vec![
        make_doc(
            "test",
            "1",
            json!({"name": "Alice", "age": 30, "active": true}),
        ),
        make_doc(
            "test",
            "2",
            json!({"name": "Bob", "age": 25, "active": false}),
        ),
        make_doc(
            "test",
            "3",
            json!({"name": "Alice", "age": 22, "active": true}),
        ),
    ];
    mock.sync_to_es(docs).unwrap();

    let query = EsQuery::must(vec![
        EsQuery::term("name", json!("Alice")),
        EsQuery::term("active", json!(true)),
    ]);
    let search = EsSearchRequest::new("test", query);
    let result = mock.search(search).unwrap();
    assert_eq!(result.total, 2);
}

#[test]
fn test_mock_bool_should_query_semantics() {
    let mock = InMemoryEsSync::new();
    let docs = vec![
        make_doc("test", "1", json!({"name": "Alice", "age": 30})),
        make_doc("test", "2", json!({"name": "Bob", "age": 25})),
        make_doc("test", "3", json!({"name": "Charlie", "age": 35})),
    ];
    mock.sync_to_es(docs).unwrap();

    let query = EsQuery::should(vec![
        EsQuery::term("name", json!("Alice")),
        EsQuery::term("name", json!("Charlie")),
    ]);
    let search = EsSearchRequest::new("test", query);
    let result = mock.search(search).unwrap();
    assert_eq!(result.total, 2);
}

#[test]
fn test_mock_delete_semantics() {
    let mock = InMemoryEsSync::new();
    let docs = vec![
        make_doc("test", "1", json!({"name": "Alice"})),
        make_doc("test", "2", json!({"name": "Bob"})),
    ];
    mock.sync_to_es(docs).unwrap();
    assert_eq!(mock.count("test").unwrap(), 2);

    let result = mock.delete_from_es("test", vec!["1".to_string()]).unwrap();
    assert_eq!(result.indexed, 1);
    assert_eq!(mock.count("test").unwrap(), 1);
}

#[test]
fn test_mock_pagination_semantics() {
    let mock = InMemoryEsSync::new();
    let docs: Vec<EsDocument> = (1..=10)
        .map(|i| make_doc("test", &i.to_string(), json!({"id": i})))
        .collect();
    mock.sync_to_es(docs).unwrap();

    let search = EsSearchRequest::new("test", EsQuery::match_all()).with_pagination(2, 3);
    let result = mock.search(search).unwrap();
    assert_eq!(result.total, 10);
    assert_eq!(result.hits.len(), 3);
}

#[test]
fn test_mock_empty_index_search() {
    let mock = InMemoryEsSync::new();
    let search = EsSearchRequest::new("nonexistent", EsQuery::match_all());
    let result = mock.search(search);
    assert!(result.is_err());
}

// ============================================================================
// Mock vs 真实 ES 差分对比（需真实 ES，标注 #[ignore]）
// ============================================================================

#[cfg(feature = "real")]
mod real_diff {
    use super::*;
    use serde_json::json;

    const ES_URL: &str = "http://localhost:9200";

    async fn es_available() -> bool {
        reqwest::Client::new()
            .get(format!("{}/_cluster/health", ES_URL))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn cleanup(index: &str) {
        let _ = reqwest::Client::new()
            .delete(format!("{}/{}", ES_URL, index))
            .send()
            .await;
    }

    #[tokio::test]
    #[ignore]
    async fn diff_match_all() {
        if !es_available().await {
            eprintln!("ES not available");
            return;
        }
        let index = "sz_orm_diff_match_all";
        cleanup(index).await;

        let mock = InMemoryEsSync::new();
        let docs = vec![
            make_doc(index, "1", json!({"name": "Alice", "age": 30})),
            make_doc(index, "2", json!({"name": "Bob", "age": 25})),
        ];
        mock.sync_to_es(docs.clone()).unwrap();

        let client = reqwest::Client::new();
        let _ = client
            .put(format!("{}/{}", ES_URL, index))
            .json(&json!({"mappings": {"properties": {"name": {"type": "keyword"}, "age": {"type": "integer"}}}}))
            .send()
            .await
            .unwrap();
        for doc in &docs {
            let id = doc.id.as_ref().unwrap();
            let _ = client
                .post(format!("{}/{}/_doc/{}", ES_URL, index, id))
                .json(&doc.source)
                .send()
                .await
                .unwrap();
        }
        let _ = client
            .post(format!("{}/{}/_refresh", ES_URL, index))
            .send()
            .await
            .unwrap();

        let mock_result = mock
            .search(EsSearchRequest::new(index, EsQuery::match_all()))
            .unwrap();
        let real_resp: serde_json::Value = client
            .post(format!("{}/{}/_search", ES_URL, index))
            .json(&json!({"query": {"match_all": {}}}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let real_total = real_resp["hits"]["total"]["value"].as_i64().unwrap();

        assert_eq!(mock_result.total as i64, real_total);

        cleanup(index).await;
    }

    #[tokio::test]
    #[ignore]
    async fn diff_term_query() {
        if !es_available().await {
            eprintln!("ES not available");
            return;
        }
        let index = "sz_orm_diff_term";
        cleanup(index).await;

        let mock = InMemoryEsSync::new();
        let docs = vec![
            make_doc(index, "1", json!({"name": "Alice", "age": 30})),
            make_doc(index, "2", json!({"name": "Bob", "age": 25})),
            make_doc(index, "3", json!({"name": "Alice", "age": 22})),
        ];
        mock.sync_to_es(docs.clone()).unwrap();

        let client = reqwest::Client::new();
        let _ = client
            .put(format!("{}/{}", ES_URL, index))
            .json(&json!({"mappings": {"properties": {"name": {"type": "keyword"}, "age": {"type": "integer"}}}}))
            .send()
            .await
            .unwrap();
        for doc in &docs {
            let id = doc.id.as_ref().unwrap();
            let _ = client
                .post(format!("{}/{}/_doc/{}", ES_URL, index, id))
                .json(&doc.source)
                .send()
                .await
                .unwrap();
        }
        let _ = client
            .post(format!("{}/{}/_refresh", ES_URL, index))
            .send()
            .await
            .unwrap();

        let mock_result = mock
            .search(EsSearchRequest::new(
                index,
                EsQuery::term("name", json!("Alice")),
            ))
            .unwrap();
        let real_resp: serde_json::Value = client
            .post(format!("{}/{}/_search", ES_URL, index))
            .json(&json!({"query": {"term": {"name": "Alice"}}}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let real_total = real_resp["hits"]["total"]["value"].as_i64().unwrap();

        assert_eq!(mock_result.total as i64, real_total);

        cleanup(index).await;
    }

    #[tokio::test]
    #[ignore]
    async fn diff_range_query() {
        if !es_available().await {
            eprintln!("ES not available");
            return;
        }
        let index = "sz_orm_diff_range";
        cleanup(index).await;

        let mock = InMemoryEsSync::new();
        let docs: Vec<EsDocument> = (1..=5)
            .map(|i| make_doc(index, &i.to_string(), json!({"age": 20 + i})))
            .collect();
        mock.sync_to_es(docs.clone()).unwrap();

        let client = reqwest::Client::new();
        let _ = client
            .put(format!("{}/{}", ES_URL, index))
            .json(&json!({"mappings": {"properties": {"age": {"type": "integer"}}}}))
            .send()
            .await
            .unwrap();
        for doc in &docs {
            let id = doc.id.as_ref().unwrap();
            let _ = client
                .post(format!("{}/{}/_doc/{}", ES_URL, index, id))
                .json(&doc.source)
                .send()
                .await
                .unwrap();
        }
        let _ = client
            .post(format!("{}/{}/_refresh", ES_URL, index))
            .send()
            .await
            .unwrap();

        let range = EsRangeQuery::new().gte(json!(22)).lte(json!(24));
        let mock_result = mock
            .search(EsSearchRequest::new(index, EsQuery::range("age", range)))
            .unwrap();
        let real_resp: serde_json::Value = client
            .post(format!("{}/{}/_search", ES_URL, index))
            .json(&json!({"query": {"range": {"age": {"gte": 22, "lte": 24}}}}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let real_total = real_resp["hits"]["total"]["value"].as_i64().unwrap();

        assert_eq!(mock_result.total as i64, real_total);

        cleanup(index).await;
    }

    #[tokio::test]
    #[ignore]
    async fn diff_delete() {
        if !es_available().await {
            eprintln!("ES not available");
            return;
        }
        let index = "sz_orm_diff_delete";
        cleanup(index).await;

        let mock = InMemoryEsSync::new();
        let docs = vec![
            make_doc(index, "1", json!({"name": "Alice"})),
            make_doc(index, "2", json!({"name": "Bob"})),
        ];
        mock.sync_to_es(docs.clone()).unwrap();
        mock.delete_from_es(index, vec!["1".to_string()]).unwrap();

        let client = reqwest::Client::new();
        let _ = client
            .put(format!("{}/{}", ES_URL, index))
            .json(&json!({"mappings": {"properties": {"name": {"type": "keyword"}}}}))
            .send()
            .await
            .unwrap();
        for doc in &docs {
            let id = doc.id.as_ref().unwrap();
            let _ = client
                .post(format!("{}/{}/_doc/{}", ES_URL, index, id))
                .json(&doc.source)
                .send()
                .await
                .unwrap();
        }
        let _ = client
            .post(format!("{}/{}/_refresh", ES_URL, index))
            .send()
            .await
            .unwrap();
        let _ = client
            .delete(format!("{}/{}/_doc/1", ES_URL, index))
            .send()
            .await
            .unwrap();
        let _ = client
            .post(format!("{}/{}/_refresh", ES_URL, index))
            .send()
            .await
            .unwrap();

        let mock_count = mock.count(index).unwrap();
        let real_resp: serde_json::Value = client
            .post(format!("{}/{}/_search", ES_URL, index))
            .json(&json!({"query": {"match_all": {}}, "size": 0}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let real_count = real_resp["hits"]["total"]["value"].as_i64().unwrap();

        assert_eq!(mock_count as i64, real_count);

        cleanup(index).await;
    }
}
