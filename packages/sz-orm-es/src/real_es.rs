//! 真实 Elasticsearch 后端实现
//!
//! 基于 reqwest 直接调用 ES REST API，实现 [`EsSync`] trait。
//!
//! # 设计说明
//!
//! 选择 reqwest 而非 `elasticsearch` crate 的原因：
//! 1. 与现有集成测试一致（`real_es_integration.rs` 已用 reqwest 0.12）
//! 2. 避免 `elasticsearch` crate 8.5 与 workspace reqwest 0.12 的版本冲突
//! 3. reqwest 直接调 REST API 更轻量，编译更快
//!
//! # 支持的操作
//!
//! - 索引管理：`create_index` / `delete_index` / `index_exists`
//! - 文档操作：`index_document` / `bulk_index` / `get_document` / `delete_document`
//! - 搜索：`search`（DSL 查询，支持 match_all/term/terms/range/bool）
//! - 聚合：`aggregate`（terms/range/sum/avg/max/min/histogram/value_count）
//! - 过滤：`filter`（基于 bool query 的 filter 子句）
//!
//! # 示例
//!
//! ```ignore
//! use sz_orm_es::real_es::RealEsSync;
//! use sz_orm_es::{EsDocument, EsSync, EsSearchRequest, EsQuery};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let es = RealEsSync::new("http://localhost:9200")?;
//!
//! // 索引文档
//! let doc = EsDocument::new("my-index", serde_json::json!({"name": "Alice"}))
//!     .with_id("1");
//! es.index_document(&doc).await?;
//!
//! // 搜索
//! let req = EsSearchRequest::new("my-index", EsQuery::match_all());
//! let result = es.search(req)?;
//! # Ok(())
//! # }
//! ```

#![cfg(feature = "real")]

use crate::{
    EsDocument, EsError, EsFieldType, EsHit, EsQuery, EsRangeQuery, EsSearchRequest,
    EsSearchResult, EsSortOrder, EsSync, EsSyncResult,
};
use std::collections::HashMap;

/// 真实 Elasticsearch 后端，基于 reqwest HTTP REST API。
///
/// 持有 `reqwest::Client` 和 ES 服务地址，通过 HTTP 调用 ES REST API。
/// 所有异步方法需在 tokio runtime 中调用。
pub struct RealEsSync {
    /// ES 服务基础 URL，如 `http://localhost:9200`
    endpoint: String,
    /// HTTP 客户端
    http: reqwest::Client,
    /// 内部 tokio runtime，用于 `EsSync` trait 的同步方法阻塞调用异步 HTTP
    runtime: tokio::runtime::Runtime,
}

