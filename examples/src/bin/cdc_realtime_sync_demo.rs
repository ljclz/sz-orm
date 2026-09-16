//! CDC 实时数据同步 demo
//!
//! 展示 CdcSyncCoordinator → KafkaSink → 检查点续传 全链路。

use std::sync::Arc;

use sz_orm_core::cdc::event::{ChangeEvent, ChangeEventType, ChangePosition};
use sz_orm_core::cdc::sinks::kafka::{BufferedProducer, KafkaSink, KafkaSinkConfig};
use sz_orm_stream::{create_memory_checkpoint, CdcSyncConfig, CdcSyncCoordinator};

#[tokio::main]
async fn main() {
    println!("=== sz-orm CDC 实时数据同步 demo ===\n");

    let producer = Arc::new(BufferedProducer::new());
    let config = KafkaSinkConfig::new("cdc_events").with_masking(vec!["password".into()]);
    let sink = Arc::new(KafkaSink::new(producer.clone(), config));
    let checkpoint = create_memory_checkpoint();
    let coord = CdcSyncCoordinator::new(vec![sink], checkpoint, CdcSyncConfig::default());

    let events = vec![
        ChangeEvent::new(
            ChangeEventType::Insert,
            "shop",
            "users",
            serde_json::json!({"id": 1, "name": "Alice", "password": "secret"}),
            ChangePosition::SqliteHook { seq: 1 },
            1000,
        ),
        ChangeEvent::new(
            ChangeEventType::Update,
            "shop",
            "users",
            serde_json::json!({"id": 2, "name": "Bob", "password": "pass"}),
            ChangePosition::SqliteHook { seq: 2 },
            2000,
        ),
        ChangeEvent::new(
            ChangeEventType::Insert,
            "shop",
            "orders",
            serde_json::json!({"id": 100, "user_id": 1, "amount": 99.9}),
            ChangePosition::SqliteHook { seq: 3 },
            3000,
        ),
    ];

    let count = coord.process_batch(events).await.unwrap();
    println!("处理事件数: {}", count);
    println!(
        "成功: {}, 失败: {}",
        coord.events_processed(),
        coord.events_failed()
    );

    let pos = ChangePosition::SqliteHook { seq: 3 };
    coord.save_checkpoint(&pos).await.unwrap();
    let resumed = coord.resume_from_checkpoint().await.unwrap();
    println!("检查点恢复: {:?}", resumed);

    println!("\nKafka 消息数: {}", producer.count());
    for (i, (topic, key, _)) in producer.messages().iter().enumerate() {
        println!("  [{}] topic={}, key={}", i, topic, key);
    }

    println!("\n=== demo 完成 ===");
}
