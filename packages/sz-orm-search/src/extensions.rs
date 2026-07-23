//! Search 深度扩展功能
//!
//! 本模块补充全文搜索扩展缺失的核心深度功能，包括：
//!
//! - **搜索结果高亮**：在匹配文本中高亮查询词，支持 HTML/纯文本两种输出格式
//! - **分词器配置**：简易分词器（按空白/标点切分），支持大小写归一化、停用词过滤
//! - **搜索权重 / boost**：字段级权重配置，影响相关性评分
//! - **分面搜索（Faceted Search）**：按字段聚合统计，返回每个分值的命中数
//!
//! # 设计说明
//!
//! 本模块以**独立函数 + 扩展 trait** 的方式提供，不修改既有 `SearchExt` trait，
//! 避免破坏已有的 memory / stub / real provider 实现。
//! 内存计算部分基于纯 Rust 实现，不依赖外部库。

#![allow(dead_code)]

use crate::error::SearchError;
use crate::types::{SearchQuery, SearchResult};
use crate::SearchExt;
use serde_json::Value;
use std::collections::HashMap;

// =============================================================================
// 一、搜索结果高亮
// =============================================================================

/// 高亮输出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightFormat {
    /// HTML 格式：`<em>matched</em>`
    Html,
    /// 纯文本格式：`**matched**`
    Text,
    /// Markdown 格式：`==matched==`
    Markdown,
}

impl HighlightFormat {
    /// 生成高亮开始标签
    pub fn prefix(&self) -> &'static str {
        match self {
            HighlightFormat::Html => "<em>",
            HighlightFormat::Text => "**",
            HighlightFormat::Markdown => "==",
        }
    }

    /// 生成高亮结束标签
    pub fn suffix(&self) -> &'static str {
        match self {
            HighlightFormat::Html => "</em>",
            HighlightFormat::Text => "**",
            HighlightFormat::Markdown => "==",
        }
    }
}

/// 高亮配置
#[derive(Debug, Clone)]
pub struct HighlightConfig {
    /// 输出格式
    pub format: HighlightFormat,
    /// 需要高亮的字段列表（为空则高亮所有文本字段）
    pub fields: Vec<String>,
    /// 高亮片段最大长度（超出则截断，0 表示不截断）
    pub fragment_size: usize,
    /// 返回的高亮片段数量
    pub number_of_fragments: usize,
}

impl HighlightConfig {
    /// 创建默认高亮配置（HTML 格式，无字段限制，片段长度 150）
    pub fn new() -> Self {
        Self {
            format: HighlightFormat::Html,
            fields: Vec::new(),
            fragment_size: 150,
            number_of_fragments: 1,
        }
    }

    /// 设置输出格式
    pub fn with_format(mut self, format: HighlightFormat) -> Self {
        self.format = format;
        self
    }

    /// 添加需要高亮的字段
    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.fields.push(field.into());
        self
    }

    /// 设置片段最大长度
    pub fn with_fragment_size(mut self, size: usize) -> Self {
        self.fragment_size = size;
        self
    }
}

impl Default for HighlightConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// 搜索结果高亮器
pub struct Highlighter {
    config: HighlightConfig,
}

impl Highlighter {
    pub fn new(config: HighlightConfig) -> Self {
        Self { config }
    }

    /// 对文本进行高亮处理
    ///
    /// 将文本中所有出现的 query 词用高亮标签包裹
    pub fn highlight_text(&self, text: &str, query: &str) -> String {
        if query.is_empty() || text.is_empty() {
            return text.to_string();
        }
        let query_lower = query.to_lowercase();
        let text_lower = text.to_lowercase();
        let prefix = self.config.format.prefix();
        let suffix = self.config.format.suffix();

        let mut result = String::with_capacity(text.len() + query.len() * 4);
        let mut last_end = 0;
        let mut remaining = text_lower.as_str();
        let mut offset = 0;

        while let Some(pos) = remaining.find(&query_lower) {
            let abs_pos = offset + pos;
            // 添加匹配前的文本
            result.push_str(&text[last_end..abs_pos]);
            // 添加高亮包裹的匹配文本
            result.push_str(prefix);
            result.push_str(&text[abs_pos..abs_pos + query.len()]);
            result.push_str(suffix);
            last_end = abs_pos + query.len();
            offset = last_end;
            remaining = &text_lower[last_end..];
        }
        // 添加剩余文本
        result.push_str(&text[last_end..]);

        // 截断片段
        if self.config.fragment_size > 0 && result.len() > self.config.fragment_size {
            // 找到第一个高亮开始位置，以此为中心截取片段
            if let Some(highlight_start) = result.find(prefix) {
                let center = highlight_start.saturating_sub(self.config.fragment_size / 3);
                let end = (center + self.config.fragment_size).min(result.len());
                let start = center.min(result.len());
                let mut fragment = String::new();
                if start > 0 {
                    fragment.push_str("...");
                }
                fragment.push_str(&result[start..end]);
                if end < result.len() {
                    fragment.push_str("...");
                }
                return fragment;
            }
        }

        result
    }

