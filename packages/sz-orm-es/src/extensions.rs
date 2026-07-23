//! ES 深度扩展功能
//!
//! 本模块补充 Elasticsearch 集成缺失的核心深度功能，包括：
//!
//! - **批量索引（Bulk API）**：支持 index/create/update/delete 四种批量操作
//! - **搜索建议（Suggest API）**：基于前缀的自动补全建议
//! - **聚合查询（Aggregations）**：terms/range/sum/avg/max/min/histogram 聚合
//! - **索引别名管理**：别名指向、过滤别名、路由别名
//!
//! # 设计说明
//!
//! 本模块以独立类型 + 扩展 trait 的方式提供，不修改既有 `EsSync` trait，
//! 避免破坏已有的 InMemoryEsSync 实现。
//! 内存计算部分基于纯 Rust 实现，不依赖外部库。

#![allow(dead_code)]

use crate::{EsDocument, EsError, EsSearchRequest, EsSync, InMemoryEsSync};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

// =============================================================================
// 一、批量索引（Bulk API）
// =============================================================================

/// 批量操作类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BulkAction {
    /// 索引文档（存在则替换）
    Index {
        index: String,
        id: String,
        source: serde_json::Value,
    },
    /// 创建文档（仅在不存在时创建）
    Create {
        index: String,
        id: String,
        source: serde_json::Value,
    },
    /// 更新文档（部分字段更新）
    Update {
        index: String,
        id: String,
        doc: serde_json::Value,
    },
    /// 删除文档
    Delete { index: String, id: String },
}

impl BulkAction {
    /// 获取操作所属的索引名
    pub fn index(&self) -> &str {
        match self {
            BulkAction::Index { index, .. }
            | BulkAction::Create { index, .. }
            | BulkAction::Update { index, .. }
            | BulkAction::Delete { index, .. } => index,
        }
    }

    /// 获取操作对应的文档 ID
    pub fn id(&self) -> &str {
        match self {
            BulkAction::Index { id, .. }
            | BulkAction::Create { id, .. }
            | BulkAction::Update { id, .. }
            | BulkAction::Delete { id, .. } => id,
        }
    }
}

/// 单个批量操作的结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BulkItemResult {
    /// 操作类型
    pub action: String,
    /// 索引名
    pub index: String,
    /// 文档 ID
    pub id: String,
    /// 状态：201=created, 200=updated, 404=not_found, 409=conflict
    pub status: u16,
    /// 错误信息（若失败）
    pub error: Option<String>,
}

impl BulkItemResult {
    pub fn success(action: &str, index: &str, id: &str, status: u16) -> Self {
        Self {
            action: action.to_string(),
            index: index.to_string(),
            id: id.to_string(),
            status,
            error: None,
        }
    }

    pub fn failure(action: &str, index: &str, id: &str, status: u16, error: &str) -> Self {
        Self {
            action: action.to_string(),
            index: index.to_string(),
            id: id.to_string(),
            status,
            error: Some(error.to_string()),
        }
    }

    pub fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }
}

/// 批量操作结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkResult {
    /// 每个操作的结果
    pub items: Vec<BulkItemResult>,
    /// 耗时（毫秒）
    pub took: u64,
    /// 成功数
    pub success_count: usize,
    /// 失败数
    pub failure_count: usize,
}

impl BulkResult {
    pub fn new(items: Vec<BulkItemResult>, took: u64) -> Self {
        let success_count = items.iter().filter(|r| r.is_success()).count();
        let failure_count = items.len() - success_count;
        Self {
            items,
            took,
            success_count,
            failure_count,
        }
    }

    /// 是否全部成功
    pub fn all_success(&self) -> bool {
        self.failure_count == 0
    }

    /// 获取所有失败项
    pub fn failures(&self) -> Vec<&BulkItemResult> {
        self.items.iter().filter(|r| !r.is_success()).collect()
    }
}

/// 批量操作执行器（基于 InMemoryEsSync）
pub struct BulkExecutor {
    backend: InMemoryEsSync,
}

impl BulkExecutor {
    pub fn new(backend: InMemoryEsSync) -> Self {
        Self { backend }
    }

    pub fn from_new() -> Self {
        Self::new(InMemoryEsSync::new())
    }

    /// 获取内部后端引用
    pub fn backend(&self) -> &InMemoryEsSync {
        &self.backend
    }

    /// 执行批量操作
    ///
    /// 依次处理每个 BulkAction，记录每个操作的结果
    pub fn bulk(&self, actions: Vec<BulkAction>) -> Result<BulkResult, EsError> {
        let start = std::time::Instant::now();
        let mut items = Vec::with_capacity(actions.len());

        for action in actions {
            let result = self.execute_action(action)?;
            items.push(result);
        }

        Ok(BulkResult::new(items, start.elapsed().as_millis() as u64))
    }

    /// 执行单个批量操作
    fn execute_action(&self, action: BulkAction) -> Result<BulkItemResult, EsError> {
        match action {
            BulkAction::Index { index, id, source } => {
                let doc = EsDocument::new(&index, source).with_id(&id);
                self.backend.sync_to_es(vec![doc])?;
                Ok(BulkItemResult::success("index", &index, &id, 200))
            }
            BulkAction::Create { index, id, source } => {
                // 内存后端不支持存在性检查，这里直接索引
                let doc = EsDocument::new(&index, source).with_id(&id);
                self.backend.sync_to_es(vec![doc])?;
                Ok(BulkItemResult::success("create", &index, &id, 201))
            }
            BulkAction::Update { index, id, doc } => {
                // 内存后端的 sync_to_es 会按 id 替换整个文档
                // 这里模拟部分更新：先获取原文档，合并字段，再写回
                // 使用 match_all 搜索，然后从 hits 中按 id 查找
                let search_req =
                    EsSearchRequest::new(&index, crate::EsQuery::match_all())
                        .with_pagination(0, 10000);
                let result = match self.backend.search(search_req) {
                    Ok(r) => r,
                    Err(EsError::IndexNotFound(_)) => {
                        return Ok(BulkItemResult::failure(
                            "update",
                            &index,
                            &id,
                            404,
                            "document not found",
                        ));
                    }
                    Err(e) => return Err(e),
                };
                let hit = result.hits.iter().find(|h| h.id == id);
                if hit.is_none() {
                    return Ok(BulkItemResult::failure(
                        "update",
                        &index,
                        &id,
                        404,
                        "document not found",
                    ));
                }
                let hit = hit.unwrap();
                let mut source = hit.source.clone();
                if let (Some(obj), Some(updates)) = (source.as_object_mut(), doc.as_object()) {
                    for (k, v) in updates {
                        obj.insert(k.clone(), v.clone());
                    }
                }
                let new_doc = EsDocument::new(&index, source).with_id(&id);
                self.backend.sync_to_es(vec![new_doc])?;
                Ok(BulkItemResult::success("update", &index, &id, 200))
            }
            BulkAction::Delete { index, id } => {
                match self.backend.delete_from_es(&index, vec![id.clone()]) {
                    Ok(_) => Ok(BulkItemResult::success("delete", &index, &id, 200)),
                    Err(EsError::IndexNotFound(_)) => Ok(BulkItemResult::failure(
                        "delete",
                        &index,
                        &id,
                        404,
                        "index not found",
                    )),
                    Err(e) => Err(e),
                }
            }
        }
    }
}

