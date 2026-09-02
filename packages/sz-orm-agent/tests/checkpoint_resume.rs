//! TASK-005 验证测试：检查点恢复

use std::collections::HashMap;
use std::sync::Arc;
use sz_orm_agent::checkpoint::{Checkpoint, CheckpointManager, CheckpointStore};
use sz_orm_agent::types::{AgentError, AgentStep, PerceptionSnapshot, TaskStatus};

fn make_step(n: usize) -> AgentStep {
    AgentStep {
        step_number: n,
        perception: PerceptionSnapshot::default(),
        thought: format!("step {n}"),
        action: "noop".to_string(),
        action_params: HashMap::new(),
        result: "ok".to_string(),
        success: true,
        timestamp: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn test_checkpoint_resume() {
    let store = Arc::new(CheckpointStore::new());
    let manager = CheckpointManager::new(store.clone());

    let steps: Vec<AgentStep> = (1..=5).map(make_step).collect();
    manager
        .save_step("task-resume", &steps, TaskStatus::Running)
        .await
        .unwrap();

    let (restored, status) = manager.resume("task-resume").await.unwrap();
    assert_eq!(restored.len(), 5, "前 5 步结果不丢失");
    assert_eq!(status, TaskStatus::Running);
}

#[tokio::test]
async fn test_checkpoint_failure() {
    let store = CheckpointStore::new();
    store.set_unavailable().await;

    let result = store
        .save(Checkpoint {
            task_id: "task-fail".to_string(),
            step_number: 1,
            steps: vec![make_step(1)],
            status: TaskStatus::Running,
            saved_at: chrono::Utc::now(),
        })
        .await;

    assert!(result.is_err());
    match result {
        Err(AgentError::CheckpointFailure(_)) => {}
        _ => panic!("期望 CheckpointFailure"),
    }
}
