use crate::error::MqError;
use crate::queue::{InMemoryQueue, Message, MessageQueue};
use async_trait::async_trait;

pub struct InMemoryKafkaQueue {
    inner: InMemoryQueue,
}

impl InMemoryKafkaQueue {
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

impl Default for InMemoryKafkaQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MessageQueue for InMemoryKafkaQueue {
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
    async fn test_kafka_publish_and_consume() {
        let queue = InMemoryKafkaQueue::new();
        queue.publish("topic-a", b"hello kafka").await.unwrap();
        assert_eq!(queue.message_count("topic-a").await, 1);

        let msg = queue
            .consume("topic-a")
            .await
            .unwrap()
            .expect("message should exist");
        assert_eq!(msg.payload, b"hello kafka");
        assert_eq!(msg.topic, "topic-a");
        assert!(!msg.id.is_empty());

        queue.ack(&msg.id).await.unwrap();
        assert_eq!(queue.in_flight_count().await, 0);
    }

    #[tokio::test]
    async fn test_kafka_consume_empty() {
        let queue = InMemoryKafkaQueue::new();
        let result = queue.consume("empty-topic").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_kafka_ack_unknown() {
        let queue = InMemoryKafkaQueue::new();
        let result = queue.ack("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_kafka_subscribe_increments() {
        let queue = InMemoryKafkaQueue::new();
        queue.subscribe("topic-a").await.unwrap();
        queue.subscribe("topic-a").await.unwrap();
        queue.subscribe("topic-b").await.unwrap();
        assert_eq!(queue.subscriber_count("topic-a").await, 2);
        assert_eq!(queue.subscriber_count("topic-b").await, 1);
    }

    #[tokio::test]
    async fn test_kafka_multiple_messages_order() {
        let queue = InMemoryKafkaQueue::new();
        queue.publish("topic-a", b"first").await.unwrap();
        queue.publish("topic-a", b"second").await.unwrap();
        queue.publish("topic-a", b"third").await.unwrap();

        let m1 = queue.consume("topic-a").await.unwrap().unwrap();
        let m2 = queue.consume("topic-a").await.unwrap().unwrap();
        let m3 = queue.consume("topic-a").await.unwrap().unwrap();
        assert_eq!(m1.payload, b"first");
        assert_eq!(m2.payload, b"second");
        assert_eq!(m3.payload, b"third");
    }
}