    /// 对 JSON 文档中的指定字段进行高亮
    ///
    /// 返回 field -> highlighted_text 的映射
    pub fn highlight_doc(&self, doc: &Value, query: &str) -> HashMap<String, String> {
        let mut highlights = HashMap::new();
        let query_terms = self.extract_query_terms(query);

        if let Some(obj) = doc.as_object() {
            for (field, value) in obj {
                // 如果配置了字段列表，只处理配置的字段
                if !self.config.fields.is_empty() && !self.config.fields.contains(field) {
                    continue;
                }
                // 只处理字符串值
                if let Some(text) = value.as_str() {
                    let mut best_highlight = String::new();
                    let mut best_score = 0;
                    for term in &query_terms {
                        let highlighted = self.highlight_text(text, term);
                        let score = text.to_lowercase().matches(&term.to_lowercase()).count();
                        if score > best_score {
                            best_score = score;
                            best_highlight = highlighted;
                        }
                    }
                    if !best_highlight.is_empty() {
                        highlights.insert(field.clone(), best_highlight);
                    }
                }
            }
        }
        highlights
    }

    /// 从查询字符串中提取查询词
    ///
    /// 简单按空白切分，去除空词
    fn extract_query_terms(&self, query: &str) -> Vec<String> {
        if query.is_empty() {
            return Vec::new();
        }
        // 如果查询是一个词，直接返回
        if !query.contains(char::is_whitespace) {
            return vec![query.to_string()];
        }
        // 多词查询：返回整个查询和各个词
        let mut terms = vec![query.to_string()];
        terms.extend(query.split_whitespace().map(|s| s.to_string()));
        terms
    }
}

impl Default for Highlighter {
    /// 使用默认配置创建高亮器
    fn default() -> Self {
        Self::new(HighlightConfig::new())
    }
}

// =============================================================================
// 二、分词器配置
// =============================================================================

/// 分词器类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenizerType {
    /// 标准分词器：按空白和标点切分
    Standard,
    /// 空白分词器：仅按空白切分
    Whitespace,
    /// 小写分词器：将文本转为小写后按非字母切分
    Lowercase,
    /// 关键词分词器：不切分，整个文本作为一个词
    Keyword,
}

impl TokenizerType {
    /// 转为 ES 分析器名称
    pub fn as_es_analyzer(&self) -> &'static str {
        match self {
            TokenizerType::Standard => "standard",
            TokenizerType::Whitespace => "whitespace",
            TokenizerType::Lowercase => "lowercase",
            TokenizerType::Keyword => "keyword",
        }
    }
}

/// 分词器配置
#[derive(Debug, Clone)]
pub struct TokenizerConfig {
    /// 分词器类型
    pub tokenizer: TokenizerType,
    /// 是否转换为小写
    pub lowercase: bool,
    /// 停用词列表
    pub stop_words: Vec<String>,
    /// 最小词长度（短于此长度的词将被丢弃）
    pub min_token_length: usize,
}

impl TokenizerConfig {
    /// 创建默认分词器配置（标准分词器，小写转换）
    pub fn new() -> Self {
        Self::standard()
    }

    /// 创建标准分词器配置
    pub fn standard() -> Self {
        Self {
            tokenizer: TokenizerType::Standard,
            lowercase: true,
            stop_words: Vec::new(),
            min_token_length: 1,
        }
    }

    /// 创建空白分词器配置
    pub fn whitespace() -> Self {
        Self {
            tokenizer: TokenizerType::Whitespace,
            lowercase: false,
            stop_words: Vec::new(),
            min_token_length: 1,
        }
    }

    /// 设置是否小写转换
    pub fn with_lowercase(mut self, lowercase: bool) -> Self {
        self.lowercase = lowercase;
        self
    }

    /// 添加停用词
    pub fn with_stop_words(mut self, words: Vec<String>) -> Self {
        self.stop_words = words;
        self
    }

    /// 设置最小词长度
    pub fn with_min_length(mut self, len: usize) -> Self {
        self.min_token_length = len;
        self
    }

