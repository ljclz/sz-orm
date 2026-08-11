//! 混合搜索（v4.0.0 M3）
//!
//! 融合向量搜索 + 全文搜索 + 结构化查询三源结果，
//! 支持 RRF/加权/级联融合排序，并行查询 ≤200ms，部分源降级。

use std::collections::HashMap;

pub mod fusion;
pub mod pushdown;
pub mod searcher;

pub use pushdown::FilterPushdown;
pub use searcher::{
    FulltextSearchSource, HybridSearcher, StructuredSearchSource, VectorSearchSource,
};

/// 融合策略
#[derive(Debug, Clone, Copy)]
pub enum FusionStrategy {
    /// Reciprocal Rank Fusion（`score = Σ 1/(k + rank_i)`，默认 k=60）
    Rrf { k: u32 },
    /// 加权融合（`score = Σ weight_i × normalized_score_i`）
    Weighted {
        vector_w: f32,
        fulltext_w: f32,
        structured_w: f32,
    },
    /// 级联融合（向量召回 → 全文精排 → 结构化过滤）
    Cascade,
}

impl Default for FusionStrategy {
    fn default() -> Self {
        Self::Rrf { k: 60 }
    }
}

/// 向量查询参数
#[derive(Debug, Clone)]
pub struct VectorQuery {
    /// 集合名称
    pub collection: String,
    /// 查询向量
    pub query_vector: Vec<f32>,
    /// 距离度量
    pub metric: VectorMetric,
    /// 过滤条件（SQL WHERE 子句，参数化）
    pub filter: Option<String>,
}

/// 距离度量
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorMetric {
    /// 余弦相似度
    Cosine,
    /// 欧氏距离
    Euclidean,
    /// 点积
    DotProduct,
}

/// 全文查询参数
#[derive(Debug, Clone)]
pub struct FulltextQuery {
    /// 索引名称
    pub index: String,
    /// 查询文本
    pub query_text: String,
    /// 搜索字段
    pub fields: Vec<String>,
}

/// 结构化查询参数
#[derive(Debug, Clone)]
pub struct StructuredQuery {
    /// 表名
    pub table: String,
    /// WHERE 条件（参数化）
    pub where_clauses: Vec<String>,
    /// ORDER BY 字段
    pub order_by: Option<String>,
}

/// 混合查询
#[derive(Debug, Clone)]
pub struct HybridQuery {
    /// 向量查询（None 表示不查询向量源）
    pub vector: Option<VectorQuery>,
    /// 全文查询（None 表示不查询全文源）
    pub fulltext: Option<FulltextQuery>,
    /// 结构化查询（None 表示不查询结构化源）
    pub structured: Option<StructuredQuery>,
    /// 融合策略
    pub strategy: FusionStrategy,
    /// 返回结果数
    pub top_k: usize,
}

/// 搜索结果来源
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchResultSource {
    /// 来自向量搜索
    Vector,
    /// 来自全文搜索
    Fulltext,
    /// 来自结构化查询
    Structured,
    /// 融合结果
    Hybrid,
}

/// 混合搜索结果
#[derive(Debug, Clone)]
pub struct HybridSearchResult {
    /// 结果 ID
    pub id: String,
    /// 融合分数
    pub score: f32,
    /// 结果来源
    pub source: SearchResultSource,
    /// 元数据
    pub metadata: HashMap<String, serde_json::Value>,
}

/// 降级状态
#[derive(Debug, Clone, Default)]
pub struct DegradationStatus {
    /// 向量源降级
    pub vector_degraded: bool,
    /// 全文源降级
    pub fulltext_degraded: bool,
    /// 结构化源降级
    pub structured_degraded: bool,
}

impl DegradationStatus {
    /// 是否所有源都降级
    pub fn all_degraded(&self) -> bool {
        self.vector_degraded && self.fulltext_degraded && self.structured_degraded
    }

    /// 是否有任一源降级
    pub fn any_degraded(&self) -> bool {
        self.vector_degraded || self.fulltext_degraded || self.structured_degraded
    }
}

/// 混合搜索响应
#[derive(Debug, Clone)]
pub struct HybridSearchResponse {
    /// 融合后的结果列表
    pub results: Vec<HybridSearchResult>,
    /// 降级状态
    pub degradation: DegradationStatus,
    /// 端到端耗时（ms）
    pub elapsed_ms: u64,
}

/// 混合搜索错误
#[derive(Debug, thiserror::Error)]
pub enum HybridError {
    /// 某源超时
    #[error("source {source_name} timeout")]
    SourceTimeout { source_name: String },

    /// 所有源均失败
    #[error("all sources failed")]
    AllSourcesFailed,

    /// 向量源错误
    #[error("vector error: {0}")]
    VectorError(String),

    /// 全文源错误
    #[error("fulltext error: {0}")]
    FulltextError(String),

    /// 结构化源错误
    #[error("structured error: {0}")]
    StructuredError(String),
}

/// 单源搜索结果（内部使用）
#[derive(Debug, Clone)]
pub struct SourceResult {
    /// 结果 ID
    pub id: String,
    /// 原始分数
    pub score: f32,
    /// 来源
    pub source: SearchResultSource,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fusion_strategy_default() {
        let strategy = FusionStrategy::default();
        assert!(matches!(strategy, FusionStrategy::Rrf { k: 60 }));
    }

    #[test]
    fn test_hybrid_query_construction() {
        let query = HybridQuery {
            vector: Some(VectorQuery {
                collection: "docs".to_string(),
                query_vector: vec![1.0, 0.0, 0.0],
                metric: VectorMetric::Cosine,
                filter: None,
            }),
            fulltext: Some(FulltextQuery {
                index: "docs_idx".to_string(),
                query_text: "hello world".to_string(),
                fields: vec!["title".to_string(), "content".to_string()],
            }),
            structured: None,
            strategy: FusionStrategy::Rrf { k: 60 },
            top_k: 10,
        };
        assert!(query.vector.is_some());
        assert!(query.fulltext.is_some());
        assert!(query.structured.is_none());
        assert_eq!(query.top_k, 10);
    }

    #[test]
    fn test_degradation_status_default() {
        let status = DegradationStatus::default();
        assert!(!status.vector_degraded);
        assert!(!status.fulltext_degraded);
        assert!(!status.structured_degraded);
        assert!(!status.all_degraded());
        assert!(!status.any_degraded());
    }

    #[test]
    fn test_degradation_status_partial() {
        let status = DegradationStatus {
            vector_degraded: false,
            fulltext_degraded: true,
            structured_degraded: false,
        };
        assert!(status.any_degraded());
        assert!(!status.all_degraded());
    }

    #[test]
    fn test_degradation_status_all() {
        let status = DegradationStatus {
            vector_degraded: true,
            fulltext_degraded: true,
            structured_degraded: true,
        };
        assert!(status.all_degraded());
        assert!(status.any_degraded());
    }

    #[test]
    fn test_hybrid_error_display() {
        let err = HybridError::SourceTimeout {
            source_name: "elasticsearch".to_string(),
        };
        assert!(err.to_string().contains("timeout"));

        let err = HybridError::AllSourcesFailed;
        assert!(err.to_string().contains("all sources failed"));

        let err = HybridError::VectorError("connection refused".to_string());
        assert!(err.to_string().contains("vector error"));
    }
}
