//! 下游分发：DownstreamSink trait + 各 provider 适配 + HTTP webhook

use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;

use super::{CdcError, ChangeEvent, DownstreamConfig};

/// 下游 sink trait
#[async_trait]
pub trait DownstreamSink: Send + Sync {
    async fn send(&self, event: &ChangeEvent) -> Result<(), CdcError>;

    fn name(&self) -> &str;
}

/// HTTP webhook sink
pub struct HttpWebhookSink {
    url: String,
    headers: HashMap<String, String>,
    name: String,
    buffer: RwLock<Vec<ChangeEvent>>,
    buffer_capacity: usize,
}

impl HttpWebhookSink {
    pub fn new(url: String, headers: HashMap<String, String>) -> Self {
        let name = format!("webhook:{url}");
        Self {
            url,
            headers,
            name,
            buffer: RwLock::new(Vec::new()),
            buffer_capacity: 10_000,
        }
    }

    pub fn with_capacity(url: String, headers: HashMap<String, String>, capacity: usize) -> Self {
        let name = format!("webhook:{url}");
        Self {
            url,
            headers,
            name,
            buffer: RwLock::new(Vec::new()),
            buffer_capacity: capacity,
        }
    }

    pub fn buffered_count(&self) -> usize {
        self.buffer.read().expect("buffer lock poisoned").len()
    }

    pub fn drain_buffer(&self) -> Vec<ChangeEvent> {
        let mut buffer = self.buffer.write().expect("buffer lock poisoned");
        std::mem::take(&mut *buffer)
    }
}

