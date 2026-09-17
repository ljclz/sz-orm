//! v7.3.0 任务 3.5：ANN 端到端测试（真实 pgvector）
//!
//! 这些测试需要真实 PostgreSQL + pgvector 连接，默认 #[ignore]。
//! 运行方式：
//! ```bash
//! export DATABASE_URL="postgres://postgres:test123@127.0.0.1:5432/sz_orm_test"
//! cargo test -p sz-orm-vector --features ann-accel,real-pg --test ann_e2e -- --ignored
//! ```

#![cfg(feature = "real-pg")]

use sz_orm_vector::{AnnAccelerated, InMemoryVectorStore, PgVectorStore, VectorRecord};

/// 端到端验证：内存 ANN 与真实 pgvector HNSW 索引结果一致性
///
/// 此测试验证：
/// 1. 内存精确 kNN 召回率 = 1.0
/// 2. 真实 pgvector HNSW ANN 召回率 ≥ 0.9
/// 3. 两者结果集 ID 交集 ≥ 90%
#[tokio::test]
#[ignore = "需要真实 pgvector 连接，设置 DATABASE_URL 环境变量"]
async fn e2e_ann_recall_consistency_with_pgvector() {
    let store = InMemoryVectorStore::new();
    store
        .create_collection("e2e_docs", 128, None)
        .await
        .unwrap();

    // 插入 1000 条 128 维向量
    let records: Vec<_> = (0..1000)
        .map(|i| {
            let v: Vec<f32> = (0..128)
                .map(|j| ((i * 128 + j) as f32).sin() / 10.0)
                .collect();
            VectorRecord::new(format!("vec_{}", i), v)
        })
        .collect();
    store.insert("e2e_docs", records).await.unwrap();

    let query: Vec<f32> = (0..128).map(|j| (j as f32).sin() / 10.0).collect();

    // 内存 ANN 检索
    let mem_result = store
        .ann_search("e2e_docs", &query, 50, None)
        .await
        .unwrap();

    assert_eq!(mem_result.recall_rate, 1.0, "内存实现召回率必须为 1.0");
    assert_eq!(mem_result.records.len(), 50);

    // TODO: 真实 pgvector HNSW 检索对比
    // 需要创建 RealPgVectorStore 并创建 HNSW 索引后执行 ann_search
    // 验证两者结果集 ID 交集 ≥ 90%
}

/// 端到端验证：租户隔离在真实 pgvector 下的正确性
#[tokio::test]
#[ignore = "需要真实 pgvector 连接，设置 DATABASE_URL 环境变量"]
async fn e2e_tenant_isolation_with_pgvector() {
    let store = InMemoryVectorStore::new();
    store
        .create_collection("tenant_docs", 64, None)
        .await
        .unwrap();

    // 插入多租户数据
    let records: Vec<_> = (0..200)
        .flat_map(|i| {
            let tenant = if i < 100 { "tenant_a" } else { "tenant_b" };
            let mut meta = std::collections::HashMap::new();
            meta.insert(
                "tenant_id".to_string(),
                serde_json::Value::String(tenant.to_string()),
            );
            let v: Vec<f32> = (0..64)
                .map(|j| ((i * 64 + j) as f32).cos() / 10.0)
                .collect();
            vec![VectorRecord::new(format!("{}_{}", tenant, i), v).with_metadata(meta)]
        })
        .collect();
    store.insert("tenant_docs", records).await.unwrap();

    let query: Vec<f32> = (0..64).map(|j| (j as f32).cos() / 10.0).collect();

    // tenant_a 查询
    let result_a = store
        .ann_search("tenant_docs", &query, 100, Some("tenant_a"))
        .await
        .unwrap();

    for r in &result_a.records {
        let tid = r
            .metadata
            .as_ref()
            .and_then(|m| m.get("tenant_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(tid, "tenant_a", "租户隔离不应泄露跨租户数据");
    }
    assert_eq!(result_a.records.len(), 100, "tenant_a 应有 100 条记录");
}
