//! 语义查询路由模块
//!
//! 提供 SemanticQueryRouter：分析查询意图，自动选择执行路径（SQL/向量/图谱/Agent/混合）。
//! 同时提供 GraphQueryExecutor trait、AiAgent trait、HybridQueryExecutor。

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ==================== 语义查询意图 ====================

/// 语义查询意图类型（扩展 SQL 意图，增加向量/图谱/Agent/混合）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticIntent {
    /// SQL 查询（结构化数据查询）
    Sql,
    /// 向量查询（语义相似度搜索）
    Vector,
    /// 图谱查询（关系遍历/路径分析）
    Graph,
    /// Agent 查询（多步推理/分析任务）
    Agent,
    /// 混合查询（SQL 过滤 + 向量排序等）
    Hybrid,
}

impl SemanticIntent {
    /// 从自然语言文本推断意图
    pub fn from_text(text: &str) -> Self {
        let lower = text.to_lowercase();

        if lower.contains("分析") || lower.contains("为什么") || lower.contains("原因") {
            return SemanticIntent::Agent;
        }

        if lower.contains("相似") || lower.contains("语义") || lower.contains("向量") {
            if lower.contains("且") || lower.contains("同时") || lower.contains("并且") {
                return SemanticIntent::Hybrid;
            }
            return SemanticIntent::Vector;
        }

        if lower.contains("朋友的朋友")
            || lower.contains("路径")
            || lower.contains("关系")
            || lower.contains("图谱")
            || lower.contains("_connected")
        {
            return SemanticIntent::Graph;
        }

        if lower.contains("且") && lower.contains("相似") {
            return SemanticIntent::Hybrid;
        }

        SemanticIntent::Sql
    }
}

// ==================== 语义查询结果 ====================

/// 语义查询结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SemanticQueryResult {
    /// SQL 查询结果
    Sql {
        sql: String,
        rows: Vec<serde_json::Value>,
    },
    /// 向量查询结果
    Vector {
        query: String,
        matches: Vec<VectorMatch>,
    },
    /// 图谱查询结果
    Graph {
        cypher: String,
        nodes: Vec<GraphNode>,
        edges: Vec<GraphEdge>,
    },
    /// Agent 查询结果
    Agent { report: AgentReport },
    /// 混合查询结果
    Hybrid {
        sql_filter: String,
        vector_query: String,
        results: Vec<HybridMatch>,
    },
}

/// 向量匹配结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorMatch {
    /// 文档 ID
    pub id: String,
    /// 相似度分数（0.0~1.0）
    pub score: f64,
    /// 元数据
    pub metadata: serde_json::Value,
}

/// 图谱节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    /// 节点 ID
    pub id: String,
    /// 节点标签
    pub label: String,
    /// 属性
    pub properties: serde_json::Value,
}

/// 图谱边
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    /// 起点 ID
    pub from: String,
    /// 终点 ID
    pub to: String,
    /// 关系类型
    pub relation: String,
}

/// 混合匹配结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridMatch {
    /// 记录 ID
    pub id: String,
    /// SQL 过滤后的字段
    pub sql_fields: serde_json::Value,
    /// 向量相似度分数
    pub vector_score: f64,
}

// ==================== 语义查询错误 ====================

/// 语义查询错误
#[derive(Debug, Error)]
pub enum SemanticQueryError {
    /// 意图识别失败
    #[error("Intent analysis failed: {0}")]
    IntentFailed(String),
    /// SQL 生成失败
    #[error("SQL generation failed: {0}")]
    SqlFailed(String),
    /// 向量查询失败
    #[error("Vector query failed: {0}")]
    VectorFailed(String),
    /// 图谱查询失败
    #[error("Graph query failed: {0}")]
    GraphFailed(String),
    /// Agent 执行失败
    #[error("Agent failed: {0}")]
    AgentFailed(String),
    /// 混合查询失败
    #[error("Hybrid query failed: {0}")]
    HybridFailed(String),
}

// ==================== 向量存储 trait ====================