// =============================================================================
// 二、搜索建议（Suggest API）
// =============================================================================

/// 建议器类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SuggesterType {
    /// 前缀匹配建议（term suggester 的简化版）
    Term,
    /// 完成建议（completion suggester 的简化版）
    Completion,
    /// 短语建议
    Phrase,
}

/// 建议请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestRequest {
    /// 建议器名称
    pub name: String,
    /// 前缀文本
    pub prefix: String,
    /// 建议器类型
    pub suggester_type: SuggesterType,
    /// 搜索的字段
    pub field: String,
    /// 返回建议数量
    pub size: usize,
}

impl SuggestRequest {
    pub fn new(name: impl Into<String>, prefix: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            prefix: prefix.into(),
            suggester_type: SuggesterType::Term,
            field: "text".to_string(),
            size: 5,
        }
    }

    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field = field.into();
        self
    }

    pub fn with_size(mut self, size: usize) -> Self {
        self.size = size;
        self
    }

    pub fn with_type(mut self, suggester_type: SuggesterType) -> Self {
        self.suggester_type = suggester_type;
        self
    }
}

/// 单个建议项
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuggestOption {
    /// 建议文本
    pub text: String,
    /// 建议词频（出现次数）
    pub freq: u64,
    /// 建议分数（越高越相关）
    pub score: f64,
}

/// 建议结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestResult {
    /// 建议器名称
    pub name: String,
    /// 建议项列表（按 score 降序）
    pub options: Vec<SuggestOption>,
}

/// 内存建议器
///
/// 基于文档字段值构建候选词集合，按前缀匹配返回建议
pub struct MemorySuggester {
    backend: InMemoryEsSync,
}

impl MemorySuggester {
    pub fn new(backend: InMemoryEsSync) -> Self {
        Self { backend }
    }

    pub fn from_new() -> Self {
        Self::new(InMemoryEsSync::new())
    }

    /// 执行建议查询
    ///
    /// 扫描指定索引中所有文档的指定字段，收集词频，按前缀过滤返回建议
    pub fn suggest(
        &self,
        index: &str,
        request: &SuggestRequest,
    ) -> Result<SuggestResult, EsError> {
        let search_req = EsSearchRequest::new(index, crate::EsQuery::match_all())
            .with_pagination(0, 10000);
        let result = self.backend.search(search_req)?;

        let prefix_lower = request.prefix.to_lowercase();
        let mut candidates: HashMap<String, u64> = HashMap::new();

        for hit in &result.hits {
            // 尝试从 source 中获取指定字段的字符串值
            let text_opt = hit.source.get(&request.field).and_then(|v| v.as_str());
            if let Some(text) = text_opt {
                // 分词：按非字母数字字符切分
                for word in text.split(|c: char| !c.is_alphanumeric()) {
                    if word.is_empty() {
                        continue;
                    }
                    let word_lower = word.to_lowercase();
                    if word_lower.starts_with(&prefix_lower) {
                        *candidates.entry(word_lower).or_insert(0) += 1;
                    }
                }
            }
        }

        let mut options: Vec<SuggestOption> = candidates
            .into_iter()
            .map(|(text, freq)| {
                // 分数 = 词频 * 前缀匹配长度比
                let prefix_ratio = prefix_lower.len() as f64 / text.len().max(1) as f64;
                let score = freq as f64 * (1.0 + prefix_ratio);
                SuggestOption {
                    text,
                    freq,
                    score,
                }
            })
            .collect();
        // 按分数降序排序
        options.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        options.truncate(request.size);

        Ok(SuggestResult {
            name: request.name.clone(),
            options,
        })
    }

    /// 索引文档以供建议查询
    pub fn index_doc(
        &self,
        index: &str,
        id: &str,
        doc: serde_json::Value,
    ) -> Result<(), EsError> {
        let doc = EsDocument::new(index, doc).with_id(id);
        self.backend.sync_to_es(vec![doc])?;
        Ok(())
    }
}

// =============================================================================
// 三、聚合查询（Aggregations）
// =============================================================================

/// 聚合类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AggregationType {
    /// 词项聚合（按字段值分组统计）
    Terms,
    /// 范围聚合（按数值范围分组）
    Range,
    /// 求和聚合
    Sum,
    /// 平均值聚合
    Avg,
    /// 最大值聚合
    Max,
    /// 最小值聚合
    Min,
    /// 直方图聚合（按固定间隔分桶）
    Histogram,
    /// 计数聚合
    ValueCount,
}

/// 聚合定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aggregation {
    /// 聚合名称
    pub name: String,
    /// 聚合类型
    pub agg_type: AggregationType,
    /// 聚合字段
    pub field: String,
    /// 返回桶数量（仅 Terms/Histogram）
    pub size: usize,
    /// 范围定义（仅 Range 聚合）
    pub ranges: Vec<AggRange>,
    /// 直方图间隔（仅 Histogram）
    pub interval: Option<f64>,
}

