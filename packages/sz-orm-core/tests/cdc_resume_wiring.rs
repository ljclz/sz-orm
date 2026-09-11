//! CDC 断点续传接线验证（v6.8.0 W2-7 CDC-RESUME-01 + W2-11 CDC-RESUME-SEC）

#![cfg(feature = "cdc-mysql")]

use std::sync::Arc;

use sz_orm_core::cdc::checkpoint::{CdcCheckpointStore, SharedCheckpointStore};
use sz_orm_core::cdc::dispatcher::{CdcEventDispatcher, MemorySink};
use sz_orm_core::cdc::event::{ChangeEvent, ChangeEventType, ChangePosition};

fn make_event(pos: u64) -> ChangeEvent {
    ChangeEvent::new(
        ChangeEventType::Insert,
        "test_db",
        "users",
        serde_json::json!({"id": pos}),
        ChangePosition::MysqlBinlog {
            filename: "bin.000001".to_string(),
            position: pos,
        },
        1000 + pos,
    )
}

#[tokio::test]
async fn resume_from_checkpoint_after_restart() {
    let store = Arc::new(CdcCheckpointStore::in_memory());

    for i in 1..=100 {
        let pos = ChangePosition::MysqlBinlog {
            filename: "bin.000001".to_string(),
            position: i,
        };
        store.save_checkpoint(&pos).await.unwrap();
    }

    let resume_pos = store.resume_from().await.unwrap().unwrap();
    assert_eq!(
        resume_pos,
        ChangePosition::MysqlBinlog {
            filename: "bin.000001".to_string(),
            position: 100,
        }
    );
}

#[tokio::test]
async fn resume_from_empty_starts_from_beginning() {
    let store = CdcCheckpointStore::in_memory();
    let resume = store.resume_from().await.unwrap();
    assert!(resume.is_none());
}

#[tokio::test]
async fn checkpoint_retry_on_failure() {
    let store = CdcCheckpointStore::in_memory();
    let pos = ChangePosition::MysqlBinlog {
        filename: "bin.001".to_string(),
        position: 42,
    };

    store.save_checkpoint_with_retry(&pos, 3).await.unwrap();
    let loaded = store.load_checkpoint().await.unwrap();
    assert_eq!(loaded, Some(pos));
}