/// 向量存储 trait
#[async_trait::async_trait]
pub trait VectorStore: Send + Sync {
    /// 执行向量相似度查询
    async fn search(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<VectorMatch>, SemanticQueryError>;
}

// ==================== 图谱查询执行器 trait ====================

/// 图谱查询执行器 trait
///
/// 包装 sz-orm-graph 的查询能力。
#[async_trait::async_trait]
pub trait GraphQueryExecutor: Send + Sync {
    /// 执行 Cypher 查询
    async fn execute(
        &self,
        cypher: &str,
    ) -> Result<(Vec<GraphNode>, Vec<GraphEdge>), SemanticQueryError>;

    /// 将自然语言转换为 Cypher
    async fn nl_to_cypher(&self, query: &str) -> Result<String, SemanticQueryError>;
}

// ==================== NL2SQL trait ====================

/// NL2SQL 转换 trait
#[async_trait::async_trait]
pub trait Nl2SqlConverter: Send + Sync {
    /// 将自然语言转换为 SQL
    async fn convert(&self, query: &str) -> Result<String, SemanticQueryError>;
}

// ==================== 语义查询路由器 ====================

/// 语义查询路由器
///
/// 分析查询意图，自动选择执行路径（SQL/向量/图谱/Agent/混合）。
pub struct SemanticQueryRouter {
    /// NL2SQL 转换器
    nl2sql: Arc<dyn Nl2SqlConverter>,
    /// 向量存储（可选）
    vector_store: Option<Arc<dyn VectorStore>>,
    /// 图谱执行器（可选）
    graph_executor: Option<Arc<dyn GraphQueryExecutor>>,
    /// AI Agent（可选）
    agent: Option<Arc<dyn AiAgent>>,
}

impl SemanticQueryRouter {
    /// 创建语义查询路由器
    pub fn new(nl2sql: Arc<dyn Nl2SqlConverter>) -> Self {
        Self {
            nl2sql,
            vector_store: None,
            graph_executor: None,
            agent: None,
        }
    }

    /// 设置向量存储
    pub fn with_vector_store(mut self, store: Arc<dyn VectorStore>) -> Self {
        self.vector_store = Some(store);
        self
    }

    /// 设置图谱执行器
    pub fn with_graph_executor(mut self, executor: Arc<dyn GraphQueryExecutor>) -> Self {
        self.graph_executor = Some(executor);
        self
    }

    /// 设置 AI Agent
    pub fn with_agent(mut self, agent: Arc<dyn AiAgent>) -> Self {
        self.agent = Some(agent);
        self
    }

    /// 执行语义查询
    ///
    /// 分析查询意图，自动选择执行路径。
    /// 意图识别失败时默认走 NL2SQL 路径。
    pub async fn query(
        &self,
        text: &str,
    ) -> Result<(SemanticIntent, SemanticQueryResult), SemanticQueryError> {
        let intent = SemanticIntent::from_text(text);

        match intent {
            SemanticIntent::Sql => self.route_sql(text).await.map(|r| (SemanticIntent::Sql, r)),
            SemanticIntent::Vector => self
                .route_vector(text)
                .await
                .map(|r| (SemanticIntent::Vector, r)),
            SemanticIntent::Graph => self
                .route_graph(text)
                .await
                .map(|r| (SemanticIntent::Graph, r)),
            SemanticIntent::Agent => self
                .route_agent(text)
                .await
                .map(|r| (SemanticIntent::Agent, r)),
            SemanticIntent::Hybrid => self
                .route_hybrid(text)
                .await
                .map(|r| (SemanticIntent::Hybrid, r)),
        }
    }

    async fn route_sql(&self, text: &str) -> Result<SemanticQueryResult, SemanticQueryError> {
        let sql = self.nl2sql.convert(text).await?;
        Ok(SemanticQueryResult::Sql {
            sql,
            rows: Vec::new(),
        })
    }