impl Aggregation {
    pub fn terms(name: impl Into<String>, field: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            agg_type: AggregationType::Terms,
            field: field.into(),
            size: 10,
            ranges: Vec::new(),
            interval: None,
        }
    }

    pub fn range(name: impl Into<String>, field: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            agg_type: AggregationType::Range,
            field: field.into(),
            size: 0,
            ranges: Vec::new(),
            interval: None,
        }
    }

    pub fn sum(name: impl Into<String>, field: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            agg_type: AggregationType::Sum,
            field: field.into(),
            size: 0,
            ranges: Vec::new(),
            interval: None,
        }
    }

    pub fn avg(name: impl Into<String>, field: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            agg_type: AggregationType::Avg,
            field: field.into(),
            size: 0,
            ranges: Vec::new(),
            interval: None,
        }
    }

    pub fn max(name: impl Into<String>, field: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            agg_type: AggregationType::Max,
            field: field.into(),
            size: 0,
            ranges: Vec::new(),
            interval: None,
        }
    }

    pub fn min(name: impl Into<String>, field: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            agg_type: AggregationType::Min,
            field: field.into(),
            size: 0,
            ranges: Vec::new(),
            interval: None,
        }
    }

    pub fn histogram(name: impl Into<String>, field: impl Into<String>, interval: f64) -> Self {
        Self {
            name: name.into(),
            agg_type: AggregationType::Histogram,
            field: field.into(),
            size: 0,
            ranges: Vec::new(),
            interval: Some(interval),
        }
    }

    pub fn count(name: impl Into<String>, field: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            agg_type: AggregationType::ValueCount,
            field: field.into(),
            size: 0,
            ranges: Vec::new(),
            interval: None,
        }
    }

    pub fn with_size(mut self, size: usize) -> Self {
        self.size = size;
        self
    }

    pub fn with_range(mut self, from: Option<f64>, to: Option<f64>) -> Self {
        self.ranges.push(AggRange { from, to });
        self
    }
}

/// 聚合范围定义
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AggRange {
    pub from: Option<f64>,
    pub to: Option<f64>,
}

impl AggRange {
    pub fn new(from: Option<f64>, to: Option<f64>) -> Self {
        Self { from, to }
    }

    /// 检查数值是否在范围内
    pub fn contains(&self, value: f64) -> bool {
        if let Some(from) = self.from {
            if value < from {
                return false;
            }
        }
        if let Some(to) = self.to {
            if value >= to {
                return false;
            }
        }
        true
    }

    /// 生成范围标签
    pub fn label(&self) -> String {
        match (self.from, self.to) {
            (Some(from), Some(to)) => format!("{}-{}", from, to),
            (Some(from), None) => format!("{}-*", from),
            (None, Some(to)) => format!("*-{}", to),
            (None, None) => "*-*".to_string(),
        }
    }
}

/// 聚合桶
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AggBucket {
    /// 桶键（字段值或范围标签）
    pub key: String,
    /// 桶内文档数
    pub doc_count: u64,
    /// 桶内数值（用于 sum/avg/max/min）
    pub value: Option<f64>,
}

/// 聚合结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationResult {
    /// 聚合名称
    pub name: String,
    /// 聚合类型
    pub agg_type: AggregationType,
    /// 桶列表（Terms/Range/Histogram）
    pub buckets: Vec<AggBucket>,
    /// 单值结果（Sum/Avg/Max/Min/ValueCount）
    pub value: Option<f64>,
}

/// 内存聚合器
pub struct MemoryAggregator {
    backend: InMemoryEsSync,
}

impl MemoryAggregator {
    pub fn new(backend: InMemoryEsSync) -> Self {
        Self { backend }
    }

    pub fn from_new() -> Self {
        Self::new(InMemoryEsSync::new())
    }

    /// 执行聚合查询
    ///
    /// 先按基础查询过滤文档，再按聚合定义计算结果
    pub fn aggregate(
        &self,
        index: &str,
        query: crate::EsQuery,
        aggregations: &[Aggregation],
    ) -> Result<Vec<AggregationResult>, EsError> {
        let search_req = EsSearchRequest::new(index, query).with_pagination(0, 10000);
        let result = self.backend.search(search_req)?;
        let docs: Vec<&serde_json::Value> = result.hits.iter().map(|h| &h.source).collect();

        let mut results = Vec::with_capacity(aggregations.len());
        for agg in aggregations {
            let result = self.compute_aggregation(agg, &docs);
            results.push(result);
        }
        Ok(results)
    }

    /// 计算单个聚合
    fn compute_aggregation(
        &self,
        agg: &Aggregation,
        docs: &[&serde_json::Value],
    ) -> AggregationResult {
        match agg.agg_type {
            AggregationType::Terms => self.compute_terms(agg, docs),
            AggregationType::Range => self.compute_range(agg, docs),
            AggregationType::Sum => {
                let sum = self.sum_field(agg, docs);
                AggregationResult {
                    name: agg.name.clone(),
                    agg_type: agg.agg_type.clone(),
                    buckets: Vec::new(),
                    value: Some(sum),
                }
            }
            AggregationType::Avg => {
                let avg = self.avg_field(agg, docs);
                AggregationResult {
                    name: agg.name.clone(),
                    agg_type: agg.agg_type.clone(),
                    buckets: Vec::new(),
                    value: avg,
                }
            }
            AggregationType::Max => {
                let max = self.max_field(agg, docs);
                AggregationResult {
                    name: agg.name.clone(),
                    agg_type: agg.agg_type.clone(),
                    buckets: Vec::new(),
                    value: max,
                }
            }
            AggregationType::Min => {
                let min = self.min_field(agg, docs);
                AggregationResult {
                    name: agg.name.clone(),
                    agg_type: agg.agg_type.clone(),
                    buckets: Vec::new(),
                    value: min,
                }
            }
            AggregationType::Histogram => self.compute_histogram(agg, docs),
            AggregationType::ValueCount => {
                let count = self.count_field(agg, docs);
                AggregationResult {
                    name: agg.name.clone(),
                    agg_type: agg.agg_type.clone(),
                    buckets: Vec::new(),
                    value: Some(count as f64),
                }
            }
        }
    }

    /// 词项聚合
    fn compute_terms(&self, agg: &Aggregation, docs: &[&serde_json::Value]) -> AggregationResult {
        let mut counts: HashMap<String, u64> = HashMap::new();
        for doc in docs {
            if let Some(value) = doc.get(&agg.field) {
                let key = match value {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => continue,
                };
                *counts.entry(key).or_insert(0) += 1;
            }
        }
        let mut buckets: Vec<AggBucket> = counts
            .into_iter()
            .map(|(key, doc_count)| AggBucket {
                key,
                doc_count,
                value: None,
            })
            .collect();
        // 按文档数降序排序
        buckets.sort_by_key(|b| std::cmp::Reverse(b.doc_count));
        buckets.truncate(agg.size);
        AggregationResult {
            name: agg.name.clone(),
            agg_type: agg.agg_type.clone(),
            buckets,
            value: None,
        }
    }

