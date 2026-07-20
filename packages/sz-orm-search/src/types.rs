//! Search 查询与结果类型定义

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 排序方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    Asc,
    Desc,
}

impl SortOrder {
    pub fn as_str(&self) -> &'static str {
        match self {
            SortOrder::Asc => "asc",
            SortOrder::Desc => "desc",
        }
    }
}

/// 排序字段
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SortField {
    pub field: String,
    pub order: SortOrder,
}

impl SortField {
    pub fn new(field: impl Into<String>, order: SortOrder) -> Self {
        Self {
            field: field.into(),
            order,
        }
    }
}

/// 搜索查询
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchQuery {
    /// 全文搜索词（为空则匹配全部）
    pub query: String,
    /// 字段过滤（field -> value 精确匹配）
    pub filters: HashMap<String, serde_json::Value>,
    /// 排序字段
    pub sort: Vec<SortField>,
    /// 分页起始位置
    pub from: usize,
    /// 每页大小
    pub size: usize,
}

impl SearchQuery {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            filters: HashMap::new(),
            sort: Vec::new(),
            from: 0,
            size: 10,
        }
    }

    pub fn match_all() -> Self {
        Self::new("")
    }

    pub fn with_filter(mut self, field: impl Into<String>, value: serde_json::Value) -> Self {
        self.filters.insert(field.into(), value);
        self
    }

    pub fn with_sort(mut self, field: impl Into<String>, order: SortOrder) -> Self {
        self.sort.push(SortField::new(field, order));
        self
    }

    pub fn with_pagination(mut self, from: usize, size: usize) -> Self {
        self.from = from;
        self.size = size;
        self
    }

    /// 转为 Elasticsearch Query DSL JSON
    pub fn to_es_dsl(&self) -> serde_json::Value {
        let query_json = if self.query.is_empty() {
            serde_json::json!({ "match_all": {} })
        } else {
            serde_json::json!({
                "multi_match": { "query": self.query }
            })
        };
        let query_json = if !self.filters.is_empty() {
            let filters: Vec<serde_json::Value> = self
                .filters
                .iter()
                .map(|(k, v)| serde_json::json!({ "term": { k: v } }))
                .collect();
            serde_json::json!({
                "bool": {
                    "must": [query_json],
                    "filter": filters
                }
            })
        } else {
            query_json
        };
        let mut body = serde_json::json!({ "query": query_json });
        if !self.sort.is_empty() {
            let sort_arr: Vec<serde_json::Value> = self
                .sort
                .iter()
                .map(|s| {
                    let mut obj = serde_json::Map::new();
                    obj.insert(
                        s.field.clone(),
                        serde_json::json!({ "order": s.order.as_str() }),
                    );
                    serde_json::Value::Object(obj)
                })
                .collect();
            body["sort"] = serde_json::Value::Array(sort_arr);
        }
        body["from"] = serde_json::Value::from(self.from as i64);
        body["size"] = serde_json::Value::from(self.size as i64);
        body
    }

    /// 转为 Meilisearch 查询参数
    pub fn to_meili_params(&self) -> serde_json::Value {
        let mut params = serde_json::json!({
            "q": self.query,
            "limit": self.size,
            "offset": self.from,
        });
        if !self.filters.is_empty() {
            let filter_str: Vec<String> = self
                .filters
                .iter()
                .map(|(k, v)| match v {
                    serde_json::Value::String(s) => format!("{} = \"{}\"", k, s),
                    _ => format!("{} = {}", k, v),
                })
                .collect();
            params["filter"] = serde_json::Value::String(filter_str.join(" AND "));
        }
        if !self.sort.is_empty() {
            let sort_arr: Vec<String> = self
                .sort
                .iter()
                .map(|s| format!("{}:{}", s.field, s.order.as_str()))
                .collect();
            params["sort"] = serde_json::Value::Array(
                sort_arr
                    .iter()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect(),
            );
        }
        params
    }
}

/// 搜索命中
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    /// 文档 ID
    pub id: String,
    /// 相关性分数（越高越相关，0 表示无评分）
    pub score: f64,
    /// 文档原文
    pub source: serde_json::Value,
}

impl SearchHit {
    pub fn new(id: impl Into<String>, score: f64, source: serde_json::Value) -> Self {
        Self {
            id: id.into(),
            score,
            source,
        }
    }
}

/// 搜索结果
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    /// 总命中数
    pub total: u64,
    /// 命中列表
    pub hits: Vec<SearchHit>,
    /// 查询耗时（毫秒）
    pub took_ms: u64,
}

impl SearchResult {
    pub fn new(total: u64, hits: Vec<SearchHit>, took_ms: u64) -> Self {
        Self {
            total,
            hits,
            took_ms,
        }
    }

    pub fn empty() -> Self {
        Self {
            total: 0,
            hits: Vec::new(),
            took_ms: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_query_new() {
        let q = SearchQuery::new("hello");
        assert_eq!(q.query, "hello");
        assert_eq!(q.size, 10);
        assert_eq!(q.from, 0);
    }

    #[test]
    fn test_search_query_builder() {
        let q = SearchQuery::new("hello")
            .with_filter("status", serde_json::json!("active"))
            .with_filter("year", serde_json::json!(2026))
            .with_sort("timestamp", SortOrder::Desc)
            .with_pagination(20, 50);
        assert_eq!(q.filters.len(), 2);
        assert_eq!(q.sort.len(), 1);
        assert_eq!(q.from, 20);
        assert_eq!(q.size, 50);
    }

    #[test]
    fn test_search_query_es_dsl() {
        let q = SearchQuery::new("hello").with_filter("status", serde_json::json!("active"));
        let dsl = q.to_es_dsl();
        assert!(dsl["query"]["bool"].is_object());
        assert!(dsl["query"]["bool"]["must"].is_array());
        assert!(dsl["query"]["bool"]["filter"].is_array());
    }

    #[test]
    fn test_search_query_es_dsl_match_all() {
        let q = SearchQuery::match_all();
        let dsl = q.to_es_dsl();
        assert!(dsl["query"]["match_all"].is_object());
    }

    #[test]
    fn test_search_query_meili_params() {
        let q = SearchQuery::new("hello").with_sort("ts", SortOrder::Desc);
        let params = q.to_meili_params();
        assert_eq!(params["q"], "hello");
        assert!(params["sort"].is_array());
    }

    #[test]
    fn test_sort_order() {
        assert_eq!(SortOrder::Asc.as_str(), "asc");
        assert_eq!(SortOrder::Desc.as_str(), "desc");
    }

    #[test]
    fn test_search_hit() {
        let hit = SearchHit::new("1", 1.5, serde_json::json!({"title": "test"}));
        assert_eq!(hit.id, "1");
        assert_eq!(hit.score, 1.5);
    }

    #[test]
    fn test_search_result_empty() {
        let r = SearchResult::empty();
        assert_eq!(r.total, 0);
        assert!(r.hits.is_empty());
    }
}
