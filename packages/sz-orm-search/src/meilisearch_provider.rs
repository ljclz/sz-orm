//! 真实 Meilisearch 实现（feature = "real-meilisearch"）
//!
//! 通过 meilisearch-sdk crate 连接真实 Meilisearch 实例。
//! Meilisearch 是轻量级开源搜索引擎，与 ES 相比启动更快、占用更少。
//!
//! # 异步任务模型
//!
//! Meilisearch 的写操作（create_index/index_doc/delete_doc）均为异步任务：
//! - 写入后立即返回 TaskInfo（含 task_uid）
//! - 需要 wait_for_task 轮询任务状态直到 Succeeded/Failed
//! - 本实现默认同步等待任务完成，简化调用方逻辑

use crate::error::SearchError;
use crate::search::{MeilisearchConfig, SearchExt};
use crate::types::{SearchHit, SearchQuery, SearchResult};
use async_trait::async_trait;
use meilisearch_sdk::client::Client as MeiliClient;
use meilisearch_sdk::indexes::Index;
use meilisearch_sdk::search::{SearchQuery as MeiliSearchQuery, SearchResults};
use meilisearch_sdk::task_info::TaskInfo;
use meilisearch_sdk::tasks::Task;
use serde_json::Value;
use std::time::Instant;

/// Meilisearch 真实实现
pub struct MeilisearchProvider {
    client: MeiliClient,
}

impl MeilisearchProvider {
    pub fn new(config: MeilisearchConfig) -> Result<Self, SearchError> {
        let api_key = config.api_key.as_deref();
        let client = MeiliClient::new(&config.url, api_key)
            .map_err(|e| SearchError::Connection(e.to_string()))?;
        Ok(Self { client })
    }

    /// 等待任务完成，失败时返回错误
    ///
    /// 默认轮询间隔 50ms，超时 5s（与 SDK 默认一致）。
    /// 若任务状态为 Failed 则返回 SearchError::Query。
    async fn wait_task(&self, task_info: TaskInfo) -> Result<(), SearchError> {
        let task = self
            .client
            .wait_for_task(task_info, None, None)
            .await
            .map_err(|e| SearchError::Query(e.to_string()))?;
        match task {
            Task::Succeeded { .. } => Ok(()),
            Task::Failed { content } => Err(SearchError::Query(format!(
                "task {} failed: {:?}",
                content.task.uid, content.error
            ))),
            _ => Err(SearchError::Query(format!(
                "unexpected task state: {:?}",
                task
            ))),
        }
    }

    /// 获取索引句柄（不验证存在，轻量级）
    fn index(&self, name: &str) -> Index {
        self.client.index(name)
    }
}

#[async_trait]
impl SearchExt for MeilisearchProvider {
    async fn create_index(&self, index: &str, _mappings: &Value) -> Result<(), SearchError> {
        // v0.2.2 修复 P1-4（第二次审查补全）：显式设置 primary_key = "id"
        // 确保即使仅调用 create_index + search（不先 index_doc）时也有正确的主键
        let task_info = self
            .client
            .create_index(index, Some("id"))
            .await
            .map_err(|e| SearchError::Query(e.to_string()))?;
        self.wait_task(task_info).await
    }

    async fn delete_index(&self, index: &str) -> Result<(), SearchError> {
        let idx = self.index(index);
        let task_info = idx
            .delete()
            .await
            .map_err(|e| SearchError::Query(e.to_string()))?;
        self.wait_task(task_info).await
    }

    async fn index_doc(&self, index: &str, id: &str, doc: &Value) -> Result<(), SearchError> {
        // v0.2.2 修复 V-5：原实现忽略 id 参数，导致 Meilisearch 自动生成 id
        // 现在：将 id 注入文档作为 primary key 字段，并显式指定 "id" 为主键
        let mut doc_with_id = doc.clone();
        if let Value::Object(ref mut map) = doc_with_id {
            // 仅在文档尚未包含 id 字段时注入，避免覆盖用户提供的 id
            if !map.contains_key("id") {
                map.insert("id".to_string(), Value::String(id.to_string()));
            }
        }
        let idx = self.index(index);
        let task_info = idx
            .add_documents(&[doc_with_id], Some("id"))
            .await
            .map_err(|e| SearchError::Query(e.to_string()))?;
        self.wait_task(task_info).await
    }

