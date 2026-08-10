//! 真实 Elasticsearch 集成测试
//!
//! 这些测试仅在 `--features real` 时编译，且标注 `#[ignore]` 默认跳过。
//! 运行方式：`cargo test -p sz-orm-es --features real --test real_es_integration -- --ignored`
//!
//! 前置条件：本地 ES 服务运行于 `http://localhost:9200`

#![cfg(feature = "real")]

use serde_json::json;

const ES_URL: &str = "http://localhost:9200";

async fn es_health_check() -> bool {
    let client = reqwest::Client::new();
    client
        .get(format!("{}/_cluster/health", ES_URL))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

async fn cleanup_index(index: &str) {
    let client = reqwest::Client::new();
    let _ = client.delete(format!("{}/{}", ES_URL, index)).send().await;
}

#[tokio::test]
#[ignore]
async fn test_real_es_index_creation() {
    if !es_health_check().await {
        eprintln!("ES not available, skipping");
        return;
    }
    let index = "sz_orm_test_index_create";
    cleanup_index(index).await;

    let client = reqwest::Client::new();
    let mappings = json!({
        "mappings": {
            "properties": {
                "name": { "type": "text" },
                "age": { "type": "integer" }
            }
        }
    });
    let resp = client
        .put(format!("{}/{}", ES_URL, index))
        .json(&mappings)
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());

    let exists_resp = client
        .head(format!("{}/{}", ES_URL, index))
        .send()
        .await
        .unwrap();
    assert!(exists_resp.status().is_success());

    cleanup_index(index).await;
}

#[tokio::test]
#[ignore]
async fn test_real_es_document_indexing() {
    if !es_health_check().await {
        eprintln!("ES not available, skipping");
        return;
    }
    let index = "sz_orm_test_doc_index";
    cleanup_index(index).await;

    let client = reqwest::Client::new();
    let _ = client
        .put(format!("{}/{}", ES_URL, index))
        .json(&json!({"mappings": {"properties": {"name": {"type": "text"}, "age": {"type": "integer"}}}}))
        .send()
        .await
        .unwrap();

    let doc = json!({"name": "Alice", "age": 30});
    let resp = client
        .post(format!("{}/{}/_doc/1", ES_URL, index))
        .json(&doc)
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());

    let refresh_resp = client
        .post(format!("{}/{}/_refresh", ES_URL, index))
        .send()
        .await
        .unwrap();
    assert!(refresh_resp.status().is_success());

    let get_resp = client
        .get(format!("{}/{}/_doc/1", ES_URL, index))
        .send()
        .await
        .unwrap();
    assert!(get_resp.status().is_success());
    let get_json: serde_json::Value = get_resp.json().await.unwrap();
    assert_eq!(get_json["_source"]["name"], json!("Alice"));
    assert_eq!(get_json["_source"]["age"], json!(30));

    cleanup_index(index).await;
}

#[tokio::test]
#[ignore]
async fn test_real_es_search_match_all() {
    if !es_health_check().await {
        eprintln!("ES not available, skipping");
        return;
    }
    let index = "sz_orm_test_search";
    cleanup_index(index).await;

    let client = reqwest::Client::new();
    let _ = client
        .put(format!("{}/{}", ES_URL, index))
        .json(&json!({"mappings": {"properties": {"name": {"type": "text"}, "age": {"type": "integer"}}}}))
        .send()
        .await
        .unwrap();

    for i in 1..=3 {
        let _ = client
            .post(format!("{}/{}/_doc/{}", ES_URL, index, i))
            .json(&json!({"name": format!("User{}", i), "age": 20 + i}))
            .send()
            .await
            .unwrap();
    }
    let _ = client
        .post(format!("{}/{}/_refresh", ES_URL, index))
        .send()
        .await
        .unwrap();

    let search_body = json!({"query": {"match_all": {}}, "size": 10});
    let resp = client
        .post(format!("{}/{}/_search", ES_URL, index))
        .json(&search_body)
        .send()
        .await
        .unwrap();
    let result: serde_json::Value = resp.json().await.unwrap();
    let total = result["hits"]["total"]["value"].as_i64().unwrap();
    assert_eq!(total, 3);

    cleanup_index(index).await;
}

