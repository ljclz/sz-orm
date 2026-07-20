//! 内存实现：简易倒排索引（不连接真实搜索引擎）
//!
//! 适用于单元测试和基准场景。支持：
//! - 全文搜索（基于子串匹配）
//! - 字段过滤（term 精确匹配）
//! - 排序（数值/字符串）
//! - 分页

use crate::error::SearchError;
use crate::search::SearchExt;
use crate::types::{SearchHit, SearchQuery, SearchResult};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// 内存 Search 实现
pub struct MemorySearch {
    /// 索引存储：index_name -> (doc_id -> doc)
    indices: Mutex<HashMap<String, HashMap<String, Value>>>,
}

impl MemorySearch {
    pub fn new() -> Self {
        Self {
            indices: Mutex::new(HashMap::new()),
        }
    }

    /// 文档匹配查询
    fn match_doc(query: &SearchQuery, doc: &Value) -> bool {
        // 检查过滤器
        for (field, expected) in &query.filters {
            let actual = doc.get(field);
            if actual != Some(expected) {
                return false;
            }
        }
        // 检查全文搜索（子串匹配）
        if !query.query.is_empty() {
            let doc_str = doc.to_string().to_lowercase();
            if !doc_str.contains(&query.query.to_lowercase()) {
                return false;
            }
        }
        true
    }

    /// 计算简易相关性分数（子串出现次数）
    fn compute_score(query: &SearchQuery, doc: &Value) -> f64 {
        if query.query.is_empty() {
            return 1.0;
        }
        let doc_str = doc.to_string().to_lowercase();
        let q = query.query.to_lowercase();
        let count = doc_str.matches(&q).count();
        count as f64
    }

    /// 从 doc 中提取排序键
    fn extract_sort_key(doc: &Value, field: &str) -> Option<f64> {
        doc.get(field)?.as_f64()
    }
}

impl Default for MemorySearch {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchExt for MemorySearch {
    async fn create_index(&self, index: &str, _mappings: &Value) -> Result<(), SearchError> {
        let mut indices = self.indices.lock().unwrap();
        if indices.contains_key(index) {
            return Err(SearchError::IndexAlreadyExists(index.to_string()));
        }
        indices.insert(index.to_string(), HashMap::new());
        Ok(())
    }

    async fn delete_index(&self, index: &str) -> Result<(), SearchError> {
        let mut indices = self.indices.lock().unwrap();
        if indices.remove(index).is_none() {
            return Err(SearchError::NotFound(format!("index: {}", index)));
        }
        Ok(())
    }

    async fn index_doc(&self, index: &str, id: &str, doc: &Value) -> Result<(), SearchError> {
        let mut indices = self.indices.lock().unwrap();
        let idx = indices
            .get_mut(index)
            .ok_or_else(|| SearchError::NotFound(format!("index: {}", index)))?;
        idx.insert(id.to_string(), doc.clone());
        Ok(())
    }

    async fn get_doc(&self, index: &str, id: &str) -> Result<Option<Value>, SearchError> {
        let indices = self.indices.lock().unwrap();
        let idx = indices
            .get(index)
            .ok_or_else(|| SearchError::NotFound(format!("index: {}", index)))?;
        Ok(idx.get(id).cloned())
    }

    async fn delete_doc(&self, index: &str, id: &str) -> Result<(), SearchError> {
        let mut indices = self.indices.lock().unwrap();
        let idx = indices
            .get_mut(index)
            .ok_or_else(|| SearchError::NotFound(format!("index: {}", index)))?;
        if idx.remove(id).is_none() {
            return Err(SearchError::DocNotFound {
                index: index.to_string(),
                id: id.to_string(),
            });
        }
        Ok(())
    }