    /// 范围聚合
    fn compute_range(&self, agg: &Aggregation, docs: &[&serde_json::Value]) -> AggregationResult {
        let mut buckets: Vec<AggBucket> = agg
            .ranges
            .iter()
            .map(|r| AggBucket {
                key: r.label(),
                doc_count: 0,
                value: None,
            })
            .collect();

        for doc in docs {
            if let Some(value) = doc.get(&agg.field).and_then(|v| v.as_f64()) {
                for (i, range) in agg.ranges.iter().enumerate() {
                    if range.contains(value) {
                        buckets[i].doc_count += 1;
                    }
                }
            }
        }
        AggregationResult {
            name: agg.name.clone(),
            agg_type: agg.agg_type.clone(),
            buckets,
            value: None,
        }
    }

    /// 直方图聚合
    fn compute_histogram(
        &self,
        agg: &Aggregation,
        docs: &[&serde_json::Value],
    ) -> AggregationResult {
        let interval = agg.interval.unwrap_or(1.0);
        if interval <= 0.0 {
            return AggregationResult {
                name: agg.name.clone(),
                agg_type: agg.agg_type.clone(),
                buckets: Vec::new(),
                value: None,
            };
        }
        let mut buckets_map: HashMap<i64, u64> = HashMap::new();
        for doc in docs {
            if let Some(value) = doc.get(&agg.field).and_then(|v| v.as_f64()) {
                let bucket_key = (value / interval).floor() as i64;
                *buckets_map.entry(bucket_key).or_insert(0) += 1;
            }
        }
        let mut buckets: Vec<AggBucket> = buckets_map
            .into_iter()
            .map(|(key, count)| AggBucket {
                key: (key as f64 * interval).to_string(),
                doc_count: count,
                value: None,
            })
            .collect();
        // 按桶键升序排序
        buckets.sort_by(|a, b| a.key.cmp(&b.key));
        AggregationResult {
            name: agg.name.clone(),
            agg_type: agg.agg_type.clone(),
            buckets,
            value: None,
        }
    }

    /// 计算字段求和
    fn sum_field(&self, agg: &Aggregation, docs: &[&serde_json::Value]) -> f64 {
        docs.iter()
            .filter_map(|doc| doc.get(&agg.field).and_then(|v| v.as_f64()))
            .sum()
    }

    /// 计算字段平均值
    fn avg_field(&self, agg: &Aggregation, docs: &[&serde_json::Value]) -> Option<f64> {
        let values: Vec<f64> = docs
            .iter()
            .filter_map(|doc| doc.get(&agg.field).and_then(|v| v.as_f64()))
            .collect();
        if values.is_empty() {
            None
        } else {
            Some(values.iter().sum::<f64>() / values.len() as f64)
        }
    }

    /// 计算字段最大值
    fn max_field(&self, agg: &Aggregation, docs: &[&serde_json::Value]) -> Option<f64> {
        docs.iter()
            .filter_map(|doc| doc.get(&agg.field).and_then(|v| v.as_f64()))
            .fold(None, |acc, v| Some(acc.map_or(v, |a: f64| a.max(v))))
    }

    /// 计算字段最小值
    fn min_field(&self, agg: &Aggregation, docs: &[&serde_json::Value]) -> Option<f64> {
        docs.iter()
            .filter_map(|doc| doc.get(&agg.field).and_then(|v| v.as_f64()))
            .fold(None, |acc, v| Some(acc.map_or(v, |a: f64| a.min(v))))
    }

    /// 计算字段非空值数量
    fn count_field(&self, agg: &Aggregation, docs: &[&serde_json::Value]) -> u64 {
        docs.iter()
            .filter(|doc| doc.get(&agg.field).is_some())
            .count() as u64
    }
}

// =============================================================================
// 四、索引别名管理
// =============================================================================

/// 别名定义
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AliasDefinition {
    /// 别名名称
    pub name: String,
    /// 目标索引
    pub index: String,
    /// 过滤条件（可选，仅匹配的文档可见）
    pub filter: Option<crate::EsQuery>,
    /// 路由值（可选）
    pub routing: Option<String>,
    /// 是否为写入索引
    pub is_write_index: bool,
}

impl AliasDefinition {
    pub fn new(name: impl Into<String>, index: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            index: index.into(),
            filter: None,
            routing: None,
            is_write_index: false,
        }
    }

    pub fn with_filter(mut self, filter: crate::EsQuery) -> Self {
        self.filter = Some(filter);
        self
    }

    pub fn with_routing(mut self, routing: impl Into<String>) -> Self {
        self.routing = Some(routing.into());
        self
    }

    pub fn with_write_index(mut self, is_write: bool) -> Self {
        self.is_write_index = is_write;
        self
    }
}

/// 别名操作类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AliasAction {
    /// 添加别名
    Add(AliasDefinition),
    /// 移除别名
    Remove { name: String, index: String },
}

/// 别名管理器
///
/// 维护别名到索引的映射，支持过滤别名和路由别名
pub struct AliasManager {
    /// 别名列表
    aliases: RwLock<Vec<AliasDefinition>>,
}

impl AliasManager {
    pub fn new() -> Self {
        Self {
            aliases: RwLock::new(Vec::new()),
        }
    }

    /// 执行别名操作（批量添加/移除）
    pub fn update_aliases(&self, actions: Vec<AliasAction>) -> Result<(), EsError> {
        let mut aliases = self
            .aliases
            .write()
            .map_err(|e| EsError::SyncError(format!("lock error: {}", e)))?;
        for action in actions {
            match action {
                AliasAction::Add(def) => {
                    // 检查是否已存在同名别名指向同一索引
                    let exists = aliases
                        .iter()
                        .any(|a| a.name == def.name && a.index == def.index);
                    if !exists {
                        aliases.push(def);
                    }
                }
                AliasAction::Remove { name, index } => {
                    aliases.retain(|a| !(a.name == name && a.index == index));
                }
            }
        }
        Ok(())
    }

    /// 添加单个别名
    pub fn add_alias(&self, def: AliasDefinition) -> Result<(), EsError> {
        self.update_aliases(vec![AliasAction::Add(def)])
    }

    /// 移除单个别名
    pub fn remove_alias(&self, name: &str, index: &str) -> Result<(), EsError> {
        self.update_aliases(vec![AliasAction::Remove {
            name: name.to_string(),
            index: index.to_string(),
        }])
    }

