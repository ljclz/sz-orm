pub mod error;
pub mod queue;

pub mod activemq;
pub mod kafka;
pub mod nats;
pub mod pulsar;
pub mod rabbitmq;
pub mod rocketmq;

pub use error::MqError;

pub use queue::ActiveConfig;
pub use queue::InMemoryQueue;
pub use queue::KafkaConfig;
pub use queue::Message;
pub use queue::MessageQueue;
pub use queue::MqProvider;
pub use queue::NatsConfig;
pub use queue::PulsarConfig;
pub use queue::QueueConfig;
pub use queue::QueueWrapper;
pub use queue::RabbitConfig;
pub use queue::RocketConfig;

pub use activemq::InMemoryActivemqQueue;
pub use kafka::InMemoryKafkaQueue;
pub use nats::InMemoryNatsQueue;
pub use pulsar::InMemoryPulsarQueue;
pub use rabbitmq::InMemoryRabbitmqQueue;
pub use rocketmq::InMemoryRocketmqQueue;

// ============================================================================
// 真实实现（通过 feature flag 启用）
// ============================================================================

// RabbitMQ: lapin (AMQP 0.9.1) — 真实实现
#[cfg(feature = "rabbitmq")]
pub mod lapin_rabbitmq;

#[cfg(feature = "rabbitmq")]
pub use lapin_rabbitmq::LapinRabbitmqQueue;

// ActiveMQ: lapin (AMQP 1.0，ActiveMQ Artemis) — 真实实现
#[cfg(feature = "activemq")]
pub mod real_activemq;

#[cfg(feature = "activemq")]
pub use real_activemq::RealActivemqQueue;

// NATS: async-nats — 真实实现
#[cfg(feature = "nats")]
pub mod real_nats;

#[cfg(feature = "nats")]
pub use real_nats::RealNatsQueue;

// Pulsar: pulsar crate — 真实实现
#[cfg(feature = "pulsar")]
pub mod real_pulsar;

#[cfg(feature = "pulsar")]
pub use real_pulsar::RealPulsarQueue;

// Kafka: rdkafka — 真实实现
#[cfg(feature = "kafka")]
pub mod real_kafka;

#[cfg(feature = "kafka")]
pub use real_kafka::RealKafkaQueue;

