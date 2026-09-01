//! NL2SQL 多轮对话模块
//!
//! 支持多轮自然语言查询，将历史查询上下文注入 LLM 提示词，
//! 要求 LLM 将后续查询解析为对前序查询的增量修改。
//!
//! 启用 `ai-nl2sql-enhanced` feature 后可用。
//! 使用 `MultiTurnNl2SqlEngine` 启用多轮对话。

use std::time::{Duration, Instant};

use crate::nl2sql::{Nl2SqlEngine, Nl2SqlError, SchemaContext, SqlQuery};

/// 单轮查询记录
#[derive(Debug, Clone)]
pub struct TurnRecord {
    /// 自然语言查询
    pub nl_query: String,
    /// 生成的 SQL
    pub generated_sql: String,
    /// 查询时间戳
    pub timestamp: Instant,
}

/// 会话上下文
#[derive(Debug, Clone)]
pub struct ConversationContext {
    /// 历史查询记录
    pub history: Vec<TurnRecord>,
    /// 最大轮数（默认 10）
    pub max_turns: usize,
    /// 会话超时（默认 30 分钟）
    pub timeout: Duration,
    /// 会话创建时间
    pub created_at: Instant,
}

impl Default for ConversationContext {
    fn default() -> Self {
        Self {
            history: Vec::new(),
            max_turns: 10,
            timeout: Duration::from_secs(30 * 60),
            created_at: Instant::now(),
        }
    }
}

impl ConversationContext {
    /// 创建新的会话上下文
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置最大轮数
    pub fn with_max_turns(mut self, max_turns: usize) -> Self {
        self.max_turns = max_turns;
        self
    }

    /// 设置超时时间
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// 添加一轮查询记录
    pub fn add_turn(&mut self, nl_query: &str, generated_sql: &str) {
        self.history.push(TurnRecord {
            nl_query: nl_query.to_string(),
            generated_sql: generated_sql.to_string(),
            timestamp: Instant::now(),
        });
        // 超过最大轮数时移除最早的记录
        if self.history.len() > self.max_turns {
            self.history.remove(0);
        }
    }

    /// 检查会话是否超时
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.timeout
    }

    /// 清理过期历史记录
    pub fn cleanup_if_expired(&mut self) -> bool {
        if self.is_expired() {
            self.history.clear();
            true
        } else {
            false
        }
    }

    /// 构建上下文提示词（将历史查询注入 LLM 提示词）
    pub fn build_context_prompt(&self, current_query: &str) -> String {
        if self.history.is_empty() {
            return current_query.to_string();
        }

        let mut prompt = String::from("Previous conversation context:\n");
        for (i, turn) in self.history.iter().enumerate() {
            prompt.push_str(&format!(
                "Turn {}: Query: \"{}\" → SQL: {}\n",
                i + 1,
                turn.nl_query,
                turn.generated_sql
            ));
        }
        prompt.push_str(&format!(
            "\nCurrent query: \"{}\"\n\
             Parse this query as an incremental modification to the previous queries. \
             Include all previous filter conditions plus any new conditions.",
            current_query
        ));
        prompt
    }
}

/// 多轮 NL2SQL 引擎
///
/// 包装现有 NL2SQL 引擎，添加多轮对话上下文支持。
/// 使用 `MultiTurnNl2SqlEngine::new` 启用多轮对话。
pub struct MultiTurnNl2SqlEngine<E: Nl2SqlEngine> {
    /// 内部 NL2SQL 引擎
    engine: E,
    /// 会话上下文
    conversation: ConversationContext,
}

impl<E: Nl2SqlEngine> MultiTurnNl2SqlEngine<E> {
    /// 创建多轮 NL2SQL 引擎
    pub fn new(engine: E) -> Self {
        Self {
            engine,
            conversation: ConversationContext::new(),
        }
    }

    /// 设置最大轮数
    pub fn with_max_turns(mut self, max_turns: usize) -> Self {
        self.conversation = self.conversation.with_max_turns(max_turns);
        self
    }

    /// 设置超时时间
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.conversation = self.conversation.with_timeout(timeout);
        self
    }

    /// 获取会话上下文引用
    pub fn conversation(&self) -> &ConversationContext {
        &self.conversation
    }

    /// 带上下文生成 SQL
    ///
    /// 将历史查询上下文注入提示词，生成包含前序过滤条件的 SQL。
    /// 会话超时时自动清理历史记录，避免内存泄漏。
    pub async fn generate_with_context(
        &mut self,
        nl_query: &str,
        schema: &SchemaContext,
    ) -> Result<SqlQuery, Nl2SqlError> {
        // 清理过期历史记录
        self.conversation.cleanup_if_expired();

        // 构建上下文提示词
        let context_prompt = self.conversation.build_context_prompt(nl_query);

        // 生成 SQL
        let result = self.engine.generate(&context_prompt, schema).await?;

        // 记录本轮查询
        self.conversation.add_turn(nl_query, &result.sql);

        Ok(result)
    }

    /// 清空会话历史
    pub fn reset(&mut self) {
        self.conversation.history.clear();
    }
}
