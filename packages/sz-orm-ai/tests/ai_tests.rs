//! sz-orm-ai 集成测试套件
//!
//! 覆盖以下公共 API：
//! - `AiError` 错误类型变体、Display、From 转换
//! - `EmbeddingError` / `VectorError` 错误类型构造与 Display
//! - `EmbeddingRecord` / `EmbeddingBatch` 构造与分块
//! - `SimpleEmbeddingModel` 嵌入模型（确定性、批处理、边界输入）
//! - `VectorRecord` / `SearchResult` / `VectorFilter` / `VectorMetric` / `CollectionMeta`
//! - `InMemoryVectorStore` 全部 `VectorStore` trait 接口（CRUD、搜索、过滤、metric、upsert、top_k）
//! - `RagConfig` / `Document` / `Chunk` / `RagEngine` / `RagSearchResult`
//! - 端到端 RAG 流程：索引、检索、删除
//!
//! 设计原则：
//! - 每个测试独立构造自己的 `InMemoryVectorStore`，互不影响
//! - 异步测试统一使用 `#[tokio::test]`
//! - 边界用例覆盖：空输入、零维度、维度不匹配、不存在集合、非法 filter JSON、空 batch

use std::collections::HashMap;
use sz_orm_ai::{
    AiError, Chunk, CollectionMeta, Document, EmbeddingBatch, EmbeddingError, EmbeddingModel,
    EmbeddingRecord, InMemoryVectorStore, RagConfig, RagEngine, RagSearchResult, SearchResult,
    SimpleEmbeddingModel, VectorError, VectorFilter, VectorMetric, VectorRecord, VectorStore,
};

/// 计算余弦相似度（测试断言辅助）
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na * nb)
}

// ===================== 错误类型测试 =====================

#[test]
fn test_ai_error_variants_display() {
    // 验证所有 AiError 变体的 Display 实现
    assert_eq!(
        AiError::Embedding("boom".into()).to_string(),
        "Embedding error: boom"
    );
    assert_eq!(
        AiError::Vector("vbo".into()).to_string(),
        "Vector store error: vbo"
    );
    assert_eq!(AiError::RAG("rbo".into()).to_string(), "RAG error: rbo");
    assert_eq!(
        AiError::Config("cbo".into()).to_string(),
        "Config error: cbo"
    );
    assert_eq!(
        AiError::ModelNotFound("X".into()).to_string(),
        "Model not found: X"
    );
    assert_eq!(
        AiError::NotSupported("ns".into()).to_string(),
        "Not supported: ns"
    );

    // 验证实现了 std::error::Error
    let err = AiError::Embedding("e".into());
    let _: &dyn std::error::Error = &err;
}

#[test]
fn test_ai_error_from_io_and_serde_json() {
    // From<std::io::Error> → AiError::Config
    let io_err = std::io::Error::other("io boom");
    let ai_err: AiError = io_err.into();
    match ai_err {
        AiError::Config(msg) => assert!(msg.contains("io boom")),
        other => panic!("expected AiError::Config, got {:?}", other),
    }

    // From<serde_json::Error> → AiError::Config
    let json_err = serde_json::from_str::<serde_json::Value>("{bad}").unwrap_err();
    let ai_err: AiError = json_err.into();
    match ai_err {
        AiError::Config(_) => {}
        other => panic!("expected AiError::Config, got {:?}", other),
    }
}

#[test]
fn test_embedding_error_and_vector_error() {
    // EmbeddingError::new
    let e = EmbeddingError::new("oops");
    assert_eq!(e.message, "oops");
    assert!(e.model.is_none());
    assert_eq!(e.to_string(), "EmbeddingError: oops");

    // EmbeddingError::with_model
    let e2 = EmbeddingError::with_model("oops", "m1");
    assert_eq!(e2.model.as_deref(), Some("m1"));
    assert_eq!(e2.to_string(), "EmbeddingError: oops (model: m1)");

    // 作为 std::error::Error
    let _: &dyn std::error::Error = &e;

    // VectorError::new
    let v = VectorError::new("vboom");
    assert_eq!(v.message, "vboom");
    assert!(v.collection.is_none());
    assert_eq!(v.to_string(), "VectorError: vboom");

    // VectorError::with_collection
    let v2 = VectorError::with_collection("vboom", "coll1");
    assert_eq!(v2.collection.as_deref(), Some("coll1"));
    assert_eq!(v2.to_string(), "VectorError: vboom (collection: coll1)");

    let _: &dyn std::error::Error = &v;
}

