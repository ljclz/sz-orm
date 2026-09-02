//! Agent 感知-决策-执行循环驱动器

use crate::perception::PerceptionCollector;
use crate::types::*;
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;

/// 数据库 Agent trait
///
/// 定义 Agent 的核心能力：委托任务、查询状态、恢复任务、取消任务、审批操作。
#[async_trait]
pub trait DatabaseAgent: Send + Sync {
    /// 委托任务给 Agent 执行
    async fn delegate_task(&self, spec: AgentTaskSpec) -> Result<TaskHandle, AgentError>;

    /// 查询任务状态
    async fn task_status(&self, task_id: &str) -> Result<TaskHandle, AgentError>;

    /// 从检查点恢复中断任务
    async fn resume_task(&self, task_id: &str) -> Result<TaskHandle, AgentError>;

    /// 取消任务
    async fn cancel_task(&self, task_id: &str) -> Result<TaskHandle, AgentError>;

    /// 审批危险操作
    async fn approve_action(
        &self,
        task_id: &str,
        step_number: usize,
        approved: bool,
    ) -> Result<(), AgentError>;
}

/// 规划器 trait
#[async_trait]
pub trait Planner: Send + Sync {
    /// 根据感知快照生成下一步行动
    async fn plan(
        &self,
        perception: &PerceptionSnapshot,
        history: &[AgentStep],
        spec: &AgentTaskSpec,
    ) -> Result<PlannerOutput, AgentError>;
}

/// 规划器输出
#[derive(Debug, Clone)]
pub struct PlannerOutput {
    /// 思考链
    pub thought: String,
    /// 行动名称
    pub action: String,
    /// 行动参数
    pub action_params: HashMap<String, String>,
}

/// 行动执行器 trait
#[async_trait]
pub trait ActionExecutor: Send + Sync {
    /// 执行行动
    async fn execute(
        &self,
        action: &str,
        params: &HashMap<String, String>,
    ) -> Result<String, AgentError>;
}

/// Agent 循环驱动器
///
/// 实现 perceive-decide-act 循环：
/// 1. 感知：采集多源诊断信号
/// 2. 决策：规划器生成行动
/// 3. 执行：执行行动并观测反馈
/// 4. 写检查点
pub struct AgentDriver {
    perception_collector: Arc<PerceptionCollector>,
    planner: Arc<dyn Planner>,
    executor: Arc<dyn ActionExecutor>,
    max_steps: usize,
}

impl AgentDriver {
    pub fn new(
        perception_collector: Arc<PerceptionCollector>,
        planner: Arc<dyn Planner>,
        executor: Arc<dyn ActionExecutor>,
    ) -> Self {
        Self {
            perception_collector,
            planner,
            executor,
            max_steps: 20,
        }
    }

    pub fn with_max_steps(mut self, max_steps: usize) -> Self {
        self.max_steps = max_steps;
        self
    }

    /// 运行感知-决策-执行循环
    pub async fn run_loop(
        &self,
        spec: &AgentTaskSpec,
        signals: PerceptionSignals,
    ) -> Result<Vec<AgentStep>, AgentError> {
        let mut steps = Vec::new();
        let max_steps = spec.max_steps.min(self.max_steps);

        for step_number in 1..=max_steps {
            // 1. 感知
            let perception = self
                .perception_collector
                .collect(
                    signals.slow_queries.clone(),
                    signals.pool_metrics.clone(),
                    signals.deadlocks.clone(),
                    signals.anomalies.clone(),
                    signals.failure_predictions.clone(),
                )
                .await?;

            // 2. 决策
            let plan = self.planner.plan(&perception, &steps, spec).await?;

            // 3. 执行
            let (result, success) = match self
                .executor
                .execute(&plan.action, &plan.action_params)
                .await
            {
                Ok(r) => (r, true),
                Err(e) => (e.to_string(), false),
            };

            // 4. 记录步骤
            let step = AgentStep {
                step_number,
                perception,
                thought: plan.thought,
                action: plan.action,
                action_params: plan.action_params,
                result,
                success,
                timestamp: Utc::now(),
            };

            let is_noop = step.action == "noop";
            let should_stop = !step.success || is_noop;
            steps.push(step);

            if should_stop {
                break;
            }
        }

        Ok(steps)
    }
}

