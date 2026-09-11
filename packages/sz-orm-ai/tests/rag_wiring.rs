//! W3-3 VEC-RAG-01：RAG 适配接线测试
//!
//! 验证 `RagEngine::query` 端到端接线：
//! - 检索 Top-K 文档 → 拼接上下文 → 调用 LLM 回调 → 返回 `RagResponse`
//! - Top-K 为空时标注 `no_context=true` + `warning=RAG_NO_CONTEXT`
//! - `source_ids` 来自检索结果 metadata 的 "source" 字段
//! - LLM 回调收到的上下文与检索结果一致
//! - LLM 错误向上传播
//!
//! 生产入口：`RagEngine::query`（packages/sz-orm-ai/src/rag/mod.rs）

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use sz_orm_ai::{
    Document, InMemoryVectorStore, RagConfig, RagEngine, RagLlmCallback, RagResponse,
    SimpleEmbeddingModel, RAG_NO_CONTEXT,
};

/// 记数式 LLM mock：记录调用次数，返回固定回答
struct CountingLlm {
    call_count: Arc<AtomicUsize>,
    fixed_answer: String,
}

impl CountingLlm {
    fn new(answer: impl Into<String>) -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
            fixed_answer: answer.into(),
        }
    }

    fn calls(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl RagLlmCallback for CountingLlm {
    async fn generate(&self, _query: &str, _context: &str) -> Result<String, sz_orm_ai::AiError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(self.fixed_answer.clone())
    }
}

/// 上下文捕获式 LLM mock：记录最后一次收到的上下文
struct ContextCapturingLlm {
    last_context: Arc<parking_lot::Mutex<String>>,
    fixed_answer: String,
}

impl ContextCapturingLlm {
    fn new(answer: impl Into<String>) -> Self {
        Self {
            last_context: Arc::new(parking_lot::Mutex::new(String::new())),
            fixed_answer: answer.into(),
        }
    }
}

#[async_trait::async_trait]
impl RagLlmCallback for ContextCapturingLlm {
    async fn generate(&self, _query: &str, context: &str) -> Result<String, sz_orm_ai::AiError> {
        *self.last_context.lock() = context.to_string();
        Ok(self.fixed_answer.clone())
    }
}

/// 错误 LLM mock：始终返回错误
struct ErrorLlm;

#[async_trait::async_trait]
impl RagLlmCallback for ErrorLlm {
    async fn generate(&self, _query: &str, _context: &str) -> Result<String, sz_orm_ai::AiError> {
        Err(sz_orm_ai::AiError::RAG("LLM unavailable".to_string()))
    }
}

/// 构造测试用 RagEngine
fn make_engine(top_k: usize) -> RagEngine<SimpleEmbeddingModel, InMemoryVectorStore> {
    let embedding = SimpleEmbeddingModel::new("test-model", 16);
    let store = InMemoryVectorStore::new();
    let config = RagConfig::new("rag_wiring_test").with_top_k(top_k);
    RagEngine::new(embedding, store, config)
}

// ===================== 接线测试 =====================

#[tokio::test]
async fn test_rag_query_with_indexed_docs_returns_answer() {
    let engine = make_engine(3);
    let docs = vec![
        Document::new("d1", "Rust is a systems programming language").with_source("doc1"),
        Document::new("d2", "Tokio is an async runtime for Rust").with_source("doc2"),
    ];
    engine.index_documents(docs).await.unwrap();

    let llm = CountingLlm::new("Rust is a systems language with async support.");
    let response: RagResponse = engine.query("What is Rust?", None, &llm).await.unwrap();

    assert!(!response.answer.is_empty());
    assert!(!response.no_context);
    assert!(response.warning.is_none());
    assert_eq!(llm.calls(), 1);
}

#[tokio::test]
async fn test_rag_query_no_docs_marks_no_context() {
    let engine = make_engine(3);
    // 不索引任何文档

    let llm = CountingLlm::new("I have no context to answer.");
    let response = engine.query("anything", None, &llm).await.unwrap();

    assert!(response.no_context);
    assert_eq!(response.warning.as_deref(), Some(RAG_NO_CONTEXT));
    assert!(response.source_ids.is_empty());
    assert_eq!(llm.calls(), 1);
}