// ===================== Embedding 测试 =====================

#[test]
fn test_embedding_record_construction() {
    // EmbeddingRecord::new
    let rec = EmbeddingRecord::new("id1", "hello", vec![1.0, 2.0, 3.0]);
    assert_eq!(rec.id, "id1");
    assert_eq!(rec.text, "hello");
    assert_eq!(rec.vector, vec![1.0, 2.0, 3.0]);
    assert!(rec.metadata.is_none());

    // EmbeddingRecord::with_metadata
    let mut md = HashMap::new();
    md.insert("k".to_string(), serde_json::json!(42));
    let rec2 = EmbeddingRecord::with_metadata("id2", "world", vec![4.0], md.clone());
    assert_eq!(rec2.id, "id2");
    assert_eq!(rec2.metadata, Some(md));
}

#[test]
fn test_embedding_batch_chunks() {
    // 默认 batch_size = 32
    let records: Vec<EmbeddingRecord> = (0..10)
        .map(|i| EmbeddingRecord::new(format!("r{}", i), "t", vec![i as f32]))
        .collect();
    let batch = EmbeddingBatch::new(records);
    assert_eq!(batch.batch_size, 32);
    let chunks = batch.batch_chunks();
    assert_eq!(chunks.len(), 1); // 10 < 32 → 1 块
    assert_eq!(chunks[0].len(), 10);

    // 自定义 batch_size = 3，10 条 → 4 块 (3+3+3+1)
    let records: Vec<EmbeddingRecord> = (0..10)
        .map(|i| EmbeddingRecord::new(format!("r{}", i), "t", vec![i as f32]))
        .collect();
    let batch = EmbeddingBatch::new(records).with_batch_size(3);
    assert_eq!(batch.batch_size, 3);
    let chunks = batch.batch_chunks();
    assert_eq!(chunks.len(), 4);
    assert_eq!(chunks[0].len(), 3);
    assert_eq!(chunks[3].len(), 1);

    // 空 batch
    let empty = EmbeddingBatch::new(vec![]);
    assert!(empty.batch_chunks().is_empty());
}

#[tokio::test]
async fn test_simple_embedding_model_basic() {
    let model = SimpleEmbeddingModel::new("simple", 32);
    assert_eq!(model.model_name(), "simple");
    assert_eq!(model.dimension(), 32);
    assert_eq!(model.vocabulary_size(), 0);

    let v = model.embed("hello world hello").await.unwrap();
    assert_eq!(v.len(), 32);

    // L2 归一化：norm ≈ 1（非空文本）
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 1e-5);

    // 至少注册了 2 个不同 token（hello / world）
    assert!(model.vocabulary_size() >= 2);
}

#[tokio::test]
async fn test_simple_embedding_model_empty_and_zero_dimension() {
    // 空文本 → 全零向量
    let model = SimpleEmbeddingModel::new("m", 8);
    let v = model.embed("").await.unwrap();
    assert_eq!(v.len(), 8);
    assert!(v.iter().all(|x| *x == 0.0));

    // 仅标点（token 化后为空）→ 全零向量
    let v2 = model.embed("!@#$%^&*()").await.unwrap();
    assert!(v2.iter().all(|x| *x == 0.0));

    // dimension = 0 → 返回空向量，不 panic
    let zero_model = SimpleEmbeddingModel::new("zero", 0);
    assert_eq!(zero_model.dimension(), 0);
    let v3 = zero_model.embed("anything").await.unwrap();
    assert!(v3.is_empty());
}

#[tokio::test]
async fn test_simple_embedding_model_deterministic_and_batch() {
    let model = SimpleEmbeddingModel::new("m", 16);

    // 相同输入 → 相同输出（逐元素相等）
    let a = model.embed("rust async programming").await.unwrap();
    let b = model.embed("rust async programming").await.unwrap();
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b.iter()) {
        assert!((x - y).abs() < 1e-6);
    }

    // embed_batch 与逐个 embed 一致
    let texts = vec!["one".to_string(), "two".to_string(), "three".to_string()];
    let batch = model.embed_batch(&texts).await.unwrap();
    assert_eq!(batch.len(), 3);
    for (i, t) in texts.iter().enumerate() {
        let single = model.embed(t).await.unwrap();
        assert_eq!(batch[i].len(), single.len());
        for (x, y) in batch[i].iter().zip(single.iter()) {
            assert!((x - y).abs() < 1e-6);
        }
    }

    // 空 batch
    let empty_batch = model.embed_batch(&[]).await.unwrap();
    assert!(empty_batch.is_empty());
}