    /// 转为 ES 分析器配置 JSON
    pub fn to_es_analyzer_config(&self) -> Value {
        let mut config = serde_json::json!({
            "type": self.tokenizer.as_es_analyzer()
        });
        if !self.stop_words.is_empty() {
            config["stopwords"] = Value::Array(
                self.stop_words
                    .iter()
                    .map(|w| Value::String(w.clone()))
                    .collect(),
            );
        }
        config
    }
}

impl Default for TokenizerConfig {
    fn default() -> Self {
        Self::standard()
    }
}

/// 简易分词器
pub struct Tokenizer {
    config: TokenizerConfig,
}

impl Tokenizer {
    pub fn new(config: TokenizerConfig) -> Self {
        Self { config }
    }

    pub fn standard() -> Self {
        Self::new(TokenizerConfig::standard())
    }

    /// 对文本进行分词
    ///
    /// 返回词列表（已应用停用词过滤和长度过滤）
    pub fn tokenize(&self, text: &str) -> Vec<String> {
        if matches!(self.config.tokenizer, TokenizerType::Keyword) {
            // 关键词分词器：不切分
            let token = if self.config.lowercase {
                text.to_lowercase()
            } else {
                text.to_string()
            };
            return vec![token];
        }

        let raw_tokens: Vec<&str> = match self.config.tokenizer {
            TokenizerType::Whitespace => text.split_whitespace().collect(),
            TokenizerType::Standard | TokenizerType::Lowercase => {
                // 按非字母数字切分
                text.split(|c: char| !c.is_alphanumeric()).collect()
            }
            TokenizerType::Keyword => unreachable!(),
        };

        raw_tokens
            .into_iter()
            .filter_map(|t| {
                let token = if self.config.lowercase {
                    t.to_lowercase()
                } else {
                    t.to_string()
                };
                // 过滤停用词
                if self.config.stop_words.contains(&token) {
                    return None;
                }
                // 过滤短词
                if token.len() < self.config.min_token_length {
                    return None;
                }
                if token.is_empty() {
                    return None;
                }
                Some(token)
            })
            .collect()
    }

    /// 分词并返回词频统计
    pub fn tokenize_with_freq(&self, text: &str) -> HashMap<String, usize> {
        let mut freq = HashMap::new();
        for token in self.tokenize(text) {
            *freq.entry(token).or_insert(0) += 1;
        }
        freq
    }
}

// =============================================================================
// 三、搜索权重 / boost
// =============================================================================

/// 字段权重配置
#[derive(Debug, Clone)]
pub struct FieldBoost {
    /// 字段名 -> 权重值（1.0 为默认）
    pub boosts: HashMap<String, f64>,
    /// 默认权重
    pub default_boost: f64,
}

impl FieldBoost {
    pub fn new() -> Self {
        Self {
            boosts: HashMap::new(),
            default_boost: 1.0,
        }
    }

    /// 设置字段权重
    pub fn with_boost(mut self, field: impl Into<String>, boost: f64) -> Self {
        self.boosts.insert(field.into(), boost);
        self
    }

    /// 设置默认权重
    pub fn with_default(mut self, boost: f64) -> Self {
        self.default_boost = boost;
        self
    }

    /// 获取字段权重
    pub fn get_boost(&self, field: &str) -> f64 {
        self.boosts.get(field).copied().unwrap_or(self.default_boost)
    }

    /// 转为 ES multi_match 配置 JSON
    pub fn to_es_fields_config(&self) -> Value {
        if self.boosts.is_empty() {
            return Value::Null;
        }
        let fields: Vec<String> = self
            .boosts
            .iter()
            .map(|(field, boost)| format!("{}^{}", field, boost))
            .collect();
        Value::Array(fields.into_iter().map(Value::String).collect())
    }
}

impl Default for FieldBoost {
    fn default() -> Self {
        Self::new()
    }
}

/// 带权重的评分计算器
pub struct BoostScorer {
    field_boost: FieldBoost,
    tokenizer: Tokenizer,
}

impl BoostScorer {
    pub fn new(field_boost: FieldBoost, tokenizer: Tokenizer) -> Self {
        Self {
            field_boost,
            tokenizer,
        }
    }