#[tokio::test]
#[ignore]
async fn test_real_es_search_term_filter() {
    if !es_health_check().await {
        eprintln!("ES not available, skipping");
        return;
    }
    let index = "sz_orm_test_term";
    cleanup_index(index).await;

    let client = reqwest::Client::new();
    let _ = client
        .put(format!("{}/{}", ES_URL, index))
        .json(&json!({"mappings": {"properties": {"name": {"type": "keyword"}, "age": {"type": "integer"}}}}))
        .send()
        .await
        .unwrap();

    let _ = client
        .post(format!("{}/{}/_doc/1", ES_URL, index))
        .json(&json!({"name": "Alice", "age": 30}))
        .send()
        .await
        .unwrap();
    let _ = client
        .post(format!("{}/{}/_doc/2", ES_URL, index))
        .json(&json!({"name": "Bob", "age": 25}))
        .send()
        .await
        .unwrap();
    let _ = client
        .post(format!("{}/{}/_refresh", ES_URL, index))
        .send()
        .await
        .unwrap();

    let search_body = json!({"query": {"term": {"name": "Alice"}}});
    let resp = client
        .post(format!("{}/{}/_search", ES_URL, index))
        .json(&search_body)
        .send()
        .await
        .unwrap();
    let result: serde_json::Value = resp.json().await.unwrap();
    let total = result["hits"]["total"]["value"].as_i64().unwrap();
    assert_eq!(total, 1);

    cleanup_index(index).await;
}

#[tokio::test]
#[ignore]
async fn test_real_es_range_filter() {
    if !es_health_check().await {
        eprintln!("ES not available, skipping");
        return;
    }
    let index = "sz_orm_test_range";
    cleanup_index(index).await;

    let client = reqwest::Client::new();
    let _ = client
        .put(format!("{}/{}", ES_URL, index))
        .json(&json!({"mappings": {"properties": {"age": {"type": "integer"}}}}))
        .send()
        .await
        .unwrap();

    for i in 1..=5 {
        let _ = client
            .post(format!("{}/{}/_doc/{}", ES_URL, index, i))
            .json(&json!({"age": 20 + i}))
            .send()
            .await
            .unwrap();
    }
    let _ = client
        .post(format!("{}/{}/_refresh", ES_URL, index))
        .send()
        .await
        .unwrap();

    let search_body = json!({"query": {"range": {"age": {"gte": 22, "lte": 24}}}});
    let resp = client
        .post(format!("{}/{}/_search", ES_URL, index))
        .json(&search_body)
        .send()
        .await
        .unwrap();
    let result: serde_json::Value = resp.json().await.unwrap();
    let total = result["hits"]["total"]["value"].as_i64().unwrap();
    assert_eq!(total, 3);

    cleanup_index(index).await;
}

