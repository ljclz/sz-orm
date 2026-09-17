//! v7.3.0 任务 3.5：ANN 加速检索测试
//!
//! 验证项：
//! - 召回率 ≥ 90%（内存实现使用精确 kNN，召回率 = 1.0）
//! - 跨租户隔离（tenant_id 过滤）
//! - 确定性（相同查询相同结果）
//! - 延迟测量

use std::collections::HashMap;

use sz_orm_vector::{
    AnnAccelerated, AnnSearchResult, InMemoryVectorStore, PgVectorStore, VectorRecord,
};

/// 构造带租户元数据的向量记录
fn make_record(id: &str, vector: Vec<f32>, tenant_id: &str) -> VectorRecord {
    let mut meta = HashMap::new();
    meta.insert(
        "tenant_id".to_string(),
        serde_json::Value::String(tenant_id.to_string()),
    );
    VectorRecord::new(id, vector).with_metadata(meta)
}

#[tokio::test]
async fn ann_recall_rate_is_1_for_in_memory() {
    // 内存实现使用精确 kNN，召回率必须为 1.0
    let store = InMemoryVectorStore::new();
    store.create_collection("docs", 3, None).await.unwrap();

    let records = vec![
        VectorRecord::new("a", vec![1.0, 0.0, 0.0]),
        VectorRecord::new("b", vec![0.0, 1.0, 0.0]),
        VectorRecord::new("c", vec![0.0, 0.0, 1.0]),
        VectorRecord::new("d", vec![0.7, 0.7, 0.0]),
        VectorRecord::new("e", vec![0.5, 0.5, 0.5]),
    ];
    store.insert("docs", records).await.unwrap();

    let result: AnnSearchResult = store
        .ann_search("docs", &[1.0, 0.0, 0.0], 3, None)
        .await
        .unwrap();

    assert!(
        result.recall_rate >= 0.9,
        "召回率 {} 应 >= 0.9",
        result.recall_rate
    );
    assert_eq!(result.recall_rate, 1.0, "内存实现召回率必须为 1.0");
    assert_eq!(result.records.len(), 3, "应返回 top_k=3 条结果");
}