    /// 计算文档的相关性分数（带字段权重）
    ///
    /// 评分逻辑：对每个字段，统计查询词出现次数，乘以字段权重，最后求和
    pub fn compute_score(&self, query: &str, doc: &Value) -> f64 {
        if query.is_empty() {
            return 1.0;
        }
        let query_terms = self.tokenizer.tokenize(query);
        if query_terms.is_empty() {
            return 0.0;
        }

        let mut total_score = 0.0;
        if let Some(obj) = doc.as_object() {
            for (field, value) in obj {
                if let Some(text) = value.as_str() {
                    let doc_tokens = self.tokenizer.tokenize(text);
                    let doc_freq: HashMap<String, usize> = {
                        let mut freq = HashMap::new();
                        for t in doc_tokens {
                            *freq.entry(t).or_insert(0) += 1;
                        }
                        freq
                    };
                    let field_score: f64 = query_terms
                        .iter()
                        .map(|term| *doc_freq.get(term).unwrap_or(&0) as f64)
                        .sum();
                    let boost = self.field_boost.get_boost(field);
                    total_score += field_score * boost;
                }
            }
        }
        total_score
    }
}

// =============================================================================
// 四、分面搜索（Faceted Search）
// =============================================================================

/// 分面字段定义
#[derive(Debug, Clone)]
pub struct FacetField {
    /// 字段名
    pub field: String,
    /// 返回的分面值数量上限
    pub size: usize,
}

impl FacetField {
    pub fn new(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            size: 10,
        }
    }

    pub fn with_size(mut self, size: usize) -> Self {
        self.size = size;
        self
    }
}

/// 单个分面值及其计数
#[derive(Debug, Clone, PartialEq)]
pub struct FacetValue {
    /// 分面值
    pub value: String,
    /// 命中数
    pub count: u64,
}

/// 分面结果
#[derive(Debug, Clone, PartialEq)]
pub struct FacetResult {
    /// 字段名
    pub field: String,
    /// 分面值列表（按 count 降序）
    pub values: Vec<FacetValue>,
}

/// 分面搜索结果
#[derive(Debug, Clone)]
pub struct FacetedSearchResult {
    /// 基础搜索结果
    pub search_result: SearchResult,
    /// 各字段的分面结果
    pub facets: Vec<FacetResult>,
}

/// 分面搜索扩展 trait
#[async_trait::async_trait]
pub trait FacetedSearchExt: SearchExt {
    /// 执行分面搜索
    ///
    /// 同时返回搜索结果和指定字段的分面统计
    async fn faceted_search(
        &self,
        index: &str,
        query: &SearchQuery,
        facet_fields: &[FacetField],
    ) -> Result<FacetedSearchResult, SearchError>;
}

// =============================================================================
// 五、内存版分面搜索实现
// =============================================================================

/// 内存版分面搜索实现
///
/// 包装 `MemorySearch`，提供分面搜索能力。
pub struct MemoryFacetedSearch {
    search: crate::memory::MemorySearch,
}

impl MemoryFacetedSearch {
    pub fn new(search: crate::memory::MemorySearch) -> Self {
        Self { search }
    }

    pub fn from_new() -> Self {
        Self::new(crate::memory::MemorySearch::new())
    }

    /// 获取内部 search 引用
    pub fn inner(&self) -> &crate::memory::MemorySearch {
        &self.search
    }

    /// 计算单字段分面
    fn compute_facet(docs: &HashMap<String, Value>, field: &str, size: usize) -> FacetResult {
        let mut counts: HashMap<String, u64> = HashMap::new();
        for doc in docs.values() {
            if let Some(val) = doc.get(field) {
                let key = match val {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => continue,
                };
                *counts.entry(key).or_insert(0) += 1;
            }
        }
        let mut values: Vec<FacetValue> = counts
            .into_iter()
            .map(|(value, count)| FacetValue { value, count })
            .collect();
        // 按 count 降序排序
        values.sort_by_key(|v| std::cmp::Reverse(v.count));
        values.truncate(size);
        FacetResult {
            field: field.to_string(),
            values,
        }
    }
}

/// 委托实现 SearchExt
#[async_trait::async_trait]
impl SearchExt for MemoryFacetedSearch {
    async fn create_index(&self, index: &str, mappings: &Value) -> Result<(), SearchError> {
        self.search.create_index(index, mappings).await
    }
    async fn delete_index(&self, index: &str) -> Result<(), SearchError> {
        self.search.delete_index(index).await
    }
    async fn index_doc(&self, index: &str, id: &str, doc: &Value) -> Result<(), SearchError> {
        self.search.index_doc(index, id, doc).await
    }
    async fn bulk_index(&self, index: &str, docs: &[(String, Value)]) -> Result<(), SearchError> {
        self.search.bulk_index(index, docs).await
    }
    async fn get_doc(&self, index: &str, id: &str) -> Result<Option<Value>, SearchError> {
        self.search.get_doc(index, id).await
    }
    async fn delete_doc(&self, index: &str, id: &str) -> Result<(), SearchError> {
        self.search.delete_doc(index, id).await
    }
    async fn search(&self, index: &str, query: &SearchQuery) -> Result<SearchResult, SearchError> {
        self.search.search(index, query).await
    }
    async fn count(&self, index: &str, query: &SearchQuery) -> Result<u64, SearchError> {
        self.search.count(index, query).await
    }
    async fn refresh(&self, index: &str) -> Result<(), SearchError> {
        self.search.refresh(index).await
    }
}