    async fn get_doc(&self, index: &str, id: &str) -> Result<Option<Value>, SearchError> {
        let idx = self.index(index);
        match idx.get_document::<Value>(id).await {
            Ok(doc) => Ok(Some(doc)),
            Err(e) => {
                let msg = e.to_string().to_lowercase();
                if msg.contains("not found") || msg.contains("404") {
                    Ok(None)
                } else {
                    Err(SearchError::Query(e.to_string()))
                }
            }
        }
    }

    async fn delete_doc(&self, index: &str, id: &str) -> Result<(), SearchError> {
        let idx = self.index(index);
        let task_info = idx
            .delete_document(id)
            .await
            .map_err(|e| SearchError::Query(e.to_string()))?;
        self.wait_task(task_info).await
    }

    async fn search(&self, index: &str, query: &SearchQuery) -> Result<SearchResult, SearchError> {
        let idx = self.index(index);
        let params = query.to_meili_params();

        let q_str = params
            .get("q")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
        let offset = params.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        let mut sq = MeiliSearchQuery::new(&idx);
        sq.with_query(&q_str);
        if limit > 0 {
            sq.with_limit(limit);
        }
        if offset > 0 {
            sq.with_offset(offset);
        }
        let filter_str = params.get("filter").and_then(|v| v.as_str());
        if let Some(filter) = filter_str {
            sq.with_filter(filter);
        }
        // v0.2.2：将 sort_strs 提升到外部作用域，避免 with_sort 的 &'a 生命周期借用早释
        let sort_strs: Vec<String> = params
            .get("sort")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let sort_refs: Vec<&str> = sort_strs.iter().map(|s| s.as_str()).collect();
        if !sort_refs.is_empty() {
            sq.with_sort(&sort_refs);
        }

        let start = Instant::now();
        let results: SearchResults<Value> = sq
            .execute()
            .await
            .map_err(|e| SearchError::Query(e.to_string()))?;
        let took_ms = start.elapsed().as_millis() as u64;

        let hits: Vec<SearchHit> = results
            .hits
            .into_iter()
            .enumerate()
            .map(|(i, hit)| {
                let doc = hit.result;
                let id = doc
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| i.to_string());
                let score = hit.ranking_score.unwrap_or(1.0);
                SearchHit::new(id, score, doc)
            })
            .collect();
        let total = results
            .estimated_total_hits
            .or(results.total_hits)
            .unwrap_or(hits.len()) as u64;
        Ok(SearchResult::new(total, hits, took_ms))
    }

    async fn count(&self, index: &str, query: &SearchQuery) -> Result<u64, SearchError> {
        // 通过 limit=0 的搜索获取总数（estimated_total_hits）
        let mut q = query.clone();
        q.size = 0;
        let result = self.search(index, &q).await?;
        Ok(result.total)
    }

    async fn refresh(&self, _index: &str) -> Result<(), SearchError> {
        // Meilisearch 默认近实时（< 1s 刷新），无需显式 refresh
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meilisearch_config_default() {
        let config = MeilisearchConfig::default();
        assert!(config.url.is_empty());
        assert!(config.api_key.is_none());
    }

    #[test]
    fn test_meilisearch_config_with_key() {
        let config = MeilisearchConfig {
            url: "http://localhost:7700".to_string(),
            api_key: Some("masterKey".to_string()),
        };
        assert_eq!(config.url, "http://localhost:7700");
        assert_eq!(config.api_key.as_deref(), Some("masterKey"));
    }

    #[tokio::test]
    async fn test_meilisearch_provider_new_invalid_url() {
        // MeiliClient::new 通常不验证 URL 格式，只是构造客户端
        // 这里测试构造不抛 panic
        let config = MeilisearchConfig {
            url: "http://localhost:7700".to_string(),
            api_key: None,
        };
        let provider = MeilisearchProvider::new(config);
        assert!(provider.is_ok(), "Provider construction should succeed");
    }
}