/// 感知信号集合
#[derive(Debug, Clone, Default)]
pub struct PerceptionSignals {
    pub slow_queries: Vec<String>,
    pub pool_metrics: HashMap<String, f64>,
    pub deadlocks: Vec<String>,
    pub anomalies: Vec<String>,
    pub failure_predictions: Vec<String>,
}

/// 简单规则规划器（用于测试和降级模式）
pub struct RuleBasedPlanner;

#[async_trait]
impl Planner for RuleBasedPlanner {
    async fn plan(
        &self,
        perception: &PerceptionSnapshot,
        _history: &[AgentStep],
        _spec: &AgentTaskSpec,
    ) -> Result<PlannerOutput, AgentError> {
        if !perception.deadlocks.is_empty() {
            return Ok(PlannerOutput {
                thought: "检测到死锁，建议终止相关事务".to_string(),
                action: "report_deadlock".to_string(),
                action_params: HashMap::from([(
                    "deadlocks".to_string(),
                    perception.deadlocks.join(","),
                )]),
            });
        }

        if !perception.slow_queries.is_empty() {
            return Ok(PlannerOutput {
                thought: "检测到慢查询，建议分析执行计划".to_string(),
                action: "analyze_slow_query".to_string(),
                action_params: HashMap::from([(
                    "queries".to_string(),
                    perception.slow_queries.join(","),
                )]),
            });
        }

        Ok(PlannerOutput {
            thought: "系统健康，无需操作".to_string(),
            action: "noop".to_string(),
            action_params: HashMap::new(),
        })
    }
}

/// 简单行动执行器（用于测试）
pub struct SimpleExecutor;

#[async_trait]
impl ActionExecutor for SimpleExecutor {
    async fn execute(
        &self,
        action: &str,
        _params: &HashMap<String, String>,
    ) -> Result<String, AgentError> {
        match action {
            "noop" => Ok("无操作".to_string()),
            "report_deadlock" => Ok("死锁已报告".to_string()),
            "analyze_slow_query" => Ok("慢查询分析完成".to_string()),
            other => Ok(format!("已执行: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_agent_loop_no_issues() {
        let driver = AgentDriver::new(
            Arc::new(PerceptionCollector::new()),
            Arc::new(RuleBasedPlanner),
            Arc::new(SimpleExecutor),
        )
        .with_max_steps(3);

        let spec = AgentTaskSpec {
            task_id: "test-1".to_string(),
            description: "巡检".to_string(),
            planner_mode: PlannerMode::RuleBased,
            max_steps: 3,
            created_at: Utc::now(),
        };

        let steps = driver
            .run_loop(&spec, PerceptionSignals::default())
            .await
            .unwrap();
        assert!(!steps.is_empty());
        assert_eq!(steps[0].action, "noop");
    }

    #[tokio::test]
    async fn test_agent_loop_with_slow_queries() {
        let driver = AgentDriver::new(
            Arc::new(PerceptionCollector::new()),
            Arc::new(RuleBasedPlanner),
            Arc::new(SimpleExecutor),
        )
        .with_max_steps(1);

        let spec = AgentTaskSpec {
            task_id: "test-2".to_string(),
            description: "巡检".to_string(),
            planner_mode: PlannerMode::RuleBased,
            max_steps: 1,
            created_at: Utc::now(),
        };

        let signals = PerceptionSignals {
            slow_queries: vec!["SELECT * FROM big".to_string()],
            ..Default::default()
        };

        let steps = driver.run_loop(&spec, signals).await.unwrap();
        assert_eq!(steps[0].action, "analyze_slow_query");
        assert!(steps[0].success);
    }
}
