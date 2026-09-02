//! NL 查询闭环主管线（TASK-008 + TASK-025 追问）

use crate::types::{NlQueryError, NlQueryResponse};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// SQL 执行器接口
///
/// 用户可实现此 trait 注入真实数据库连接，使 `NlQueryPipeline::query` 返回真实 rows。
/// 未注入执行器时，`query` 返回空 rows（降级模式）。
#[async_trait]
pub trait SqlExecutor: Send + Sync {
    /// 执行 SQL，返回 JSON 格式的结果行集
    async fn execute(&self, sql: &str) -> Result<serde_json::Value, NlQueryError>;
}

/// NL2SQL 生成器接口
///
/// 用户可实现此 trait 注入 LLM 引擎，替换规则型 NL2SQL。
/// 未注入时使用 `nl2sql_rule_based` 降级实现。
#[async_trait]
pub trait Nl2SqlGenerator: Send + Sync {
    /// 将自然语言转换为 SQL
    async fn generate(&self, nl: &str) -> Result<String, NlQueryError>;
}

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
    executor: Option<Arc<dyn SqlExecutor>>,
    generator: Option<Arc<dyn Nl2SqlGenerator>>,
}

impl NlQueryPipeline {
    pub fn new() -> Self {
        Self {
            context: ConversationContext::new(),
            executor: None,
            generator: None,
        }
    }

    /// 注入 SQL 执行器，使 `query` 返回真实数据库结果
    pub fn with_executor(mut self, executor: Arc<dyn SqlExecutor>) -> Self {
        self.executor = Some(executor);
        self
    }

    /// 注入 NL2SQL 生成器（如 LLM 引擎），替换规则型转换
    pub fn with_llm_generator(mut self, generator: Arc<dyn Nl2SqlGenerator>) -> Self {
        self.generator = Some(generator);
        self
    }

    /// 执行 NL 查询
    ///
    /// - 注入了 `Nl2SqlGenerator`（LLM 引擎）时：使用 LLM 生成 SQL
    /// - 未注入 generator 时：使用规则型 NL2SQL 降级生成
    /// - 注入了 `SqlExecutor` 时：执行 SQL 返回真实 rows
    /// - 未注入执行器时：`rows` 为空数组（降级模式）
    pub async fn query(&self, nl: &str) -> Result<NlQueryResponse, NlQueryError> {
        let sql = if let Some(generator) = &self.generator {
            generator.generate(nl).await?
        } else {
            Self::nl2sql_rule_based(nl)?
        };

        let rows = if let Some(executor) = &self.executor {
            executor.execute(&sql).await?
        } else {
            serde_json::Value::Array(vec![])
        };

        Ok(NlQueryResponse {
            sql: sql.clone(),
            sql_explanation: format!("将自然语言 '{}' 转换为 SQL", nl),
            rows,
            visualization: None,
            insight: None,
            truncated: false,
        })
    }