#[tokio::test]
async fn test_simple_embedding_similarity_ordering() {
    // 相同文本 → 余弦相似度 = 1
    let model = SimpleEmbeddingModel::new("m", 64);
    let v_same_a = model.embed("rust rust rust").await.unwrap();
    let v_same_b = model.embed("rust rust rust").await.unwrap();
    let sim_same = cosine(&v_same_a, &v_same_b);
    assert!((sim_same - 1.0).abs() < 1e-5);

    // 不同文本相似度 ≤ 相同文本相似度
    let v_diff = model.embed("zzz qqq xxx").await.unwrap();
    let sim_diff = cosine(&v_same_a, &v_diff);
    assert!(sim_same >= sim_diff);
}

// ===================== Vector 数据结构测试 =====================

#[test]
fn test_vector_record_construction_and_from_embedding() {
    // VectorRecord::new
    let rec = VectorRecord::new("v1", vec![1.0, 0.0]);
    assert_eq!(rec.id, "v1");
    assert_eq!(rec.vector, vec![1.0, 0.0]);
    assert!(rec.score.is_none());
    assert!(rec.metadata.is_none());

    // 链式 with_score / with_metadata
    let md = HashMap::from([("k".to_string(), serde_json::json!("v"))]);
    let rec2 = VectorRecord::new("v2", vec![1.0])
        .with_score(0.5)
        .with_metadata(md.clone());
    assert_eq!(rec2.score, Some(0.5));
    assert_eq!(rec2.metadata, Some(md));

    // from_embedding：从 EmbeddingRecord 转换
    let emb_md = HashMap::from([("k".to_string(), serde_json::json!(1))]);
    let emb = EmbeddingRecord::with_metadata("e1", "text", vec![0.1, 0.2], emb_md.clone());
    let vr = VectorRecord::from_embedding(&emb);
    assert_eq!(vr.id, "e1");
    assert_eq!(vr.vector, vec![0.1, 0.2]);
    assert!(vr.score.is_none());
    assert_eq!(vr.metadata, Some(emb_md));
}

