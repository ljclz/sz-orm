//! CDC 同步端到端接线测试
//!
//! 验证 CdcSyncCoordinator → KafkaSink → 检查点 全链路接线。

use std::sync::Arc;

use sz_orm_core::cdc::checkpoint::SharedCheckpointStore;
use sz_orm_core::cdc::dispatcher::CdcSink;
use sz_orm_core::cdc::event::{ChangeEvent, ChangeEventType, ChangePosition};
use sz_orm_core::cdc::sinks::kafka::{BufferedProducer, KafkaSink, KafkaSinkConfig};
use sz_orm_stream::{create_memory_checkpoint, CdcSyncConfig, CdcSyncCoordinator};

fn make_event(id: &str, table: &str) -> ChangeEvent {
    ChangeEvent::new(
        ChangeEventType::Insert,
        "source_db",
        table,
        serde_json::json!({"id": id, "name": format!("user_{}", id)}),
        ChangePosition::SqliteHook { seq: 1 },
        1000,
    )
}

#[tokio::test]
async fn wiring_coordinator_with_kafka_sink() {
    let producer = Arc::new(BufferedProducer::new());
    let sink = Arc::new(KafkaSink::new(
        producer.clone(),
        KafkaSinkConfig::new("cdc_events"),
    ));
    let checkpoint: SharedCheckpointStore = create_memory_checkpoint();
    let coord = CdcSyncCoordinator::new(vec![sink], checkpoint, CdcSyncConfig::default());

    let event = make_event("1", "users");
    coord.process_event(event).await.unwrap();
    assert_eq!(coord.events_processed(), 1);
    assert_eq!(producer.count(), 1);
}

#[tokio::test]
async fn wiring_batch_to_kafka() {
    let producer = Arc::new(BufferedProducer::new());
    let sink = Arc::new(KafkaSink::new(
        producer.clone(),
        KafkaSinkConfig::new("batch_topic"),
    ));
    let checkpoint: SharedCheckpointStore = create_memory_checkpoint();
    let coord = CdcSyncCoordinator::new(vec![sink], checkpoint, CdcSyncConfig::default());

    let events = vec![
        make_event("1", "users"),
        make_event("2", "users"),
        make_event("3", "posts"),
    ];
    coord.process_batch(events).await.unwrap();
    assert_eq!(producer.count(), 3);
}

#[tokio::test]
async fn wiring_checkpoint_persistence() {
    let producer = Arc::new(BufferedProducer::new());
    let sink = Arc::new(KafkaSink::new(producer, KafkaSinkConfig::new("topic")));
    let checkpoint: SharedCheckpointStore = create_memory_checkpoint();
    let coord = CdcSyncCoordinator::new(vec![sink], checkpoint, CdcSyncConfig::default());

    let pos = ChangePosition::SqliteHook { seq: 100 };
    coord.save_checkpoint(&pos).await.unwrap();
    let resumed = coord.resume_from_checkpoint().await.unwrap();
    assert!(resumed.is_some());
}

#[tokio::test]
async fn wiring_masking_through_pipeline() {
    let producer = Arc::new(BufferedProducer::new());
    let config = KafkaSinkConfig::new("masked_topic").with_masking(vec!["password".into()]);
    let sink = Arc::new(KafkaSink::new(producer.clone(), config));
    let checkpoint: SharedCheckpointStore = create_memory_checkpoint();
    let coord = CdcSyncCoordinator::new(vec![sink], checkpoint, CdcSyncConfig::default());

    let event = ChangeEvent::new(
        ChangeEventType::Insert,
        "db",
        "users",
        serde_json::json!({"id": 1, "password": "secret"}),
        ChangePosition::SqliteHook { seq: 1 },
        1000,
    );
    coord.process_event(event).await.unwrap();
    let msgs = producer.messages();
    assert!(msgs[0].2.contains("***"));
    assert!(!msgs[0].2.contains("secret"));
}

#[tokio::test]
async fn wiring_multi_sink_parallel_dispatch() {
    let producer1 = Arc::new(BufferedProducer::new());
    let producer2 = Arc::new(BufferedProducer::new());
    let sink1 = Arc::new(KafkaSink::new(
        producer1.clone(),
        KafkaSinkConfig::new("topic1"),
    ));
    let sink2 = Arc::new(KafkaSink::new(
        producer2.clone(),
        KafkaSinkConfig::new("topic2"),
    ));
    let checkpoint: SharedCheckpointStore = create_memory_checkpoint();
    let coord = CdcSyncCoordinator::new(vec![sink1, sink2], checkpoint, CdcSyncConfig::default());

    coord.process_event(make_event("1", "users")).await.unwrap();
    assert_eq!(producer1.count(), 1);
    assert_eq!(producer2.count(), 1);
    assert_eq!(coord.sink_count(), 2);
}

#[tokio::test]
async fn wiring_at_least_once_with_retry() {
    struct RetrySink {
        count: std::sync::atomic::AtomicU32,
    }
    #[async_trait::async_trait]
    impl CdcSink for RetrySink {
        async fn handle(
            &self,
            _: &ChangeEvent,
        ) -> Result<(), sz_orm_core::cdc::checkpoint::CdcError> {
            let n = self
                .count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n < 1 {
                Err(sz_orm_core::cdc::checkpoint::CdcError::Backpressure)
            } else {
                Ok(())
            }
        }
    }
    let sink = Arc::new(RetrySink {
        count: std::sync::atomic::AtomicU32::new(0),
    });
    let checkpoint: SharedCheckpointStore = create_memory_checkpoint();
    let coord = CdcSyncCoordinator::new(vec![sink], checkpoint, CdcSyncConfig::default());
    coord.process_event(make_event("1", "users")).await.unwrap();
    assert_eq!(coord.events_processed(), 1);
    assert_eq!(coord.events_failed(), 0);
}