    async fn search(&self, index: &str, query: &SearchQuery) -> Result<SearchResult, SearchError> {
        let start = Instant::now();
        let indices = self.indices.lock().unwrap();
        let idx = indices
            .get(index)
            .ok_or_else(|| SearchError::NotFound(format!("index: {}", index)))?;

        // 收集匹配的文档
        let mut matched: Vec<(String, f64, Value)> = idx
            .iter()
            .filter(|(_, doc)| Self::match_doc(query, doc))
            .map(|(id, doc)| {
                let score = Self::compute_score(query, doc);
                (id.clone(), score, doc.clone())
            })
            .collect();

        // 排序
        for sort_field in query.sort.iter().rev() {
            // 从后往前排序，保证第一个 sort 字段优先级最高
            matched.sort_by(|a, b| {
                let a_val = Self::extract_sort_key(&a.2, &sort_field.field);
                let b_val = Self::extract_sort_key(&b.2, &sort_field.field);
                match (a_val, b_val) {
                    (Some(av), Some(bv)) => {
                        if sort_field.order == crate::types::SortOrder::Asc {
                            av.partial_cmp(&bv).unwrap_or(std::cmp::Ordering::Equal)
                        } else {
                            bv.partial_cmp(&av).unwrap_or(std::cmp::Ordering::Equal)
                        }
                    }
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            });
        }
        // 若无排序字段，按分数降序
        if query.sort.is_empty() {
            matched.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        }

        let total = matched.len() as u64;
        // 分页
        let paged: Vec<SearchHit> = matched
            .into_iter()
            .skip(query.from)
            .take(query.size)
            .map(|(id, score, source)| SearchHit::new(id, score, source))
            .collect();
        let took_ms = start.elapsed().as_millis() as u64;

        Ok(SearchResult {
            total,
            hits: paged,
            took_ms,
        })
    }

    async fn count(&self, index: &str, query: &SearchQuery) -> Result<u64, SearchError> {
        let indices = self.indices.lock().unwrap();
        let idx = indices
            .get(index)
            .ok_or_else(|| SearchError::NotFound(format!("index: {}", index)))?;
        let count = idx
            .iter()
            .filter(|(_, doc)| Self::match_doc(query, doc))
            .count();
        Ok(count as u64)
    }

    async fn refresh(&self, _index: &str) -> Result<(), SearchError> {
        // 内存实现：no-op（数据立即可见）
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SortOrder;

    #[tokio::test]
    async fn test_memory_create_and_delete_index() {
        let s = MemorySearch::new();
        s.create_index("test", &serde_json::json!({}))
            .await
            .unwrap();
        // 重复创建
        let result = s.create_index("test", &serde_json::json!({})).await;
        assert!(matches!(result, Err(SearchError::IndexAlreadyExists(_))));
        s.delete_index("test").await.unwrap();
        let result = s.delete_index("test").await;
        assert!(matches!(result, Err(SearchError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_memory_index_and_get_doc() {
        let s = MemorySearch::new();
        s.create_index("test", &serde_json::json!({}))
            .await
            .unwrap();
        let doc = serde_json::json!({"title": "hello world", "status": "active"});
        s.index_doc("test", "1", &doc).await.unwrap();
        let got = s.get_doc("test", "1").await.unwrap();
        assert_eq!(got, Some(doc));
        let missing = s.get_doc("test", "999").await.unwrap();
        assert_eq!(missing, None);
    }

    #[tokio::test]
    async fn test_memory_delete_doc() {
        let s = MemorySearch::new();
        s.create_index("test", &serde_json::json!({}))
            .await
            .unwrap();
        s.index_doc("test", "1", &serde_json::json!({"title": "test"}))
            .await
            .unwrap();
        s.delete_doc("test", "1").await.unwrap();
        let result = s.delete_doc("test", "1").await;
        assert!(matches!(result, Err(SearchError::DocNotFound { .. })));
    }

    #[tokio::test]
    async fn test_memory_search_fulltext() {
        let s = MemorySearch::new();
        s.create_index("docs", &serde_json::json!({}))
            .await
            .unwrap();
        s.index_doc("docs", "1", &serde_json::json!({"title": "hello world"}))
            .await
            .unwrap();
        s.index_doc("docs", "2", &serde_json::json!({"title": "hello rust"}))
            .await
            .unwrap();
        s.index_doc("docs", "3", &serde_json::json!({"title": "goodbye world"}))
            .await
            .unwrap();

        let result = s.search("docs", &SearchQuery::new("hello")).await.unwrap();
        assert_eq!(result.total, 2);
        assert_eq!(result.hits.len(), 2);
    }

    #[tokio::test]
    async fn test_memory_search_with_filter() {
        let s = MemorySearch::new();
        s.create_index("docs", &serde_json::json!({}))
            .await
            .unwrap();
        s.index_doc(
            "docs",
            "1",
            &serde_json::json!({"title": "test", "status": "active"}),
        )
        .await
        .unwrap();
        s.index_doc(
            "docs",
            "2",
            &serde_json::json!({"title": "test", "status": "inactive"}),
        )
        .await
        .unwrap();

        let q = SearchQuery::new("test").with_filter("status", serde_json::json!("active"));
        let result = s.search("docs", &q).await.unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.hits[0].id, "1");
    }

    #[tokio::test]
    async fn test_memory_search_pagination() {
        let s = MemorySearch::new();
        s.create_index("docs", &serde_json::json!({}))
            .await
            .unwrap();
        for i in 0..10 {
            s.index_doc(
                "docs",
                &i.to_string(),
                &serde_json::json!({"title": "test"}),
            )
            .await
            .unwrap();
        }
        let q = SearchQuery::new("test").with_pagination(5, 3);
        let result = s.search("docs", &q).await.unwrap();
        assert_eq!(result.total, 10);
        assert_eq!(result.hits.len(), 3); // 只返回 3 条
    }

    #[tokio::test]
    async fn test_memory_search_sort() {
        let s = MemorySearch::new();
        s.create_index("docs", &serde_json::json!({}))
            .await
            .unwrap();
        s.index_doc(
            "docs",
            "1",
            &serde_json::json!({"title": "test", "score": 30}),
        )
        .await
        .unwrap();
        s.index_doc(
            "docs",
            "2",
            &serde_json::json!({"title": "test", "score": 10}),
        )
        .await
        .unwrap();
        s.index_doc(
            "docs",
            "3",
            &serde_json::json!({"title": "test", "score": 20}),
        )
        .await
        .unwrap();

        let q = SearchQuery::new("test").with_sort("score", SortOrder::Desc);
        let result = s.search("docs", &q).await.unwrap();
        assert_eq!(result.hits[0].id, "1"); // score=30
        assert_eq!(result.hits[1].id, "3"); // score=20
        assert_eq!(result.hits[2].id, "2"); // score=10
    }

    #[tokio::test]
    async fn test_memory_count() {
        let s = MemorySearch::new();
        s.create_index("docs", &serde_json::json!({}))
            .await
            .unwrap();
        s.index_doc("docs", "1", &serde_json::json!({"title": "hello"}))
            .await
            .unwrap();
        s.index_doc("docs", "2", &serde_json::json!({"title": "world"}))
            .await
            .unwrap();
        let count = s.count("docs", &SearchQuery::match_all()).await.unwrap();
        assert_eq!(count, 2);
        let count = s.count("docs", &SearchQuery::new("hello")).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_memory_bulk_index() {
        let s = MemorySearch::new();
        s.create_index("docs", &serde_json::json!({}))
            .await
            .unwrap();
        let docs = vec![
            ("1".to_string(), serde_json::json!({"title": "a"})),
            ("2".to_string(), serde_json::json!({"title": "b"})),
            ("3".to_string(), serde_json::json!({"title": "c"})),
        ];
        s.bulk_index("docs", &docs).await.unwrap();
        let count = s.count("docs", &SearchQuery::match_all()).await.unwrap();
        assert_eq!(count, 3);
    }
}
