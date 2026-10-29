//! 真实 Elasticsearch 实现（feature = "real-es"）
//!
//! v10.13.0 安全修复（2026-10-27 H-1）：将底层 HTTP 客户端从 elasticsearch crate
//! 8.5.0-alpha.1（传递依赖 reqwest 0.11 → rustls 0.21.12，含 RUSTSEC-2026-0346 /
//! RUSTSEC-2026-0349 / RUSTSEC-2026-0382）迁移为 reqwest 0.12（rustls 0.23 线）直连
//! ES REST API，消除 rustls 0.21 漏洞链。公共 API（`SearchExt` trait / `ElasticsearchConfig`）
//! 签名零变更。
//!
//! REST 端点映射（与 elasticsearch crate 行为一致）：
//! - `create_index`  → `PUT    /{index}`
//! - `delete_index`  → `DELETE /{index}`（404 视为成功）
//! - `index_doc`     → `PUT    /{index}/_doc/{id}`
//! - `get_doc`       → `GET    /{index}/_doc/{id}`（404 → None）
//! - `delete_doc`    → `DELETE /{index}/_doc/{id}`（404 视为成功）
//! - `search`        → `POST   /{index}/_search`
//! - `count`         → `POST   /{index}/_count`
//! - `refresh`       → `POST   /{index}/_refresh`
//!
//! 路径段经 `Url::path_segments_mut` 自动百分号编码，防止 index/id 中的特殊字符
//! 破坏 URL 结构。

use crate::error::SearchError;
use crate::search::{ElasticsearchConfig, SearchExt};
use crate::types::{SearchHit, SearchQuery, SearchResult};
use async_trait::async_trait;
use reqwest::{Client, Method, RequestBuilder, StatusCode, Url};
use serde_json::Value;

/// Elasticsearch 真实实现
pub struct ElasticsearchProvider {
    client: Client,
    /// 规范化后的 base URL（仅用于派生请求 URL）
    base_url: Url,
    /// Basic 认证凭据（username + password 同时提供时启用）
    credentials: Option<(String, String)>,
}

impl ElasticsearchProvider {
    pub fn new(config: ElasticsearchConfig) -> Result<Self, SearchError> {
        // 校验 base URL（与迁移前 Url::parse 校验行为一致）
        let base_url = Url::parse(&config.url)
            .map_err(|e| SearchError::Connection(format!("invalid url: {}", e)))?;
        let client = Client::builder()
            .build()
            .map_err(|e| SearchError::Connection(e.to_string()))?;
        let credentials = match (&config.username, &config.password) {
            (Some(user), Some(pass)) => Some((user.clone(), pass.clone())),
            _ => None,
        };
        Ok(Self {
            client,
            base_url,
            credentials,
        })
    }

    /// 构造带认证的请求；路径段自动百分号编码（index/id 含空格、`#`、`?` 等字符时仍保持 URL 结构）
    fn request(&self, method: Method, segments: &[&str]) -> Result<RequestBuilder, SearchError> {
        let mut url = self.base_url.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| SearchError::Connection("base url cannot be a base".to_string()))?;
            for segment in segments {
                path.push(segment);
            }
        }
        let mut rb = self.client.request(method, url);
        if let Some((user, pass)) = &self.credentials {
            rb = rb.basic_auth(user, Some(pass));
        }
        Ok(rb)
    }

    /// 发送请求并校验状态码；`allow_404` 时 404 视为成功
    async fn send_checked(
        rb: RequestBuilder,
        op: &str,
        allow_404: bool,
    ) -> Result<reqwest::Response, SearchError> {
        let response = rb
            .send()
            .await
            .map_err(|e| SearchError::Query(format!("{} failed: {}", op, e)))?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND && allow_404 {
            return Ok(response);
        }
        if !status.is_success() {
            return Err(SearchError::Query(format!("{} failed: {}", op, status)));
        }
        Ok(response)
    }

    /// 解析响应 JSON body
    async fn json(response: reqwest::Response, op: &str) -> Result<Value, SearchError> {
        response
            .json::<Value>()
            .await
            .map_err(|e| SearchError::Query(format!("{} failed: {}", op, e)))
    }
}

#[async_trait]
impl SearchExt for ElasticsearchProvider {
    async fn create_index(&self, index: &str, mappings: &Value) -> Result<(), SearchError> {
        let rb = self.request(Method::PUT, &[index])?.json(mappings);
        Self::send_checked(rb, "create index", false).await?;
        Ok(())
    }

    async fn delete_index(&self, index: &str) -> Result<(), SearchError> {
        let rb = self.request(Method::DELETE, &[index])?;
        Self::send_checked(rb, "delete index", true).await?;
        Ok(())
    }

    async fn index_doc(&self, index: &str, id: &str, doc: &Value) -> Result<(), SearchError> {
        let rb = self.request(Method::PUT, &[index, "_doc", id])?.json(doc);
        Self::send_checked(rb, "index doc", false).await?;
        Ok(())
    }

    async fn get_doc(&self, index: &str, id: &str) -> Result<Option<Value>, SearchError> {
        let rb = self.request(Method::GET, &[index, "_doc", id])?;
        let response = Self::send_checked(rb, "get doc", true).await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let body = Self::json(response, "get doc").await?;
        // ES Get API 返回的文档源字段名是 `_source`（带下划线前缀），不是 `source`
        Ok(body.get("_source").cloned())
    }

    async fn delete_doc(&self, index: &str, id: &str) -> Result<(), SearchError> {
        let rb = self.request(Method::DELETE, &[index, "_doc", id])?;
        Self::send_checked(rb, "delete doc", true).await?;
        Ok(())
    }

    async fn search(&self, index: &str, query: &SearchQuery) -> Result<SearchResult, SearchError> {
        let dsl = query.to_es_dsl();
        let rb = self.request(Method::POST, &[index, "_search"])?.json(&dsl);
        let response = Self::send_checked(rb, "search", false).await?;
        let body = Self::json(response, "search").await?;
        let took_ms = body.get("took").and_then(|v| v.as_u64()).unwrap_or(0);
        let total = body
            .get("hits")
            .and_then(|h| h.get("total"))
            .and_then(|t| t.get("value"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let hits: Vec<SearchHit> = body
            .get("hits")
            .and_then(|h| h.get("hits"))
            .and_then(|h| h.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|hit| {
                        let id = hit.get("_id")?.as_str()?.to_string();
                        let score = hit.get("_score").and_then(|s| s.as_f64()).unwrap_or(0.0);
                        let source = hit.get("_source")?.clone();
                        Some(SearchHit::new(id, score, source))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(SearchResult::new(total, hits, took_ms))
    }

    async fn count(&self, index: &str, query: &SearchQuery) -> Result<u64, SearchError> {
        let dsl = serde_json::json!({ "query": query.to_es_dsl()["query"] });
        let rb = self.request(Method::POST, &[index, "_count"])?.json(&dsl);
        let response = Self::send_checked(rb, "count", false).await?;
        let body = Self::json(response, "count").await?;
        Ok(body.get("count").and_then(|c| c.as_u64()).unwrap_or(0))
    }

    async fn refresh(&self, index: &str) -> Result<(), SearchError> {
        let rb = self.request(Method::POST, &[index, "_refresh"])?;
        Self::send_checked(rb, "refresh", false).await?;
        Ok(())
    }
}
