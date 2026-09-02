//! NL 查询闭环主管线（TASK-008 + TASK-025 追问）

use crate::types::{NlQueryError, NlQueryResponse};
use serde::{Deserialize, Serialize};

/// 对话上下文（TASK-025）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationContext {
    pub history: Vec<ConversationTurn>,
    pub last_sql: Option<String>,
    pub last_tables: Vec<String>,
}

/// 对话轮次
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub user_query: String,
    pub generated_sql: String,
    pub timestamp: String,
}

impl ConversationContext {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            last_sql: None,
            last_tables: Vec::new(),
        }
    }

    pub fn add_turn(&mut self, query: &str, sql: &str) {
        self.last_sql = Some(sql.to_string());
        self.history.push(ConversationTurn {
            user_query: query.to_string(),
            generated_sql: sql.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
    }

    /// 构建带上下文的追问 prompt
    pub fn build_refine_prompt(&self, new_query: &str) -> String {
        let mut prompt = String::new();
        if let Some(last_sql) = &self.last_sql {
            prompt.push_str(&format!("上一轮 SQL: {}\n", last_sql));
        }
        if !self.last_tables.is_empty() {
            prompt.push_str(&format!("相关表: {}\n", self.last_tables.join(", ")));
        }
        prompt.push_str(&format!("追问: {}", new_query));
        prompt
    }
}

impl Default for ConversationContext {
    fn default() -> Self {
        Self::new()
    }
}

/// NL 查询管线
pub struct NlQueryPipeline {
    context: ConversationContext,
}

impl NlQueryPipeline {
    pub fn new() -> Self {
        Self {
            context: ConversationContext::new(),
        }
    }

    pub async fn query(&self, nl: &str) -> Result<NlQueryResponse, NlQueryError> {
        let sql = Self::nl2sql_stub(nl)?;
        Ok(NlQueryResponse {
            sql: sql.clone(),
            sql_explanation: format!("将自然语言 '{}' 转换为 SQL", nl),
            rows: serde_json::Value::Array(vec![]),
            visualization: None,
            insight: None,
            truncated: false,
        })
    }

    /// 追问查询（TASK-025）：基于上下文细化查询
    pub async fn refine(&mut self, new_query: &str) -> Result<NlQueryResponse, NlQueryError> {
        let prompt = self.context.build_refine_prompt(new_query);
        let response = self.query(&prompt).await?;

        self.context.add_turn(new_query, &response.sql);

        Ok(response)
    }

    /// 获取当前对话上下文
    pub fn context(&self) -> &ConversationContext {
        &self.context
    }

    /// 设置相关表
    pub fn set_tables(&mut self, tables: Vec<String>) {
        self.context.last_tables = tables;
    }

    fn nl2sql_stub(nl: &str) -> Result<String, NlQueryError> {
        if nl.is_empty() {
            return Err(NlQueryError::Nl2SqlFailed("空查询".to_string()));
        }
        Ok(format!("-- Generated from: {}\nSELECT * FROM data", nl))
    }
}

impl Default for NlQueryPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_query_basic() {
        let pipeline = NlQueryPipeline::new();
        let result = pipeline.query("查询所有用户").await;
        assert!(result.is_ok());
        assert!(result.unwrap().sql.contains("SELECT"));
    }

    #[tokio::test]
    async fn test_refine_with_context() {
        let mut pipeline = NlQueryPipeline::new();
        pipeline.set_tables(vec!["users".to_string()]);

        let refined = pipeline.refine("只看活跃用户").await.unwrap();
        assert!(refined.sql.contains("SELECT"));
        assert!(pipeline.context().history.len() >= 1);
    }

    #[test]
    fn test_conversation_context_build_prompt() {
        let mut ctx = ConversationContext::new();
        ctx.add_turn("查询订单", "SELECT * FROM orders");
        ctx.last_tables = vec!["orders".to_string()];

        let prompt = ctx.build_refine_prompt("按金额排序");
        assert!(prompt.contains("SELECT * FROM orders"));
        assert!(prompt.contains("orders"));
        assert!(prompt.contains("按金额排序"));
    }

    #[tokio::test]
    async fn test_refine_accumulates_history() {
        let mut pipeline = NlQueryPipeline::new();

        pipeline.refine("查询用户").await.unwrap();
        pipeline.refine("只看活跃的").await.unwrap();
        pipeline.refine("按注册时间排序").await.unwrap();

        assert_eq!(pipeline.context().history.len(), 3);
    }

    #[tokio::test]
    async fn test_empty_query_fails() {
        let pipeline = NlQueryPipeline::new();
        let result = pipeline.query("").await;
        assert!(result.is_err());
    }
}