// RocketMQ: 无成熟 Rust 客户端，保持 stub
// 跟踪项：https://github.com/mxsm/rocketmq-rust （未来可能可用）

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_new() {
        let msg = Message::new("test", vec![1, 2, 3]);
        assert_eq!(msg.topic, "test");
        assert_eq!(msg.payload, vec![1, 2, 3]);
        assert!(msg.key.is_none());
    }

    #[test]
    fn test_message_with_key() {
        let msg = Message::new("test", vec![]).with_key("mykey");
        assert_eq!(msg.key, Some("mykey".to_string()));
    }

    #[test]
    fn test_message_text() {
        let msg = Message::text_message("test", "hello");
        assert_eq!(msg.text(), Some("hello"));
    }

    #[test]
    fn test_message_json() {
        let msg = Message::json_message("test", &serde_json::json!({"key": "value"})).unwrap();
        let parsed: serde_json::Value = msg.json().unwrap();
        assert_eq!(parsed["key"], "value");
    }

    #[test]
    fn test_queue_config_default() {
        let config = QueueConfig::default();
        assert_eq!(config.brokers, vec!["localhost:9092".to_string()]);
        assert!(matches!(config.provider, MqProvider::Kafka(_)));
    }

    #[test]
    fn test_queue_config_builder() {
        let config = QueueConfig::new()
            .with_provider(MqProvider::RabbitMQ(RabbitConfig::default()))
            .with_brokers(vec!["localhost:5672".to_string()])
            .with_group("my-group")
            .with_auth("user", "pass");

        assert!(matches!(config.provider, MqProvider::RabbitMQ(_)));
        assert_eq!(config.brokers, vec!["localhost:5672".to_string()]);
        assert_eq!(config.group_id, Some("my-group".to_string()));
        assert_eq!(config.username, Some("user".to_string()));
        assert_eq!(config.password, Some("pass".to_string()));
    }

    #[tokio::test]
    async fn test_queue_wrapper_publish() {
        let wrapper = QueueWrapper::new(MqProvider::Kafka(KafkaConfig::default()));
        let result = wrapper.publish("test-topic", b"message").await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_kafka_config_default() {
        let config = KafkaConfig::default();
        assert!(config.client_id.is_none());
        assert!(config.acks.is_none());
    }

    #[test]
    fn test_rabbit_config_default() {
        let config = RabbitConfig::default();
        assert!(config.virtual_host.is_none());
    }

    #[test]
    fn test_rocket_config_default() {
        let config = RocketConfig::default();
        assert!(config.namespace.is_none());
    }

    #[test]
    fn test_active_config_default() {
        let config = ActiveConfig::default();
        assert!(config.broker_url.is_none());
    }

    #[test]
    fn test_nats_config_default() {
        let config = NatsConfig::default();
        assert!(config.name.is_none());
    }

    #[test]
    fn test_pulsar_config_default() {
        let config = PulsarConfig::default();
        assert!(config.service_url.is_none());
    }

    #[test]
    fn test_message_timestamp_set() {
        let msg = Message::new("test", vec![]);
        assert!(msg.timestamp > 0);
    }

    #[test]
    fn test_message_headers() {
        let msg = Message::new("test", vec![]);
        assert!(msg.headers.is_empty());
    }

    #[tokio::test]
    async fn test_in_memory_queue_publish_and_consume() {
        let queue = InMemoryQueue::new();
        queue.publish("orders", b"order-1").await.unwrap();
        assert_eq!(queue.message_count("orders").await, 1);

        let msg = queue
            .consume("orders")
            .await
            .unwrap()
            .expect("message should exist");
        assert_eq!(msg.payload, b"order-1");
        assert_eq!(msg.topic, "orders");
        assert!(!msg.id.is_empty());
        assert_eq!(queue.message_count("orders").await, 0);
    }

    #[tokio::test]
    async fn test_in_memory_queue_consume_empty() {
        let queue = InMemoryQueue::new();
        let result = queue.consume("empty-topic").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_in_memory_queue_ack() {
        let queue = InMemoryQueue::new();
        queue.publish("topic", b"data").await.unwrap();
        let msg = queue.consume("topic").await.unwrap().unwrap();

        assert_eq!(queue.in_flight_count().await, 1);
        queue.ack(&msg.id).await.unwrap();
        assert_eq!(queue.in_flight_count().await, 0);
    }

    #[tokio::test]
    async fn test_in_memory_queue_ack_unknown_id() {
        let queue = InMemoryQueue::new();
        let result = queue.ack("unknown-id").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_in_memory_queue_subscribe() {
        let queue = InMemoryQueue::new();
        queue.subscribe("topic-a").await.unwrap();
        queue.subscribe("topic-a").await.unwrap();
        queue.subscribe("topic-b").await.unwrap();
        assert_eq!(queue.subscriber_count("topic-a").await, 2);
        assert_eq!(queue.subscriber_count("topic-b").await, 1);
        assert_eq!(queue.subscriber_count("topic-c").await, 0);
    }

    #[tokio::test]
    async fn test_in_memory_queue_multiple_topics() {
        let queue = InMemoryQueue::new();
        queue.publish("topic-a", b"a1").await.unwrap();
        queue.publish("topic-b", b"b1").await.unwrap();
        queue.publish("topic-a", b"a2").await.unwrap();

        assert_eq!(queue.message_count("topic-a").await, 2);
        assert_eq!(queue.message_count("topic-b").await, 1);
    }

    #[tokio::test]
    async fn test_in_memory_queue_fifo_order() {
        let queue = InMemoryQueue::new();
        queue.publish("topic", b"first").await.unwrap();
        queue.publish("topic", b"second").await.unwrap();
        queue.publish("topic", b"third").await.unwrap();

        let m1 = queue.consume("topic").await.unwrap().unwrap();
        let m2 = queue.consume("topic").await.unwrap().unwrap();
        let m3 = queue.consume("topic").await.unwrap().unwrap();
        assert_eq!(m1.payload, b"first");
        assert_eq!(m2.payload, b"second");
        assert_eq!(m3.payload, b"third");
    }

    #[tokio::test]
    async fn test_queue_wrapper_publish_and_consume() {
        let wrapper = QueueWrapper::new(MqProvider::Kafka(KafkaConfig::default()));
        wrapper.publish("wrapper-topic", b"payload").await.unwrap();

        let msg = wrapper
            .consume("wrapper-topic")
            .await
            .unwrap()
            .expect("message should exist");
        assert_eq!(msg.payload, b"payload");
        wrapper.ack(&msg.id).await.unwrap();
    }

    #[tokio::test]
    async fn test_queue_wrapper_subscribe_and_ack() {
        let wrapper = QueueWrapper::new(MqProvider::RabbitMQ(RabbitConfig::default()));
        wrapper.subscribe("sub-topic").await.unwrap();
        wrapper.publish("sub-topic", b"hello").await.unwrap();

        let msg = wrapper
            .consume("sub-topic")
            .await
            .unwrap()
            .expect("message should exist");
        assert_eq!(msg.payload, b"hello");
        assert!(wrapper.ack(&msg.id).await.is_ok());
    }

    #[tokio::test]
    async fn test_queue_wrapper_consume_empty() {
        let wrapper = QueueWrapper::new(MqProvider::Nats(NatsConfig::default()));
        let result = wrapper.consume("empty").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_queue_wrapper_ack_unknown() {
        let wrapper = QueueWrapper::new(MqProvider::Pulsar(PulsarConfig::default()));
        assert!(wrapper.ack("unknown").await.is_err());
    }

    #[tokio::test]
    async fn test_queue_wrapper_with_each_provider() {
        let providers = vec![
            MqProvider::Kafka(KafkaConfig::default()),
            MqProvider::RabbitMQ(RabbitConfig::default()),
            MqProvider::RocketMQ(RocketConfig::default()),
            MqProvider::ActiveMQ(ActiveConfig::default()),
            MqProvider::Nats(NatsConfig::default()),
            MqProvider::Pulsar(PulsarConfig::default()),
        ];

        for provider in providers {
            let wrapper = QueueWrapper::new(provider);
            wrapper.publish("topic", b"data").await.unwrap();
            let msg = wrapper
                .consume("topic")
                .await
                .unwrap()
                .expect("message should exist");
            assert_eq!(msg.payload, b"data");
            wrapper.ack(&msg.id).await.unwrap();
        }
    }
}