#[tokio::test]
async fn file_checkpoint_resume_after_save() {
    let path = std::path::PathBuf::from("test_cdc_resume_checkpoint.json");
    let store = CdcCheckpointStore::file(path.clone());

    for i in 1..=50 {
        let pos = ChangePosition::MysqlBinlog {
            filename: "bin.001".to_string(),
            position: i * 10,
        };
        store.save_checkpoint(&pos).await.unwrap();
    }

    let resume = store.resume_from().await.unwrap().unwrap();
    assert_eq!(
        resume,
        ChangePosition::MysqlBinlog {
            filename: "bin.001".to_string(),
            position: 500,
        }
    );

    store.clear().await.unwrap();
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn dispatch_and_confirm_advances_checkpoint() {
    let sink = Arc::new(MemorySink::new());
    let dispatcher = CdcEventDispatcher::new(vec![sink.clone()], 100);
    let checkpoint: SharedCheckpointStore = Arc::new(CdcCheckpointStore::in_memory());

    let event = make_event(1);
    let confirmed = dispatcher
        .dispatch_and_confirm(event.clone(), &checkpoint)
        .await
        .unwrap();
    assert!(confirmed);
    assert_eq!(sink.count(), 1);

    let saved = checkpoint.load_checkpoint().await.unwrap().unwrap();
    assert_eq!(saved, event.position);
}

#[tokio::test]
async fn dispatch_and_confirm_no_skip_on_sink_failure() {
    use async_trait::async_trait;
    use sz_orm_core::cdc::checkpoint::CdcError;
    use sz_orm_core::cdc::dispatcher::CdcSink;

    struct FailingSink;
    #[async_trait]
    impl CdcSink for FailingSink {
        async fn handle(&self, _event: &ChangeEvent) -> Result<(), CdcError> {
            Err(CdcError::SinkError("intentional failure".into()))
        }
    }

    let dispatcher = CdcEventDispatcher::new(vec![Arc::new(FailingSink)], 100);
    let checkpoint: SharedCheckpointStore = Arc::new(CdcCheckpointStore::in_memory());

    let event = make_event(1);
    let result = dispatcher.dispatch_and_confirm(event, &checkpoint).await;

    assert!(result.is_err());
    let saved = checkpoint.load_checkpoint().await.unwrap();
    assert!(
        saved.is_none(),
        "checkpoint must NOT advance on sink failure"
    );
}

#[tokio::test]
async fn resume_does_not_repeat_confirmed_events() {
    let store = Arc::new(CdcCheckpointStore::in_memory());
    let sink = Arc::new(MemorySink::new());
    let dispatcher = CdcEventDispatcher::new(vec![sink.clone()], 100);

    for i in 1..=100 {
        let event = make_event(i);
        dispatcher
            .dispatch_and_confirm(event, &store)
            .await
            .unwrap();
    }

    let resume_pos = store.resume_from().await.unwrap().unwrap();
    let last_pos = match resume_pos {
        ChangePosition::MysqlBinlog { position, .. } => position,
        _ => 0,
    };
    assert_eq!(last_pos, 100);
    assert_eq!(sink.count(), 100);
}

#[tokio::test]
async fn is_confirmed_checks_exact_position() {
    let store = CdcCheckpointStore::in_memory();
    let pos1 = ChangePosition::MysqlBinlog {
        filename: "bin.001".to_string(),
        position: 100,
    };
    let pos2 = ChangePosition::MysqlBinlog {
        filename: "bin.001".to_string(),
        position: 200,
    };

    store.save_checkpoint(&pos1).await.unwrap();
    assert!(store.is_confirmed(&pos1).await.unwrap());
    assert!(!store.is_confirmed(&pos2).await.unwrap());
}

#[tokio::test]
async fn batch_dispatch_with_checkpoint_progress() {
    let store = Arc::new(CdcCheckpointStore::in_memory());
    let sink = Arc::new(MemorySink::new());
    let dispatcher = CdcEventDispatcher::new(vec![sink.clone()], 100);

    let events: Vec<_> = (1..=50).map(make_event).collect();
    for event in events {
        dispatcher
            .dispatch_and_confirm(event, &store)
            .await
            .unwrap();
    }

    let resume = store.resume_from().await.unwrap().unwrap();
    let pos = match resume {
        ChangePosition::MysqlBinlog { position, .. } => position,
        _ => 0,
    };
    assert_eq!(pos, 50);
    assert_eq!(sink.count(), 50);
}

#[tokio::test]
async fn partial_dispatch_no_checkpoint_advance() {
    use async_trait::async_trait;
    use sz_orm_core::cdc::checkpoint::CdcError;
    use sz_orm_core::cdc::dispatcher::CdcSink;

    struct ConditionalSink {
        fail_after: u64,
        count: std::sync::atomic::AtomicU64,
    }

    #[async_trait]
    impl CdcSink for ConditionalSink {
        async fn handle(&self, _event: &ChangeEvent) -> Result<(), CdcError> {
            let n = self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n >= self.fail_after {
                Err(CdcError::SinkError("fail after threshold".into()))
            } else {
                Ok(())
            }
        }
    }

    let conditional = Arc::new(ConditionalSink {
        fail_after: 50,
        count: std::sync::atomic::AtomicU64::new(0),
    });
    let dispatcher = CdcEventDispatcher::new(vec![conditional], 100);
    let checkpoint: SharedCheckpointStore = Arc::new(CdcCheckpointStore::in_memory());

    let mut confirmed_count = 0;
    for i in 1..=100 {
        let event = make_event(i);
        if dispatcher
            .dispatch_and_confirm(event, &checkpoint)
            .await
            .is_ok()
        {
            confirmed_count += 1;
        }
    }

    assert_eq!(confirmed_count, 50);
    let resume = checkpoint.resume_from().await.unwrap().unwrap();
    let pos = match resume {
        ChangePosition::MysqlBinlog { position, .. } => position,
        _ => 0,
    };
    assert_eq!(pos, 50, "checkpoint at last confirmed position, not 100");
}

#[tokio::test]
async fn retry_exponential_backoff_completes() {
    let store = CdcCheckpointStore::in_memory();
    let pos = ChangePosition::MysqlBinlog {
        filename: "bin.001".to_string(),
        position: 999,
    };

    let start = std::time::Instant::now();
    store.save_checkpoint_with_retry(&pos, 5).await.unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 1000,
        "retry should complete quickly on success"
    );
    assert!(store.is_confirmed(&pos).await.unwrap());
}