#[tokio::test]
#[ignore]
async fn test_real_es_aggregation() {
    if !es_health_check().await {
        eprintln!("ES not available, skipping");
        return;
    }
    let index = "sz_orm_test_agg";
    cleanup_index(index).await;

    let client = reqwest::Client::new();
    let _ = client
        .put(format!("{}/{}", ES_URL, index))
        .json(&json!({"mappings": {"properties": {"category": {"type": "keyword"}, "price": {"type": "double"}}}}))
        .send()
        .await
        .unwrap();

    let docs = [
        json!({"category": "A", "price": 10.0}),
        json!({"category": "A", "price": 20.0}),
        json!({"category": "B", "price": 15.0}),
    ];
    for (i, doc) in docs.iter().enumerate() {
        let _ = client
            .post(format!("{}/{}/_doc/{}", ES_URL, index, i + 1))
            .json(doc)
            .send()
            .await
            .unwrap();
    }
    let _ = client
        .post(format!("{}/{}/_refresh", ES_URL, index))
        .send()
        .await
        .unwrap();

    let agg_body = json!({
        "size": 0,
        "aggs": {
            "by_category": {
                "terms": {"field": "category"},
                "aggs": {
                    "avg_price": {"avg": {"field": "price"}}
                }
            }
        }
    });
    let resp = client
        .post(format!("{}/{}/_search", ES_URL, index))
        .json(&agg_body)
        .send()
        .await
        .unwrap();
    let result: serde_json::Value = resp.json().await.unwrap();
    let buckets = result["aggregations"]["by_category"]["buckets"]
        .as_array()
        .unwrap();
    assert_eq!(buckets.len(), 2);

    cleanup_index(index).await;
}

#[tokio::test]
#[ignore]
async fn test_real_es_bool_query() {
    if !es_health_check().await {
        eprintln!("ES not available, skipping");
        return;
    }
    let index = "sz_orm_test_bool";
    cleanup_index(index).await;

    let client = reqwest::Client::new();
    let _ = client
        .put(format!("{}/{}", ES_URL, index))
        .json(&json!({"mappings": {"properties": {"name": {"type": "keyword"}, "age": {"type": "integer"}, "active": {"type": "boolean"}}}}))
        .send()
        .await
        .unwrap();

    let _ = client
        .post(format!("{}/{}/_doc/1", ES_URL, index))
        .json(&json!({"name": "Alice", "age": 30, "active": true}))
        .send()
        .await
        .unwrap();
    let _ = client
        .post(format!("{}/{}/_doc/2", ES_URL, index))
        .json(&json!({"name": "Bob", "age": 25, "active": false}))
        .send()
        .await
        .unwrap();
    let _ = client
        .post(format!("{}/{}/_refresh", ES_URL, index))
        .send()
        .await
        .unwrap();

    let bool_query = json!({
        "query": {
            "bool": {
                "must": [{"term": {"active": true}}],
                "filter": [{"range": {"age": {"gte": 25}}}]
            }
        }
    });
    let resp = client
        .post(format!("{}/{}/_search", ES_URL, index))
        .json(&bool_query)
        .send()
        .await
        .unwrap();
    let result: serde_json::Value = resp.json().await.unwrap();
    let total = result["hits"]["total"]["value"].as_i64().unwrap();
    assert_eq!(total, 1);

    cleanup_index(index).await;
}

#[tokio::test]
#[ignore]
async fn test_real_es_delete_document() {
    if !es_health_check().await {
        eprintln!("ES not available, skipping");
        return;
    }
    let index = "sz_orm_test_delete";
    cleanup_index(index).await;

    let client = reqwest::Client::new();
    let _ = client
        .put(format!("{}/{}", ES_URL, index))
        .json(&json!({"mappings": {"properties": {"name": {"type": "keyword"}}}}))
        .send()
        .await
        .unwrap();

    let _ = client
        .post(format!("{}/{}/_doc/1", ES_URL, index))
        .json(&json!({"name": "Alice"}))
        .send()
        .await
        .unwrap();
    let _ = client
        .post(format!("{}/{}/_refresh", ES_URL, index))
        .send()
        .await
        .unwrap();

    let del_resp = client
        .delete(format!("{}/{}/_doc/1", ES_URL, index))
        .send()
        .await
        .unwrap();
    assert!(del_resp.status().is_success());

    let _ = client
        .post(format!("{}/{}/_refresh", ES_URL, index))
        .send()
        .await
        .unwrap();

    let get_resp = client
        .get(format!("{}/{}/_doc/1", ES_URL, index))
        .send()
        .await
        .unwrap();
    assert_eq!(get_resp.status(), reqwest::StatusCode::NOT_FOUND);

    cleanup_index(index).await;
}
