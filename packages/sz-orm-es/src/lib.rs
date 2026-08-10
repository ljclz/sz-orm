//! # SZ-ORM ES — Elasticsearch 文档存储（**MOCK-ONLY，非生产可用**）
//!
//! ⚠️ **重要警告：本 crate 仅为内存 Mock 实现，未集成真实 Elasticsearch 客户端。**
//!
//! - 所有数据存储在进程内 `HashMap`，重启即丢失
//! - 不支持分布式、副本、分片、持久化、TLS、认证等任何 ES 生产特性
//! - 查询语义为简化版，不保证与真实 ES 行为一致
//! - **请勿用于生产环境**——仅适用于单元测试与本地开发
//!
//! ## 替代方案
//!
//! 如需真实 ES 集成，请：
//! 1. 实现 [`EsSync`] trait（接入 `elasticsearch` 官方 crate）
//! 2. 通过 [`EsSyncManager::with_backend`] 注入自定义后端
//!
//! ## 主要类型
//!
//! - [`EsDocument`] — 文档模型
//! - [`EsSearchRequest`] — 搜索请求
//! - [`EsSync`] — 同步后端抽象（可被真实 ES 客户端实现）
//! - [`InMemoryEsSync`] — **Mock 实现**（仅供测试）
//! - [`EsSyncManager`] — 同步管理器（默认使用 Mock，可通过 `with_backend` 替换）

#[cfg(feature = "real")]
pub mod real_es;

pub mod extensions;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EsDocument {
    pub id: Option<String>,
    pub index: String,
    pub source: serde_json::Value,
    pub timestamp: i64,
}