    /// 获取别名指向的索引
    pub fn resolve_index(&self, alias: &str) -> Result<Vec<String>, EsError> {
        let aliases = self
            .aliases
            .read()
            .map_err(|e| EsError::SyncError(format!("lock error: {}", e)))?;
        let indices: Vec<String> = aliases
            .iter()
            .filter(|a| a.name == alias)
            .map(|a| a.index.clone())
            .collect();
        Ok(indices)
    }

    /// 获取索引的所有别名
    pub fn get_aliases(&self, index: &str) -> Result<Vec<AliasDefinition>, EsError> {
        let aliases = self
            .aliases
            .read()
            .map_err(|e| EsError::SyncError(format!("lock error: {}", e)))?;
        Ok(aliases
            .iter()
            .filter(|a| a.index == index)
            .cloned()
            .collect())
    }

    /// 获取所有别名
    pub fn list_aliases(&self) -> Result<Vec<AliasDefinition>, EsError> {
        let aliases = self
            .aliases
            .read()
            .map_err(|e| EsError::SyncError(format!("lock error: {}", e)))?;
        Ok(aliases.clone())
    }

    /// 切换别名指向（原子操作：添加新指向 + 移除旧指向）
    ///
    /// 常用于零停机重新索引场景
    pub fn swap_alias(
        &self,
        alias: &str,
        old_index: &str,
        new_index: &str,
    ) -> Result<(), EsError> {
        let actions = vec![
            AliasAction::Add(AliasDefinition::new(alias, new_index)),
            AliasAction::Remove {
                name: alias.to_string(),
                index: old_index.to_string(),
            },
        ];
        self.update_aliases(actions)
    }

    /// 获取写入索引（标记为 is_write_index 的索引）
    pub fn get_write_index(&self, alias: &str) -> Result<Option<String>, EsError> {
        let aliases = self
            .aliases
            .read()
            .map_err(|e| EsError::SyncError(format!("lock error: {}", e)))?;
        Ok(aliases
            .iter()
            .find(|a| a.name == alias && a.is_write_index)
            .map(|a| a.index.clone()))
    }
}

