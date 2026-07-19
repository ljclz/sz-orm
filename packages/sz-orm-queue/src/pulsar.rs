use crate::error::MqError;
use crate::queue::{InMemoryQueue, Message, MessageQueue};
use async_trait::async_trait;

pub struct InMemoryPulsarQueue {
    inner: InMemoryQueue,
}

impl InMemoryPulsarQueue {
    pub fn new() -> Self {
        Self {
            inner: InMemoryQueue::new(),
        }
    }

    pub async fn message_count(&self, topic: &str) -> usize {
        self.inner.message_count(topic).await
    }

    pub async fn subscriber_count(&self, topic: &str) -> usize {
        self.inner.subscriber_count(topic).await
    }

    pub async fn in_flight_count(&self) -> usize {
        self.inner.in_flight_count().await
    }
}

impl Default for InMemoryPulsarQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MessageQueue for InMemoryPulsarQueue {
    async fn publish(&self, topic: &str, message: &[u8]) -> Result<(), MqError> {
        self.inner.publish(topic, message).await
    }

    async fn consume(&self, topic: &str) -> Result<Option<Message>, MqError> {
        self.inner.consume(topic).await
    }

    async fn ack(&self, message_id: &str) -> Result<(), MqError> {
        self.inner.ack(message_id).await
    }

    async fn subscribe(&self, topic: &str) -> Result<(), MqError> {
        self.inner.subscribe(topic).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pulsar_publish_and_consume() {
        let queue = InMemoryPulsarQueue::new();
        queue
            .publish("persistent://tenant/ns/topic", b"hello pulsar")
            .await
            .unwrap();
        let msg = queue
            .consume("persistent://tenant/ns/topic")
            .await
            .unwrap()
            .expect("message should exist");
        assert_eq!(msg.payload, b"hello pulsar");
        queue.ack(&msg.id).await.unwrap();
    }

    #[tokio::test]
    async fn test_pulsar_consume_empty() {
        let queue = InMemoryPulsarQueue::new();
        assert!(queue.consume("empty").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_pulsar_subscribe() {
        let queue = InMemoryPulsarQueue::new();
        queue.subscribe("pulsar-topic").await.unwrap();
        assert_eq!(queue.subscriber_count("pulsar-topic").await, 1);
    }
}