impl EsDocument {
    pub fn new(index: impl Into<String>, source: serde_json::Value) -> Self {
        Self {
            id: None,
            index: index.into(),
            source,
            timestamp: current_timestamp(),
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EsSearchRequest {
    pub index: String,
    pub query: EsQuery,
    pub from: usize,
    pub size: usize,
    pub sort: Vec<EsSort>,
}

impl EsSearchRequest {
    pub fn new(index: impl Into<String>, query: EsQuery) -> Self {
        Self {
            index: index.into(),
            query,
            from: 0,
            size: 10,
            sort: Vec::new(),
        }
    }

    pub fn with_pagination(mut self, from: usize, size: usize) -> Self {
        self.from = from;
        self.size = size;
        self
    }

    pub fn with_sort(mut self, field: impl Into<String>, order: EsSortOrder) -> Self {
        self.sort.push(EsSort {
            field: field.into(),
            order,
        });
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EsSort {
    pub field: String,
    pub order: EsSortOrder,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EsSortOrder {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum EsQuery {
    MatchAll,
    Term(HashMap<String, serde_json::Value>),
    Terms(HashMap<String, Vec<serde_json::Value>>),
    Range(HashMap<String, EsRangeQuery>),
    Bool(EsBoolQuery),
}

impl EsQuery {
    pub fn match_all() -> Self {
        EsQuery::MatchAll
    }

    pub fn term(field: impl Into<String>, value: serde_json::Value) -> Self {
        let mut terms = HashMap::new();
        terms.insert(field.into(), value);
        EsQuery::Term(terms)
    }

    pub fn terms(field: impl Into<String>, values: Vec<serde_json::Value>) -> Self {
        let mut terms = HashMap::new();
        terms.insert(field.into(), values);
        EsQuery::Terms(terms)
    }

    pub fn range(field: impl Into<String>, range: EsRangeQuery) -> Self {
        let mut ranges = HashMap::new();
        ranges.insert(field.into(), range);
        EsQuery::Range(ranges)
    }

    pub fn must(queries: Vec<EsQuery>) -> Self {
        EsQuery::Bool(EsBoolQuery {
            must: Some(queries),
            should: None,
            filter: None,
            must_not: None,
            minimum_should_match: None,
        })
    }

    pub fn should(queries: Vec<EsQuery>) -> Self {
        EsQuery::Bool(EsBoolQuery {
            must: None,
            should: Some(queries),
            filter: None,
            must_not: None,
            minimum_should_match: None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EsBoolQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub must: Option<Vec<EsQuery>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub should: Option<Vec<EsQuery>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<Vec<EsQuery>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub must_not: Option<Vec<EsQuery>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_should_match: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EsRangeQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gt: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gte: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lt: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lte: Option<serde_json::Value>,
}

impl EsRangeQuery {
    pub fn new() -> Self {
        Self {
            gt: None,
            gte: None,
            lt: None,
            lte: None,
        }
    }

    pub fn gt(mut self, value: serde_json::Value) -> Self {
        self.gt = Some(value);
        self
    }

    pub fn gte(mut self, value: serde_json::Value) -> Self {
        self.gte = Some(value);
        self
    }

    pub fn lt(mut self, value: serde_json::Value) -> Self {
        self.lt = Some(value);
        self
    }

    pub fn lte(mut self, value: serde_json::Value) -> Self {
        self.lte = Some(value);
        self
    }
}

impl Default for EsRangeQuery {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EsSearchResult {
    pub total: usize,
    pub hits: Vec<EsHit>,
    pub took: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EsHit {
    pub id: String,
    pub score: f64,
    pub source: serde_json::Value,
}

pub trait EsSync: Send + Sync {
    fn sync_to_es(&self, documents: Vec<EsDocument>) -> Result<EsSyncResult, EsError>;
    fn delete_from_es(&self, index: &str, ids: Vec<String>) -> Result<EsSyncResult, EsError>;
    fn search(&self, request: EsSearchRequest) -> Result<EsSearchResult, EsError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EsSyncResult {
    pub indexed: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

impl EsSyncResult {
    pub fn success(indexed: usize) -> Self {
        Self {
            indexed,
            failed: 0,
            errors: Vec::new(),
        }
    }

    pub fn with_errors(indexed: usize, errors: Vec<String>) -> Self {
        let failed = errors.len();
        Self {
            indexed,
            failed,
            errors,
        }
    }
}

pub struct EsSyncManager {
    index_mappings: HashMap<String, HashMap<String, EsFieldType>>,
    /// 后端实现（默认为 `InMemoryEsSync` Mock，可通过 `with_backend` 注入真实 ES 客户端）
    backend: Box<dyn EsSync>,
    /// Mock 后端引用（仅在 `new()` 默认构造时存在，用于支持 `count` 等 Mock 专属方法）
    mock_backend: Option<InMemoryEsSync>,
}

impl EsSyncManager {
    /// 创建默认 Manager（**使用 Mock 后端，仅供测试**）。
    ///
    /// ⚠️ **生产环境请使用 [`EsSyncManager::with_backend`] 注入真实 ES 客户端实现。**
    pub fn new() -> Self {
        let mock = InMemoryEsSync::new();
        Self {
            index_mappings: HashMap::new(),
            backend: Box::new(mock.clone()),
            mock_backend: Some(mock),
        }
    }

    /// 使用指定后端创建 Manager（用于注入真实 ES 客户端）。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use sz_orm_es::{EsSyncManager, EsSync};
    ///
    /// struct RealEsClient { /* elasticsearch::http::transport::Transport */ }
    /// impl EsSync for RealEsClient { /* ... */ }
    ///
    /// let manager = EsSyncManager::with_backend(Box::new(RealEsClient { /* ... */ }));
    /// ```
    pub fn with_backend(backend: Box<dyn EsSync>) -> Self {
        Self {
            index_mappings: HashMap::new(),
            backend,
            mock_backend: None,
        }
    }

    pub fn create_index(
        &mut self,
        index: impl Into<String>,
        mapping: HashMap<String, EsFieldType>,
    ) {
        self.index_mappings.insert(index.into(), mapping);
    }

    pub fn get_mapping(&self, index: &str) -> Option<&HashMap<String, EsFieldType>> {
        self.index_mappings.get(index)
    }

    pub fn sync_to_es(&self, documents: Vec<EsDocument>) -> Result<EsSyncResult, EsError> {
        self.backend.sync_to_es(documents)
    }

    pub fn delete_from_es(&self, index: &str, ids: Vec<String>) -> Result<EsSyncResult, EsError> {
        self.backend.delete_from_es(index, ids)
    }

    pub fn search(&self, request: EsSearchRequest) -> Result<EsSearchResult, EsError> {
        self.backend.search(request)
    }

    /// Returns the total number of documents stored in `index`.
    ///
    /// ⚠️ 仅在默认 `new()` 构造（Mock 后端）时可用；使用 `with_backend` 注入自定义后端时返回 `EsError::BackendNotSupported`。
    pub fn count(&self, index: &str) -> Result<usize, EsError> {
        match &self.mock_backend {
            Some(mock) => mock.count(index),
            None => Err(EsError::BackendNotSupported {
                operation: "count".to_string(),
                reason:
                    "current backend is not InMemoryEsSync; implement count on your EsSync impl"
                        .to_string(),
            }),
        }
    }
}

impl Default for EsSyncManager {
    fn default() -> Self {
        Self::new()
    }
}

/// In-memory `EsSync` implementation backed by a `HashMap<index, HashMap<id, EsDocument>>`.
///
/// ⚠️ **MOCK 实现——仅供单元测试与本地开发，请勿用于生产环境。**
///
/// - 数据存储在进程内 `HashMap`，重启即丢失
/// - 不支持分布式、副本、分片、持久化等任何 ES 生产特性
/// - 查询语义为简化版，不保证与真实 ES 行为一致
///
/// Suitable for unit tests and small in-process workloads.
/// `search` supports `MatchAll`, `Term` (exact match), `Terms` (membership),
/// `Range` (numeric/string bounds), and `Bool` (must/should/must_not).
///
/// `Clone` 通过 `Arc` 共享内部状态——克隆的是引用而非数据副本。
#[derive(Clone)]
pub struct InMemoryEsSync {
    documents: Arc<RwLock<HashMap<String, HashMap<String, EsDocument>>>>,
}

/// `MockEsSync` 是 [`InMemoryEsSync`] 的语义化别名，明确表达"测试用 Mock"含义。
///
/// 在新代码中推荐使用 `MockEsSync` 替代 `InMemoryEsSync`，以让"非生产可用"的含义更清晰。
pub type MockEsSync = InMemoryEsSync;

impl InMemoryEsSync {
    pub fn new() -> Self {
        Self {
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn count(&self, index: &str) -> Result<usize, EsError> {
        let docs = self
            .documents
            .read()
            .map_err(|e| EsError::SyncError(format!("lock error: {}", e)))?;
        Ok(docs.get(index).map(|m| m.len()).unwrap_or(0))
    }

    fn generate_id(doc: &EsDocument, counter: usize) -> String {
        doc.id
            .clone()
            .unwrap_or_else(|| format!("auto-{}-{}", doc.timestamp, counter))
    }

    fn matches_query(doc: &EsDocument, query: &EsQuery) -> bool {
        match query {
            EsQuery::MatchAll => true,
            EsQuery::Term(terms) => {
                if let Some(obj) = doc.source.as_object() {
                    terms.iter().all(|(k, v)| obj.get(k) == Some(v))
                } else {
                    false
                }
            }
            EsQuery::Terms(terms) => {
                let Some(obj) = doc.source.as_object() else {
                    return false;
                };
                terms.iter().all(|(k, values)| {
                    if let Some(v) = obj.get(k) {
                        values.contains(v)
                    } else {
                        false
                    }
                })
            }
            EsQuery::Range(ranges) => {
                let Some(obj) = doc.source.as_object() else {
                    return false;
                };
                ranges.iter().all(|(field, range)| match obj.get(field) {
                    Some(value) => range_match(value, range),
                    None => false,
                })
            }
            EsQuery::Bool(b) => bool_match(doc, b),
        }
    }
}

impl Default for InMemoryEsSync {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn range_match(value: &serde_json::Value, range: &EsRangeQuery) -> bool {
    match (value.as_f64(), value.as_str()) {
        (Some(num), _) => {
            if let Some(gt) = range.gt.as_ref().and_then(|v| v.as_f64()) {
                if !(num > gt) {
                    return false;
                }
            }
            if let Some(gte) = range.gte.as_ref().and_then(|v| v.as_f64()) {
                if !(num >= gte) {
                    return false;
                }
            }
            if let Some(lt) = range.lt.as_ref().and_then(|v| v.as_f64()) {
                if !(num < lt) {
                    return false;
                }
            }
            if let Some(lte) = range.lte.as_ref().and_then(|v| v.as_f64()) {
                if !(num <= lte) {
                    return false;
                }
            }
            true
        }
        (_, Some(s)) => {
            if let Some(gt) = range.gt.as_ref().and_then(|v| v.as_str()) {
                if s <= gt {
                    return false;
                }
            }
            if let Some(gte) = range.gte.as_ref().and_then(|v| v.as_str()) {
                if s < gte {
                    return false;
                }
            }
            if let Some(lt) = range.lt.as_ref().and_then(|v| v.as_str()) {
                if s >= lt {
                    return false;
                }
            }
            if let Some(lte) = range.lte.as_ref().and_then(|v| v.as_str()) {
                if s > lte {
                    return false;
                }
            }
            true
        }
        _ => false,
    }
}

fn bool_match(doc: &EsDocument, b: &EsBoolQuery) -> bool {
    if let Some(must) = &b.must {
        if !must.iter().all(|q| InMemoryEsSync::matches_query(doc, q)) {
            return false;
        }
    }
    if let Some(must_not) = &b.must_not {
        if must_not
            .iter()
            .any(|q| InMemoryEsSync::matches_query(doc, q))
        {
            return false;
        }
    }
    if let Some(filter) = &b.filter {
        if !filter.iter().all(|q| InMemoryEsSync::matches_query(doc, q)) {
            return false;
        }
    }
    if let Some(should) = &b.should {
        let matched = should
            .iter()
            .filter(|q| InMemoryEsSync::matches_query(doc, q))
            .count();
        let min_match = b.minimum_should_match.unwrap_or(1);
        if matched < min_match {
            return false;
        }
    }
    true
}

impl EsSync for InMemoryEsSync {
    fn sync_to_es(&self, documents: Vec<EsDocument>) -> Result<EsSyncResult, EsError> {
        let mut store = self
            .documents
            .write()
            .map_err(|e| EsError::SyncError(format!("lock error: {}", e)))?;
        let mut indexed = 0usize;
        let mut errors: Vec<String> = Vec::new();
        for (i, doc) in documents.into_iter().enumerate() {
            if doc.index.is_empty() {
                errors.push(format!("document {} has empty index", i));
                continue;
            }
            let id = Self::generate_id(&doc, i);
            let map = store.entry(doc.index.clone()).or_default();
            map.insert(id, doc);
            indexed += 1;
        }
        if errors.is_empty() {
            Ok(EsSyncResult::success(indexed))
        } else {
            Ok(EsSyncResult::with_errors(indexed, errors))
        }
    }

    fn delete_from_es(&self, index: &str, ids: Vec<String>) -> Result<EsSyncResult, EsError> {
        let mut store = self
            .documents
            .write()
            .map_err(|e| EsError::SyncError(format!("lock error: {}", e)))?;
        let map = store
            .get_mut(index)
            .ok_or_else(|| EsError::IndexNotFound(index.to_string()))?;
        let mut deleted = 0usize;
        let mut errors: Vec<String> = Vec::new();
        for id in ids {
            if map.remove(&id).is_some() {
                deleted += 1;
            } else {
                errors.push(format!("document not found: {}", id));
            }
        }
        if errors.is_empty() {
            Ok(EsSyncResult::success(deleted))
        } else {
            Ok(EsSyncResult::with_errors(deleted, errors))
        }
    }

    fn search(&self, request: EsSearchRequest) -> Result<EsSearchResult, EsError> {
        let store = self
            .documents
            .read()
            .map_err(|e| EsError::SyncError(format!("lock error: {}", e)))?;
        let Some(map) = store.get(&request.index) else {
            return Err(EsError::IndexNotFound(request.index.clone()));
        };

        let start = std::time::Instant::now();
        let mut hits: Vec<(String, EsDocument)> = map
            .iter()
            .filter(|(_, doc)| Self::matches_query(doc, &request.query))
            .map(|(id, doc)| (id.clone(), doc.clone()))
            .collect();

        // Sort by fields in `request.sort`.
        for sort in request.sort.iter().rev() {
            let field = &sort.field;
            let asc = matches!(sort.order, EsSortOrder::Asc);
            hits.sort_by(|a, b| {
                let av =
                    a.1.source
                        .get(field)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                let bv =
                    b.1.source
                        .get(field)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                let ord = compare_values(&av, &bv);
                if asc {
                    ord
                } else {
                    ord.reverse()
                }
            });
        }

        let total = hits.len();
        let paged = hits
            .into_iter()
            .skip(request.from)
            .take(request.size)
            .map(|(id, doc)| EsHit {
                id,
                score: 1.0,
                source: doc.source,
            })
            .collect::<Vec<_>>();

        Ok(EsSearchResult {
            total,
            hits: paged,
            took: start.elapsed().as_millis() as i64,
        })
    }
}

fn compare_values(a: &serde_json::Value, b: &serde_json::Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        _ => a.to_string().cmp(&b.to_string()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EsFieldType {
    Text,
    Keyword,
    Integer,
    Long,
    Float,
    Double,
    Boolean,
    Date,
    Object,
    Nested,
    Ip,
    GeoPoint,
}

#[derive(Debug)]
pub enum EsError {
    ConnectionFailed(String),
    IndexNotFound(String),
    DocumentNotFound(String),
    MappingError(String),
    QueryError(String),
    SyncError(String),
    /// 当前后端不支持的操作（如 `EsSyncManager::with_backend` 注入自定义后端后调用 Mock 专属方法）
    BackendNotSupported {
        operation: String,
        reason: String,
    },
}

impl std::fmt::Display for EsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EsError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            EsError::IndexNotFound(idx) => write!(f, "Index not found: {}", idx),
            EsError::DocumentNotFound(id) => write!(f, "Document not found: {}", id),
            EsError::MappingError(msg) => write!(f, "Mapping error: {}", msg),
            EsError::QueryError(msg) => write!(f, "Query error: {}", msg),
            EsError::SyncError(msg) => write!(f, "Sync error: {}", msg),
            EsError::BackendNotSupported { operation, reason } => {
                write!(f, "Backend not supported: {} ({})", operation, reason)
            }
        }
    }
}

impl std::error::Error for EsError {}

impl serde::Serialize for EsError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_es_document_new() {
        let doc = EsDocument::new("test-index", serde_json::json!({"name": "test"}));
        assert_eq!(doc.index, "test-index");
        assert_eq!(doc.source["name"], "test");
        assert!(doc.id.is_none());
    }

    #[test]
    fn test_es_document_with_id() {
        let doc = EsDocument::new("test-index", serde_json::json!({})).with_id("doc1");
        assert_eq!(doc.id, Some("doc1".to_string()));
    }

    #[test]
    fn test_es_search_request_new() {
        let query = EsQuery::match_all();
        let request = EsSearchRequest::new("test-index", query);
        assert_eq!(request.index, "test-index");
        assert_eq!(request.from, 0);
        assert_eq!(request.size, 10);
    }

    #[test]
    fn test_es_search_request_pagination() {
        let query = EsQuery::match_all();
        let request = EsSearchRequest::new("test", query).with_pagination(20, 50);
        assert_eq!(request.from, 20);
        assert_eq!(request.size, 50);
    }

    #[test]
    fn test_es_search_request_sort() {
        let query = EsQuery::match_all();
        let request = EsSearchRequest::new("test", query).with_sort("date", EsSortOrder::Desc);
        assert_eq!(request.sort.len(), 1);
        assert_eq!(request.sort[0].field, "date");
        assert_eq!(request.sort[0].order, EsSortOrder::Desc);
    }

    #[test]
    fn test_es_query_match_all() {
        let query = EsQuery::match_all();
        assert_eq!(query, EsQuery::MatchAll);
    }

    #[test]
    fn test_es_query_term() {
        let query = EsQuery::term("status", serde_json::json!("active"));
        assert_eq!(
            query,
            EsQuery::Term(std::collections::HashMap::from([(
                "status".to_string(),
                serde_json::json!("active")
            )]))
        );
    }

    #[test]
    fn test_es_query_terms() {
        let query = EsQuery::terms("tags", vec![serde_json::json!("a"), serde_json::json!("b")]);
        assert!(matches!(query, EsQuery::Terms(_)));
    }

    #[test]
    fn test_es_query_range() {
        let range = EsRangeQuery::new().gte(serde_json::json!(100));
        let query = EsQuery::range("price", range);
        assert!(matches!(query, EsQuery::Range(_)));
    }

    #[test]
    fn test_es_query_bool_must() {
        let queries = vec![
            EsQuery::term("status", serde_json::json!("active")),
            EsQuery::term("type", serde_json::json!("post")),
        ];
        let query = EsQuery::must(queries);
        assert!(matches!(query, EsQuery::Bool(_)));
    }

    #[test]
    fn test_es_range_query() {
        let range = EsRangeQuery::new()
            .gte(serde_json::json!(0))
            .lt(serde_json::json!(100));

        assert!(range.gte.is_some());
        assert!(range.lt.is_some());
    }

    #[test]
    fn test_es_sync_result_success() {
        let result = EsSyncResult::success(10);
        assert_eq!(result.indexed, 10);
        assert_eq!(result.failed, 0);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_es_sync_result_with_errors() {
        let errors = vec!["error1".to_string(), "error2".to_string()];
        let result = EsSyncResult::with_errors(8, errors.clone());
        assert_eq!(result.indexed, 8);
        assert_eq!(result.failed, 2);
        assert_eq!(result.errors, errors);
    }

    #[test]
    fn test_es_sync_manager_new() {
        let manager = EsSyncManager::new();
        assert!(manager.index_mappings.is_empty());
    }

    #[test]
    fn test_es_sync_manager_create_index() {
        let mut manager = EsSyncManager::new();
        let mut mapping = HashMap::new();
        mapping.insert("title".to_string(), EsFieldType::Text);
        mapping.insert("count".to_string(), EsFieldType::Integer);

        manager.create_index("test-index", mapping);

        let retrieved = manager.get_mapping("test-index");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().len(), 2);
    }

    #[test]
    fn test_es_sync_manager_get_mapping_not_found() {
        let manager = EsSyncManager::new();
        let mapping = manager.get_mapping("nonexistent");
        assert!(mapping.is_none());
    }

    #[test]
    fn test_inmemory_sync_and_count() {
        let sync = InMemoryEsSync::new();
        let docs = vec![
            EsDocument::new("users", serde_json::json!({"name": "alice"})).with_id("u1"),
            EsDocument::new("users", serde_json::json!({"name": "bob"})).with_id("u2"),
        ];
        let result = sync.sync_to_es(docs).unwrap();
        assert_eq!(result.indexed, 2);
        assert_eq!(sync.count("users").unwrap(), 2);
    }

    #[test]
    fn test_inmemory_sync_replaces_by_id() {
        let sync = InMemoryEsSync::new();
        sync.sync_to_es(vec![
            EsDocument::new("idx", serde_json::json!({"v": 1})).with_id("d1")
        ])
        .unwrap();
        sync.sync_to_es(vec![
            EsDocument::new("idx", serde_json::json!({"v": 2})).with_id("d1")
        ])
        .unwrap();
        assert_eq!(sync.count("idx").unwrap(), 1);

        let req = EsSearchRequest::new("idx", EsQuery::match_all());
        let res = sync.search(req).unwrap();
        assert_eq!(res.hits[0].source["v"], 2);
    }

    #[test]
    fn test_inmemory_sync_rejects_empty_index() {
        let sync = InMemoryEsSync::new();
        let result = sync
            .sync_to_es(vec![EsDocument::new("", serde_json::json!({}))])
            .unwrap();
        assert_eq!(result.indexed, 0);
        assert_eq!(result.failed, 1);
    }

    #[test]
    fn test_inmemory_delete_real() {
        let sync = InMemoryEsSync::new();
        sync.sync_to_es(vec![
            EsDocument::new("i", serde_json::json!({"k": "a"})).with_id("1"),
            EsDocument::new("i", serde_json::json!({"k": "b"})).with_id("2"),
        ])
        .unwrap();

        let result = sync.delete_from_es("i", vec!["1".to_string()]).unwrap();
        assert_eq!(result.indexed, 1);
        assert_eq!(sync.count("i").unwrap(), 1);

        let err = sync.delete_from_es("missing", vec!["x".to_string()]);
        assert!(matches!(err, Err(EsError::IndexNotFound(_))));
    }

    #[test]
    fn test_inmemory_delete_missing_records_error() {
        let sync = InMemoryEsSync::new();
        sync.sync_to_es(vec![
            EsDocument::new("i", serde_json::json!({"k": "a"})).with_id("1")
        ])
        .unwrap();
        let r = sync
            .delete_from_es("i", vec!["1".to_string(), "999".to_string()])
            .unwrap();
        assert_eq!(r.indexed, 1);
        assert_eq!(r.failed, 1);
    }

    #[test]
    fn test_inmemory_search_match_all() {
        let sync = InMemoryEsSync::new();
        sync.sync_to_es(vec![
            EsDocument::new("i", serde_json::json!({"n": 1})).with_id("1"),
            EsDocument::new("i", serde_json::json!({"n": 2})).with_id("2"),
        ])
        .unwrap();
        let req = EsSearchRequest::new("i", EsQuery::match_all());
        let res = sync.search(req).unwrap();
        assert_eq!(res.total, 2);
        assert_eq!(res.hits.len(), 2);
    }

    #[test]
    fn test_inmemory_search_term_filter() {
        let sync = InMemoryEsSync::new();
        sync.sync_to_es(vec![
            EsDocument::new("i", serde_json::json!({"status": "active"})).with_id("1"),
            EsDocument::new("i", serde_json::json!({"status": "inactive"})).with_id("2"),
        ])
        .unwrap();
        let req = EsSearchRequest::new("i", EsQuery::term("status", serde_json::json!("active")));
        let res = sync.search(req).unwrap();
        assert_eq!(res.total, 1);
        assert_eq!(res.hits[0].source["status"], "active");
    }

    #[test]
    fn test_inmemory_search_terms_filter() {
        let sync = InMemoryEsSync::new();
        sync.sync_to_es(vec![
            EsDocument::new("i", serde_json::json!({"tag": "a"})).with_id("1"),
            EsDocument::new("i", serde_json::json!({"tag": "b"})).with_id("2"),
            EsDocument::new("i", serde_json::json!({"tag": "c"})).with_id("3"),
        ])
        .unwrap();
        let req = EsSearchRequest::new(
            "i",
            EsQuery::terms("tag", vec![serde_json::json!("a"), serde_json::json!("c")]),
        );
        let res = sync.search(req).unwrap();
        assert_eq!(res.total, 2);
    }

    #[test]
    fn test_inmemory_search_range_filter() {
        let sync = InMemoryEsSync::new();
        sync.sync_to_es(vec![
            EsDocument::new("i", serde_json::json!({"age": 10})).with_id("1"),
            EsDocument::new("i", serde_json::json!({"age": 20})).with_id("2"),
            EsDocument::new("i", serde_json::json!({"age": 30})).with_id("3"),
        ])
        .unwrap();
        let req = EsSearchRequest::new(
            "i",
            EsQuery::range("age", EsRangeQuery::new().gte(serde_json::json!(20))),
        );
        let res = sync.search(req).unwrap();
        assert_eq!(res.total, 2);
    }

    #[test]
    fn test_inmemory_search_bool_query() {
        let sync = InMemoryEsSync::new();
        sync.sync_to_es(vec![
            EsDocument::new("i", serde_json::json!({"status": "active", "n": 5})).with_id("1"),
            EsDocument::new("i", serde_json::json!({"status": "active", "n": 50})).with_id("2"),
            EsDocument::new("i", serde_json::json!({"status": "inactive", "n": 5})).with_id("3"),
        ])
        .unwrap();
        // status=active AND n>=10
        let bool = EsQuery::must(vec![
            EsQuery::term("status", serde_json::json!("active")),
            EsQuery::range("n", EsRangeQuery::new().gte(serde_json::json!(10))),
        ]);
        let req = EsSearchRequest::new("i", bool);
        let res = sync.search(req).unwrap();
        assert_eq!(res.total, 1);
        assert_eq!(res.hits[0].source["n"], 50);
    }

    #[test]
    fn test_inmemory_search_pagination_and_sort() {
        let sync = InMemoryEsSync::new();
        let docs: Vec<EsDocument> = (1..=5)
            .map(|i| EsDocument::new("i", serde_json::json!({"v": i})).with_id(format!("d{}", i)))
            .collect();
        sync.sync_to_es(docs).unwrap();

        let req = EsSearchRequest::new("i", EsQuery::match_all())
            .with_pagination(0, 2)
            .with_sort("v", EsSortOrder::Desc);
        let res = sync.search(req).unwrap();
        assert_eq!(res.total, 5);
        assert_eq!(res.hits.len(), 2);
        // Descending sort means the highest values come first.
        assert_eq!(res.hits[0].source["v"], 5);
        assert_eq!(res.hits[1].source["v"], 4);

        // Now test pagination: skip the first, take the next two.
        let req2 = EsSearchRequest::new("i", EsQuery::match_all())
            .with_pagination(1, 2)
            .with_sort("v", EsSortOrder::Desc);
        let res2 = sync.search(req2).unwrap();
        assert_eq!(res2.hits.len(), 2);
        assert_eq!(res2.hits[0].source["v"], 4);
        assert_eq!(res2.hits[1].source["v"], 3);

        // Ascending order should reverse the result.
        let req3 = EsSearchRequest::new("i", EsQuery::match_all())
            .with_pagination(0, 2)
            .with_sort("v", EsSortOrder::Asc);
        let res3 = sync.search(req3).unwrap();
        assert_eq!(res3.hits[0].source["v"], 1);
        assert_eq!(res3.hits[1].source["v"], 2);
    }

    #[test]
    fn test_inmemory_search_missing_index_errors() {
        let sync = InMemoryEsSync::new();
        let req = EsSearchRequest::new("missing", EsQuery::match_all());
        let err = sync.search(req);
        assert!(matches!(err, Err(EsError::IndexNotFound(_))));
    }

    #[test]
    fn test_sync_manager_delegates_to_backend() {
        let mut manager = EsSyncManager::new();
        manager.create_index("idx", HashMap::new());

        let r = manager
            .sync_to_es(vec![
                EsDocument::new("idx", serde_json::json!({"k": "v"})).with_id("d1")
            ])
            .unwrap();
        assert_eq!(r.indexed, 1);
        assert_eq!(manager.count("idx").unwrap(), 1);

        let req = EsSearchRequest::new("idx", EsQuery::match_all());
        let res = manager.search(req).unwrap();
        assert_eq!(res.total, 1);

        let r = manager
            .delete_from_es("idx", vec!["d1".to_string()])
            .unwrap();
        assert_eq!(r.indexed, 1);
        assert_eq!(manager.count("idx").unwrap(), 0);
    }

    #[test]
    fn test_es_sync_trait_object_dyn() {
        // Ensures the trait is object-safe and usable via dynamic dispatch.
        let sync: Box<dyn EsSync> = Box::new(InMemoryEsSync::new());
        sync.sync_to_es(vec![
            EsDocument::new("i", serde_json::json!({"k": 1})).with_id("1")
        ])
        .unwrap();
        let req = EsSearchRequest::new("i", EsQuery::match_all());
        let res = sync.search(req).unwrap();
        assert_eq!(res.total, 1);
    }

    /// 验证：`with_backend` 注入自定义后端后，`count` 应返回 `BackendNotSupported`
    /// （因为 mock_backend 为 None）。
    ///
    /// 这确保用户在注入真实 ES 客户端时，不会被静默地回退到 Mock 行为。
    #[test]
    fn test_with_backend_count_returns_not_supported() {
        // 一个假后端：sync_to_es / delete_from_es / search 都返回 Ok（空结果）
        struct StubBackend;
        impl EsSync for StubBackend {
            fn sync_to_es(&self, _documents: Vec<EsDocument>) -> Result<EsSyncResult, EsError> {
                Ok(EsSyncResult::success(0))
            }
            fn delete_from_es(
                &self,
                _index: &str,
                _ids: Vec<String>,
            ) -> Result<EsSyncResult, EsError> {
                Ok(EsSyncResult::success(0))
            }
            fn search(&self, _request: EsSearchRequest) -> Result<EsSearchResult, EsError> {
                Ok(EsSearchResult {
                    total: 0,
                    hits: Vec::new(),
                    took: 0,
                })
            }
        }

        let manager = EsSyncManager::with_backend(Box::new(StubBackend));
        let result = manager.count("any_index");
        assert!(
            matches!(result, Err(EsError::BackendNotSupported { .. })),
            "with_backend 注入自定义后端后 count 应返回 BackendNotSupported，实际: {:?}",
            result
        );

        // 但 sync_to_es / delete / search 应正常委托给自定义后端
        let sync_result = manager
            .sync_to_es(vec![EsDocument::new("i", serde_json::json!({}))])
            .unwrap();
        assert_eq!(sync_result.indexed, 0);
    }

    /// 验证：默认 `new()` 构造的 Manager 仍可使用 `count`（Mock 后端可用）
    #[test]
    fn test_default_new_count_works_with_mock() {
        let manager = EsSyncManager::new();
        // 空索引应返回 0 而非错误
        let count = manager.count("empty_idx").unwrap();
        assert_eq!(count, 0);
    }

    /// 验证：`MockEsSync` 别名与 `InMemoryEsSync` 等价
    #[test]
    fn test_mock_es_sync_alias_equivalence() {
        let mock: MockEsSync = MockEsSync::new();
        let _: InMemoryEsSync = mock; // 类型应等价
        let mock2 = InMemoryEsSync::new();
        let _clone: InMemoryEsSync = mock2.clone(); // Clone 应可用
    }
}
