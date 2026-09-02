//! TASK-017 验证测试：工作流编排

use std::collections::HashMap;
use std::time::Instant;
use sz_orm_agent::workflow::{WorkflowOrchestrator, WorkflowTask};

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

    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| r.success));
    assert!(
        start.elapsed() < tokio::time::Duration::from_millis(250),
        "并行 3 个 100ms 任务应 < 250ms"
    );
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
    assert!(WorkflowOrchestrator::new(tasks).is_err());
}