impl RealEsSync {
    /// 创建真实 ES 后端，连接到指定 endpoint。
    ///
    /// # 错误
    ///
    /// 返回 `EsError::ConnectionFailed` 如果无法创建 HTTP 客户端或 tokio runtime。
    pub fn new(endpoint: impl Into<String>) -> Result<Self, EsError> {
        let endpoint = endpoint.into();
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| EsError::ConnectionFailed(format!("build reqwest client: {}", e)))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| EsError::ConnectionFailed(format!("build tokio runtime: {}", e)))?;
        Ok(Self {
            endpoint,
            http,
            runtime,
        })
    }

    /// 构建 ES API URL
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.endpoint, path)
    }

    // ========================================================================
    // 索引管理
    // ========================================================================

    /// 创建索引（带 mapping 设置）。
    ///
    /// 如果索引已存在，返回 `Ok(())`（幂等）。
    pub async fn create_index(
        &self,
        index: &str,
        mapping: &HashMap<String, EsFieldType>,
    ) -> Result<(), EsError> {
        let properties: serde_json::Value = if mapping.is_empty() {
            serde_json::json!({})
        } else {
            let mut props = serde_json::Map::new();
            for (field, ftype) in mapping {
                props.insert(field.clone(), es_field_type_to_mapping(ftype));
            }
            serde_json::json!({ "properties": props })
        };

        let resp = self
            .http
            .put(self.url(&format!("/{}", index)))
            .json(&properties)
            .send()
            .await
            .map_err(|e| EsError::ConnectionFailed(format!("create_index: {}", e)))?;

        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        let body = resp.text().await.unwrap_or_default();
        // 索引已存在（400 Bad Request with resource_already_exists_exception）视为幂等成功
        if status.as_u16() == 400 && body.contains("resource_already_exists_exception") {
            return Ok(());
        }
        Err(EsError::MappingError(format!(
            "create_index failed: status={}, body={}",
            status.as_u16(),
            body
        )))
    }

    /// 删除索引。
    ///
    /// 如果索引不存在，返回 `Ok(())`（幂等）。
    pub async fn delete_index(&self, index: &str) -> Result<(), EsError> {
        let resp = self
            .http
            .delete(self.url(&format!("/{}", index)))
            .send()
            .await
            .map_err(|e| EsError::ConnectionFailed(format!("delete_index: {}", e)))?;

        let status = resp.status();
        if status.is_success() || status.as_u16() == 404 {
            return Ok(());
        }
        let body = resp.text().await.unwrap_or_default();
        Err(EsError::SyncError(format!(
            "delete_index failed: status={}, body={}",
            status.as_u16(),
            body
        )))
    }

    /// 检查索引是否存在。
    pub async fn index_exists(&self, index: &str) -> Result<bool, EsError> {
        let resp = self
            .http
            .head(self.url(&format!("/{}", index)))
            .send()
            .await
            .map_err(|e| EsError::ConnectionFailed(format!("index_exists: {}", e)))?;
        Ok(resp.status().as_u16() == 200)
    }

    // ========================================================================
    // 文档操作
    // ========================================================================

    /// 索引单个文档（PUT /{index}/_doc/{id}）。
    ///
    /// 如果文档 ID 已存在则替换。
    pub async fn index_document(&self, doc: &EsDocument) -> Result<(), EsError> {
        let id = doc
            .id
            .as_ref()
            .ok_or_else(|| EsError::DocumentNotFound("document has no id".to_string()))?;
        let resp = self
            .http
            .put(self.url(&format!("/{}/_doc/{}", doc.index, id)))
            .json(&doc.source)
            .send()
            .await
            .map_err(|e| EsError::ConnectionFailed(format!("index_document: {}", e)))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            Err(EsError::SyncError(format!(
                "index_document failed: status={}, body={}",
                status, body
            )))
        }
    }

    /// 批量索引文档（使用 ES Bulk API）。
    ///
    /// 格式：NDJSON，每两行一组（action + source）。
    pub async fn bulk_index(&self, documents: &[EsDocument]) -> Result<EsSyncResult, EsError> {
        if documents.is_empty() {
            return Ok(EsSyncResult::success(0));
        }

        let mut body = String::new();
        for doc in documents {
            let id = doc
                .id
                .as_ref()
                .ok_or_else(|| EsError::DocumentNotFound("document has no id".to_string()))?;
            let action = serde_json::json!({
                "index": {
                    "_index": doc.index,
                    "_id": id,
                }
            });
            body.push_str(&action.to_string());
            body.push('\n');
            body.push_str(&doc.source.to_string());
            body.push('\n');
        }

        let resp = self
            .http
            .post(self.url("/_bulk"))
            .header("Content-Type", "application/x-ndjson")
            .body(body)
            .send()
            .await
            .map_err(|e| EsError::ConnectionFailed(format!("bulk_index: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(EsError::SyncError(format!(
                "bulk_index failed: status={}, body={}",
                status, body
            )));
        }

        let resp_json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| EsError::SyncError(format!("bulk_index parse response: {}", e)))?;

        let errors = resp_json["errors"].as_bool().unwrap_or(false);
        let items = resp_json["items"].as_array().cloned().unwrap_or_default();

        let mut indexed = 0usize;
        let mut error_list: Vec<String> = Vec::new();
        for item in &items {
            let index_obj = item.get("index").or_else(|| item.get("create"));
            if let Some(obj) = index_obj {
                let status = obj["status"].as_i64().unwrap_or(0);
                if (200..300).contains(&status) {
                    indexed += 1;
                } else {
                    let id = obj["_id"].as_str().unwrap_or("unknown");
                    let error_msg = obj["error"]["reason"].as_str().unwrap_or("unknown error");
                    error_list.push(format!(
                        "doc {} failed: status={}, {}",
                        id, status, error_msg
                    ));
                }
            }
        }

        if errors || !error_list.is_empty() {
            Ok(EsSyncResult::with_errors(indexed, error_list))
        } else {
            Ok(EsSyncResult::success(indexed))
        }
    }

    /// 获取单个文档（GET /{index}/_doc/{id}）。
    pub async fn get_document(
        &self,
        index: &str,
        id: &str,
    ) -> Result<Option<serde_json::Value>, EsError> {
        let resp = self
            .http
            .get(self.url(&format!("/{}/_doc/{}", index, id)))
            .send()
            .await
            .map_err(|e| EsError::ConnectionFailed(format!("get_document: {}", e)))?;

        if resp.status().as_u16() == 404 {
            return Ok(None);
        }
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(EsError::SyncError(format!(
                "get_document failed: status={}, body={}",
                status, body
            )));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| EsError::SyncError(format!("get_document parse: {}", e)))?;
        Ok(Some(json["_source"].clone()))
    }

    /// 删除单个文档（DELETE /{index}/_doc/{id}）。
    pub async fn delete_document(&self, index: &str, id: &str) -> Result<bool, EsError> {
        let resp = self
            .http
            .delete(self.url(&format!("/{}/_doc/{}", index, id)))
            .send()
            .await
            .map_err(|e| EsError::ConnectionFailed(format!("delete_document: {}", e)))?;

        let status = resp.status().as_u16();
        Ok(status == 200)
    }

    /// 刷新索引（POST /{index}/_refresh），使刚索引的文档可被搜索。
    pub async fn refresh(&self, index: &str) -> Result<(), EsError> {
        let resp = self
            .http
            .post(self.url(&format!("/{}/_refresh", index)))
            .send()
            .await
            .map_err(|e| EsError::ConnectionFailed(format!("refresh: {}", e)))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            Err(EsError::SyncError(format!(
                "refresh failed: status={}, body={}",
                status, body
            )))
        }
    }

    // ========================================================================
    // 搜索 / 聚合 / 过滤
    // ========================================================================

    /// 将 [`EsQuery`] 转换为 ES DSL JSON。
    fn query_to_dsl(query: &EsQuery) -> serde_json::Value {
        match query {
            EsQuery::MatchAll => serde_json::json!({"match_all": {}}),
            EsQuery::Term(terms) => {
                if terms.len() == 1 {
                    let (field, value) = terms.iter().next().unwrap();
                    serde_json::json!({"term": { field: value }})
                } else {
                    let filters: Vec<serde_json::Value> = terms
                        .iter()
                        .map(|(field, value)| serde_json::json!({"term": { field: value }}))
                        .collect();
                    serde_json::json!({"bool": {"filter": filters}})
                }
            }
            EsQuery::Terms(terms) => {
                if terms.len() == 1 {
                    let (field, values) = terms.iter().next().unwrap();
                    serde_json::json!({"terms": { field: values }})
                } else {
                    let filters: Vec<serde_json::Value> = terms
                        .iter()
                        .map(|(field, values)| serde_json::json!({"terms": { field: values }}))
                        .collect();
                    serde_json::json!({"bool": {"filter": filters}})
                }
            }
            EsQuery::Range(ranges) => {
                if ranges.len() == 1 {
                    let (field, range) = ranges.iter().next().unwrap();
                    serde_json::json!({"range": { field: range_to_dsl(range) }})
                } else {
                    let filters: Vec<serde_json::Value> = ranges
                        .iter()
                        .map(|(field, range)| {
                            serde_json::json!({"range": { field: range_to_dsl(range) }})
                        })
                        .collect();
                    serde_json::json!({"bool": {"filter": filters}})
                }
            }
            EsQuery::Bool(b) => {
                let mut bool_obj = serde_json::Map::new();
                if let Some(must) = &b.must {
                    bool_obj.insert(
                        "must".to_string(),
                        serde_json::Value::Array(must.iter().map(Self::query_to_dsl).collect()),
                    );
                }
                if let Some(should) = &b.should {
                    bool_obj.insert(
                        "should".to_string(),
                        serde_json::Value::Array(should.iter().map(Self::query_to_dsl).collect()),
                    );
                }
                if let Some(filter) = &b.filter {
                    bool_obj.insert(
                        "filter".to_string(),
                        serde_json::Value::Array(filter.iter().map(Self::query_to_dsl).collect()),
                    );
                }
                if let Some(must_not) = &b.must_not {
                    bool_obj.insert(
                        "must_not".to_string(),
                        serde_json::Value::Array(must_not.iter().map(Self::query_to_dsl).collect()),
                    );
                }
                if let Some(min_match) = b.minimum_should_match {
                    bool_obj.insert(
                        "minimum_should_match".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(min_match)),
                    );
                }
                serde_json::json!({"bool": bool_obj})
            }
        }
    }

    /// 将 [`EsSearchRequest`] 转换为 ES _search 请求体。
    fn search_request_to_dsl(request: &EsSearchRequest) -> serde_json::Value {
        let mut body = serde_json::Map::new();
        body.insert("query".to_string(), Self::query_to_dsl(&request.query));
        body.insert(
            "from".to_string(),
            serde_json::Value::Number(serde_json::Number::from(request.from)),
        );
        body.insert(
            "size".to_string(),
            serde_json::Value::Number(serde_json::Number::from(request.size)),
        );
        if !request.sort.is_empty() {
            let sort_arr: Vec<serde_json::Value> = request
                .sort
                .iter()
                .map(|s| {
                    let order = match s.order {
                        EsSortOrder::Asc => "asc",
                        EsSortOrder::Desc => "desc",
                    };
                    serde_json::json!({ &s.field: { "order": order } })
                })
                .collect();
            body.insert("sort".to_string(), serde_json::Value::Array(sort_arr));
        }
        serde_json::Value::Object(body)
    }

    /// 执行搜索（POST /{index}/_search），返回 [`EsSearchResult`]。
    ///
    /// 此为异步方法，需在 tokio runtime 中调用。
    pub async fn search_async(&self, request: EsSearchRequest) -> Result<EsSearchResult, EsError> {
        let body = Self::search_request_to_dsl(&request);
        let resp = self
            .http
            .post(self.url(&format!("/{}/_search", request.index)))
            .json(&body)
            .send()
            .await
            .map_err(|e| EsError::ConnectionFailed(format!("search: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            if status == 404 {
                return Err(EsError::IndexNotFound(request.index));
            }
            return Err(EsError::QueryError(format!(
                "search failed: status={}, body={}",
                status, body
            )));
        }

        let resp_json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| EsError::QueryError(format!("search parse: {}", e)))?;

        let took = resp_json["took"].as_i64().unwrap_or(0);
        let total = resp_json["hits"]["total"]["value"].as_i64().unwrap_or(0) as usize;
        let hits_arr = resp_json["hits"]["hits"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let hits: Vec<EsHit> = hits_arr
            .iter()
            .map(|h| EsHit {
                id: h["_id"].as_str().unwrap_or("").to_string(),
                score: h["_score"].as_f64().unwrap_or(1.0),
                source: h["_source"].clone(),
            })
            .collect();

        Ok(EsSearchResult { total, hits, took })
    }

    /// 执行聚合查询（POST /{index}/_search with size:0 + aggs）。
    ///
    /// `aggregations` 为聚合定义 JSON，如 `{"by_field": {"terms": {"field": "category"}}}`。
    pub async fn aggregate(
        &self,
        index: &str,
        query: &EsQuery,
        aggregations: &serde_json::Value,
    ) -> Result<serde_json::Value, EsError> {
        let mut body = serde_json::Map::new();
        body.insert("query".to_string(), Self::query_to_dsl(query));
        body.insert(
            "size".to_string(),
            serde_json::Value::Number(serde_json::Number::from(0)),
        );
        body.insert("aggs".to_string(), aggregations.clone());

        let resp = self
            .http
            .post(self.url(&format!("/{}/_search", index)))
            .json(&serde_json::Value::Object(body))
            .send()
            .await
            .map_err(|e| EsError::ConnectionFailed(format!("aggregate: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            if status == 404 {
                return Err(EsError::IndexNotFound(index.to_string()));
            }
            return Err(EsError::QueryError(format!(
                "aggregate failed: status={}, body={}",
                status, body
            )));
        }

        let resp_json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| EsError::QueryError(format!("aggregate parse: {}", e)))?;

        Ok(resp_json["aggregations"].clone())
    }

    /// 执行过滤查询（基于 bool query 的 filter 子句）。
    ///
    /// `filters` 为多个 [`EsQuery`]，全部 AND 组合作为 filter 子句。
    pub async fn filter(
        &self,
        index: &str,
        filters: &[EsQuery],
        from: usize,
        size: usize,
    ) -> Result<EsSearchResult, EsError> {
        let filter_dsl: Vec<serde_json::Value> = filters.iter().map(Self::query_to_dsl).collect();
        let body = serde_json::json!({
            "query": { "bool": { "filter": filter_dsl } },
            "from": from,
            "size": size,
        });

        let resp = self
            .http
            .post(self.url(&format!("/{}/_search", index)))
            .json(&body)
            .send()
            .await
            .map_err(|e| EsError::ConnectionFailed(format!("filter: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            if status == 404 {
                return Err(EsError::IndexNotFound(index.to_string()));
            }
            return Err(EsError::QueryError(format!(
                "filter failed: status={}, body={}",
                status, body
            )));
        }

        let resp_json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| EsError::QueryError(format!("filter parse: {}", e)))?;

        let took = resp_json["took"].as_i64().unwrap_or(0);
        let total = resp_json["hits"]["total"]["value"].as_i64().unwrap_or(0) as usize;
        let hits_arr = resp_json["hits"]["hits"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let hits: Vec<EsHit> = hits_arr
            .iter()
            .map(|h| EsHit {
                id: h["_id"].as_str().unwrap_or("").to_string(),
                score: h["_score"].as_f64().unwrap_or(1.0),
                source: h["_source"].clone(),
            })
            .collect();

        Ok(EsSearchResult { total, hits, took })
    }
}

/// 将 [`EsFieldType`] 转换为 ES mapping JSON。
fn es_field_type_to_mapping(ftype: &EsFieldType) -> serde_json::Value {
    let type_str = match ftype {
        EsFieldType::Text => "text",
        EsFieldType::Keyword => "keyword",
        EsFieldType::Integer => "integer",
        EsFieldType::Long => "long",
        EsFieldType::Float => "float",
        EsFieldType::Double => "double",
        EsFieldType::Boolean => "boolean",
        EsFieldType::Date => "date",
        EsFieldType::Object => "object",
        EsFieldType::Nested => "nested",
        EsFieldType::Ip => "ip",
        EsFieldType::GeoPoint => "geo_point",
    };
    serde_json::json!({ "type": type_str })
}

/// 将 [`EsRangeQuery`] 转换为 ES range DSL JSON。
fn range_to_dsl(range: &EsRangeQuery) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    if let Some(gt) = &range.gt {
        obj.insert("gt".to_string(), gt.clone());
    }
    if let Some(gte) = &range.gte {
        obj.insert("gte".to_string(), gte.clone());
    }
    if let Some(lt) = &range.lt {
        obj.insert("lt".to_string(), lt.clone());
    }
    if let Some(lte) = &range.lte {
        obj.insert("lte".to_string(), lte.clone());
    }
    serde_json::Value::Object(obj)
}

