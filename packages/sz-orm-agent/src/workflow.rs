//! 工作流编排与并行（TASK-017）
//!
//! DAG 拓扑排序，无依赖子任务并行执行，有依赖子任务按拓扑序串行执行。

use crate::types::AgentError;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;

/// 工作流子任务
#[derive(Debug, Clone)]
pub struct WorkflowTask {
    pub id: String,
    pub action: String,
    pub params: HashMap<String, String>,
    pub dependencies: Vec<String>,
}

/// 工作流执行结果
#[derive(Debug, Clone)]
pub struct WorkflowResult {
    pub task_id: String,
    pub success: bool,
    pub result: String,
}

/// 工作流编排器
pub struct WorkflowOrchestrator {
    tasks: Vec<WorkflowTask>,
    results: Arc<Mutex<Vec<WorkflowResult>>>,
}

impl WorkflowOrchestrator {
    pub fn new(tasks: Vec<WorkflowTask>) -> Result<Self, AgentError> {
        Self::validate_dag(&tasks)?;
        Ok(Self {
            tasks,
            results: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// 验证 DAG 无环
    fn validate_dag(tasks: &[WorkflowTask]) -> Result<(), AgentError> {
        let ids: HashSet<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
        for task in tasks {
            for dep in &task.dependencies {
                if !ids.contains(dep.as_str()) {
                    return Err(AgentError::ToolExecutionFailed(format!(
                        "任务 {} 依赖不存在的任务: {}",
                        task.id, dep
                    )));
                }
            }
        }
        let mut visited = HashSet::new();
        let mut stack = HashSet::new();
        for task in tasks {
            if !visited.contains(&task.id) {
                Self::detect_cycle(&task.id, tasks, &mut visited, &mut stack)?;
            }
        }
        Ok(())
    }

    fn detect_cycle(
        id: &str,
        tasks: &[WorkflowTask],
        visited: &mut HashSet<String>,
        stack: &mut HashSet<String>,
    ) -> Result<(), AgentError> {
        if stack.contains(id) {
            return Err(AgentError::ToolExecutionFailed(format!(
                "检测到循环依赖: {id}"
            )));
        }
        if visited.contains(id) {
            return Ok(());
        }
        visited.insert(id.to_string());
        stack.insert(id.to_string());

        let task = tasks.iter().find(|t| t.id == id);
        if let Some(task) = task {
            for dep in &task.dependencies {
                Self::detect_cycle(dep, tasks, visited, stack)?;
            }
        }
        stack.remove(id);
        Ok(())
    }

    /// 拓扑排序：返回按层级组织的任务列表，每层内的任务可并行执行
    fn topological_sort(&self) -> Result<Vec<Vec<WorkflowTask>>, AgentError> {
        let mut layers = Vec::new();
        let mut completed: HashSet<String> = HashSet::new();
        let total = self.tasks.len();

        while completed.len() < total {
            let ready: Vec<WorkflowTask> = self
                .tasks
                .iter()
                .filter(|t| {
                    !completed.contains(&t.id)
                        && t.dependencies.iter().all(|d| completed.contains(d))
                })
                .cloned()
                .collect();

            if ready.is_empty() {
                return Err(AgentError::ToolExecutionFailed(
                    "拓扑排序失败: 存在未解决的依赖".into(),
                ));
            }

            for t in &ready {
                completed.insert(t.id.clone());
            }
            layers.push(ready);
        }
        Ok(layers)
    }

    /// 执行工作流
    pub async fn execute<F, Fut>(&self, executor: F) -> Result<Vec<WorkflowResult>, AgentError>
    where
        F: Fn(WorkflowTask) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<String, AgentError>> + Send,
    {
        let layers = self.topological_sort()?;
        let executor = Arc::new(executor);

        for layer in layers {
            let layer_results: Vec<WorkflowResult> =
                futures::future::join_all(layer.into_iter().map(|task| {
                    let executor = executor.clone();
                    async move {
                        let result = executor(task.clone()).await;
                        WorkflowResult {
                            task_id: task.id,
                            success: result.is_ok(),
                            result: result.unwrap_or_else(|e| e.to_string()),
                        }
                    }
                }))
                .await;

            self.results.lock().await.extend(layer_results);
        }

        Ok(self.results.lock().await.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn test_parallel_execution() {
        let tasks = vec![
            WorkflowTask {
                id: "a".to_string(),
                action: "sleep".to_string(),
                params: HashMap::from([("ms".to_string(), "100".to_string())]),
                dependencies: vec![],
            },
            WorkflowTask {
                id: "b".to_string(),
                action: "sleep".to_string(),
                params: HashMap::from([("ms".to_string(), "100".to_string())]),
                dependencies: vec![],
            },
            WorkflowTask {
                id: "c".to_string(),
                action: "sleep".to_string(),
                params: HashMap::from([("ms".to_string(), "100".to_string())]),
                dependencies: vec![],
            },
        ];

        let orchestrator = WorkflowOrchestrator::new(tasks).unwrap();
        let start = Instant::now();
        let results = orchestrator
            .execute(|task| async move {
                let ms: u64 = task.params.get("ms").unwrap().parse().unwrap();
                tokio::time::sleep(tokio::time::Duration::from_millis(ms)).await;
                Ok(format!("{} done", task.id))
            })
            .await
            .unwrap();
        let elapsed = start.elapsed();

        assert_eq!(results.len(), 3);
        assert!(all_succeeded(&results));
        assert!(
            elapsed < tokio::time::Duration::from_millis(250),
            "并行执行 3 个 100ms 任务应 < 250ms，实际: {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_sequential_with_dependencies() {
        let tasks = vec![
            WorkflowTask {
                id: "a".to_string(),
                action: "step".to_string(),
                params: HashMap::new(),
                dependencies: vec![],
            },
            WorkflowTask {
                id: "b".to_string(),
                action: "step".to_string(),
                params: HashMap::new(),
                dependencies: vec!["a".to_string()],
            },
            WorkflowTask {
                id: "c".to_string(),
                action: "step".to_string(),
                params: HashMap::new(),
                dependencies: vec!["b".to_string()],
            },
        ];

        let orchestrator = WorkflowOrchestrator::new(tasks).unwrap();
        let results = orchestrator
            .execute(|task| async move { Ok(format!("{} done", task.id)) })
            .await
            .unwrap();

        assert_eq!(results.len(), 3);
        assert!(all_succeeded(&results));
    }

    #[tokio::test]
    async fn test_cycle_detection() {
        let tasks = vec![
            WorkflowTask {
                id: "a".to_string(),
                action: "step".to_string(),
                params: HashMap::new(),
                dependencies: vec!["b".to_string()],
            },
            WorkflowTask {
                id: "b".to_string(),
                action: "step".to_string(),
                params: HashMap::new(),
                dependencies: vec!["a".to_string()],
            },
        ];

        let result = WorkflowOrchestrator::new(tasks);
        assert!(result.is_err(), "循环依赖应被检测");
    }

    fn all_succeeded(results: &[WorkflowResult]) -> bool {
        results.iter().all(|r| r.success)
    }
}