#[test]
fn test_search_result_and_vector_filter_build() {
    // SearchResult 构造
    let sr = SearchResult::new("s1", 0.9, vec![1.0]).with_text("hello");
    assert_eq!(sr.id, "s1");
    assert!((sr.score - 0.9).abs() < 1e-6);
    assert_eq!(sr.text.as_deref(), Some("hello"));
    assert!(sr.metadata.is_none());

    // VectorFilter::eq
    let f = VectorFilter::new().field("kind").eq("alpha");
    assert_eq!(f.build().as_deref(), Some(r#"{"kind": {"eq": "alpha"}}"#));

    // VectorFilter::gt（数值）
    let f2 = VectorFilter::new().field("score").gt(5);
    assert_eq!(f2.build().as_deref(), Some(r#"{"score": {"gt": 5}}"#));

    // VectorFilter::lt（数值）
    let f3 = VectorFilter::new().field("score").lt(10);
    assert_eq!(f3.build().as_deref(), Some(r#"{"score": {"lt": 10}}"#));

    // 缺字段 → build 返回 None
    assert!(VectorFilter::new().eq("v").build().is_none());
    assert!(VectorFilter::new().build().is_none());
}

#[test]
fn test_vector_metric_and_collection_meta() {
    // VectorMetric::as_str
    assert_eq!(VectorMetric::Cosine.as_str(), "cosine");
    assert_eq!(VectorMetric::Euclidean.as_str(), "euclidean");
    assert_eq!(VectorMetric::DotProduct.as_str(), "dotproduct");

    // Default 是 Cosine
    assert!(matches!(VectorMetric::default(), VectorMetric::Cosine));

    // CollectionMeta 默认 metric = Cosine
    let meta = CollectionMeta::new("coll", 128);
    assert_eq!(meta.name, "coll");
    assert_eq!(meta.dimension, 128);
    assert_eq!(meta.count, 0);
    assert!(matches!(meta.metric, VectorMetric::Cosine));

    // with_metric
    let meta2 = CollectionMeta::new("c2", 4).with_metric(VectorMetric::Euclidean);
    assert!(matches!(meta2.metric, VectorMetric::Euclidean));
}

// ===================== InMemoryVectorStore 测试 =====================

#[tokio::test]
async fn test_in_memory_vector_store_full_crud() {
    let store = InMemoryVectorStore::new();
    // Default trait
    let _default_store = InMemoryVectorStore::default();

    // create_collection
    store.create_collection("docs", 3, None).await.unwrap();
    assert_eq!(store.count("docs").await.unwrap(), 0);

    // insert
    let records = vec![
        VectorRecord::new("a", vec![1.0, 0.0, 0.0]),
        VectorRecord::new("b", vec![0.0, 1.0, 0.0]),
    ];
    store.insert("docs", records).await.unwrap();
    assert_eq!(store.count("docs").await.unwrap(), 2);

    // get 命中
    let fetched = store.get("docs", "a").await.unwrap().unwrap();
    assert_eq!(fetched.id, "a");
    assert_eq!(fetched.vector, vec![1.0, 0.0, 0.0]);

    // get 未命中
    assert!(store.get("docs", "missing").await.unwrap().is_none());

    // get 不存在的 collection → Err
    assert!(store.get("nope", "a").await.is_err());

    // delete
    let removed = store.delete("docs", vec!["a".to_string()]).await.unwrap();
    assert_eq!(removed, 1);
    assert_eq!(store.count("docs").await.unwrap(), 1);

    // delete 不存在的 collection → Err
    assert!(store.delete("nope", vec!["a".to_string()]).await.is_err());

    // delete_collection
    store.delete_collection("docs").await.unwrap();
    assert_eq!(store.count("docs").await.unwrap(), 0);

    // count 不存在的 collection → 0（不报错）
    assert_eq!(store.count("never_exists").await.unwrap(), 0);
}

#[tokio::test]
async fn test_in_memory_vector_store_insert_errors() {
    let store = InMemoryVectorStore::new();

    // insert 到不存在的 collection → Err
    let err = store
        .insert("missing", vec![VectorRecord::new("x", vec![1.0])])
        .await;
    assert!(err.is_err());

    // 维度不匹配 → Err
    store.create_collection("docs", 3, None).await.unwrap();
    let err = store
        .insert("docs", vec![VectorRecord::new("x", vec![1.0, 2.0])])
        .await;
    assert!(err.is_err());

    // search 不存在的 collection → Err
    assert!(store.search("missing", &[1.0], 1, None).await.is_err());
}

#[tokio::test]
async fn test_in_memory_vector_store_search_with_metrics() {
    // 验证三种 metric 都能正确排序
    let query = vec![1.0, 0.0, 0.0];

    for metric in [
        VectorMetric::Cosine,
        VectorMetric::Euclidean,
        VectorMetric::DotProduct,
    ] {
        let store = InMemoryVectorStore::new();
        store.create_collection("c", 3, Some(metric)).await.unwrap();
        let records = vec![
            VectorRecord::new("same", vec![1.0, 0.0, 0.0]),
            VectorRecord::new("orth", vec![0.0, 1.0, 0.0]),
            VectorRecord::new("mix", vec![1.0, 1.0, 0.0]),
        ];
        store.insert("c", records).await.unwrap();

        let results = store.search("c", &query, 3, None).await.unwrap();
        assert_eq!(results.len(), 3, "metric {:?}: 应返回 3 条", metric);

        // "same" 与 query 完全相同，相似度最高
        assert_eq!(
            results[0].id, "same",
            "metric {:?}: 顶部结果应为 same",
            metric
        );

        // 评分按降序排列
        assert!(results[0].score >= results[1].score);
        assert!(results[1].score >= results[2].score);
    }
}

#[tokio::test]
async fn test_in_memory_vector_store_upsert_and_top_k() {
    let store = InMemoryVectorStore::new();
    store.create_collection("c", 2, None).await.unwrap();

    // 同 id 重复插入 → upsert 语义（覆盖，不新增）
    store
        .insert("c", vec![VectorRecord::new("r1", vec![1.0, 0.0])])
        .await
        .unwrap();
    store
        .insert("c", vec![VectorRecord::new("r1", vec![0.0, 1.0])])
        .await
        .unwrap();
    assert_eq!(store.count("c").await.unwrap(), 1);
    let fetched = store.get("c", "r1").await.unwrap().unwrap();
    assert_eq!(fetched.vector, vec![0.0, 1.0]);

    // 再插入 4 条，共 5 条
    for i in 0..4 {
        store
            .insert(
                "c",
                vec![VectorRecord::new(
                    format!("r{}", i + 2),
                    vec![i as f32, 1.0],
                )],
            )
            .await
            .unwrap();
    }
    assert_eq!(store.count("c").await.unwrap(), 5);

    // top_k 大于总数 → 返回总数
    let results = store.search("c", &[0.0, 1.0], 100, None).await.unwrap();
    assert_eq!(results.len(), 5);

    // top_k = 0 → 空
    let results = store.search("c", &[0.0, 1.0], 0, None).await.unwrap();
    assert_eq!(results.len(), 0);
}

#[tokio::test]
async fn test_in_memory_vector_store_filter() {
    let store = InMemoryVectorStore::new();
    store.create_collection("c", 2, None).await.unwrap();

    let mut md_a = HashMap::new();
    md_a.insert("kind".to_string(), serde_json::json!("alpha"));
    md_a.insert("score".to_string(), serde_json::json!(5));
    let r1 = VectorRecord::new("a", vec![1.0, 0.0]).with_metadata(md_a);

    let mut md_b = HashMap::new();
    md_b.insert("kind".to_string(), serde_json::json!("beta"));
    md_b.insert("score".to_string(), serde_json::json!(10));
    let r2 = VectorRecord::new("b", vec![1.0, 0.0]).with_metadata(md_b);

    let r3 = VectorRecord::new("c", vec![1.0, 0.0]); // 无 metadata
    store.insert("c", vec![r1, r2, r3]).await.unwrap();

    // eq filter：只匹配 kind=alpha
    let res = store
        .search("c", &[1.0, 0.0], 10, Some(r#"{"kind": {"eq": "alpha"}}"#))
        .await
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, "a");

    // gt filter：score > 7 → 只匹配 b
    let res = store
        .search("c", &[1.0, 0.0], 10, Some(r#"{"score": {"gt": 7}}"#))
        .await
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, "b");

    // lt filter：score < 7 → 只匹配 a
    let res = store
        .search("c", &[1.0, 0.0], 10, Some(r#"{"score": {"lt": 7}}"#))
        .await
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, "a");

    // 无 metadata 的记录在 filter 下被排除
    let res = store
        .search("c", &[1.0, 0.0], 10, Some(r#"{"kind": {"eq": "alpha"}}"#))
        .await
        .unwrap();
    assert_eq!(res.iter().filter(|r| r.id == "c").count(), 0);

    // 非法 JSON filter → 无匹配（不 panic）
    let res = store
        .search("c", &[1.0, 0.0], 10, Some(r#"not json"#))
        .await
        .unwrap();
    assert_eq!(res.len(), 0);

    // 不存在的字段 → 无匹配
    let res = store
        .search("c", &[1.0, 0.0], 10, Some(r#"{"missing": {"eq": "x"}}"#))
        .await
        .unwrap();
    assert_eq!(res.len(), 0);

    // 无 filter → 返回全部 3 条
    let res = store.search("c", &[1.0, 0.0], 100, None).await.unwrap();
    assert_eq!(res.len(), 3);
}

#[tokio::test]
async fn test_in_memory_vector_store_empty_query_vector() {
    // 维度为 0 的 query 向量：cosine_similarity 返回 0（不 panic）
    let store = InMemoryVectorStore::new();
    store.create_collection("c", 3, None).await.unwrap();
    store
        .insert("c", vec![VectorRecord::new("a", vec![1.0, 0.0, 0.0])])
        .await
        .unwrap();

    let results = store.search("c", &[], 10, None).await.unwrap();
    assert_eq!(results.len(), 1);
    // cosine_similarity 对 a.len() != b.len() 返回 0
    assert!((results[0].score - 0.0).abs() < 1e-6);
}

// ===================== RAG 测试 =====================

#[test]
fn test_rag_config_builder() {
    // Default
    let default = RagConfig::default();
    assert_eq!(default.collection_name, "rag_documents");
    assert_eq!(default.chunk_size, 512);
    assert_eq!(default.chunk_overlap, 50);
    assert_eq!(default.top_k, 3);

    // Builder 链
    let cfg = RagConfig::new("my_coll")
        .with_chunk_size(100)
        .with_chunk_overlap(10)
        .with_top_k(5);
    assert_eq!(cfg.collection_name, "my_coll");
    assert_eq!(cfg.chunk_size, 100);
    assert_eq!(cfg.chunk_overlap, 10);
    assert_eq!(cfg.top_k, 5);

    // new 只设置 collection_name，其他保持默认
    let cfg2 = RagConfig::new("only_name");
    assert_eq!(cfg2.collection_name, "only_name");
    assert_eq!(cfg2.chunk_size, 512);
    assert_eq!(cfg2.top_k, 3);
}

#[test]
fn test_document_and_chunk_builders() {
    // Document builder
    let doc = Document::new("d1", "hello world")
        .with_source("src1")
        .with_metadata("k", serde_json::json!("v"));
    assert_eq!(doc.id, "d1");
    assert_eq!(doc.content, "hello world");
    assert_eq!(doc.source.as_deref(), Some("src1"));
    assert_eq!(doc.metadata.get("k"), Some(&serde_json::json!("v")));

    // Document::new 默认无 source / 空 metadata
    let bare = Document::new("d2", "txt");
    assert!(bare.source.is_none());
    assert!(bare.metadata.is_empty());

    // Chunk builder
    let chunk = Chunk::new("c1", "d1", "hello", 0, 0, 5).with_metadata("pos", serde_json::json!(0));
    assert_eq!(chunk.id, "c1");
    assert_eq!(chunk.document_id, "d1");
    assert_eq!(chunk.content, "hello");
    assert_eq!(chunk.index, 0);
    assert_eq!(chunk.start_char, 0);
    assert_eq!(chunk.end_char, 5);
    assert_eq!(chunk.metadata.get("pos"), Some(&serde_json::json!(0)));
}

#[tokio::test]
async fn test_rag_engine_index_and_search() {
    let model = SimpleEmbeddingModel::new("rag-model", 32);
    let store = InMemoryVectorStore::new();
    let cfg = RagConfig::new("test_coll")
        .with_chunk_size(10)
        .with_chunk_overlap(2)
        .with_top_k(2);
    let engine = RagEngine::new(model, store, cfg);

    // 索引多个文档
    let docs = vec![
        Document::new("d1", "rust async programming language").with_source("s1"),
        Document::new("d2", "python data science machine learning").with_source("s2"),
        Document::new("d3", "rust async runtime tokio"),
    ];
    let indexed = engine.index_documents(docs).await.unwrap();
    assert!(indexed >= 3, "至少应索引 3 个 chunk，实际 {}", indexed);

    // 搜索：query 包含 "rust async" 应匹配 d1 / d3 的 chunks
    let results = engine.search("rust async", None).await.unwrap();
    assert!(!results.is_empty());
    assert!(results.len() <= 2, "top_k=2，实际 {}", results.len());

    for r in &results {
        assert!(!r.id.is_empty());
        assert!(r.score.is_finite());
    }

    // 评分降序
    for w in results.windows(2) {
        assert!(w[0].score >= w[1].score);
    }
}

#[tokio::test]
async fn test_rag_engine_empty_and_edge_cases() {
    let model = SimpleEmbeddingModel::new("m", 16);
    let store = InMemoryVectorStore::new();
    let cfg = RagConfig::new("empty_coll");
    let engine = RagEngine::new(model, store, cfg);

    // 索引空文档列表（仍会创建 collection）
    let indexed = engine.index_documents(vec![]).await.unwrap();
    assert_eq!(indexed, 0);

    // 在空集合上搜索 → 空结果
    let results = engine.search("anything", None).await.unwrap();
    assert!(results.is_empty());

    // delete_document 在空集合上 → 0
    let deleted = engine.delete_document("d1").await.unwrap();
    assert_eq!(deleted, 0);
}

#[tokio::test]
async fn test_rag_engine_with_config_swap() {
    let model = SimpleEmbeddingModel::new("m", 8);
    let store = InMemoryVectorStore::new();
    let cfg1 = RagConfig::new("c1");
    let engine = RagEngine::new(model, store, cfg1);

    // 切换到新 config（不同 collection_name、top_k=1）
    let cfg2 = RagConfig::new("c2").with_top_k(1);
    let engine = engine.with_config(cfg2);

    let docs = vec![Document::new("d1", "hello world")];
    let n = engine.index_documents(docs).await.unwrap();
    assert!(n >= 1);

    // top_k=1 限制
    let results = engine.search("hello", None).await.unwrap();
    assert!(results.len() <= 1);
}

#[tokio::test]
async fn test_rag_engine_delete_document() {
    let model = SimpleEmbeddingModel::new("m", 16);
    let store = InMemoryVectorStore::new();
    // chunk_size=100 让每个文档只产生 1 个 chunk
    let cfg = RagConfig::new("del_coll").with_chunk_size(100);
    let engine = RagEngine::new(model, store, cfg);

    let docs = vec![
        Document::new("d1", "the quick brown fox jumps over the lazy dog"),
        Document::new("d2", "rust programming is fun and productive"),
    ];
    let _ = engine.index_documents(docs).await.unwrap();

    // 删除 d1（其 chunk id 形如 "d1_0"）
    let deleted = engine.delete_document("d1").await.unwrap();
    assert!(deleted >= 1, "应至少删除 1 个 chunk");

    // 再次删除 d1 → 0（已无匹配）
    let deleted2 = engine.delete_document("d1").await.unwrap();
    assert_eq!(deleted2, 0);

    // d2 的 chunk 仍然存在
    let results = engine.search("rust", None).await.unwrap();
    assert!(!results.is_empty());
    for r in &results {
        assert!(
            r.id.starts_with("d2"),
            "剩余 chunk 应属于 d2，实际 id={}",
            r.id
        );
    }
}

#[tokio::test]
async fn test_rag_engine_filter_passthrough() {
    // RAG engine 将 filter 透传给 VectorStore
    // 注意：split_documents 只把 "source" 写入 chunk metadata，
    // document.metadata 不会传递到 chunk，因此按其他字段过滤会返回空。
    let model = SimpleEmbeddingModel::new("m", 16);
    let store = InMemoryVectorStore::new();
    let cfg = RagConfig::new("filter_coll").with_chunk_size(100);
    let engine = RagEngine::new(model, store, cfg);

    let docs = vec![
        Document::new("d1", "alpha beta gamma").with_metadata("kind", serde_json::json!("x")),
        Document::new("d2", "delta epsilon zeta").with_metadata("kind", serde_json::json!("y")),
    ];
    let _ = engine.index_documents(docs).await.unwrap();

    // 按 "kind" 过滤（chunk metadata 只有 "source"）→ 空
    let results = engine
        .search("alpha", Some(r#"{"kind": {"eq": "x"}}"#))
        .await
        .unwrap();
    assert!(results.is_empty());

    // 无 filter → 有结果
    let results = engine.search("alpha", None).await.unwrap();
    assert!(!results.is_empty());
}

// ===================== 端到端集成测试 =====================

#[tokio::test]
async fn test_end_to_end_rag_pipeline() {
    // 模拟完整 RAG 流程：索引 → 检索 → 验证排序 → Clone/Debug
    let model = SimpleEmbeddingModel::new("e2e", 64);
    let store = InMemoryVectorStore::new();
    let cfg = RagConfig::new("e2e_coll")
        .with_chunk_size(20)
        .with_chunk_overlap(5)
        .with_top_k(3);
    let engine = RagEngine::new(model, store, cfg);

    let docs = vec![
        Document::new(
            "doc_rust",
            "rust is a systems programming language focused on safety and performance",
        )
        .with_source("rust-doc"),
        Document::new(
            "doc_python",
            "python is a high-level programming language popular for data science",
        ),
        Document::new(
            "doc_js",
            "javascript is the programming language of the web",
        ),
    ];
    let n = engine.index_documents(docs).await.unwrap();
    assert!(n >= 3, "应索引多个 chunk，实际 {}", n);

    // 查询 "programming language" 应匹配多个文档的 chunks
    let results = engine.search("programming language", None).await.unwrap();
    assert!(!results.is_empty());
    assert!(results.len() <= 3, "top_k=3，实际 {}", results.len());

    // 验证 RagSearchResult 字段
    for r in &results {
        assert!(r.score.is_finite(), "score 必须是有限值");
        // content 来自 chunk 的 metadata.text（VectorRecord 无 text 字段，故为空字符串）
        // id 非空
        assert!(!r.id.is_empty());
    }

    // 评分降序
    for w in results.windows(2) {
        assert!(w[0].score >= w[1].score, "评分应降序排列");
    }

    // RagSearchResult 的 Clone / Debug trait
    let cloned: Vec<RagSearchResult> = results.clone();
    assert_eq!(cloned.len(), results.len());
    let debug_str = format!("{:?}", cloned);
    assert!(debug_str.contains("RagSearchResult"));
}
