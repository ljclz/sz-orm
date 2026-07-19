use crate::error::MqError;
use crate::queue::{InMemoryQueue, Message, MessageQueue};
use async_trait::async_trait;

pub struct InMemoryRocketmqQueue {
    inner: InMemoryQueue,
}

impl InMemoryRocketmqQueue {
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

impl Default for InMemoryRocketmqQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MessageQueue for InMemoryRocketmqQueue {
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
    async fn test_rocketmq_publish_and_consume() {
        let queue = InMemoryRocketmqQueue::new();
        queue
            .publish("rocket-topic", b"hello rocket")
            .await
            .unwrap();
        let msg = queue
            .consume("rocket-topic")
            .await
            .unwrap()
            .expect("message should exist");
        assert_eq!(msg.payload, b"hello rocket");
        queue.ack(&msg.id).await.unwrap();
    }

    #[tokio::test]
    async fn test_rocketmq_consume_empty() {
        let queue = InMemoryRocketmqQueue::new();
        assert!(queue.consume("empty").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_rocketmq_subscribe() {
        let queue = InMemoryRocketmqQueue::new();
        queue.subscribe("rocket-topic").await.unwrap();
        assert_eq!(queue.subscriber_count("rocket-topic").await, 1);
    }
}
