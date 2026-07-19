use crate::error::MqError;
use crate::queue::{InMemoryQueue, Message, MessageQueue};
use async_trait::async_trait;

pub struct InMemoryRabbitmqQueue {
    inner: InMemoryQueue,
}

impl InMemoryRabbitmqQueue {
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

impl Default for InMemoryRabbitmqQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MessageQueue for InMemoryRabbitmqQueue {
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
    async fn test_rabbitmq_publish_and_consume() {
        let queue = InMemoryRabbitmqQueue::new();
        queue.publish("amq-topic", b"hello rabbit").await.unwrap();
        assert_eq!(queue.message_count("amq-topic").await, 1);

        let msg = queue
            .consume("amq-topic")
            .await
            .unwrap()
            .expect("message should exist");
        assert_eq!(msg.payload, b"hello rabbit");
        assert!(!msg.id.is_empty());

        queue.ack(&msg.id).await.unwrap();
        assert_eq!(queue.in_flight_count().await, 0);
    }

    #[tokio::test]
    async fn test_rabbitmq_consume_empty() {
        let queue = InMemoryRabbitmqQueue::new();
        let result = queue.consume("empty").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_rabbitmq_ack_unknown() {
        let queue = InMemoryRabbitmqQueue::new();
        assert!(queue.ack("no-such-id").await.is_err());
    }

    #[tokio::test]
    async fn test_rabbitmq_subscribe_increments() {
        let queue = InMemoryRabbitmqQueue::new();
        queue.subscribe("amq-topic").await.unwrap();
        assert_eq!(queue.subscriber_count("amq-topic").await, 1);
    }
}
