use crate::error::MqError;
use crate::queue::{Message, MessageQueue};
use async_trait::async_trait;
use futures::StreamExt;
use lapin::{
    options::*, types::FieldTable, BasicProperties, Channel, Connection, ConnectionProperties,
    Consumer,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{timeout, Duration};

pub struct LapinRabbitmqQueue {
    broker_url: String,
    connection: Option<Arc<Connection>>,
    channel: Option<Channel>,
    consumers: Arc<RwLock<HashMap<String, Arc<Mutex<Consumer>>>>>,
    in_flight: Arc<RwLock<HashMap<String, u64>>>,
}

impl LapinRabbitmqQueue {
    pub fn new(broker_url: impl Into<String>) -> Self {
        Self {
            broker_url: broker_url.into(),
            connection: None,
            channel: None,
            consumers: Arc::new(RwLock::new(HashMap::new())),
            in_flight: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn connect(&mut self) -> Result<(), MqError> {
        let conn = Connection::connect(&self.broker_url, ConnectionProperties::default())
            .await
            .map_err(|e| MqError::Connection(e.to_string()))?;
        let channel = conn
            .create_channel()
            .await
            .map_err(|e| MqError::Connection(e.to_string()))?;
        self.connection = Some(Arc::new(conn));
        self.channel = Some(channel);
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }
}

fn current_timestamp_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[async_trait]
impl MessageQueue for LapinRabbitmqQueue {
    async fn publish(&self, topic: &str, message: &[u8]) -> Result<(), MqError> {
        let channel = self
            .channel
            .as_ref()
            .ok_or_else(|| MqError::Connection("not connected".to_string()))?;
        channel
            .queue_declare(
                topic.into(),
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| MqError::Publish(e.to_string()))?;
        channel
            .basic_publish(
                "".into(),
                topic.into(),
                BasicPublishOptions::default(),
                message,
                BasicProperties::default(),
            )
            .await
            .map_err(|e| MqError::Publish(e.to_string()))?;
        Ok(())
    }

    async fn consume(&self, topic: &str) -> Result<Option<Message>, MqError> {
        let channel = self
            .channel
            .as_ref()
            .ok_or_else(|| MqError::Connection("not connected".to_string()))?;

        let need_create = {
            let consumers = self.consumers.read().await;
            !consumers.contains_key(topic)
        };

        if need_create {
            let mut consumers = self.consumers.write().await;
            if !consumers.contains_key(topic) {
                channel
                    .queue_declare(
                        topic.into(),
                        QueueDeclareOptions::default(),
                        FieldTable::default(),
                    )
                    .await
                    .map_err(|e| MqError::Subscribe(e.to_string()))?;
                let consumer = channel
                    .basic_consume(
                        topic.into(),
                        topic.into(),
                        BasicConsumeOptions::default(),
                        FieldTable::default(),
                    )
                    .await
                    .map_err(|e| MqError::Subscribe(e.to_string()))?;
                consumers.insert(topic.to_string(), Arc::new(Mutex::new(consumer)));
            }
        }

        let consumer_arc = {
            let consumers = self.consumers.read().await;
            consumers.get(topic).cloned()
        };

        if let Some(consumer_mutex) = consumer_arc {
            let mut consumer = consumer_mutex.lock().await;
            match timeout(Duration::from_millis(100), consumer.next()).await {
                Ok(Some(Ok(delivery))) => {
                    let delivery_tag = delivery.delivery_tag;
                    let msg_id = format!("rmq-{}", delivery_tag);
                    let msg = Message {
                        topic: topic.to_string(),
                        payload: delivery.data.clone(),
                        key: None,
                        timestamp: current_timestamp_millis(),
                        headers: HashMap::new(),
                        id: msg_id.clone(),
                    };
                    self.in_flight.write().await.insert(msg_id, delivery_tag);
                    Ok(Some(msg))
                }
                _ => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    async fn ack(&self, message_id: &str) -> Result<(), MqError> {
        let channel = self
            .channel
            .as_ref()
            .ok_or_else(|| MqError::Connection("not connected".to_string()))?;
        let delivery_tag = {
            let mut in_flight = self.in_flight.write().await;
            in_flight.remove(message_id).ok_or_else(|| {
                MqError::NotSupported(format!("Message not found for ack: {}", message_id))
            })?
        };
        channel
            .basic_ack(delivery_tag, BasicAckOptions::default())
            .await
            .map_err(|e| MqError::Publish(e.to_string()))?;
        Ok(())
    }

    async fn subscribe(&self, topic: &str) -> Result<(), MqError> {
        let channel = self
            .channel
            .as_ref()
            .ok_or_else(|| MqError::Connection("not connected".to_string()))?;
        channel
            .queue_declare(
                topic.into(),
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| MqError::Subscribe(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lapin_queue_new() {
        let q = LapinRabbitmqQueue::new("amqp://guest:guest@localhost:5672//");
        assert!(!q.is_connected());
    }

    #[tokio::test]
    async fn test_lapin_queue_publish_not_connected_fails() {
        let q = LapinRabbitmqQueue::new("amqp://guest:guest@localhost:5672//");
        let result = q.publish("test", b"hello").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_lapin_queue_consume_not_connected_fails() {
        let q = LapinRabbitmqQueue::new("amqp://guest:guest@localhost:5672//");
        let result = q.consume("test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_lapin_queue_subscribe_not_connected_fails() {
        let q = LapinRabbitmqQueue::new("amqp://guest:guest@localhost:5672//");
        let result = q.subscribe("test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore = "requires a real RabbitMQ at amqp://guest:guest@localhost:5672//"]
    async fn test_lapin_rabbitmq_connect_publish_consume() {
        let mut q = LapinRabbitmqQueue::new("amqp://guest:guest@localhost:5672//");
        q.connect().await.unwrap();
        assert!(q.is_connected());

        let topic = "test_lapin_topic";
        q.subscribe(topic).await.unwrap();
        q.publish(topic, b"hello rabbitmq").await.unwrap();

        let msg = q
            .consume(topic)
            .await
            .unwrap()
            .expect("message should exist");
        assert_eq!(msg.payload, b"hello rabbitmq");
        assert_eq!(msg.topic, topic);
        q.ack(&msg.id).await.unwrap();
    }
}