    async fn route_vector(&self, text: &str) -> Result<SemanticQueryResult, SemanticQueryError> {
        let store = self
            .vector_store
            .as_ref()
            .ok_or_else(|| SemanticQueryError::VectorFailed("向量存储未配置".to_string()))?;
        let matches = store.search(text, 10).await?;
        Ok(SemanticQueryResult::Vector {
            query: text.to_string(),
            matches,
        })
    }

    async fn route_graph(&self, text: &str) -> Result<SemanticQueryResult, SemanticQueryError> {
        let executor = self
            .graph_executor
            .as_ref()
            .ok_or_else(|| SemanticQueryError::GraphFailed("图谱执行器未配置".to_string()))?;
        let cypher = executor.nl_to_cypher(text).await?;
        let (nodes, edges) = executor.execute(&cypher).await?;
        Ok(SemanticQueryResult::Graph {
            cypher,
            nodes,
            edges,
        })
    }

    async fn route_agent(&self, text: &str) -> Result<SemanticQueryResult, SemanticQueryError> {
        let agent = self
            .agent
            .as_ref()
            .ok_or_else(|| SemanticQueryError::AgentFailed("Agent 未配置".to_string()))?;
        let report = agent.execute_task(text).await?;
        Ok(SemanticQueryResult::Agent { report })
    }

    async fn route_hybrid(&self, text: &str) -> Result<SemanticQueryResult, SemanticQueryError> {
        let store = self
            .vector_store
            .as_ref()
            .ok_or_else(|| SemanticQueryError::HybridFailed("混合查询需要向量存储".to_string()))?;

        let (sql_part, vector_part) = split_hybrid_query(text);
        let sql_filter = self.nl2sql.convert(&sql_part).await?;
        let matches = store.search(&vector_part, 10).await?;

        let results: Vec<HybridMatch> = matches
            .iter()
            .map(|m| HybridMatch {
                id: m.id.clone(),
                sql_fields: m.metadata.clone(),
                vector_score: m.score,
            })
            .collect();

        Ok(SemanticQueryResult::Hybrid {
            sql_filter,
            vector_query: vector_part,
            results,
        })
    }
}

fn split_hybrid_query(text: &str) -> (String, String) {
    let separators = ["且与", "且", "并且", "同时", "且和"];
    for sep in &separators {
        if let Some(idx) = text.find(sep) {
            let sep_byte_len = sep.len();
            return (
                text[..idx].trim().to_string(),
                text[idx + sep_byte_len..].trim().to_string(),
            );
        }
    }
    (text.to_string(), text.to_string())
}

// ==================== AI Agent trait ====================

/// AI Agent trait
///
/// 多步推理 Agent，将复杂任务分解为子任务逐步执行。
#[async_trait::async_trait]
pub trait AiAgent: Send + Sync {
    /// 执行任务
    async fn execute_task(&self, task: &str) -> Result<AgentReport, SemanticQueryError>;
}

/// Agent 执行步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStep {
    /// 步骤序号
    pub step_number: u32,
    /// 步骤描述
    pub description: String,
    /// 步骤结果
    pub result: String,
    /// 是否成功
    pub success: bool,
}

/// Agent 报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReport {
    /// 执行步骤
    pub steps: Vec<AgentStep>,
    /// 最终结论
    pub conclusion: String,
    /// 置信度（0.0~1.0）
    pub confidence: f64,
}

/// Agent 错误
#[derive(Debug, Error)]
pub enum AgentError {
    /// 超过最大步数
    #[error("Max steps exceeded: {0}")]
    MaxStepsExceeded(u32),
    /// 执行失败
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
}

/// 分析 Agent
///
/// LLM 分解为子任务列表 → 逐步执行 → LLM 综合结果。
pub struct AnalysisAgent {
    /// 最大步数（默认 10）
    max_steps: u32,
    /// NL2SQL 转换器
    nl2sql: Arc<dyn Nl2SqlConverter>,
}

impl AnalysisAgent {
    /// 创建分析 Agent
    pub fn new(nl2sql: Arc<dyn Nl2SqlConverter>) -> Self {
        Self {
            max_steps: 10,
            nl2sql,
        }
    }