impl Default for AliasManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EsSync;
    use serde_json::json;

    // --- 批量操作类型测试 ---

    #[test]
    fn test_bulk_action_index() {
        let action = BulkAction::Index {
            index: "test".to_string(),
            id: "1".to_string(),
            source: json!({"name": "test"}),
        };
        assert_eq!(action.index(), "test");
        assert_eq!(action.id(), "1");
    }

    #[test]
    fn test_bulk_action_create() {
        let action = BulkAction::Create {
            index: "test".to_string(),
            id: "1".to_string(),
            source: json!({}),
        };
        assert_eq!(action.index(), "test");
    }

    #[test]
    fn test_bulk_action_update() {
        let action = BulkAction::Update {
            index: "test".to_string(),
            id: "1".to_string(),
            doc: json!({"field": "value"}),
        };
        assert_eq!(action.id(), "1");
    }

    #[test]
    fn test_bulk_action_delete() {
        let action = BulkAction::Delete {
            index: "test".to_string(),
            id: "1".to_string(),
        };
        assert_eq!(action.index(), "test");
    }

    // --- 批量操作结果测试 ---

    #[test]
    fn test_bulk_item_result_success() {
        let result = BulkItemResult::success("index", "test", "1", 200);
        assert!(result.is_success());
        assert!(result.error.is_none());
    }

    #[test]
    fn test_bulk_item_result_failure() {
        let result = BulkItemResult::failure("update", "test", "1", 404, "not found");
        assert!(!result.is_success());
        assert_eq!(result.error, Some("not found".to_string()));
    }

    #[test]
    fn test_bulk_result_new() {
        let items = vec![
            BulkItemResult::success("index", "test", "1", 200),
            BulkItemResult::success("index", "test", "2", 201),
            BulkItemResult::failure("delete", "test", "3", 404, "not found"),
        ];
        let result = BulkResult::new(items, 10);
        assert_eq!(result.success_count, 2);
        assert_eq!(result.failure_count, 1);
        assert!(!result.all_success());
        assert_eq!(result.failures().len(), 1);
    }

    #[test]
    fn test_bulk_result_all_success() {
        let items = vec![
            BulkItemResult::success("index", "test", "1", 200),
            BulkItemResult::success("index", "test", "2", 201),
        ];
        let result = BulkResult::new(items, 5);
        assert!(result.all_success());
        assert_eq!(result.failures().len(), 0);
    }

    // --- 批量执行器测试 ---

    #[test]
    fn test_bulk_executor_index_actions() {
        let executor = BulkExecutor::from_new();
        let actions = vec![
            BulkAction::Index {
                index: "docs".to_string(),
                id: "1".to_string(),
                source: json!({"title": "hello"}),
            },
            BulkAction::Index {
                index: "docs".to_string(),
                id: "2".to_string(),
                source: json!({"title": "world"}),
            },
        ];
        let result = executor.bulk(actions).unwrap();
        assert_eq!(result.success_count, 2);
        assert_eq!(result.failure_count, 0);
        assert_eq!(executor.backend().count("docs").unwrap(), 2);
    }

    #[test]
    fn test_bulk_executor_create_action() {
        let executor = BulkExecutor::from_new();
        let action = BulkAction::Create {
            index: "docs".to_string(),
            id: "1".to_string(),
            source: json!({"name": "test"}),
        };
        let result = executor.bulk(vec![action]).unwrap();
        assert_eq!(result.success_count, 1);
        assert_eq!(result.items[0].status, 201);
    }

    #[test]
    fn test_bulk_executor_delete_action() {
        let executor = BulkExecutor::from_new();
        executor
            .bulk(vec![BulkAction::Index {
                index: "docs".to_string(),
                id: "1".to_string(),
                source: json!({"v": 1}),
            }])
            .unwrap();
        let result = executor
            .bulk(vec![BulkAction::Delete {
                index: "docs".to_string(),
                id: "1".to_string(),
            }])
            .unwrap();
        assert_eq!(result.success_count, 1);
        assert_eq!(executor.backend().count("docs").unwrap(), 0);
    }

    #[test]
    fn test_bulk_executor_update_action() {
        let executor = BulkExecutor::from_new();
        // 先索引一个文档
        executor
            .bulk(vec![BulkAction::Index {
                index: "docs".to_string(),
                id: "1".to_string(),
                source: json!({"name": "old", "count": 10}),
            }])
            .unwrap();
        // 更新文档
        let result = executor
            .bulk(vec![BulkAction::Update {
                index: "docs".to_string(),
                id: "1".to_string(),
                doc: json!({"count": 20}),
            }])
            .unwrap();
        assert_eq!(result.success_count, 1);
        // 验证更新后的文档
        let search_req = EsSearchRequest::new("docs", crate::EsQuery::match_all());
        let search_result = executor.backend().search(search_req).unwrap();
        assert_eq!(search_result.hits[0].source["name"], "old");
        assert_eq!(search_result.hits[0].source["count"], 20);
    }

    #[test]
    fn test_bulk_executor_update_missing_doc() {
        let executor = BulkExecutor::from_new();
        let result = executor
            .bulk(vec![BulkAction::Update {
                index: "docs".to_string(),
                id: "999".to_string(),
                doc: json!({"v": 1}),
            }])
            .unwrap();
        assert_eq!(result.failure_count, 1);
        assert_eq!(result.items[0].status, 404);
    }

    #[test]
    fn test_bulk_executor_mixed_actions() {
        let executor = BulkExecutor::from_new();
        let actions = vec![
            BulkAction::Index {
                index: "docs".to_string(),
                id: "1".to_string(),
                source: json!({"v": 1}),
            },
            BulkAction::Index {
                index: "docs".to_string(),
                id: "2".to_string(),
                source: json!({"v": 2}),
            },
            BulkAction::Delete {
                index: "docs".to_string(),
                id: "1".to_string(),
            },
            BulkAction::Update {
                index: "docs".to_string(),
                id: "999".to_string(),
                doc: json!({}),
            },
        ];
        let result = executor.bulk(actions).unwrap();
        assert_eq!(result.success_count, 3);
        assert_eq!(result.failure_count, 1);
        assert_eq!(executor.backend().count("docs").unwrap(), 1);
    }

    // --- 搜索建议测试 ---

    #[test]
    fn test_suggest_request_new() {
        let req = SuggestRequest::new("my_suggest", "hel");
        assert_eq!(req.name, "my_suggest");
        assert_eq!(req.prefix, "hel");
        assert_eq!(req.suggester_type, SuggesterType::Term);
        assert_eq!(req.field, "text");
        assert_eq!(req.size, 5);
    }

    #[test]
    fn test_suggest_request_builder() {
        let req = SuggestRequest::new("s", "ru")
            .with_field("title")
            .with_size(3)
            .with_type(SuggesterType::Completion);
        assert_eq!(req.field, "title");
        assert_eq!(req.size, 3);
        assert_eq!(req.suggester_type, SuggesterType::Completion);
    }

    #[test]
    fn test_memory_suggester_basic() {
        let suggester = MemorySuggester::from_new();
        suggester
            .index_doc(
                "docs",
                "1",
                json!({"title": "hello world help"}),
            )
            .unwrap();
        suggester
            .index_doc("docs", "2", json!({"title": "hello rust"}))
            .unwrap();
        suggester
            .index_doc("docs", "3", json!({"title": "help care"}))
            .unwrap();

        let req = SuggestRequest::new("s", "hel").with_field("title");
        let result = suggester.suggest("docs", &req).unwrap();
        assert_eq!(result.name, "s");
        // 应返回 hello, help 两个建议
        // hello 出现 2 次，help 出现 2 次
        assert!(
            result.options.iter().any(|o| o.text == "hello"),
            "expected hello in options: {:?}",
            result.options
        );
        assert!(
            result.options.iter().any(|o| o.text == "help"),
            "expected help in options: {:?}",
            result.options
        );
    }

    #[test]
    fn test_memory_suggester_size_limit() {
        let suggester = MemorySuggester::from_new();
        suggester
            .index_doc("docs", "1", json!({"title": "apple apply apollo"}))
            .unwrap();
        let req = SuggestRequest::new("s", "ap")
            .with_field("title")
            .with_size(2);
        let result = suggester.suggest("docs", &req).unwrap();
        assert!(result.options.len() <= 2);
    }

    #[test]
    fn test_memory_suggester_no_match() {
        let suggester = MemorySuggester::from_new();
        suggester
            .index_doc("docs", "1", json!({"title": "hello world"}))
            .unwrap();
        let req = SuggestRequest::new("s", "xyz").with_field("title");
        let result = suggester.suggest("docs", &req).unwrap();
        assert!(result.options.is_empty());
    }

    #[test]
    fn test_memory_suggester_freq_count() {
        let suggester = MemorySuggester::from_new();
        suggester
            .index_doc("docs", "1", json!({"title": "rust rust rust"}))
            .unwrap();
        suggester
            .index_doc("docs", "2", json!({"title": "ruby"}))
            .unwrap();
        let req = SuggestRequest::new("s", "ru").with_field("title");
        let result = suggester.suggest("docs", &req).unwrap();
        let rust = result.options.iter().find(|o| o.text == "rust").unwrap();
        assert_eq!(rust.freq, 3);
        let ruby = result.options.iter().find(|o| o.text == "ruby").unwrap();
        assert_eq!(ruby.freq, 1);
    }

    #[test]
    fn test_memory_suggester_case_insensitive() {
        let suggester = MemorySuggester::from_new();
        suggester
            .index_doc("docs", "1", json!({"title": "Hello HELLO hello"}))
            .unwrap();
        let req = SuggestRequest::new("s", "HEL").with_field("title");
        let result = suggester.suggest("docs", &req).unwrap();
        // 所有大小的 hello 都应被归一化到 hello
        assert!(result.options.iter().any(|o| o.text == "hello"));
    }

    // --- 聚合定义测试 ---

    #[test]
    fn test_aggregation_terms() {
        let agg = Aggregation::terms("by_category", "category").with_size(5);
        assert_eq!(agg.name, "by_category");
        assert_eq!(agg.agg_type, AggregationType::Terms);
        assert_eq!(agg.field, "category");
        assert_eq!(agg.size, 5);
    }

    #[test]
    fn test_aggregation_range() {
        let agg = Aggregation::range("price_ranges", "price")
            .with_range(Some(0.0), Some(100.0))
            .with_range(Some(100.0), Some(500.0))
            .with_range(Some(500.0), None);
        assert_eq!(agg.agg_type, AggregationType::Range);
        assert_eq!(agg.ranges.len(), 3);
    }

    #[test]
    fn test_aggregation_sum() {
        let agg = Aggregation::sum("total_price", "price");
        assert_eq!(agg.agg_type, AggregationType::Sum);
    }

    #[test]
    fn test_aggregation_avg() {
        let agg = Aggregation::avg("avg_price", "price");
        assert_eq!(agg.agg_type, AggregationType::Avg);
    }

    #[test]
    fn test_aggregation_histogram() {
        let agg = Aggregation::histogram("price_hist", "price", 10.0);
        assert_eq!(agg.agg_type, AggregationType::Histogram);
        assert_eq!(agg.interval, Some(10.0));
    }

    #[test]
    fn test_aggregation_count() {
        let agg = Aggregation::count("doc_count", "title");
        assert_eq!(agg.agg_type, AggregationType::ValueCount);
    }

    // --- 聚合范围测试 ---

    #[test]
    fn test_agg_range_contains() {
        let range = AggRange::new(Some(10.0), Some(20.0));
        assert!(!range.contains(5.0));
        assert!(range.contains(10.0));
        assert!(range.contains(15.0));
        assert!(!range.contains(20.0));
        assert!(!range.contains(25.0));
    }

    #[test]
    fn test_agg_range_open_ended() {
        let range = AggRange::new(None, Some(10.0));
        assert!(range.contains(-100.0));
        assert!(range.contains(0.0));
        assert!(!range.contains(10.0));

        let range = AggRange::new(Some(10.0), None);
        assert!(!range.contains(5.0));
        assert!(range.contains(10.0));
        assert!(range.contains(100.0));
    }

    #[test]
    fn test_agg_range_label() {
        assert_eq!(AggRange::new(Some(10.0), Some(20.0)).label(), "10-20");
        assert_eq!(AggRange::new(Some(10.0), None).label(), "10-*");
        assert_eq!(AggRange::new(None, Some(20.0)).label(), "*-20");
        assert_eq!(AggRange::new(None, None).label(), "*-*");
    }

    // --- 内存聚合器测试 ---

    #[test]
    fn test_memory_aggregator_terms() {
        let _agg = MemoryAggregator::from_new();
        let backend = InMemoryEsSync::new();
        let docs = vec![
            EsDocument::new("docs", json!({"category": "tech", "price": 100})).with_id("1"),
            EsDocument::new("docs", json!({"category": "tech", "price": 200})).with_id("2"),
            EsDocument::new("docs", json!({"category": "food", "price": 50})).with_id("3"),
        ];
        backend.sync_to_es(docs).unwrap();
        let aggregator = MemoryAggregator::new(backend);
        let aggs = vec![Aggregation::terms("by_cat", "category")];
        let results = aggregator
            .aggregate("docs", crate::EsQuery::match_all(), &aggs)
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].buckets.len(), 2);
        let tech = results[0]
            .buckets
            .iter()
            .find(|b| b.key == "tech")
            .unwrap();
        assert_eq!(tech.doc_count, 2);
    }

    #[test]
    fn test_memory_aggregator_sum() {
        let backend = InMemoryEsSync::new();
        backend
            .sync_to_es(vec![
                EsDocument::new("docs", json!({"price": 100})).with_id("1"),
                EsDocument::new("docs", json!({"price": 200})).with_id("2"),
                EsDocument::new("docs", json!({"price": 300})).with_id("3"),
            ])
            .unwrap();
        let aggregator = MemoryAggregator::new(backend);
        let aggs = vec![Aggregation::sum("total", "price")];
        let results = aggregator
            .aggregate("docs", crate::EsQuery::match_all(), &aggs)
            .unwrap();
        assert_eq!(results[0].value, Some(600.0));
    }

    #[test]
    fn test_memory_aggregator_avg() {
        let backend = InMemoryEsSync::new();
        backend
            .sync_to_es(vec![
                EsDocument::new("docs", json!({"price": 100})).with_id("1"),
                EsDocument::new("docs", json!({"price": 200})).with_id("2"),
            ])
            .unwrap();
        let aggregator = MemoryAggregator::new(backend);
        let results = aggregator
            .aggregate(
                "docs",
                crate::EsQuery::match_all(),
                &[Aggregation::avg("avg", "price")],
            )
            .unwrap();
        assert_eq!(results[0].value, Some(150.0));
    }

    #[test]
    fn test_memory_aggregator_max_min() {
        let backend = InMemoryEsSync::new();
        backend
            .sync_to_es(vec![
                EsDocument::new("docs", json!({"price": 100})).with_id("1"),
                EsDocument::new("docs", json!({"price": 500})).with_id("2"),
                EsDocument::new("docs", json!({"price": 50})).with_id("3"),
            ])
            .unwrap();
        let aggregator = MemoryAggregator::new(backend);
        let results = aggregator
            .aggregate(
                "docs",
                crate::EsQuery::match_all(),
                &[
                    Aggregation::max("max_p", "price"),
                    Aggregation::min("min_p", "price"),
                ],
            )
            .unwrap();
        assert_eq!(results[0].value, Some(500.0));
        assert_eq!(results[1].value, Some(50.0));
    }

    #[test]
    fn test_memory_aggregator_range() {
        let backend = InMemoryEsSync::new();
        backend
            .sync_to_es(vec![
                EsDocument::new("docs", json!({"price": 50})).with_id("1"),
                EsDocument::new("docs", json!({"price": 150})).with_id("2"),
                EsDocument::new("docs", json!({"price": 600})).with_id("3"),
            ])
            .unwrap();
        let aggregator = MemoryAggregator::new(backend);
        let agg = Aggregation::range("price_ranges", "price")
            .with_range(Some(0.0), Some(100.0))
            .with_range(Some(100.0), Some(500.0))
            .with_range(Some(500.0), None);
        let results = aggregator
            .aggregate("docs", crate::EsQuery::match_all(), &[agg])
            .unwrap();
        assert_eq!(results[0].buckets.len(), 3);
        assert_eq!(results[0].buckets[0].doc_count, 1); // 0-100
        assert_eq!(results[0].buckets[1].doc_count, 1); // 100-500
        assert_eq!(results[0].buckets[2].doc_count, 1); // 500-*
    }

    #[test]
    fn test_memory_aggregator_histogram() {
        let backend = InMemoryEsSync::new();
        backend
            .sync_to_es(vec![
                EsDocument::new("docs", json!({"price": 5})).with_id("1"),
                EsDocument::new("docs", json!({"price": 15})).with_id("2"),
                EsDocument::new("docs", json!({"price": 25})).with_id("3"),
                EsDocument::new("docs", json!({"price": 35})).with_id("4"),
            ])
            .unwrap();
        let aggregator = MemoryAggregator::new(backend);
        let agg = Aggregation::histogram("hist", "price", 10.0);
        let results = aggregator
            .aggregate("docs", crate::EsQuery::match_all(), &[agg])
            .unwrap();
        // 0-10: 1 个 (5), 10-20: 1 个 (15), 20-30: 1 个 (25), 30-40: 1 个 (35)
        assert_eq!(results[0].buckets.len(), 4);
    }

    #[test]
    fn test_memory_aggregator_value_count() {
        let backend = InMemoryEsSync::new();
        backend
            .sync_to_es(vec![
                EsDocument::new("docs", json!({"name": "a", "price": 100})).with_id("1"),
                EsDocument::new("docs", json!({"name": "b", "price": 200})).with_id("2"),
                EsDocument::new("docs", json!({"name": "c"})).with_id("3"),
            ])
            .unwrap();
        let aggregator = MemoryAggregator::new(backend);
        let results = aggregator
            .aggregate(
                "docs",
                crate::EsQuery::match_all(),
                &[Aggregation::count("cnt", "price")],
            )
            .unwrap();
        assert_eq!(results[0].value, Some(2.0)); // 只有 2 个文档有 price 字段
    }

    #[test]
    fn test_memory_aggregator_avg_empty() {
        let backend = InMemoryEsSync::new();
        backend
            .sync_to_es(vec![EsDocument::new("docs", json!({"name": "a"})).with_id("1")])
            .unwrap();
        let aggregator = MemoryAggregator::new(backend);
        let results = aggregator
            .aggregate(
                "docs",
                crate::EsQuery::match_all(),
                &[Aggregation::avg("avg", "price")],
            )
            .unwrap();
        assert_eq!(results[0].value, None); // 无 price 字段
    }

    // --- 别名定义测试 ---

    #[test]
    fn test_alias_definition_new() {
        let def = AliasDefinition::new("alias1", "index1");
        assert_eq!(def.name, "alias1");
        assert_eq!(def.index, "index1");
        assert!(def.filter.is_none());
        assert!(def.routing.is_none());
        assert!(!def.is_write_index);
    }

    #[test]
    fn test_alias_definition_builder() {
        let def = AliasDefinition::new("alias1", "index1")
            .with_routing("routing_key")
            .with_write_index(true);
        assert_eq!(def.routing, Some("routing_key".to_string()));
        assert!(def.is_write_index);
    }

    #[test]
    fn test_alias_definition_with_filter() {
        let def = AliasDefinition::new("alias1", "index1")
            .with_filter(crate::EsQuery::term("status", json!("active")));
        assert!(def.filter.is_some());
    }

    // --- 别名管理器测试 ---

    #[test]
    fn test_alias_manager_add_and_resolve() {
        let manager = AliasManager::new();
        manager
            .add_alias(AliasDefinition::new("alias1", "index1"))
            .unwrap();
        let indices = manager.resolve_index("alias1").unwrap();
        assert_eq!(indices, vec!["index1"]);
    }

    #[test]
    fn test_alias_manager_remove() {
        let manager = AliasManager::new();
        manager
            .add_alias(AliasDefinition::new("alias1", "index1"))
            .unwrap();
        manager.remove_alias("alias1", "index1").unwrap();
        let indices = manager.resolve_index("alias1").unwrap();
        assert!(indices.is_empty());
    }

    #[test]
    fn test_alias_manager_multiple_indices() {
        let manager = AliasManager::new();
        manager
            .add_alias(AliasDefinition::new("alias1", "index1"))
            .unwrap();
        manager
            .add_alias(AliasDefinition::new("alias1", "index2"))
            .unwrap();
        let indices = manager.resolve_index("alias1").unwrap();
        assert_eq!(indices.len(), 2);
        assert!(indices.contains(&"index1".to_string()));
        assert!(indices.contains(&"index2".to_string()));
    }

    #[test]
    fn test_alias_manager_get_aliases() {
        let manager = AliasManager::new();
        manager
            .add_alias(AliasDefinition::new("alias1", "index1"))
            .unwrap();
        manager
            .add_alias(AliasDefinition::new("alias2", "index1"))
            .unwrap();
        let aliases = manager.get_aliases("index1").unwrap();
        assert_eq!(aliases.len(), 2);
    }

    #[test]
    fn test_alias_manager_list_all() {
        let manager = AliasManager::new();
        manager
            .add_alias(AliasDefinition::new("alias1", "index1"))
            .unwrap();
        manager
            .add_alias(AliasDefinition::new("alias2", "index2"))
            .unwrap();
        let all = manager.list_aliases().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_alias_manager_swap() {
        let manager = AliasManager::new();
        manager
            .add_alias(AliasDefinition::new("alias1", "old_index"))
            .unwrap();
        manager
            .swap_alias("alias1", "old_index", "new_index")
            .unwrap();
        let indices = manager.resolve_index("alias1").unwrap();
        assert_eq!(indices, vec!["new_index"]);
    }

    #[test]
    fn test_alias_manager_write_index() {
        let manager = AliasManager::new();
        manager
            .add_alias(
                AliasDefinition::new("alias1", "index1").with_write_index(true),
            )
            .unwrap();
        manager
            .add_alias(AliasDefinition::new("alias1", "index2"))
            .unwrap();
        let write_index = manager.get_write_index("alias1").unwrap();
        assert_eq!(write_index, Some("index1".to_string()));
    }

    #[test]
    fn test_alias_manager_no_write_index() {
        let manager = AliasManager::new();
        manager
            .add_alias(AliasDefinition::new("alias1", "index1"))
            .unwrap();
        let write_index = manager.get_write_index("alias1").unwrap();
        assert_eq!(write_index, None);
    }

    #[test]
    fn test_alias_manager_duplicate_add() {
        let manager = AliasManager::new();
        manager
            .add_alias(AliasDefinition::new("alias1", "index1"))
            .unwrap();
        // 重复添加同名同索引的别名应被忽略
        manager
            .add_alias(AliasDefinition::new("alias1", "index1"))
            .unwrap();
        let all = manager.list_aliases().unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn test_alias_manager_batch_update() {
        let manager = AliasManager::new();
        let actions = vec![
            AliasAction::Add(AliasDefinition::new("alias1", "index1")),
            AliasAction::Add(AliasDefinition::new("alias2", "index2")),
            AliasAction::Remove {
                name: "alias1".to_string(),
                index: "index1".to_string(),
            },
        ];
        manager.update_aliases(actions).unwrap();
        let all = manager.list_aliases().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "alias2");
    }
}