#[tokio::test]
async fn test_rag_query_passes_context_to_llm() {
    let engine = make_engine(3);
    let docs = vec![Document::new("d1", "The capital of France is Paris").with_source("doc1")];
    engine.index_documents(docs).await.unwrap();

    let llm = ContextCapturingLlm::new("Paris");
    let _response = engine.query("capital of France", None, &llm).await.unwrap();

    let captured = llm.last_context.lock().clone();
    assert!(
        captured.contains("Paris"),
        "LLM should receive context containing 'Paris', got: {captured}"
    );
}

#[tokio::test]
async fn test_rag_query_source_ids_from_metadata() {
    let engine = make_engine(3);
    let docs = vec![
        Document::new("d1", "content one").with_source("source_alpha"),
        Document::new("d2", "content two").with_source("source_beta"),
    ];
    engine.index_documents(docs).await.unwrap();

    let llm = CountingLlm::new("answer");
    let response = engine.query("content", None, &llm).await.unwrap();

    assert!(
        !response.source_ids.is_empty(),
        "source_ids should not be empty when docs are indexed"
    );
    for sid in &response.source_ids {
        assert!(
            sid.starts_with("source_"),
            "source_id should come from metadata 'source' field, got: {sid}"
        );
    }
}

#[tokio::test]
async fn test_rag_query_llm_error_propagates() {
    let engine = make_engine(3);
    let docs = vec![Document::new("d1", "some content").with_source("doc1")];
    engine.index_documents(docs).await.unwrap();

    let result = engine.query("query", None, &ErrorLlm).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, sz_orm_ai::AiError::RAG(_)));
}

#[tokio::test]
async fn test_rag_query_empty_string_query_works() {
    let engine = make_engine(3);
    let docs = vec![Document::new("d1", "hello world").with_source("doc1")];
    engine.index_documents(docs).await.unwrap();

    let llm = CountingLlm::new("empty query answer");
    let response = engine.query("", None, &llm).await.unwrap();

    assert_eq!(response.answer, "empty query answer");
    assert_eq!(llm.calls(), 1);
}

#[tokio::test]
async fn test_rag_no_context_constant_value() {
    assert_eq!(RAG_NO_CONTEXT, "RAG_NO_CONTEXT");
}

#[tokio::test]
async fn test_rag_query_with_filter_none_completes() {
    let engine = make_engine(2);
    let docs = vec![
        Document::new("d1", "async programming in Rust").with_source("doc1"),
        Document::new("d2", "memory safety in Rust").with_source("doc2"),
        Document::new("d3", "concurrency patterns").with_source("doc3"),
    ];
    engine.index_documents(docs).await.unwrap();

    let llm = CountingLlm::new("filtered answer");
    let response = engine.query("Rust", None, &llm).await.unwrap();

    assert!(!response.no_context);
    assert!(response.source_ids.len() <= 2);
    assert_eq!(response.answer, "filtered answer");
}

#[tokio::test]
async fn test_rag_query_response_fields_populated() {
    let engine = make_engine(3);
    let docs = vec![Document::new("d1", "vector databases are fast").with_source("vec_doc")];
    engine.index_documents(docs).await.unwrap();

    let llm = CountingLlm::new("Vector DBs provide fast similarity search.");
    let response = engine.query("vector database", None, &llm).await.unwrap();

    assert!(!response.answer.is_empty());
    assert!(!response.no_context);
    assert!(response.warning.is_none());
    assert!(!response.source_ids.is_empty());
}

#[tokio::test]
async fn test_rag_query_no_context_still_calls_llm() {
    let engine = make_engine(3);
    // 无文档，但仍应调用 LLM（以空上下文）

    let llm = ContextCapturingLlm::new("no context answer");
    let response = engine.query("test", None, &llm).await.unwrap();

    assert!(response.no_context);
    let captured = llm.last_context.lock().clone();
    assert!(captured.is_empty(), "context should be empty when no docs");
    assert_eq!(response.answer, "no context answer");
}