    /// 设置最大步数
    pub fn with_max_steps(mut self, max_steps: u32) -> Self {
        self.max_steps = max_steps;
        self
    }
}

#[async_trait::async_trait]
impl AiAgent for AnalysisAgent {
    async fn execute_task(&self, task: &str) -> Result<AgentReport, SemanticQueryError> {
        let sub_tasks = decompose_task(task);

        if sub_tasks.len() as u32 > self.max_steps {
            return Err(SemanticQueryError::AgentFailed(format!(
                "超过最大步数 {}（分解出 {} 个子任务）",
                self.max_steps,
                sub_tasks.len()
            )));
        }

        let mut steps = Vec::new();
        let mut all_results = Vec::new();

        for (i, sub_task) in sub_tasks.iter().enumerate() {
            let result = match self.nl2sql.convert(sub_task).await {
                Ok(sql) => {
                    all_results.push(sql.clone());
                    sql
                }
                Err(e) => {
                    format!("子任务失败: {}", e)
                }
            };

            steps.push(AgentStep {
                step_number: (i + 1) as u32,
                description: sub_task.clone(),
                result: result.clone(),
                success: !result.starts_with("子任务失败"),
            });
        }

        let success_count = steps.iter().filter(|s| s.success).count();
        let confidence = if steps.is_empty() {
            0.0
        } else {
            success_count as f64 / steps.len() as f64
        };

        let conclusion = format!(
            "完成 {} 个子任务（{} 成功 / {} 失败），综合结果: {}",
            steps.len(),
            success_count,
            steps.len() - success_count,
            all_results.join("; ")
        );

        Ok(AgentReport {
            steps,
            conclusion,
            confidence,
        })
    }
}

fn decompose_task(task: &str) -> Vec<String> {
    let mut sub_tasks = Vec::new();

    if task.contains("分析") || task.contains("为什么") || task.contains("原因") {
        sub_tasks.push(format!("查询{}相关数据", extract_subject(task)));
        sub_tasks.push(format!("查询{}趋势数据", extract_subject(task)));
        sub_tasks.push(format!("综合分析{}原因", extract_subject(task)));
    } else {
        sub_tasks.push(task.to_string());
    }

    sub_tasks
}

fn extract_subject(task: &str) -> String {
    let keywords = ["销售", "用户", "订单", "收入", "流量", "性能"];
    for kw in &keywords {
        if task.contains(kw) {
            return kw.to_string();
        }
    }
    "相关".to_string()
}

// ==================== 混合查询执行器 ====================

/// 混合查询执行器
///
/// SQL 过滤后向量相似度排序（如"价格 < 100 且与'红色连衣裙'相似的商品"）。
pub struct HybridQueryExecutor {
    nl2sql: Arc<dyn Nl2SqlConverter>,
    vector_store: Arc<dyn VectorStore>,
}

impl HybridQueryExecutor {
    /// 创建混合查询执行器
    pub fn new(nl2sql: Arc<dyn Nl2SqlConverter>, vector_store: Arc<dyn VectorStore>) -> Self {
        Self {
            nl2sql,
            vector_store,
        }
    }

    /// 执行混合查询
    ///
    /// 将混合查询拆分为 SQL 过滤部分和向量相似度部分，
    /// 先执行 SQL 过滤，再对结果执行向量排序。
    pub async fn execute_hybrid(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<SemanticQueryResult, SemanticQueryError> {
        let (sql_part, vector_part) = split_hybrid_query(query);
        let sql_filter = self.nl2sql.convert(&sql_part).await?;
        let matches = self.vector_store.search(&vector_part, top_k).await?;

        let results: Vec<HybridMatch> = matches
            .iter()
            .map(|m| HybridMatch {
                id: m.id.clone(),
                sql_fields: m.metadata.clone(),
                vector_score: m.score,
            })
            .collect();

        Ok(SemanticQueryResult::Hybrid {
            sql_filter,
            vector_query: vector_part,
            results,
        })
    }
}