#[async_trait::async_trait]
impl FacetedSearchExt for MemoryFacetedSearch {
    async fn faceted_search(
        &self,
        index: &str,
        query: &SearchQuery,
        facet_fields: &[FacetField],
    ) -> Result<FacetedSearchResult, SearchError> {
        // 先执行普通搜索
        let search_result = self.search.search(index, query).await?;

        // 获取索引内所有文档用于分面计算
        // 注意：生产环境应在搜索引擎侧聚合，这里用内存遍历
        let all_docs = self.search.get_all_docs(index)?;

        // 计算每个字段的分面
        let facets: Vec<FacetResult> = facet_fields
            .iter()
            .map(|ff| Self::compute_facet(&all_docs, &ff.field, ff.size))
            .collect();

        Ok(FacetedSearchResult {
            search_result,
            facets,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- 高亮格式测试 ---

    #[test]
    fn test_highlight_format_prefix_suffix() {
        assert_eq!(HighlightFormat::Html.prefix(), "<em>");
        assert_eq!(HighlightFormat::Html.suffix(), "</em>");
        assert_eq!(HighlightFormat::Text.prefix(), "**");
        assert_eq!(HighlightFormat::Text.suffix(), "**");
        assert_eq!(HighlightFormat::Markdown.prefix(), "==");
        assert_eq!(HighlightFormat::Markdown.suffix(), "==");
    }

    // --- 高亮配置测试 ---

    #[test]
    fn test_highlight_config_default() {
        let config = HighlightConfig::new();
        assert_eq!(config.format, HighlightFormat::Html);
        assert!(config.fields.is_empty());
        assert_eq!(config.fragment_size, 150);
    }

    #[test]
    fn test_highlight_config_builder() {
        let config = HighlightConfig::new()
            .with_format(HighlightFormat::Text)
            .with_field("title")
            .with_field("content")
            .with_fragment_size(200);
        assert_eq!(config.format, HighlightFormat::Text);
        assert_eq!(config.fields.len(), 2);
        assert_eq!(config.fragment_size, 200);
    }

    // --- 高亮器测试 ---

    #[test]
    fn test_highlighter_html_basic() {
        let hl = Highlighter::default();
        let result = hl.highlight_text("hello world hello rust", "hello");
        assert!(result.contains("<em>hello</em>"));
        // 应高亮所有出现
        let count = result.matches("<em>hello</em>").count();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_highlighter_text_format() {
        let config = HighlightConfig::new().with_format(HighlightFormat::Text);
        let hl = Highlighter::new(config);
        let result = hl.highlight_text("hello world", "hello");
        assert_eq!(result, "**hello** world");
    }

    #[test]
    fn test_highlighter_markdown_format() {
        let config = HighlightConfig::new().with_format(HighlightFormat::Markdown);
        let hl = Highlighter::new(config);
        let result = hl.highlight_text("hello world", "hello");
        assert_eq!(result, "==hello== world");
    }

    #[test]
    fn test_highlighter_case_insensitive() {
        let hl = Highlighter::default();
        let result = hl.highlight_text("Hello HELLO hello", "hello");
        // 所有大小写变体都应被高亮
        assert!(result.contains("<em>Hello</em>"));
        assert!(result.contains("<em>HELLO</em>"));
        assert!(result.contains("<em>hello</em>"));
    }

    #[test]
    fn test_highlighter_empty_query() {
        let hl = Highlighter::default();
        let result = hl.highlight_text("hello world", "");
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_highlighter_empty_text() {
        let hl = Highlighter::default();
        let result = hl.highlight_text("", "hello");
        assert_eq!(result, "");
    }

    #[test]
    fn test_highlighter_no_match() {
        let hl = Highlighter::default();
        let result = hl.highlight_text("hello world", "rust");
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_highlighter_fragment_truncation() {
        let config = HighlightConfig::new().with_fragment_size(20);
        let hl = Highlighter::new(config);
        let long_text = "a".repeat(100) + " hello " + &"b".repeat(100);
        let result = hl.highlight_text(&long_text, "hello");
        // 应被截断
        assert!(result.contains("..."));
        assert!(result.contains("<em>hello</em>"));
    }

    #[test]
    fn test_highlighter_doc() {
        let hl = Highlighter::default();
        let doc = json!({"title": "hello world", "content": "rust programming"});
        let highlights = hl.highlight_doc(&doc, "hello");
        assert!(highlights.contains_key("title"));
        assert!(highlights["title"].contains("<em>hello</em>"));
    }

    #[test]
    fn test_highlighter_doc_with_field_filter() {
        let config = HighlightConfig::new().with_field("title");
        let hl = Highlighter::new(config);
        let doc = json!({"title": "hello world", "content": "hello rust"});
        let highlights = hl.highlight_doc(&doc, "hello");
        // 只应高亮 title 字段
        assert!(highlights.contains_key("title"));
        assert!(!highlights.contains_key("content"));
    }

    #[test]
    fn test_highlighter_doc_multi_word_query() {
        let hl = Highlighter::default();
        let doc = json!({"title": "hello world rust"});
        let highlights = hl.highlight_doc(&doc, "hello rust");
        assert!(highlights.contains_key("title"));
    }

    // --- 分词器类型测试 ---

    #[test]
    fn test_tokenizer_type_es_analyzer() {
        assert_eq!(TokenizerType::Standard.as_es_analyzer(), "standard");
        assert_eq!(TokenizerType::Whitespace.as_es_analyzer(), "whitespace");
        assert_eq!(TokenizerType::Lowercase.as_es_analyzer(), "lowercase");
        assert_eq!(TokenizerType::Keyword.as_es_analyzer(), "keyword");
    }

    // --- 分词器配置测试 ---

    #[test]
    fn test_tokenizer_config_standard() {
        let config = TokenizerConfig::standard();
        assert_eq!(config.tokenizer, TokenizerType::Standard);
        assert!(config.lowercase);
    }

    #[test]
    fn test_tokenizer_config_whitespace() {
        let config = TokenizerConfig::whitespace();
        assert_eq!(config.tokenizer, TokenizerType::Whitespace);
        assert!(!config.lowercase);
    }

    #[test]
    fn test_tokenizer_config_builder() {
        let config = TokenizerConfig::standard()
            .with_lowercase(false)
            .with_stop_words(vec!["the".to_string(), "a".to_string()])
            .with_min_length(2);
        assert!(!config.lowercase);
        assert_eq!(config.stop_words.len(), 2);
        assert_eq!(config.min_token_length, 2);
    }

    #[test]
    fn test_tokenizer_config_to_es_json() {
        let config = TokenizerConfig::standard()
            .with_stop_words(vec!["the".to_string()]);
        let json = config.to_es_analyzer_config();
        assert_eq!(json["type"], "standard");
        assert!(json["stopwords"].is_array());
    }

    // --- 分词器测试 ---

    #[test]
    fn test_tokenizer_standard_basic() {
        let tk = Tokenizer::standard();
        let tokens = tk.tokenize("Hello World Rust");
        assert_eq!(tokens, vec!["hello", "world", "rust"]);
    }

    #[test]
    fn test_tokenizer_with_punctuation() {
        let tk = Tokenizer::standard();
        let tokens = tk.tokenize("Hello, World! Rust.");
        assert_eq!(tokens, vec!["hello", "world", "rust"]);
    }

    #[test]
    fn test_tokenizer_whitespace() {
        let config = TokenizerConfig::whitespace();
        let tk = Tokenizer::new(config);
        let tokens = tk.tokenize("Hello World Rust");
        assert_eq!(tokens, vec!["Hello", "World", "Rust"]);
    }

    #[test]
    fn test_tokenizer_keyword() {
        let config = TokenizerConfig::new().with_lowercase(true);
        let mut config = config;
        config.tokenizer = TokenizerType::Keyword;
        let tk = Tokenizer::new(config);
        let tokens = tk.tokenize("Hello World Rust");
        assert_eq!(tokens, vec!["hello world rust"]);
    }

    #[test]
    fn test_tokenizer_stop_words() {
        let config = TokenizerConfig::standard()
            .with_stop_words(vec!["the".to_string(), "a".to_string(), "an".to_string()]);
        let tk = Tokenizer::new(config);
        let tokens = tk.tokenize("the quick brown fox a lazy dog an apple");
        assert_eq!(tokens, vec!["quick", "brown", "fox", "lazy", "dog", "apple"]);
    }

    #[test]
    fn test_tokenizer_min_length() {
        let config = TokenizerConfig::standard().with_min_length(3);
        let tk = Tokenizer::new(config);
        let tokens = tk.tokenize("a ab abc abcd");
        assert_eq!(tokens, vec!["abc", "abcd"]);
    }

    #[test]
    fn test_tokenizer_empty_text() {
        let tk = Tokenizer::standard();
        let tokens = tk.tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenizer_with_freq() {
        let tk = Tokenizer::standard();
        let freq = tk.tokenize_with_freq("hello world hello rust hello");
        assert_eq!(freq.get("hello"), Some(&3));
        assert_eq!(freq.get("world"), Some(&1));
        assert_eq!(freq.get("rust"), Some(&1));
    }

    // --- 字段权重测试 ---

    #[test]
    fn test_field_boost_default() {
        let fb = FieldBoost::new();
        assert_eq!(fb.get_boost("any_field"), 1.0);
    }

    #[test]
    fn test_field_boost_with_boost() {
        let fb = FieldBoost::new()
            .with_boost("title", 3.0)
            .with_boost("content", 1.5);
        assert_eq!(fb.get_boost("title"), 3.0);
        assert_eq!(fb.get_boost("content"), 1.5);
        assert_eq!(fb.get_boost("unknown"), 1.0);
    }

    #[test]
    fn test_field_boost_to_es_fields() {
        let fb = FieldBoost::new()
            .with_boost("title", 3.0)
            .with_boost("content", 1.5);
        let json = fb.to_es_fields_config();
        assert!(json.is_array());
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        // 应包含 ^boost 格式
        assert!(arr.iter().any(|v| v == "title^3" || v == "title^3.0"));
    }

    #[test]
    fn test_field_boost_empty_to_es() {
        let fb = FieldBoost::new();
        assert!(fb.to_es_fields_config().is_null());
    }

    // --- 评分器测试 ---

    #[test]
    fn test_boost_scorer_basic() {
        let fb = FieldBoost::new().with_boost("title", 2.0);
        let scorer = BoostScorer::new(fb, Tokenizer::standard());
        let doc = json!({"title": "hello world", "content": "hello rust"});
        let score = scorer.compute_score("hello", &doc);
        // title 中 hello 出现 1 次 * boost 2.0 = 2.0
        // content 中 hello 出现 1 次 * boost 1.0 = 1.0
        // 总分 = 3.0
        assert!((score - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_boost_scorer_empty_query() {
        let scorer = BoostScorer::new(FieldBoost::new(), Tokenizer::standard());
        let doc = json!({"title": "hello"});
        let score = scorer.compute_score("", &doc);
        assert!((score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_boost_scorer_no_match() {
        let scorer = BoostScorer::new(FieldBoost::new(), Tokenizer::standard());
        let doc = json!({"title": "hello world"});
        let score = scorer.compute_score("rust", &doc);
        assert!((score - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_boost_scorer_multi_term() {
        let fb = FieldBoost::new().with_boost("title", 3.0);
        let scorer = BoostScorer::new(fb, Tokenizer::standard());
        let doc = json!({"title": "hello world hello rust", "content": "world"});
        let score = scorer.compute_score("hello world", &doc);
        // title: hello*2 + world*1 = 3 次 * boost 3.0 = 9.0
        // content: world*1 = 1 次 * boost 1.0 = 1.0
        // 总分 = 10.0
        assert!((score - 10.0).abs() < 1e-6);
    }

    // --- 分面字段测试 ---

    #[test]
    fn test_facet_field_new() {
        let ff = FacetField::new("category");
        assert_eq!(ff.field, "category");
        assert_eq!(ff.size, 10);
    }

    #[test]
    fn test_facet_field_with_size() {
        let ff = FacetField::new("category").with_size(5);
        assert_eq!(ff.size, 5);
    }

    // --- 分面搜索测试 ---

    #[tokio::test]
    async fn test_memory_faceted_search_basic() {
        let fs = MemoryFacetedSearch::from_new();
        fs.create_index("docs", &json!({})).await.unwrap();
        fs.index_doc("docs", "1", &json!({"title": "rust", "category": "tech"}))
            .await
            .unwrap();
        fs.index_doc("docs", "2", &json!({"title": "go", "category": "tech"}))
            .await
            .unwrap();
        fs.index_doc("docs", "3", &json!({"title": "cooking", "category": "food"}))
            .await
            .unwrap();

        let query = SearchQuery::match_all();
        let facets = vec![FacetField::new("category")];
        let result = fs.faceted_search("docs", &query, &facets).await.unwrap();

        // 应返回 3 条搜索结果
        assert_eq!(result.search_result.total, 3);
        // 应有 1 个分面字段
        assert_eq!(result.facets.len(), 1);
        assert_eq!(result.facets[0].field, "category");
        // tech 应有 2 条
        let tech = result.facets[0]
            .values
            .iter()
            .find(|v| v.value == "tech")
            .unwrap();
        assert_eq!(tech.count, 2);
        // food 应有 1 条
        let food = result.facets[0]
            .values
            .iter()
            .find(|v| v.value == "food")
            .unwrap();
        assert_eq!(food.count, 1);
    }

    #[tokio::test]
    async fn test_memory_faceted_search_with_query() {
        let fs = MemoryFacetedSearch::from_new();
        fs.create_index("docs", &json!({})).await.unwrap();
        fs.index_doc("docs", "1", &json!({"title": "rust intro", "category": "tech"}))
            .await
            .unwrap();
        fs.index_doc("docs", "2", &json!({"title": "rust advanced", "category": "tech"}))
            .await
            .unwrap();
        fs.index_doc("docs", "3", &json!({"title": "cooking", "category": "food"}))
            .await
            .unwrap();

        // 搜索 "rust"：应命中 2 条 tech 类文档
        let query = SearchQuery::new("rust");
        let facets = vec![FacetField::new("category")];
        let result = fs.faceted_search("docs", &query, &facets).await.unwrap();

        assert_eq!(result.search_result.total, 2);
        // 分面应统计索引内所有文档（而非仅搜索结果）
        let tech = result.facets[0]
            .values
            .iter()
            .find(|v| v.value == "tech")
            .unwrap();
        assert_eq!(tech.count, 2);
    }

    #[tokio::test]
    async fn test_memory_faceted_search_multi_fields() {
        let fs = MemoryFacetedSearch::from_new();
        fs.create_index("docs", &json!({})).await.unwrap();
        fs.index_doc("docs", "1", &json!({"title": "a", "category": "tech", "lang": "rust"}))
            .await
            .unwrap();
        fs.index_doc("docs", "2", &json!({"title": "b", "category": "tech", "lang": "go"}))
            .await
            .unwrap();
        fs.index_doc("docs", "3", &json!({"title": "c", "category": "food", "lang": "rust"}))
            .await
            .unwrap();

        let query = SearchQuery::match_all();
        let facets = vec![
            FacetField::new("category"),
            FacetField::new("lang"),
        ];
        let result = fs.faceted_search("docs", &query, &facets).await.unwrap();

        assert_eq!(result.facets.len(), 2);
        // category 分面
        assert_eq!(result.facets[0].values.len(), 2); // tech, food
        // lang 分面
        assert_eq!(result.facets[1].values.len(), 2); // rust, go
    }

    #[tokio::test]
    async fn test_memory_faceted_search_size_limit() {
        let fs = MemoryFacetedSearch::from_new();
        fs.create_index("docs", &json!({})).await.unwrap();
        for i in 0..5 {
            fs.index_doc(
                "docs",
                &i.to_string(),
                &json!({"category": format!("cat_{}", i)}),
            )
            .await
            .unwrap();
        }

        let query = SearchQuery::match_all();
        let facets = vec![FacetField::new("category").with_size(3)];
        let result = fs.faceted_search("docs", &query, &facets).await.unwrap();

        // 应只返回 3 个分面值
        assert_eq!(result.facets[0].values.len(), 3);
    }

    #[tokio::test]
    async fn test_memory_faceted_search_facet_sorting() {
        let fs = MemoryFacetedSearch::from_new();
        fs.create_index("docs", &json!({})).await.unwrap();
        // tech 出现 3 次，food 出现 1 次
        fs.index_doc("docs", "1", &json!({"category": "tech"}))
            .await
            .unwrap();
        fs.index_doc("docs", "2", &json!({"category": "tech"}))
            .await
            .unwrap();
        fs.index_doc("docs", "3", &json!({"category": "tech"}))
            .await
            .unwrap();
        fs.index_doc("docs", "4", &json!({"category": "food"}))
            .await
            .unwrap();

        let query = SearchQuery::match_all();
        let facets = vec![FacetField::new("category")];
        let result = fs.faceted_search("docs", &query, &facets).await.unwrap();

        // 第一个应是 count 最高的 tech
        assert_eq!(result.facets[0].values[0].value, "tech");
        assert_eq!(result.facets[0].values[0].count, 3);
    }
}
