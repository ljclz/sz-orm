//! # SZ-ORM MQTT — MQTT 协议支持
//!
//! 提供 MQTT 消息中间件抽象，包含 broker 客户端、QoS 级别与主题通配符匹配，
//! 启用 `real-broker` feature 后接入真实 MQTT broker。
//!
//! ## 主要模块
//!
//! - [`broker`] — 客户端配置与插件入口
//! - [`qos`] — 服务质量等级（0/1/2）
//! - [`topics`] — 主题通配符匹配

pub mod broker;
pub mod error;
pub mod qos;
pub mod topics;

pub use error::MqttError;

pub use broker::MqttConfig;
pub use broker::MqttMessage;
pub use broker::MqttPlugin;
pub use broker::MqttTopic;
pub use broker::QoS;

#[cfg(feature = "real-broker")]
pub mod real_broker;

#[cfg(feature = "real-broker")]
pub use real_broker::RealMqttClient;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qos_levels() {
        assert_eq!(QoS::AtMostOnce.level(), 0);
        assert_eq!(QoS::AtLeastOnce.level(), 1);
        assert_eq!(QoS::ExactlyOnce.level(), 2);
    }

    #[test]
    fn test_qos_from_level() {
        assert_eq!(QoS::from_level(0), QoS::AtMostOnce);
        assert_eq!(QoS::from_level(1), QoS::AtLeastOnce);
        assert_eq!(QoS::from_level(2), QoS::ExactlyOnce);
        assert_eq!(QoS::from_level(99), QoS::AtMostOnce);
    }

    #[test]
    fn test_mqtt_topic_new() {
        let topic = MqttTopic::new("home/living/temperature");
        assert_eq!(topic.name, "home/living/temperature");
        assert_eq!(topic.qos, QoS::AtMostOnce);
    }

    #[test]
    fn test_mqtt_topic_with_qos() {
        let topic = MqttTopic::new("test").with_qos(QoS::ExactlyOnce);
        assert_eq!(topic.qos, QoS::ExactlyOnce);
    }

    #[test]
    fn test_mqtt_topic_wildcard() {
        let topic = MqttTopic::new("home/#");
        assert!(topic.wildcard());

        let topic2 = MqttTopic::new("home/+/temperature");
        assert!(topic2.wildcard());

        let topic3 = MqttTopic::new("home/living/temperature");
        assert!(!topic3.wildcard());
    }

    #[test]
    fn test_mqtt_topic_levels() {
        let topic = MqttTopic::new("home/living/temperature");
        let levels = topic.levels();
        assert_eq!(levels, vec!["home", "living", "temperature"]);
    }

    #[test]
    fn test_mqtt_message_new() {
        let msg = MqttMessage::new("test/topic", vec![1, 2, 3]);
        assert_eq!(msg.topic, "test/topic");
        assert_eq!(msg.payload, vec![1, 2, 3]);
        assert_eq!(msg.qos, QoS::AtMostOnce);
        assert!(!msg.retain);
    }

    #[test]
    fn test_mqtt_message_with_qos() {
        let msg = MqttMessage::new("test", vec![]).with_qos(QoS::AtLeastOnce);
        assert_eq!(msg.qos, QoS::AtLeastOnce);
    }

    #[test]
    fn test_mqtt_message_retain() {
        let msg = MqttMessage::new("test", vec![]).retain();
        assert!(msg.retain);
    }

    #[test]
    fn test_mqtt_message_text() {
        let msg = MqttMessage::text_message("test", "hello world");
        assert_eq!(msg.text(), Some("hello world"));
    }

    #[test]
    fn test_mqtt_message_json() {
        let msg = MqttMessage::json_message("test", &serde_json::json!({"key": "value"})).unwrap();
        let parsed: serde_json::Value = msg.json().unwrap();
        assert_eq!(parsed["key"], "value");
    }

    #[test]
    fn test_mqtt_config_default() {
        let config = MqttConfig::default();
        assert_eq!(config.broker_url, "tcp://localhost:1883");
        assert_eq!(config.keep_alive, 60);
        assert!(config.clean_session);
    }

    #[test]
    fn test_mqtt_config_builder() {
        let config = MqttConfig::new("tcp://broker.local:1883")
            .with_client_id("my-client")
            .with_auth("user", "pass")
            .with_keep_alive(120);

        assert_eq!(config.broker_url, "tcp://broker.local:1883");
        assert_eq!(config.client_id, Some("my-client".to_string()));
        assert_eq!(config.username, Some("user".to_string()));
        assert_eq!(config.password, Some("pass".to_string()));
        assert_eq!(config.keep_alive, 120);
    }

    #[test]
    fn test_mqtt_config_add_topic() {
        let config = MqttConfig::new("tcp://localhost:1883").add_topic(MqttTopic::new("home/#"));

        assert_eq!(config.topics.len(), 1);
    }

    #[tokio::test]
    async fn test_mqtt_plugin_new() {
        let config = MqttConfig::default();
        let plugin = MqttPlugin::new(config);
        assert!(!plugin.is_connected());
    }

    #[tokio::test]
    async fn test_mqtt_plugin_connect() {
        let config = MqttConfig::default();
        let mut plugin = MqttPlugin::new(config);

        let result = plugin.connect().await;
        assert!(result.is_ok());
        assert!(plugin.is_connected());
    }

    #[tokio::test]
    async fn test_mqtt_plugin_disconnect() {
        let config = MqttConfig::default();
        let mut plugin = MqttPlugin::new(config);

        plugin.connect().await.unwrap();
        plugin.disconnect().await.unwrap();
        assert!(!plugin.is_connected());
    }

    #[tokio::test]
    async fn test_mqtt_plugin_publish_not_connected() {
        let config = MqttConfig::default();
        let plugin = MqttPlugin::new(config);

        let result = plugin.publish("test", vec![], QoS::AtMostOnce).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mqtt_plugin_publish() {
        let config = MqttConfig::default();
        let mut plugin = MqttPlugin::new(config);

        plugin.connect().await.unwrap();
        let result = plugin
            .publish("test", vec![1, 2, 3], QoS::AtLeastOnce)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mqtt_plugin_subscribe() {
        let config = MqttConfig::default();
        let mut plugin = MqttPlugin::new(config);

        plugin.connect().await.unwrap();
        let result = plugin.subscribe("home/#", QoS::ExactlyOnce).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mqtt_plugin_unsubscribe() {
        let config = MqttConfig::default();
        let mut plugin = MqttPlugin::new(config);

        plugin.connect().await.unwrap();
        let result = plugin.unsubscribe("home/living").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mqtt_plugin_topic_matches() {
        let config = MqttConfig::default();
        let plugin = MqttPlugin::new(config);

        assert!(plugin.topic_matches("home/living/temperature", "home/#"));
        assert!(plugin.topic_matches("home/living/temperature", "home/+/temperature"));
        assert!(plugin.topic_matches("home/living/temperature", "home/living/temperature"));
        assert!(!plugin.topic_matches("home/living/temperature", "home/kitchen/temperature"));
    }

    #[test]
    fn test_mqtt_topic_from_str() {
        let topic: MqttTopic = "test/topic".into();
        assert_eq!(topic.name, "test/topic");
    }
}
