//! 核心数据结构定义

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 规划器模式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlannerMode {
    /// ReAct：交替推理与行动
    ReAct,
    /// Plan-and-Execute：先规划再执行
    PlanAndExecute,
    /// 规则降级模式（无 LLM）
    RuleBased,
}

/// 任务状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    /// 待执行
    Pending,
    /// 执行中
    Running,
    /// 等待人工确认
    WaitingApproval,
    /// 已完成
    Completed,
    /// 已失败
    Failed,
    /// 已取消
    Cancelled,
}

/// Agent 任务规格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskSpec {
    /// 任务 ID
    pub task_id: String,
    /// 自然语言任务描述
    pub description: String,
    /// 规划器模式
    pub planner_mode: PlannerMode,
    /// 最大步数
    pub max_steps: usize,
    /// 创建时间
    pub created_at: DateTime<Utc>,
}

/// 任务句柄
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskHandle {
    /// 任务 ID
    pub task_id: String,
    /// 当前状态
    pub status: TaskStatus,
    /// 已执行步数
    pub steps_completed: usize,
    /// 最大步数
    pub max_steps: usize,
}

/// 感知快照：多源信号聚合
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerceptionSnapshot {
    /// 采集时间
    pub timestamp: DateTime<Utc>,
    /// 慢查询信号
    pub slow_queries: Vec<String>,
    /// 连接池信号
    pub pool_metrics: HashMap<String, f64>,
    /// 死锁信号
    pub deadlocks: Vec<String>,
    /// 异常信号
    pub anomalies: Vec<String>,
    /// 故障预测信号
    pub failure_predictions: Vec<String>,
    /// 综合健康评分 [0, 1]
    pub health_score: f64,
}

/// Agent 执行步骤记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStep {
    /// 步骤号
    pub step_number: usize,
    /// 感知快照
    pub perception: PerceptionSnapshot,
    /// 思考链（LLM 推理过程）
    pub thought: String,
    /// 行动（工具调用名）
    pub action: String,
    /// 行动参数
    pub action_params: HashMap<String, String>,
    /// 执行结果
    pub result: String,
    /// 是否成功
    pub success: bool,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
}

/// Agent 错误类型
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("任务不存在: {0}")]
    TaskNotFound(String),
    #[error("任务已取消: {0}")]
    TaskCancelled(String),
    #[error("超过最大步数限制: {0}")]
    MaxStepsExceeded(String),
    #[error("LLM 服务不可用: {0}")]
    LlmUnavailable(String),
    #[error("工具执行失败: {0}")]
    ToolExecutionFailed(String),
    #[error("权限拒绝: {0}")]
    PermissionDenied(String),
    #[error("审批超时: {0}")]
    ApprovalTimeout(String),
    #[error("检查点写入失败: {0}")]
    CheckpointFailure(String),
    #[error("感知信号采集失败: {0}")]
    PerceptionFailed(String),
}

impl Default for PerceptionSnapshot {
    fn default() -> Self {
        Self {
            timestamp: Utc::now(),
            slow_queries: Vec::new(),
            pool_metrics: HashMap::new(),
            deadlocks: Vec::new(),
            anomalies: Vec::new(),
            failure_predictions: Vec::new(),
            health_score: 1.0,
        }
    }
}
