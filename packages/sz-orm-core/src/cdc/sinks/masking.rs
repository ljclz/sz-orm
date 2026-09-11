//! CDC 脱敏 Sink（v6.8.0 CDC-MASK-01）
//!
//! 接收变更事件，对事件载荷中匹配规则的字段进行脱敏，
//! 设置 `masked=true`，然后转发到下游 sink。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use super::super::checkpoint::CdcError;
use super::super::dispatcher::CdcSink;
use super::super::event::ChangeEvent;
use sz_orm_masking::{DataMasker, MaskingRule};

/// CDC 脱敏 Sink
///
/// 对事件载荷中匹配规则的字段执行脱敏，设置 `masked=true`，转发到下游 sink。
pub struct MaskingSink {
    rules: HashMap<String, MaskingRule>,
    downstream: Vec<Arc<dyn CdcSink>>,
    masked_count: std::sync::atomic::AtomicU64,
}

impl MaskingSink {
    /// 创建脱敏 Sink
    pub fn new(rules: HashMap<String, MaskingRule>, downstream: Vec<Arc<dyn CdcSink>>) -> Self {
        Self {
            rules,
            downstream,
            masked_count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// 返回已脱敏的事件总数
    pub fn masked_total(&self) -> u64 {
        self.masked_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 对事件载荷执行脱敏，返回脱敏后的事件
    pub fn mask_event(&self, event: &ChangeEvent) -> ChangeEvent {
        let mut masked_event = event.clone();
        if let serde_json::Value::Object(ref mut map) = masked_event.row_data {
            let mut masked_fields = 0u64;
            for (field, rule) in &self.rules {
                if let Some(val) = map.get(field) {
                    if let Some(s) = val.as_str() {
                        let masked = DataMasker::apply(rule, s);
                        map.insert(field.clone(), serde_json::Value::String(masked));
                        masked_fields += 1;
                    }
                }
            }
            self.masked_count
                .fetch_add(masked_fields, std::sync::atomic::Ordering::Relaxed);
        }
        masked_event.masked = true;
        masked_event
    }
}

#[async_trait]
impl CdcSink for MaskingSink {
    async fn handle(&self, event: &ChangeEvent) -> Result<(), CdcError> {
        let masked_event = self.mask_event(event);
        for sink in &self.downstream {
            sink.handle(&masked_event).await?;
        }
        Ok(())
    }
}

/// 仅对指定表执行脱敏的过滤型 Sink
pub struct TableFilteredMaskingSink {
    inner: MaskingSink,
    watched_tables: std::collections::HashSet<String>,
}

impl TableFilteredMaskingSink {
    /// 创建表过滤脱敏 Sink
    pub fn new(
        rules: HashMap<String, MaskingRule>,
        downstream: Vec<Arc<dyn CdcSink>>,
        watched_tables: Vec<String>,
    ) -> Self {
        Self {
            inner: MaskingSink::new(rules, downstream),
            watched_tables: watched_tables.into_iter().collect(),
        }
    }

    /// 返回已脱敏的事件总数
    pub fn masked_total(&self) -> u64 {
        self.inner.masked_total()
    }
}

#[async_trait]
impl CdcSink for TableFilteredMaskingSink {
    async fn handle(&self, event: &ChangeEvent) -> Result<(), CdcError> {
        if !self.watched_tables.contains(&event.source_table) {
            for sink in &self.inner.downstream {
                sink.handle(event).await?;
            }
            return Ok(());
        }
        self.inner.handle(event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdc::event::{ChangeEvent, ChangeEventType, ChangePosition};
    use crate::cdc::sinks::memory::CountingSink;
    use parking_lot::RwLock;
    use serde_json::json;

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
    }

    #[async_trait]
    impl CdcSink for CapturingSink {
        async fn handle(&self, event: &ChangeEvent) -> Result<(), CdcError> {
            self.events.write().push(event.clone());
            Ok(())
        }
    }

    fn make_event_with_phone(phone: &str) -> ChangeEvent {
        ChangeEvent::new(
            ChangeEventType::Update,
            "test_db",
            "users",
            json!({ "id": "1", "phone": phone, "name": "张三" }),
            ChangePosition::MysqlBinlog {
                filename: "bin.000001".to_string(),
                position: 100,
            },
            1000,
        )
    }

    #[tokio::test]
    async fn masking_sink_masks_phone_field() {
        let capture = Arc::new(CapturingSink::new());
        let mut rules = HashMap::new();
        rules.insert("phone".to_string(), MaskingRule::Phone);
        let sink = MaskingSink::new(rules, vec![capture.clone()]);

        let event = make_event_with_phone("13812345678");
        sink.handle(&event).await.unwrap();

        let last = capture.last().unwrap();
        assert!(last.masked);
        assert_ne!(last.row_data["phone"], json!("13812345678"));
    }

    #[tokio::test]
    async fn masking_sink_sets_masked_flag() {
        let capture = Arc::new(CapturingSink::new());
        let mut rules = HashMap::new();
        rules.insert("phone".to_string(), MaskingRule::Phone);
        let sink = MaskingSink::new(rules, vec![capture.clone()]);

        let event = make_event_with_phone("13812345678");
        let masked = sink.mask_event(&event);
        assert!(masked.masked);
    }

    #[tokio::test]
    async fn masking_sink_preserves_non_sensitive_fields() {
        let capture = Arc::new(CapturingSink::new());
        let mut rules = HashMap::new();
        rules.insert("phone".to_string(), MaskingRule::Phone);
        let sink = MaskingSink::new(rules, vec![capture.clone()]);

        let event = make_event_with_phone("13812345678");
        let masked = sink.mask_event(&event);
        assert_eq!(masked.row_data["id"], json!("1"));
        assert_eq!(masked.row_data["name"], json!("张三"));
    }

    #[tokio::test]
    async fn masking_sink_no_rules_passes_through() {
        let counter = Arc::new(CountingSink::new());
        let sink = MaskingSink::new(HashMap::new(), vec![counter.clone()]);

        let event = make_event_with_phone("13812345678");
        let masked = sink.mask_event(&event);
        assert!(masked.masked);
        assert_eq!(masked.row_data["phone"], json!("13812345678"));
    }

    #[tokio::test]
    async fn table_filtered_masking_only_masks_watched_tables() {
        let capture = Arc::new(CapturingSink::new());
        let mut rules = HashMap::new();
        rules.insert("phone".to_string(), MaskingRule::Phone);
        let sink =
            TableFilteredMaskingSink::new(rules, vec![capture.clone()], vec!["users".to_string()]);

        let users_event = make_event_with_phone("13812345678");
        sink.handle(&users_event).await.unwrap();
        let last = capture.last().unwrap();
        assert!(last.masked);
        assert_ne!(last.row_data["phone"], json!("13812345678"));

        let orders_event = ChangeEvent::new(
            ChangeEventType::Update,
            "test_db",
            "orders",
            json!({ "id": "1", "phone": "13812345678" }),
            ChangePosition::MysqlBinlog {
                filename: "bin.000001".to_string(),
                position: 200,
            },
            2000,
        );
        sink.handle(&orders_event).await.unwrap();
        let last = capture.last().unwrap();
        assert!(!last.masked);
        assert_eq!(last.row_data["phone"], json!("13812345678"));
    }

    #[tokio::test]
    async fn masking_sink_multiple_rules_apply_independently() {
        let capture = Arc::new(CapturingSink::new());
        let mut rules = HashMap::new();
        rules.insert("phone".to_string(), MaskingRule::Phone);
        rules.insert("name".to_string(), MaskingRule::Name);
        let sink = MaskingSink::new(rules, vec![capture.clone()]);

        let event = make_event_with_phone("13812345678");
        let masked = sink.mask_event(&event);
        assert!(masked.masked);
        assert_ne!(masked.row_data["phone"], json!("13812345678"));
        assert_ne!(masked.row_data["name"], json!("张三"));
    }

    #[tokio::test]
    async fn masking_sink_password_rule_replaces_with_stars() {
        let capture = Arc::new(CapturingSink::new());
        let mut rules = HashMap::new();
        rules.insert("password".to_string(), MaskingRule::Password);
        let sink = MaskingSink::new(rules, vec![capture.clone()]);

        let event = ChangeEvent::new(
            ChangeEventType::Insert,
            "test_db",
            "users",
            json!({ "id": "1", "password": "secret123" }),
            ChangePosition::MysqlBinlog {
                filename: "bin.000001".to_string(),
                position: 100,
            },
            1000,
        );
        let masked = sink.mask_event(&event);
        assert_eq!(masked.row_data["password"], json!("***"));
    }

    #[tokio::test]
    async fn masking_sink_non_string_field_skipped() {
        let capture = Arc::new(CapturingSink::new());
        let mut rules = HashMap::new();
        rules.insert("id".to_string(), MaskingRule::Phone);
        let sink = MaskingSink::new(rules, vec![capture.clone()]);

        let event = ChangeEvent::new(
            ChangeEventType::Update,
            "test_db",
            "users",
            json!({ "id": 12345, "phone": "13812345678" }),
            ChangePosition::MysqlBinlog {
                filename: "bin.000001".to_string(),
                position: 100,
            },
            1000,
        );
        let masked = sink.mask_event(&event);
        assert_eq!(masked.row_data["id"], json!(12345));
    }
}
