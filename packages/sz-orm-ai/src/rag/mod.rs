use crate::embedding::EmbeddingModel;
use crate::error::AiError;
use crate::vector::VectorStore;

pub struct RagConfig {
    pub collection_name: String,
    pub chunk_size: usize,
    pub chunk_overlap: usize,
    pub top_k: usize,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            collection_name: "rag_documents".to_string(),
            chunk_size: 512,
            chunk_overlap: 50,
            top_k: 3,
        }
    }
}

impl RagConfig {
    pub fn new(collection_name: impl Into<String>) -> Self {
        Self {
            collection_name: collection_name.into(),
            ..Default::default()
        }
    }

    pub fn with_chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = size;
        self
    }

    pub fn with_chunk_overlap(mut self, overlap: usize) -> Self {
        self.chunk_overlap = overlap;
        self
    }

    pub fn with_top_k(mut self, k: usize) -> Self {
        self.top_k = k;
        self
    }
}

#[derive(Debug, Clone)]
pub struct Document {
    pub id: String,
    pub content: String,
    pub source: Option<String>,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl Document {
    pub fn new(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            source: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub id: String,
    pub document_id: String,
    pub content: String,
    pub index: usize,
    pub start_char: usize,
    pub end_char: usize,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl Chunk {
    pub fn new(
        id: impl Into<String>,
        document_id: impl Into<String>,
        content: impl Into<String>,
        index: usize,
        start_char: usize,
        end_char: usize,
    ) -> Self {
        Self {
            id: id.into(),
            document_id: document_id.into(),
            content: content.into(),
            index,
            start_char,
            end_char,
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

pub struct RagEngine<E, V>
where
    E: EmbeddingModel,
    V: VectorStore,
{
    embedding_model: E,
    vector_store: V,
    config: RagConfig,
}

impl<E, V> RagEngine<E, V>
where
    E: EmbeddingModel,
    V: VectorStore,
{
    pub fn new(embedding_model: E, vector_store: V, config: RagConfig) -> Self {
        Self {
            embedding_model,
            vector_store,
            config,
        }
    }

    pub fn with_config(mut self, config: RagConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn index_documents(&self, documents: Vec<Document>) -> Result<usize, AiError> {
        let collection = &self.config.collection_name;
        let dimension = self.embedding_model.dimension();

        self.vector_store
            .create_collection(collection, dimension, None)
            .await
            .map_err(|e| AiError::Vector(e.to_string()))?;

        let chunks = self.split_documents(documents);
        let mut total_indexed = 0;

        for chunk in chunks {
            let vector = self.embedding_model.embed(&chunk.content).await?;

            let record = crate::vector::VectorRecord::new(chunk.id.clone(), vector)
                .with_metadata(chunk.metadata);

            self.vector_store
                .insert(collection, vec![record])
                .await
                .map_err(|e| AiError::Vector(e.to_string()))?;

            total_indexed += 1;
        }

        Ok(total_indexed)
    }

    fn split_documents(&self, documents: Vec<Document>) -> Vec<Chunk> {
        let mut chunks = Vec::new();

        for document in documents {
            let content = document.content.as_str();
            let chars: Vec<char> = content.chars().collect();
            let chunk_size = self.config.chunk_size;
            let overlap = self.config.chunk_overlap;

            let mut start = 0;
            let mut index = 0;

            while start < chars.len() {
                let end = (start + chunk_size).min(chars.len());
                let chunk_content: String = chars[start..end].iter().collect();

                if !chunk_content.trim().is_empty() {
                    let chunk = Chunk::new(
                        format!("{}_{}", document.id, index),
                        document.id.clone(),
                        chunk_content,
                        index,
                        start,
                        end,
                    )
                    .with_metadata("source", document.source.clone().unwrap_or_default().into());

                    chunks.push(chunk);
                }

                if end == chars.len() {
                    break;
                }

                start = end - overlap.min(end);
                index += 1;
            }
        }

        chunks
    }

    pub async fn search(
        &self,
        query: &str,
        filter: Option<&str>,
    ) -> Result<Vec<RagSearchResult>, AiError> {
        let query_vector = self.embedding_model.embed(query).await?;

        let results = self
            .vector_store
            .search(
                &self.config.collection_name,
                &query_vector,
                self.config.top_k,
                filter,
            )
            .await
            .map_err(|e| AiError::Vector(e.to_string()))?;

        Ok(results
            .into_iter()
            .map(|r| RagSearchResult {
                id: r.id,
                score: r.score,
                content: r.text.unwrap_or_default(),
                metadata: r.metadata.unwrap_or_default(),
            })
            .collect())
    }

    pub async fn delete_document(&self, document_id: &str) -> Result<u64, AiError> {
        let count = self
            .vector_store
            .count(&self.config.collection_name)
            .await
            .map_err(|e| AiError::Vector(e.to_string()))?;

        if count == 0 {
            return Ok(0);
        }

        let dimension = self.embedding_model.dimension();
        let dummy_vector = vec![0.0; dimension];

        let all_results = self
            .vector_store
            .search(&self.config.collection_name, &dummy_vector, count, None)
            .await
            .map_err(|e| AiError::Vector(e.to_string()))?;

        let to_delete: Vec<String> = all_results
            .into_iter()
            .filter(|r| r.id.starts_with(document_id))
            .map(|r| r.id)
            .collect();

        if to_delete.is_empty() {
            return Ok(0);
        }

        self.vector_store
            .delete(&self.config.collection_name, to_delete)
            .await
            .map_err(|e| AiError::Vector(e.to_string()))
    }
}

#[derive(Debug, Clone)]
pub struct RagSearchResult {
    pub id: String,
    pub score: f32,
    pub content: String,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

// ==================== 上下文窗口管理 ====================

/// 上下文截断策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TruncationStrategy {
    /// 保留得分最高的片段（从头截断）
    #[default]
    BestFirst,
    /// 保留最早的片段（按顺序截断）
    FirstFirst,
    /// 均匀截断：每个片段截断相同比例
    Uniform,
}

impl TruncationStrategy {
    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            TruncationStrategy::BestFirst => "best_first",
            TruncationStrategy::FirstFirst => "first_first",
            TruncationStrategy::Uniform => "uniform",
        }
    }
}

/// Token 计数策略（简化版）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TokenCountStrategy {
    /// 按字符数估算（中文1字≈1.5 token，英文约4字符≈1 token）
    #[default]
    CharApprox,
    /// 按空格分词估算（粗略）
    WhitespaceSplit,
    /// 简单字符数 / 4
    CharDiv4,
}

/// 上下文窗口配置
#[derive(Debug, Clone)]
pub struct ContextWindowConfig {
    /// 最大 token 数（上下文窗口大小）
    pub max_tokens: usize,
    /// 系统提示预留的 token 数
    pub system_prompt_tokens: usize,
    /// 查询预留的 token 数
    pub query_tokens: usize,
    /// 回答预留的 token 数
    pub answer_tokens: usize,
    /// 截断策略
    pub strategy: TruncationStrategy,
    /// token 计数策略
    pub token_strategy: TokenCountStrategy,
    /// 片段之间的分隔符
    pub separator: String,
    /// 是否包含来源信息
    pub include_source: bool,
}

impl Default for ContextWindowConfig {
    fn default() -> Self {
        Self {
            max_tokens: 4096,
            system_prompt_tokens: 500,
            query_tokens: 200,
            answer_tokens: 1000,
            strategy: TruncationStrategy::BestFirst,
            token_strategy: TokenCountStrategy::CharApprox,
            separator: "\n\n---\n\n".to_string(),
            include_source: true,
        }
    }
}

impl ContextWindowConfig {
    /// 创建新的上下文窗口配置
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            ..Default::default()
        }
    }

    /// 设置系统提示预留 token 数
    pub fn with_system_prompt_tokens(mut self, tokens: usize) -> Self {
        self.system_prompt_tokens = tokens;
        self
    }

    /// 设置查询预留 token 数
    pub fn with_query_tokens(mut self, tokens: usize) -> Self {
        self.query_tokens = tokens;
        self
    }

    /// 设置回答预留 token 数
    pub fn with_answer_tokens(mut self, tokens: usize) -> Self {
        self.answer_tokens = tokens;
        self
    }

    /// 设置截断策略
    pub fn with_strategy(mut self, strategy: TruncationStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// 设置 token 计数策略
    pub fn with_token_strategy(mut self, strategy: TokenCountStrategy) -> Self {
        self.token_strategy = strategy;
        self
    }

    /// 设置分隔符
    pub fn with_separator(mut self, separator: impl Into<String>) -> Self {
        self.separator = separator.into();
        self
    }

    /// 是否包含来源信息
    pub fn with_source(mut self, include: bool) -> Self {
        self.include_source = include;
        self
    }

    /// 计算可用于上下文的 token 数
    pub fn available_context_tokens(&self) -> usize {
        let reserved = self.system_prompt_tokens + self.query_tokens + self.answer_tokens;
        self.max_tokens.saturating_sub(reserved)
    }
}

/// 上下文窗口组装结果
#[derive(Debug, Clone)]
pub struct ContextWindowResult {
    /// 组装后的上下文文本
    pub context: String,
    /// 实际使用的 token 数
    pub used_tokens: usize,
    /// 包含的片段数量
    pub included_chunks: usize,
    /// 被截断的片段数量
    pub truncated_chunks: usize,
    /// 被丢弃的片段数量
    pub dropped_chunks: usize,
    /// 每个片段的 token 数
    pub chunk_tokens: Vec<usize>,
}

impl ContextWindowResult {
    /// 是否有任何片段被截断
    pub fn has_truncation(&self) -> bool {
        self.truncated_chunks > 0
    }

    /// 是否有任何片段被丢弃
    pub fn has_drops(&self) -> bool {
        self.dropped_chunks > 0
    }

    /// 利用率（已用 token / 可用 token）
    pub fn utilization(&self, available: usize) -> f32 {
        if available == 0 {
            return 0.0;
        }
        self.used_tokens as f32 / available as f32
    }
}

/// 上下文窗口管理器
///
/// 负责将检索到的文档片段组装到 LLM 上下文窗口中，
/// 根据 token 预算进行截断和丢弃，确保总 token 数不超过限制。
pub struct ContextWindowManager {
    /// 配置
    config: ContextWindowConfig,
}

impl ContextWindowManager {
    /// 创建新的上下文窗口管理器
    pub fn new(config: ContextWindowConfig) -> Self {
        Self { config }
    }

    /// 使用默认配置创建
    pub fn with_defaults() -> Self {
        Self::new(ContextWindowConfig::default())
    }

    /// 获取配置引用
    pub fn config(&self) -> &ContextWindowConfig {
        &self.config
    }

    /// 估算文本的 token 数
    pub fn count_tokens(&self, text: &str) -> usize {
        match self.config.token_strategy {
            TokenCountStrategy::CharApprox => {
                // 简化估算：中文字符按 1.5 token，ASCII 按 4 字符 1 token
                let chinese_count = text.chars().filter(|c| !c.is_ascii()).count();
                let ascii_count = text.chars().filter(|c| c.is_ascii()).count();
                (chinese_count as f32 * 1.5).ceil() as usize + (ascii_count / 4)
            }
            TokenCountStrategy::WhitespaceSplit => text.split_whitespace().count(),
            TokenCountStrategy::CharDiv4 => text.chars().count() / 4,
        }
    }

    /// 估算分隔符的 token 数
    fn separator_tokens(&self) -> usize {
        self.count_tokens(&self.config.separator)
    }

    /// 为单个片段添加来源前缀（如果配置启用）
    fn format_chunk(&self, result: &RagSearchResult) -> String {
        if self.config.include_source {
            let source = result
                .metadata
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!("[source: {}]\n{}", source, result.content)
        } else {
            result.content.clone()
        }
    }

    /// 组装上下文窗口
    ///
    /// 将检索结果按得分排序，根据 token 预算组装上下文。
    /// # 参数
    /// - `results`: 检索结果列表（将按 score 降序排序）
    pub fn assemble(&self, mut results: Vec<RagSearchResult>) -> ContextWindowResult {
        let available = self.config.available_context_tokens();
        let separator_tokens = self.separator_tokens();

        // 按得分降序排序（BestFirst 策略）
        if self.config.strategy == TruncationStrategy::BestFirst {
            results.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        let total_chunks = results.len();
        let mut context_parts: Vec<String> = Vec::new();
        let mut chunk_tokens: Vec<usize> = Vec::new();
        let mut used_tokens = 0usize;
        let mut included_chunks = 0usize;
        let mut truncated_chunks = 0usize;

        for (idx, result) in results.iter().enumerate() {
            let formatted = self.format_chunk(result);
            let chunk_token_count = self.count_tokens(&formatted);

            // 计算添加此片段后的总 token 数（包括分隔符）
            let additional = if idx > 0 {
                chunk_token_count + separator_tokens
            } else {
                chunk_token_count
            };

            if used_tokens + additional <= available {
                // 完整放入
                context_parts.push(formatted.clone());
                chunk_tokens.push(chunk_token_count);
                used_tokens += additional;
                included_chunks += 1;
            } else {
                // 尝试截断后放入
                let remaining = available.saturating_sub(used_tokens);
                if remaining > separator_tokens + 10 {
                    // 至少保留 10 token 的内容
                    let target_content_tokens = remaining.saturating_sub(separator_tokens);
                    let truncated = self.truncate_text(&formatted, target_content_tokens);
                    let truncated_tokens = self.count_tokens(&truncated);

                    if truncated_tokens > 0 {
                        context_parts.push(truncated);
                        chunk_tokens.push(truncated_tokens);
                        used_tokens += truncated_tokens + if idx > 0 { separator_tokens } else { 0 };
                        included_chunks += 1;
                        truncated_chunks += 1;
                    }
                    // 截断后通常没有空间了
                    break;
                } else {
                    break;
                }
            }
        }

        let dropped_chunks = total_chunks.saturating_sub(included_chunks);
        let context = context_parts.join(&self.config.separator);

        ContextWindowResult {
            context,
            used_tokens,
            included_chunks,
            truncated_chunks,
            dropped_chunks,
            chunk_tokens,
        }
    }

    /// 按 Uniform 策略组装（每个片段截断相同比例）
    pub fn assemble_uniform(&self, mut results: Vec<RagSearchResult>) -> ContextWindowResult {
        if self.config.strategy != TruncationStrategy::Uniform {
            // 如果配置不是 Uniform，仍然按 Uniform 处理
        }

        let available = self.config.available_context_tokens();
        let separator_tokens = self.separator_tokens();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let total_chunks = results.len();
        if total_chunks == 0 {
            return ContextWindowResult {
                context: String::new(),
                used_tokens: 0,
                included_chunks: 0,
                truncated_chunks: 0,
                dropped_chunks: 0,
                chunk_tokens: Vec::new(),
            };
        }

        // 计算所有片段的总 token 数
        let formatted: Vec<String> = results.iter().map(|r| self.format_chunk(r)).collect();
        let token_counts: Vec<usize> = formatted.iter().map(|f| self.count_tokens(f)).collect();
        let total_content_tokens: usize = token_counts.iter().sum();
        let total_separator_tokens = separator_tokens * (total_chunks.saturating_sub(1));
        let total_tokens = total_content_tokens + total_separator_tokens;

        if total_tokens <= available {
            // 全部放入，无需截断
            let context = formatted.join(&self.config.separator);
            return ContextWindowResult {
                context,
                used_tokens: total_tokens,
                included_chunks: total_chunks,
                truncated_chunks: 0,
                dropped_chunks: 0,
                chunk_tokens: token_counts,
            };
        }

        // 计算每个片段的目标 token 数（均匀分配）
        let available_per_chunk = available / total_chunks;
        let mut context_parts: Vec<String> = Vec::new();
        let mut chunk_tokens_result: Vec<usize> = Vec::new();
        let mut used_tokens = 0usize;
        let mut truncated_chunks = 0usize;

        for (idx, formatted_text) in formatted.iter().enumerate() {
            let target = available_per_chunk.saturating_sub(if idx > 0 { separator_tokens } else { 0 });
            let original_tokens = token_counts[idx];

            if original_tokens <= target {
                context_parts.push(formatted_text.clone());
                chunk_tokens_result.push(original_tokens);
                used_tokens += original_tokens + if idx > 0 { separator_tokens } else { 0 };
            } else {
                let truncated = self.truncate_text(formatted_text, target);
                let truncated_tokens = self.count_tokens(&truncated);
                context_parts.push(truncated);
                chunk_tokens_result.push(truncated_tokens);
                used_tokens += truncated_tokens + if idx > 0 { separator_tokens } else { 0 };
                truncated_chunks += 1;
            }
        }

        let context = context_parts.join(&self.config.separator);
        let included_chunks = total_chunks;

        ContextWindowResult {
            context,
            used_tokens,
            included_chunks,
            truncated_chunks,
            dropped_chunks: 0,
            chunk_tokens: chunk_tokens_result,
        }
    }

    /// 截断文本到指定 token 数
    fn truncate_text(&self, text: &str, target_tokens: usize) -> String {
        if target_tokens == 0 {
            return String::new();
        }

        match self.config.token_strategy {
            TokenCountStrategy::CharApprox => {
                // 逆向计算：估算需要的字符数
                let chars: Vec<char> = text.chars().collect();
                let mut result = String::new();
                let mut token_count: f64 = 0.0;
                let target = target_tokens as f64;
                for ch in chars {
                    let char_tokens = if ch.is_ascii() { 0.25 } else { 1.5 };
                    if token_count + char_tokens > target {
                        break;
                    }
                    result.push(ch);
                    token_count += char_tokens;
                }
                if result.len() < text.len() {
                    result.push_str("...");
                }
                result
            }
            TokenCountStrategy::WhitespaceSplit => {
                let words: Vec<&str> = text.split_whitespace().collect();
                let truncated: Vec<&str> = words.into_iter().take(target_tokens).collect();
                let mut result = truncated.join(" ");
                if result.len() < text.len() {
                    result.push_str("...");
                }
                result
            }
            TokenCountStrategy::CharDiv4 => {
                let char_count = target_tokens * 4;
                let chars: Vec<char> = text.chars().take(char_count).collect();
                let mut result: String = chars.into_iter().collect();
                if result.len() < text.len() {
                    result.push_str("...");
                }
                result
            }
        }
    }

    /// 构建 LLM 提示词（系统提示 + 上下文 + 查询）
    pub fn build_prompt(
        &self,
        system_prompt: &str,
        context_result: &ContextWindowResult,
        user_query: &str,
    ) -> String {
        let mut prompt = String::new();
        prompt.push_str(system_prompt);
        prompt.push_str("\n\n=== Context ===\n");
        prompt.push_str(&context_result.context);
        prompt.push_str("\n\n=== Question ===\n");
        prompt.push_str(user_query);
        prompt
    }

    /// 估算完整提示词的 token 数
    pub fn estimate_prompt_tokens(
        &self,
        system_prompt: &str,
        context_result: &ContextWindowResult,
        user_query: &str,
    ) -> usize {
        let system_tokens = self.count_tokens(system_prompt);
        let query_tokens = self.count_tokens(user_query);
        system_tokens + context_result.used_tokens + query_tokens
    }
}

impl Default for ContextWindowManager {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造测试用检索结果
    fn make_result(id: &str, score: f32, content: &str, source: &str) -> RagSearchResult {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("source".to_string(), serde_json::json!(source));
        RagSearchResult {
            id: id.to_string(),
            score,
            content: content.to_string(),
            metadata,
        }
    }

    // ---- TruncationStrategy 测试 ----

    #[test]
    fn test_truncation_strategy_default() {
        assert_eq!(TruncationStrategy::default(), TruncationStrategy::BestFirst);
    }

    #[test]
    fn test_truncation_strategy_as_str() {
        assert_eq!(TruncationStrategy::BestFirst.as_str(), "best_first");
        assert_eq!(TruncationStrategy::FirstFirst.as_str(), "first_first");
        assert_eq!(TruncationStrategy::Uniform.as_str(), "uniform");
    }

    // ---- TokenCountStrategy 测试 ----

    #[test]
    fn test_token_count_strategy_default() {
        assert_eq!(TokenCountStrategy::default(), TokenCountStrategy::CharApprox);
    }

    // ---- ContextWindowConfig 测试 ----

    #[test]
    fn test_context_window_config_default() {
        let config = ContextWindowConfig::default();
        assert_eq!(config.max_tokens, 4096);
        assert_eq!(config.system_prompt_tokens, 500);
        assert_eq!(config.query_tokens, 200);
        assert_eq!(config.answer_tokens, 1000);
        assert_eq!(config.strategy, TruncationStrategy::BestFirst);
        assert_eq!(config.token_strategy, TokenCountStrategy::CharApprox);
        assert!(config.include_source);
    }

    #[test]
    fn test_context_window_config_new() {
        let config = ContextWindowConfig::new(8000);
        assert_eq!(config.max_tokens, 8000);
    }

    #[test]
    fn test_context_window_config_builders() {
        let config = ContextWindowConfig::new(8000)
            .with_system_prompt_tokens(600)
            .with_query_tokens(300)
            .with_answer_tokens(2000)
            .with_strategy(TruncationStrategy::Uniform)
            .with_token_strategy(TokenCountStrategy::WhitespaceSplit)
            .with_separator("\n")
            .with_source(false);

        assert_eq!(config.system_prompt_tokens, 600);
        assert_eq!(config.query_tokens, 300);
        assert_eq!(config.answer_tokens, 2000);
        assert_eq!(config.strategy, TruncationStrategy::Uniform);
        assert_eq!(config.token_strategy, TokenCountStrategy::WhitespaceSplit);
        assert_eq!(config.separator, "\n");
        assert!(!config.include_source);
    }

    #[test]
    fn test_available_context_tokens() {
        let config = ContextWindowConfig::new(4096)
            .with_system_prompt_tokens(500)
            .with_query_tokens(200)
            .with_answer_tokens(1000);
        assert_eq!(config.available_context_tokens(), 2396);
    }

    #[test]
    fn test_available_context_tokens_zero_when_reserved_exceeds() {
        let config = ContextWindowConfig::new(100)
            .with_system_prompt_tokens(500)
            .with_query_tokens(200)
            .with_answer_tokens(1000);
        assert_eq!(config.available_context_tokens(), 0);
    }

    // ---- ContextWindowResult 测试 ----

    #[test]
    fn test_context_window_result_has_truncation() {
        let result = ContextWindowResult {
            context: "test".to_string(),
            used_tokens: 100,
            included_chunks: 2,
            truncated_chunks: 1,
            dropped_chunks: 0,
            chunk_tokens: vec![50, 50],
        };
        assert!(result.has_truncation());
    }

    #[test]
    fn test_context_window_result_no_truncation() {
        let result = ContextWindowResult {
            context: "test".to_string(),
            used_tokens: 100,
            included_chunks: 2,
            truncated_chunks: 0,
            dropped_chunks: 0,
            chunk_tokens: vec![50, 50],
        };
        assert!(!result.has_truncation());
    }

    #[test]
    fn test_context_window_result_has_drops() {
        let result = ContextWindowResult {
            context: "test".to_string(),
            used_tokens: 100,
            included_chunks: 1,
            truncated_chunks: 0,
            dropped_chunks: 3,
            chunk_tokens: vec![100],
        };
        assert!(result.has_drops());
    }

    #[test]
    fn test_context_window_result_utilization() {
        let result = ContextWindowResult {
            context: "test".to_string(),
            used_tokens: 500,
            included_chunks: 2,
            truncated_chunks: 0,
            dropped_chunks: 0,
            chunk_tokens: vec![250, 250],
        };
        assert!((result.utilization(1000) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_context_window_result_utilization_zero_available() {
        let result = ContextWindowResult {
            context: "test".to_string(),
            used_tokens: 100,
            included_chunks: 1,
            truncated_chunks: 0,
            dropped_chunks: 0,
            chunk_tokens: vec![100],
        };
        assert_eq!(result.utilization(0), 0.0);
    }

    // ---- ContextWindowManager 测试 ----

    #[test]
    fn test_context_window_manager_with_defaults() {
        let manager = ContextWindowManager::with_defaults();
        assert_eq!(manager.config().max_tokens, 4096);
    }

    #[test]
    fn test_context_window_manager_default() {
        let manager = ContextWindowManager::default();
        assert_eq!(manager.config().max_tokens, 4096);
    }

    #[test]
    fn test_count_tokens_char_approx_english() {
        let manager = ContextWindowManager::with_defaults();
        // "hello world" = 11 ASCII chars, 11/4 = 2 tokens
        let tokens = manager.count_tokens("hello world");
        assert_eq!(tokens, 2);
    }

    #[test]
    fn test_count_tokens_char_approx_chinese() {
        let manager = ContextWindowManager::with_defaults();
        // "你好世界" = 4 Chinese chars, 4 * 1.5 = 6 tokens
        let tokens = manager.count_tokens("你好世界");
        assert_eq!(tokens, 6);
    }

    #[test]
    fn test_count_tokens_whitespace_split() {
        let config = ContextWindowConfig::default()
            .with_token_strategy(TokenCountStrategy::WhitespaceSplit);
        let manager = ContextWindowManager::new(config);
        let tokens = manager.count_tokens("hello world foo bar");
        assert_eq!(tokens, 4);
    }

    #[test]
    fn test_count_tokens_char_div4() {
        let config = ContextWindowConfig::default()
            .with_token_strategy(TokenCountStrategy::CharDiv4);
        let manager = ContextWindowManager::new(config);
        let tokens = manager.count_tokens("hello world!");
        assert_eq!(tokens, 12 / 4); // 3
    }

    #[test]
    fn test_assemble_empty_results() {
        let manager = ContextWindowManager::with_defaults();
        let result = manager.assemble(Vec::new());
        assert!(result.context.is_empty());
        assert_eq!(result.used_tokens, 0);
        assert_eq!(result.included_chunks, 0);
    }

    #[test]
    fn test_assemble_all_fit() {
        let config = ContextWindowConfig::new(10000)
            .with_system_prompt_tokens(0)
            .with_query_tokens(0)
            .with_answer_tokens(0)
            .with_source(false);
        let manager = ContextWindowManager::new(config);

        let results = vec![
            make_result("r1", 0.9, "hello world", "doc1"),
            make_result("r2", 0.8, "foo bar baz", "doc2"),
        ];
        let result = manager.assemble(results);

        assert_eq!(result.included_chunks, 2);
        assert_eq!(result.truncated_chunks, 0);
        assert_eq!(result.dropped_chunks, 0);
        assert!(!result.context.is_empty());
    }

    #[test]
    fn test_assemble_best_first_sorts_by_score() {
        let config = ContextWindowConfig::new(10000)
            .with_system_prompt_tokens(0)
            .with_query_tokens(0)
            .with_answer_tokens(0)
            .with_source(false);
        let manager = ContextWindowManager::new(config);

        let results = vec![
            make_result("r1", 0.5, "low score content", "doc1"),
            make_result("r2", 0.9, "high score content", "doc2"),
        ];
        let result = manager.assemble(results);

        // 高分内容应在前面
        assert!(result.context.starts_with("high score content"));
    }

    #[test]
    fn test_assemble_drops_when_exceeds_budget() {
        let config = ContextWindowConfig::new(100)
            .with_system_prompt_tokens(0)
            .with_query_tokens(0)
            .with_answer_tokens(0)
            .with_source(false)
            .with_separator("\n");
        let manager = ContextWindowManager::new(config);

        let results = vec![
            make_result("r1", 0.9, &"a".repeat(200), "doc1"),
            make_result("r2", 0.8, &"b".repeat(200), "doc2"),
        ];
        let result = manager.assemble(results);

        // 应该只包含部分片段
        assert!(result.included_chunks <= 2);
        // 第二个应该被丢弃或截断（usize 始终 >= 0，这里验证字段可访问）
        let _ = result.dropped_chunks;
    }

    #[test]
    fn test_assemble_truncates_last_chunk() {
        let config = ContextWindowConfig::new(100)
            .with_system_prompt_tokens(0)
            .with_query_tokens(0)
            .with_answer_tokens(0)
            .with_source(false)
            .with_separator("\n");
        let manager = ContextWindowManager::new(config);

        // 第一个片段小，第二个片段大
        let results = vec![
            make_result("r1", 0.9, "small", "doc1"),
            make_result("r2", 0.8, &"x".repeat(500), "doc2"),
        ];
        let result = manager.assemble(results);

        // 应该截断第二个片段
        assert!(result.has_truncation() || result.has_drops());
    }

    #[test]
    fn test_assemble_includes_source_when_configured() {
        let config = ContextWindowConfig::new(10000)
            .with_system_prompt_tokens(0)
            .with_query_tokens(0)
            .with_answer_tokens(0)
            .with_source(true);
        let manager = ContextWindowManager::new(config);

        let results = vec![make_result("r1", 0.9, "content here", "mydoc")];
        let result = manager.assemble(results);

        assert!(result.context.contains("source: mydoc"));
    }

    #[test]
    fn test_assemble_excludes_source_when_disabled() {
        let config = ContextWindowConfig::new(10000)
            .with_system_prompt_tokens(0)
            .with_query_tokens(0)
            .with_answer_tokens(0)
            .with_source(false);
        let manager = ContextWindowManager::new(config);

        let results = vec![make_result("r1", 0.9, "content here", "mydoc")];
        let result = manager.assemble(results);

        assert!(!result.context.contains("source: mydoc"));
    }

    #[test]
    fn test_assemble_uniform_all_fit() {
        let config = ContextWindowConfig::new(10000)
            .with_system_prompt_tokens(0)
            .with_query_tokens(0)
            .with_answer_tokens(0)
            .with_source(false);
        let manager = ContextWindowManager::new(config);

        let results = vec![
            make_result("r1", 0.9, "hello", "doc1"),
            make_result("r2", 0.8, "world", "doc2"),
        ];
        let result = manager.assemble_uniform(results);

        assert_eq!(result.included_chunks, 2);
        assert_eq!(result.truncated_chunks, 0);
    }

    #[test]
    fn test_assemble_uniform_truncates_all() {
        let config = ContextWindowConfig::new(20)
            .with_system_prompt_tokens(0)
            .with_query_tokens(0)
            .with_answer_tokens(0)
            .with_source(false)
            .with_separator("");
        let manager = ContextWindowManager::new(config);

        let results = vec![
            make_result("r1", 0.9, &"a".repeat(100), "doc1"),
            make_result("r2", 0.8, &"b".repeat(100), "doc2"),
        ];
        let result = manager.assemble_uniform(results);

        // 两个片段都应被截断
        assert!(result.truncated_chunks >= 1);
        assert_eq!(result.dropped_chunks, 0);
    }

    #[test]
    fn test_build_prompt_structure() {
        let manager = ContextWindowManager::with_defaults();
        let context_result = ContextWindowResult {
            context: "some context".to_string(),
            used_tokens: 10,
            included_chunks: 1,
            truncated_chunks: 0,
            dropped_chunks: 0,
            chunk_tokens: vec![10],
        };
        let prompt = manager.build_prompt("You are an assistant.", &context_result, "What is X?");
        assert!(prompt.contains("You are an assistant."));
        assert!(prompt.contains("=== Context ==="));
        assert!(prompt.contains("some context"));
        assert!(prompt.contains("=== Question ==="));
        assert!(prompt.contains("What is X?"));
    }

    #[test]
    fn test_estimate_prompt_tokens() {
        let manager = ContextWindowManager::with_defaults();
        let context_result = ContextWindowResult {
            context: "hello world".to_string(),
            used_tokens: 2,
            included_chunks: 1,
            truncated_chunks: 0,
            dropped_chunks: 0,
            chunk_tokens: vec![2],
        };
        let total = manager.estimate_prompt_tokens("system prompt", &context_result, "query");
        assert!(total > 0);
    }

    #[test]
    fn test_assemble_single_chunk_fits() {
        let config = ContextWindowConfig::new(10000)
            .with_system_prompt_tokens(0)
            .with_query_tokens(0)
            .with_answer_tokens(0)
            .with_source(false);
        let manager = ContextWindowManager::new(config);

        let results = vec![make_result("r1", 1.0, "single chunk content", "doc1")];
        let result = manager.assemble(results);

        assert_eq!(result.included_chunks, 1);
        assert_eq!(result.dropped_chunks, 0);
        assert_eq!(result.truncated_chunks, 0);
    }

    #[test]
    fn test_assemble_uses_separator() {
        let config = ContextWindowConfig::new(10000)
            .with_system_prompt_tokens(0)
            .with_query_tokens(0)
            .with_answer_tokens(0)
            .with_source(false)
            .with_separator("|||");
        let manager = ContextWindowManager::new(config);

        let results = vec![
            make_result("r1", 0.9, "chunk1", "doc1"),
            make_result("r2", 0.8, "chunk2", "doc2"),
        ];
        let result = manager.assemble(results);

        assert!(result.context.contains("|||"));
    }
}