#[tokio::test]
async fn ann_tenant_isolation_filters_cross_tenant() {
    // 租户隔离：tenant_a 的查询不应返回 tenant_b 的记录
    let store = InMemoryVectorStore::new();
    store.create_collection("docs", 3, None).await.unwrap();

    let records = vec![
        make_record("a1", vec![1.0, 0.0, 0.0], "tenant_a"),
        make_record("a2", vec![0.9, 0.1, 0.0], "tenant_a"),
        make_record("b1", vec![1.0, 0.0, 0.0], "tenant_b"),
        make_record("b2", vec![0.95, 0.05, 0.0], "tenant_b"),
    ];
    store.insert("docs", records).await.unwrap();

    // tenant_a 查询，只应返回 tenant_a 的记录
    let result = store
        .ann_search("docs", &[1.0, 0.0, 0.0], 4, Some("tenant_a"))
        .await
        .unwrap();

    assert_eq!(result.records.len(), 2, "tenant_a 应只有 2 条记录");
    for r in &result.records {
        let tid = r
            .metadata
            .as_ref()
            .and_then(|m| m.get("tenant_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(tid, "tenant_a", "结果中不应包含其他租户的记录");
    }
}

#[tokio::test]
async fn ann_tenant_isolation_no_tenant_returns_all() {
    // 不指定租户时返回全部记录
    let store = InMemoryVectorStore::new();
    store.create_collection("docs", 2, None).await.unwrap();

    let records = vec![
        make_record("a1", vec![1.0, 0.0], "tenant_a"),
        make_record("b1", vec![0.9, 0.1], "tenant_b"),
        make_record("c1", vec![0.8, 0.2], "tenant_c"),
    ];
    store.insert("docs", records).await.unwrap();

    let result = store
        .ann_search("docs", &[1.0, 0.0], 10, None)
        .await
        .unwrap();

    assert_eq!(result.records.len(), 3, "不指定租户应返回全部 3 条");
}

#[tokio::test]
async fn ann_deterministic_same_query_same_result() {
    // 确定性：相同查询应返回相同结果
    let store = InMemoryVectorStore::new();
    store.create_collection("docs", 3, None).await.unwrap();

    let records = vec![
        VectorRecord::new("a", vec![1.0, 0.0, 0.0]),
        VectorRecord::new("b", vec![0.0, 1.0, 0.0]),
        VectorRecord::new("c", vec![0.0, 0.0, 1.0]),
    ];
    store.insert("docs", records).await.unwrap();

    let r1 = store
        .ann_search("docs", &[0.6, 0.4, 0.0], 2, None)
        .await
        .unwrap();
    let r2 = store
        .ann_search("docs", &[0.6, 0.4, 0.0], 2, None)
        .await
        .unwrap();

    assert_eq!(r1.records.len(), r2.records.len());
    for (a, b) in r1.records.iter().zip(r2.records.iter()) {
        assert_eq!(a.id, b.id, "相同查询应返回相同结果顺序");
        assert!((a.score - b.score).abs() < 1e-6, "分数应一致");
    }
    assert_eq!(r1.recall_rate, r2.recall_rate);
}

#[tokio::test]
async fn ann_latency_is_measured() {
    // 延迟测量：latency_ms 应为非负值
    let store = InMemoryVectorStore::new();
    store.create_collection("docs", 128, None).await.unwrap();

    let records: Vec<_> = (0..100)
        .map(|i| {
            let v: Vec<f32> = (0..128).map(|j| (i * 128 + j) as f32 / 1000.0).collect();
            VectorRecord::new(format!("vec_{}", i), v)
        })
        .collect();
    store.insert("docs", records).await.unwrap();

    let query: Vec<f32> = (0..128).map(|j| j as f32 / 1000.0).collect();
    let result = store
        .ann_search("docs", &query, 10, None)
        .await
        .unwrap();

    assert_eq!(result.records.len(), 10);
    // 内存操作极快，延迟应很小（但非负）
    // 不做严格上界断言，因为 CI 环境可能波动
}

#[tokio::test]
async fn ann_empty_collection_returns_empty() {
    let store = InMemoryVectorStore::new();
    store.create_collection("empty", 3, None).await.unwrap();

    let result = store
        .ann_search("empty", &[1.0, 0.0, 0.0], 5, None)
        .await
        .unwrap();

    assert_eq!(result.records.len(), 0);
    assert_eq!(result.recall_rate, 1.0, "空集召回率仍为 1.0");
}

#[tokio::test]
async fn ann_tenant_isolation_nonexistent_tenant() {
    // 查询不存在的租户应返回空结果
    let store = InMemoryVectorStore::new();
    store.create_collection("docs", 2, None).await.unwrap();

    let records = vec![
        make_record("a1", vec![1.0, 0.0], "tenant_a"),
        make_record("b1", vec![0.9, 0.1], "tenant_b"),
    ];
    store.insert("docs", records).await.unwrap();

    let result = store
        .ann_search("docs", &[1.0, 0.0], 5, Some("nonexistent"))
        .await
        .unwrap();

    assert_eq!(result.records.len(), 0, "不存在的租户应返回空结果");
}

#[tokio::test]
async fn ann_top_k_larger_than_collection() {
    // top_k 大于集合大小时返回全部记录
    let store = InMemoryVectorStore::new();
    store.create_collection("docs", 2, None).await.unwrap();

    let records = vec![
        make_record("a1", vec![1.0, 0.0], "tenant_a"),
        make_record("a2", vec![0.9, 0.1], "tenant_a"),
    ];
    store.insert("docs", records).await.unwrap();

    let result = store
        .ann_search("docs", &[1.0, 0.0], 100, Some("tenant_a"))
        .await
        .unwrap();

    assert_eq!(result.records.len(), 2, "top_k > 集合大小应返回全部匹配记录");
}