    /// 直接执行 SQL（跳过 NL2SQL 转换）
    ///
    /// 当用户已持有合法 SQL 时，绕过 NL2SQL 规则引擎，直接通过执行器执行。
    /// 未注入执行器时返回 `Nl2SqlFailed` 错误。
    pub async fn execute_sql(&self, sql: &str) -> Result<NlQueryResponse, NlQueryError> {
        let executor = self.executor.as_ref().ok_or_else(|| {
            NlQueryError::Nl2SqlFailed("execute_sql 需要注入 SqlExecutor".to_string())
        })?;

        let rows = executor.execute(sql).await?;

        Ok(NlQueryResponse {
            sql: sql.to_string(),
            sql_explanation: "直接执行用户提供的 SQL".to_string(),
            rows,
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

    /// 规则型 NL2SQL 转换
    ///
    /// 通过中文关键词映射表提取目标表名，生成基础 SELECT 语句。
    /// 这是 LLM 不可用时的降级实现，生产环境应接入 NL2SqlEngine。
    fn nl2sql_rule_based(nl: &str) -> Result<String, NlQueryError> {
        if nl.is_empty() {
            return Err(NlQueryError::Nl2SqlFailed("空查询".to_string()));
        }

        let table = Self::detect_table(nl);
        Ok(format!(
            "-- Generated from: {}\nSELECT * FROM {}",
            nl, table
        ))
    }

    /// 从自然语言中检测目标表名
    ///
    /// 中文关键词 → 英文表名映射，支持常见的业务表名。
    /// v6.0.0 优化：扩展至 20 个关键词，覆盖支付/财务/权限场景。
    fn detect_table(nl: &str) -> String {
        let mappings: &[(&str, &str)] = &[
            ("用户", "users"),
            ("订单", "orders"),
            ("产品", "products"),
            ("商品", "products"),
            ("客户", "customers"),
            ("销售", "sales"),
            ("库存", "inventory"),
            ("日志", "logs"),
            ("文章", "articles"),
            ("评论", "comments"),
            ("支付", "payments"),
            ("退款", "refunds"),
            ("结算", "settlements"),
            ("商户", "merchants"),
            ("渠道", "channels"),
            ("权限", "permissions"),
            ("角色", "roles"),
            ("菜单", "menus"),
            ("配置", "configs"),
            ("通知", "notifications"),
        ];

        for (keyword, table) in mappings {
            if nl.contains(keyword) {
                return table.to_string();
            }
        }

        "data".to_string()
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
        assert!(!pipeline.context().history.is_empty());
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

    #[tokio::test]
    async fn test_table_name_extraction() {
        let pipeline = NlQueryPipeline::new();
        let resp = pipeline.query("查询所有用户").await.unwrap();
        assert!(
            resp.sql.contains("FROM users"),
            "应从'用户'提取表名 users，实际: {}",
            resp.sql
        );
    }

    #[tokio::test]
    async fn test_table_name_orders() {
        let pipeline = NlQueryPipeline::new();
        let resp = pipeline.query("查看订单").await.unwrap();
        assert!(
            resp.sql.contains("FROM orders"),
            "应从'订单'提取表名 orders，实际: {}",
            resp.sql
        );
    }

    #[tokio::test]
    async fn test_llm_generator_overrides_rule_based() {
        struct MockGenerator;
        #[async_trait]
        impl Nl2SqlGenerator for MockGenerator {
            async fn generate(&self, _nl: &str) -> Result<String, NlQueryError> {
                Ok("SELECT 1 AS llm_active".to_string())
            }
        }

        let pipeline = NlQueryPipeline::new().with_llm_generator(Arc::new(MockGenerator));
        let resp = pipeline.query("查询所有用户").await.unwrap();
        assert_eq!(
            resp.sql, "SELECT 1 AS llm_active",
            "注入 LLM generator 后应使用 generator 而非规则引擎"
        );
    }

    #[tokio::test]
    async fn test_llm_generator_error_propagates() {
        struct FailingGenerator;
        #[async_trait]
        impl Nl2SqlGenerator for FailingGenerator {
            async fn generate(&self, _nl: &str) -> Result<String, NlQueryError> {
                Err(NlQueryError::Nl2SqlFailed("LLM 不可用".to_string()))
            }
        }

        let pipeline = NlQueryPipeline::new().with_llm_generator(Arc::new(FailingGenerator));
        let result = pipeline.query("查询用户").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_detect_table_extended_keywords() {
        let pipeline = NlQueryPipeline::new();

        let resp = pipeline.query("查询支付记录").await.unwrap();
        assert!(resp.sql.contains("FROM payments"));

        let resp = pipeline.query("查看退款").await.unwrap();
        assert!(resp.sql.contains("FROM refunds"));

        let resp = pipeline.query("商户列表").await.unwrap();
        assert!(resp.sql.contains("FROM merchants"));

        let resp = pipeline.query("渠道信息").await.unwrap();
        assert!(resp.sql.contains("FROM channels"));

        let resp = pipeline.query("权限配置").await.unwrap();
        assert!(resp.sql.contains("FROM permissions"));
    }
}
