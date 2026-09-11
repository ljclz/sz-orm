//! CDC-MASK-01 接线验证测试（v6.8.0）
//!
//! 验证 CDC 变更事件 → MaskingSink → DataMasker::mask → 下游 sink 端到端管线。

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde_json::json;
use sz_orm_core::cdc::dispatcher::{CdcEventDispatcher, CdcSink};
use sz_orm_core::cdc::event::{ChangeEvent, ChangeEventType, ChangePosition};
use sz_orm_core::cdc::sinks::masking::{MaskingSink, TableFilteredMaskingSink};
use sz_orm_masking::MaskingRule;

struct CapturingSink {
    events: RwLock<Vec<ChangeEvent>>,
}

impl CapturingSink {
    fn new() -> Self {
        Self {
            events: RwLock::new(Vec::new()),
        }
    }
    fn last(&self) -> Option<ChangeEvent> {
        self.events.read().last().cloned()
    }
    fn count(&self) -> usize {
        self.events.read().len()
    }
}

#[async_trait::async_trait]
impl CdcSink for CapturingSink {
    async fn handle(
        &self,
        event: &ChangeEvent,
    ) -> Result<(), sz_orm_core::cdc::checkpoint::CdcError> {
        self.events.write().push(event.clone());
        Ok(())
    }
}

fn make_event(table: &str, phone: &str, id: &str, position: u64) -> ChangeEvent {
    ChangeEvent::new(
        ChangeEventType::Update,
        "test_db",
        table,
        json!({ "id": id, "phone": phone, "name": "张三" }),
        ChangePosition::MysqlBinlog {
            filename: "bin.000001".to_string(),
            position,
        },
        1000 + position,
    )
}

#[tokio::test]
async fn wiring_phone_masked_in_downstream() {
    let capture = Arc::new(CapturingSink::new());
    let mut rules = HashMap::new();
    rules.insert("phone".to_string(), MaskingRule::Phone);
    let sink = Arc::new(MaskingSink::new(rules, vec![capture.clone()]));
    let dispatcher = CdcEventDispatcher::new(vec![sink], 100);

    dispatcher
        .dispatch(make_event("users", "13812345678", "1", 100))
        .await
        .unwrap();

    let last = capture.last().unwrap();
    assert!(last.masked);
    assert_ne!(last.row_data["phone"], json!("13812345678"));
    assert_eq!(last.row_data["name"], json!("张三"));
}

#[tokio::test]
async fn wiring_email_masked_in_downstream() {
    let capture = Arc::new(CapturingSink::new());
    let mut rules = HashMap::new();
    rules.insert("email".to_string(), MaskingRule::Email);
    let sink = Arc::new(MaskingSink::new(rules, vec![capture.clone()]));
    let dispatcher = CdcEventDispatcher::new(vec![sink], 100);

    let event = ChangeEvent::new(
        ChangeEventType::Insert,
        "test_db",
        "users",
        json!({ "id": "1", "email": "user@example.com" }),
        ChangePosition::MysqlBinlog {
            filename: "bin.000001".to_string(),
            position: 100,
        },
        1000,
    );
    dispatcher.dispatch(event).await.unwrap();

    let last = capture.last().unwrap();
    assert!(last.masked);
    assert_ne!(last.row_data["email"], json!("user@example.com"));
}

#[tokio::test]
async fn wiring_multiple_fields_masked() {
    let capture = Arc::new(CapturingSink::new());
    let mut rules = HashMap::new();
    rules.insert("phone".to_string(), MaskingRule::Phone);
    rules.insert("name".to_string(), MaskingRule::Name);
    let sink = Arc::new(MaskingSink::new(rules, vec![capture.clone()]));
    let dispatcher = CdcEventDispatcher::new(vec![sink], 100);

    dispatcher
        .dispatch(make_event("users", "13812345678", "1", 100))
        .await
        .unwrap();

    let last = capture.last().unwrap();
    assert!(last.masked);
    assert_ne!(last.row_data["phone"], json!("13812345678"));
    assert_ne!(last.row_data["name"], json!("张三"));
}

#[tokio::test]
async fn wiring_table_filter_masks_only_watched() {
    let capture = Arc::new(CapturingSink::new());
    let mut rules = HashMap::new();
    rules.insert("phone".to_string(), MaskingRule::Phone);
    let sink = Arc::new(TableFilteredMaskingSink::new(
        rules,
        vec![capture.clone()],
        vec!["users".to_string()],
    ));
    let dispatcher = CdcEventDispatcher::new(vec![sink], 100);

    dispatcher
        .dispatch(make_event("users", "13812345678", "1", 100))
        .await
        .unwrap();
    let masked_event = capture.last().unwrap();
    assert!(masked_event.masked);
    assert_ne!(masked_event.row_data["phone"], json!("13812345678"));

    dispatcher
        .dispatch(make_event("logs", "13812345678", "2", 200))
        .await
        .unwrap();
    let unmasked_event = capture.last().unwrap();
    assert!(!unmasked_event.masked);
    assert_eq!(unmasked_event.row_data["phone"], json!("13812345678"));
}

#[tokio::test]
async fn wiring_password_masked_to_stars() {
    let capture = Arc::new(CapturingSink::new());
    let mut rules = HashMap::new();
    rules.insert("password".to_string(), MaskingRule::Password);
    let sink = Arc::new(MaskingSink::new(rules, vec![capture.clone()]));
    let dispatcher = CdcEventDispatcher::new(vec![sink], 100);

    let event = ChangeEvent::new(
        ChangeEventType::Insert,
        "test_db",
        "users",
        json!({ "id": "1", "password": "s3cr3t_p@ss" }),
        ChangePosition::MysqlBinlog {
            filename: "bin.000001".to_string(),
            position: 100,
        },
        1000,
    );
    dispatcher.dispatch(event).await.unwrap();

    let last = capture.last().unwrap();
    assert_eq!(last.row_data["password"], json!("***"));
    assert!(last.masked);
}

#[tokio::test]
async fn wiring_batch_events_all_masked() {
    let capture = Arc::new(CapturingSink::new());
    let mut rules = HashMap::new();
    rules.insert("phone".to_string(), MaskingRule::Phone);
    let sink = Arc::new(MaskingSink::new(rules, vec![capture.clone()]));
    let dispatcher = CdcEventDispatcher::new(vec![sink], 100);

    let events: Vec<_> = (1..=5)
        .map(|i| make_event("users", &format!("138{}", i), &i.to_string(), 100 + i))
        .collect();
    dispatcher.dispatch_batch(events).await.unwrap();

    assert_eq!(capture.count(), 5);
    let last = capture.last().unwrap();
    assert!(last.masked);
}