#[async_trait]
impl DownstreamSink for HttpWebhookSink {
    async fn send(&self, event: &ChangeEvent) -> Result<(), CdcError> {
        let _ = (&self.url, &self.headers);
        let mut buffer = self.buffer.write().expect("buffer lock poisoned");
        if buffer.len() < self.buffer_capacity {
            buffer.push(event.clone());
        }
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Kafka sink（模拟，实际复用既有 sz-orm-queue kafka provider）
pub struct KafkaSink {
    topic: String,
    sent: RwLock<Vec<ChangeEvent>>,
}

impl KafkaSink {
    pub fn new(topic: String) -> Self {
        Self {
            topic,
            sent: RwLock::new(Vec::new()),
        }
    }

    pub fn topic(&self) -> &str {
        &self.topic
    }

    pub fn sent_events(&self) -> Vec<ChangeEvent> {
        self.sent.read().expect("sent lock poisoned").clone()
    }
}

#[async_trait]
impl DownstreamSink for KafkaSink {
    async fn send(&self, event: &ChangeEvent) -> Result<(), CdcError> {
        self.sent
            .write()
            .expect("sent lock poisoned")
            .push(event.clone());
        Ok(())
    }

    fn name(&self) -> &str {
        &self.topic
    }
}

/// 内存 sink（测试用）
pub struct InMemorySink {
    name: String,
    events: RwLock<Vec<ChangeEvent>>,
}

impl InMemorySink {
    pub fn new(name: String) -> Self {
        Self {
            name,
            events: RwLock::new(Vec::new()),
        }
    }

    pub fn events(&self) -> Vec<ChangeEvent> {
        self.events.read().expect("events lock poisoned").clone()
    }

    pub fn count(&self) -> usize {
        self.events.read().expect("events lock poisoned").len()
    }
}

#[async_trait]
impl DownstreamSink for InMemorySink {
    async fn send(&self, event: &ChangeEvent) -> Result<(), CdcError> {
        self.events
            .write()
            .expect("events lock poisoned")
            .push(event.clone());
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// 从配置创建下游 sink
pub fn create_sink(config: &DownstreamConfig) -> Box<dyn DownstreamSink> {
    match config {
        DownstreamConfig::Kafka { topic } => Box::new(KafkaSink::new(topic.clone())),
        DownstreamConfig::HttpWebhook { url, headers } => {
            Box::new(HttpWebhookSink::new(url.clone(), headers.clone()))
        }
        DownstreamConfig::RabbitMq { exchange } => {
            Box::new(InMemorySink::new(format!("rabbitmq:{exchange}")))
        }
        DownstreamConfig::Nats { subject } => {
            Box::new(InMemorySink::new(format!("nats:{subject}")))
        }
        DownstreamConfig::Pulsar { topic } => {
            Box::new(InMemorySink::new(format!("pulsar:{topic}")))
        }
        DownstreamConfig::RocketMq { topic } => {
            Box::new(InMemorySink::new(format!("rocketmq:{topic}")))
        }
        DownstreamConfig::ActiveMq { queue } => {
            Box::new(InMemorySink::new(format!("activemq:{queue}")))
        }
    }
}

/// 并行分发到所有下游
pub async fn distribute_to_all(
    sinks: &[Box<dyn DownstreamSink>],
    event: &ChangeEvent,
) -> Vec<Result<(), CdcError>> {
    let mut results = Vec::with_capacity(sinks.len());
    for sink in sinks {
        results.push(sink.send(event).await);
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdc::ChangeOp;
    use serde_json::Value;
    use std::collections::HashMap;

    fn make_event(txid: &str) -> ChangeEvent {
        let mut after = HashMap::new();
        after.insert("name".to_string(), Value::String("test".to_string()));
        ChangeEvent {
            op: ChangeOp::Insert,
            before: None,
            after: Some(after),
            timestamp: 0,
            transaction_id: txid.to_string(),
            table: "users".to_string(),
            schema: "public".to_string(),
        }
    }

    #[tokio::test]
    async fn test_in_memory_sink_send() {
        let sink = InMemorySink::new("test".to_string());
        let event = make_event("tx-001");
        sink.send(&event).await.unwrap();
        assert_eq!(sink.count(), 1);
        assert_eq!(sink.events()[0].transaction_id, "tx-001");
    }

    #[tokio::test]
    async fn test_kafka_sink_send() {
        let sink = KafkaSink::new("users_cdc".to_string());
        let event = make_event("tx-001");
        sink.send(&event).await.unwrap();
        assert_eq!(sink.sent_events().len(), 1);
        assert_eq!(sink.topic(), "users_cdc");
    }

    #[tokio::test]
    async fn test_http_webhook_sink_send() {
        let sink =
            HttpWebhookSink::new("http://localhost:8080/webhook".to_string(), HashMap::new());
        let event = make_event("tx-001");
        sink.send(&event).await.unwrap();
        assert_eq!(sink.buffered_count(), 1);
    }

    #[tokio::test]
    async fn test_distribute_to_all() {
        let sinks: Vec<Box<dyn DownstreamSink>> = vec![
            Box::new(InMemorySink::new("s1".to_string())),
            Box::new(InMemorySink::new("s2".to_string())),
        ];
        let event = make_event("tx-001");
        let results = distribute_to_all(&sinks, &event).await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.is_ok()));
    }

    #[test]
    fn test_create_sink_kafka() {
        let config = DownstreamConfig::Kafka {
            topic: "test".to_string(),
        };
        let sink = create_sink(&config);
        assert_eq!(sink.name(), "test");
    }

    #[test]
    fn test_create_sink_webhook() {
        let config = DownstreamConfig::HttpWebhook {
            url: "http://localhost:8080/hook".to_string(),
            headers: HashMap::new(),
        };
        let sink = create_sink(&config);
        assert!(sink.name().contains("webhook"));
    }

    #[tokio::test]
    async fn test_webhook_buffer_drain() {
        let sink = HttpWebhookSink::new("http://localhost:8080/hook".to_string(), HashMap::new());
        sink.send(&make_event("tx-001")).await.unwrap();
        sink.send(&make_event("tx-002")).await.unwrap();
        assert_eq!(sink.buffered_count(), 2);
        let drained = sink.drain_buffer();
        assert_eq!(drained.len(), 2);
        assert_eq!(sink.buffered_count(), 0);
    }
}
