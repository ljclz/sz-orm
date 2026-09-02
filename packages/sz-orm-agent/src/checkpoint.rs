//! Agent 状态持久化/检查点恢复（TASK-005）

use crate::types::{AgentError, AgentStep, TaskHandle, TaskStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 检查点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub task_id: String,
    pub step_number: usize,
    pub steps: Vec<AgentStep>,
    pub status: TaskStatus,
    pub saved_at: DateTime<Utc>,
}

/// 检查点存储
pub struct CheckpointStore {
    checkpoints: Mutex<HashMap<String, Checkpoint>>,
    available: Mutex<bool>,
}

impl CheckpointStore {
    /// 默认最大步数
    pub const DEFAULT_MAX_STEPS: usize = 20;

    pub fn new() -> Self {
        Self {
            checkpoints: Mutex::new(HashMap::new()),
            available: Mutex::new(true),
        }
    }

    /// 写入检查点
    pub async fn save(&self, checkpoint: Checkpoint) -> Result<(), AgentError> {
        let available = self.available.lock().await;
        if !*available {
            return Err(AgentError::CheckpointFailure("存储不可用".into()));
        }
        drop(available);

        let serialized = serde_json::to_string(&checkpoint)
            .map_err(|e| AgentError::CheckpointFailure(e.to_string()))?;

        if serialized.len() > 16384 {
            return Err(AgentError::CheckpointFailure(format!(
                "检查点过大: {} bytes > 16KB",
                serialized.len()
            )));
        }

        self.checkpoints
            .lock()
            .await
            .insert(checkpoint.task_id.clone(), checkpoint);
        Ok(())
    }

    /// 加载检查点
    pub async fn load(&self, task_id: &str) -> Result<Option<Checkpoint>, AgentError> {
        let available = self.available.lock().await;
        if !*available {
            return Err(AgentError::CheckpointFailure("存储不可用".into()));
        }
        drop(available);

        Ok(self.checkpoints.lock().await.get(task_id).cloned())
    }

    /// 从检查点恢复任务
    ///
    /// `max_steps` 默认为 `DEFAULT_MAX_STEPS`（20），
    /// 可通过 `resume_task_with_limit` 指定。
    pub async fn resume_task(&self, task_id: &str) -> Result<TaskHandle, AgentError> {
        self.resume_task_with_limit(task_id, Self::DEFAULT_MAX_STEPS)
            .await
    }

    /// 从检查点恢复任务，指定最大步数
    pub async fn resume_task_with_limit(
        &self,
        task_id: &str,
        max_steps: usize,
    ) -> Result<TaskHandle, AgentError> {
        let checkpoint = self
            .load(task_id)
            .await?
            .ok_or_else(|| AgentError::TaskNotFound(task_id.to_string()))?;

        Ok(TaskHandle {
            task_id: checkpoint.task_id,
            status: checkpoint.status,
            steps_completed: checkpoint.step_number,
            max_steps,
        })
    }

    /// 标记存储不可用（模拟故障）
    pub async fn set_unavailable(&self) {
        *self.available.lock().await = false;
    }

    /// 检查存储是否可用
    pub async fn is_available(&self) -> bool {
        *self.available.lock().await
    }
}

impl Default for CheckpointStore {
    fn default() -> Self {
        Self::new()
    }
}

/// 检查点管理器：协调 Agent 循环与检查点存储
pub struct CheckpointManager {
    store: Arc<CheckpointStore>,
}

impl CheckpointManager {
    pub fn new(store: Arc<CheckpointStore>) -> Self {
        Self { store }
    }

    /// 每步执行后保存检查点
    pub async fn save_step(
        &self,
        task_id: &str,
        steps: &[AgentStep],
        status: TaskStatus,
    ) -> Result<(), AgentError> {
        let step_number = steps.len();
        let checkpoint = Checkpoint {
            task_id: task_id.to_string(),
            step_number,
            steps: steps.to_vec(),
            status,
            saved_at: Utc::now(),
        };
        self.store.save(checkpoint).await
    }

    /// 从检查点恢复
    pub async fn resume(&self, task_id: &str) -> Result<(Vec<AgentStep>, TaskStatus), AgentError> {
        let checkpoint = self
            .store
            .load(task_id)
            .await?
            .ok_or_else(|| AgentError::TaskNotFound(task_id.to_string()))?;
        Ok((checkpoint.steps, checkpoint.status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AgentStep, PerceptionSnapshot};

    fn make_step(n: usize) -> AgentStep {
        AgentStep {
            step_number: n,
            perception: PerceptionSnapshot::default(),
            thought: format!("step {n}"),
            action: "noop".to_string(),
            action_params: HashMap::new(),
            result: "ok".to_string(),
            success: true,
            timestamp: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let store = CheckpointStore::new();
        let steps = vec![make_step(1), make_step(2), make_step(3)];

        store
            .save(Checkpoint {
                task_id: "task-1".to_string(),
                step_number: 3,
                steps: steps.clone(),
                status: TaskStatus::Running,
                saved_at: Utc::now(),
            })
            .await
            .unwrap();

        let loaded = store.load("task-1").await.unwrap().unwrap();
        assert_eq!(loaded.step_number, 3);
        assert_eq!(loaded.steps.len(), 3);
    }

    #[tokio::test]
    async fn test_resume_from_checkpoint() {
        let store = Arc::new(CheckpointStore::new());
        let manager = CheckpointManager::new(store.clone());

        let steps = vec![make_step(1), make_step(2), make_step(3), make_step(4)];
        manager
            .save_step("task-2", &steps, TaskStatus::Running)
            .await
            .unwrap();

        let (restored_steps, status) = manager.resume("task-2").await.unwrap();
        assert_eq!(restored_steps.len(), 4);
        assert_eq!(status, TaskStatus::Running);
    }

    #[tokio::test]
    async fn test_checkpoint_failure() {
        let store = CheckpointStore::new();
        store.set_unavailable().await;

        let result = store
            .save(Checkpoint {
                task_id: "task-3".to_string(),
                step_number: 1,
                steps: vec![make_step(1)],
                status: TaskStatus::Running,
                saved_at: Utc::now(),
            })
            .await;

        assert!(result.is_err());
        match result {
            Err(AgentError::CheckpointFailure(_)) => {}
            _ => panic!("期望 CheckpointFailure"),
        }
    }
}