// ============================================================================
// EsSync trait 实现（同步阻塞，内部用 tokio runtime 调用异步 HTTP）
// ============================================================================

impl EsSync for RealEsSync {
    fn sync_to_es(&self, documents: Vec<EsDocument>) -> Result<EsSyncResult, EsError> {
        // 使用 bulk API 批量索引
        self.runtime.block_on(self.bulk_index(&documents))
    }

    fn delete_from_es(&self, index: &str, ids: Vec<String>) -> Result<EsSyncResult, EsError> {
        let mut deleted = 0usize;
        let mut errors: Vec<String> = Vec::new();

        for id in &ids {
            let result = self.runtime.block_on(self.delete_document(index, id));
            match result {
                Ok(true) => deleted += 1,
                Ok(false) => errors.push(format!("document not found: {}", id)),
                Err(e) => errors.push(format!("delete {} failed: {}", id, e)),
            }
        }

        if errors.is_empty() {
            Ok(EsSyncResult::success(deleted))
        } else {
            Ok(EsSyncResult::with_errors(deleted, errors))
        }
    }

    fn search(&self, request: EsSearchRequest) -> Result<EsSearchResult, EsError> {
        self.runtime.block_on(self.search_async(request))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_real_es_sync_construct() {
        // 验证可构造（不连接真实 ES）
        let es = RealEsSync::new("http://localhost:9200");
        assert!(es.is_ok());
    }

    #[test]
    fn test_query_to_dsl_match_all() {
        let dsl = RealEsSync::query_to_dsl(&EsQuery::MatchAll);
        assert_eq!(dsl, serde_json::json!({"match_all": {}}));
    }

    #[test]
    fn test_query_to_dsl_term() {
        let q = EsQuery::term("status", serde_json::json!("active"));
        let dsl = RealEsSync::query_to_dsl(&q);
        assert_eq!(dsl, serde_json::json!({"term": { "status": "active" }}));
    }

    #[test]
    fn test_query_to_dsl_terms() {
        let q = EsQuery::terms("tag", vec![serde_json::json!("a"), serde_json::json!("b")]);
        let dsl = RealEsSync::query_to_dsl(&q);
        assert_eq!(dsl, serde_json::json!({"terms": { "tag": ["a", "b"] }}));
    }

    #[test]
    fn test_query_to_dsl_range() {
        let q = EsQuery::range(
            "age",
            EsRangeQuery::new()
                .gte(serde_json::json!(18))
                .lt(serde_json::json!(65)),
        );
        let dsl = RealEsSync::query_to_dsl(&q);
        assert_eq!(
            dsl,
            serde_json::json!({"range": { "age": { "gte": 18, "lt": 65 } }})
        );
    }

    #[test]
    fn test_query_to_dsl_bool_must() {
        let q = EsQuery::must(vec![
            EsQuery::term("status", serde_json::json!("active")),
            EsQuery::term("type", serde_json::json!("post")),
        ]);
        let dsl = RealEsSync::query_to_dsl(&q);
        assert_eq!(
            dsl,
            serde_json::json!({
                "bool": {
                    "must": [
                        {"term": { "status": "active" }},
                        {"term": { "type": "post" }}
                    ]
                }
            })
        );
    }

    #[test]
    fn test_search_request_to_dsl_basic() {
        let req = EsSearchRequest::new("my-index", EsQuery::match_all());
        let dsl = RealEsSync::search_request_to_dsl(&req);
        assert_eq!(dsl["query"], serde_json::json!({"match_all": {}}));
        assert_eq!(dsl["from"], 0);
        assert_eq!(dsl["size"], 10);
    }

    #[test]
    fn test_search_request_to_dsl_with_sort() {
        let req = EsSearchRequest::new("idx", EsQuery::match_all())
            .with_pagination(5, 20)
            .with_sort("date", EsSortOrder::Desc);
        let dsl = RealEsSync::search_request_to_dsl(&req);
        assert_eq!(dsl["from"], 5);
        assert_eq!(dsl["size"], 20);
        assert_eq!(
            dsl["sort"],
            serde_json::json!([{ "date": { "order": "desc" } }])
        );
    }

    #[test]
    fn test_es_field_type_to_mapping() {
        assert_eq!(
            es_field_type_to_mapping(&EsFieldType::Text),
            serde_json::json!({ "type": "text" })
        );
        assert_eq!(
            es_field_type_to_mapping(&EsFieldType::Keyword),
            serde_json::json!({ "type": "keyword" })
        );
        assert_eq!(
            es_field_type_to_mapping(&EsFieldType::Integer),
            serde_json::json!({ "type": "integer" })
        );
        assert_eq!(
            es_field_type_to_mapping(&EsFieldType::GeoPoint),
            serde_json::json!({ "type": "geo_point" })
        );
    }

    #[test]
    fn test_range_to_dsl_full() {
        let range = EsRangeQuery::new()
            .gt(serde_json::json!(0))
            .gte(serde_json::json!(1))
            .lt(serde_json::json!(100))
            .lte(serde_json::json!(99));
        let dsl = range_to_dsl(&range);
        assert_eq!(dsl["gt"], 0);
        assert_eq!(dsl["gte"], 1);
        assert_eq!(dsl["lt"], 100);
        assert_eq!(dsl["lte"], 99);
    }

    #[test]
    fn test_range_to_dsl_empty() {
        let range = EsRangeQuery::new();
        let dsl = range_to_dsl(&range);
        assert!(dsl.as_object().unwrap().is_empty());
    }
}
