use crate::error::MqttError;
use crate::topics::{topic_matches, TopicFilter};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub use crate::qos::QoS;

#[derive(Debug, Clone)]
pub struct MqttTopic {
    pub name: String,
    pub qos: QoS,
}

impl MqttTopic {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            qos: QoS::default(),
        }
    }

    pub fn with_qos(mut self, qos: QoS) -> Self {
        self.qos = qos;
        self
    }

    pub fn wildcard(&self) -> bool {
        self.name.contains('#') || self.name.contains('+')
    }

    pub fn levels(&self) -> Vec<&str> {
        self.name.split('/').collect()
    }

    pub fn matches(&self, topic: &str) -> bool {
        topic_matches(topic, &self.name)
    }
}

impl From<&str> for MqttTopic {
    fn from(s: &str) -> Self {
        MqttTopic::new(s)
    }
}

impl From<String> for MqttTopic {
    fn from(s: String) -> Self {
        MqttTopic::new(s)
    }
}

#[derive(Debug, Clone)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: QoS,
    pub retain: bool,
    pub client_id: Option<String>,
    pub timestamp: i64,
}

impl MqttMessage {
    pub fn new(topic: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            topic: topic.into(),
            payload,
            qos: QoS::default(),
            retain: false,
            client_id: None,
            timestamp: current_timestamp(),
        }
    }

    pub fn with_qos(mut self, qos: QoS) -> Self {
        self.qos = qos;
        self
    }

    pub fn retain(mut self) -> Self {
        self.retain = true;
        self
    }

    pub fn with_client(mut self, client_id: impl Into<String>) -> Self {
        self.client_id = Some(client_id.into());
        self
    }

    pub fn text(&self) -> Option<&str> {
        std::str::from_utf8(&self.payload).ok()
    }

    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Option<T> {
        serde_json::from_slice(&self.payload).ok()
    }
}

impl MqttMessage {
    pub fn text_message(topic: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(topic, text.into().into_bytes())
    }

    pub fn json_message<T: serde::Serialize>(
        topic: impl Into<String>,
        data: &T,
    ) -> Result<Self, MqttError> {
        let payload = serde_json::to_vec(data)?;
        Ok(Self::new(topic, payload))
    }
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub struct MqttConfig {
    pub broker_url: String,
    pub client_id: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub keep_alive: u16,
    pub clean_session: bool,
    pub topics: Vec<MqttTopic>,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            broker_url: "tcp://localhost:1883".to_string(),
            client_id: None,
            username: None,
            password: None,
            keep_alive: 60,
            clean_session: true,
            topics: Vec::new(),
        }
    }
}

impl MqttConfig {
    pub fn new(broker_url: impl Into<String>) -> Self {
        Self {
            broker_url: broker_url.into(),
            ..Default::default()
        }
    }

    pub fn with_client_id(mut self, client_id: impl Into<String>) -> Self {
        self.client_id = Some(client_id.into());
        self
    }

    pub fn with_auth(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }

    pub fn with_keep_alive(mut self, seconds: u16) -> Self {
        self.keep_alive = seconds;
        self
    }

    pub fn with_topics(mut self, topics: Vec<MqttTopic>) -> Self {
        self.topics = topics;
        self
    }

    pub fn add_topic(mut self, topic: MqttTopic) -> Self {
        self.topics.push(topic);
        self
    }
}

#[derive(Debug, Clone)]
struct Subscription {
    filter: String,
    qos: QoS,
}

pub struct MqttPlugin {
    config: MqttConfig,
    connected: bool,
    messages: Arc<RwLock<Vec<MqttMessage>>>,
    retained: Arc<RwLock<HashMap<String, MqttMessage>>>,
    subscriptions: Arc<RwLock<Vec<Subscription>>>,
}

impl MqttPlugin {
    pub fn new(config: MqttConfig) -> Self {
        Self {
            config,
            connected: false,
            messages: Arc::new(RwLock::new(Vec::new())),
            retained: Arc::new(RwLock::new(HashMap::new())),
            subscriptions: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn connect(&mut self) -> Result<(), MqttError> {
        self.connected = true;
        Ok(())
    }

    pub async fn disconnect(&mut self) -> Result<(), MqttError> {
        self.connected = false;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub async fn publish(&self, topic: &str, payload: Vec<u8>, qos: QoS) -> Result<(), MqttError> {
        if !self.connected {
            return Err(MqttError::Connection("Not connected".to_string()));
        }

        let client_id = self.config.client_id.clone();
        let msg = MqttMessage::new(topic, payload)
            .with_qos(qos)
            .with_client(client_id.unwrap_or_else(|| "default".to_string()));

        if msg.retain {
            let mut retained = self.retained.write().await;
            retained.insert(topic.to_string(), msg.clone());
        }

        let mut messages = self.messages.write().await;
        messages.push(msg);
        Ok(())
    }

    pub async fn publish_retain(
        &self,
        topic: &str,
        payload: Vec<u8>,
        qos: QoS,
    ) -> Result<(), MqttError> {
        if !self.connected {
            return Err(MqttError::Connection("Not connected".to_string()));
        }

        let client_id = self.config.client_id.clone();
        let msg = MqttMessage::new(topic, payload)
            .with_qos(qos)
            .with_client(client_id.unwrap_or_else(|| "default".to_string()))
            .retain();

        {
            let mut retained = self.retained.write().await;
            retained.insert(topic.to_string(), msg.clone());
        }

        let mut messages = self.messages.write().await;
        messages.push(msg);
        Ok(())
    }

    pub async fn subscribe(&self, topic: &str, qos: QoS) -> Result<(), MqttError> {
        if !self.connected {
            return Err(MqttError::Connection("Not connected".to_string()));
        }

        let _filter = TopicFilter::new(topic).map_err(|e| MqttError::Subscribe(e.to_string()))?;

        let mut subscriptions = self.subscriptions.write().await;
        if let Some(existing) = subscriptions.iter_mut().find(|s| s.filter == topic) {
            existing.qos = qos;
        } else {
            subscriptions.push(Subscription {
                filter: topic.to_string(),
                qos,
            });
        }
        Ok(())
    }

    pub async fn unsubscribe(&self, topic: &str) -> Result<(), MqttError> {
        if !self.connected {
            return Err(MqttError::Connection("Not connected".to_string()));
        }

        let mut subscriptions = self.subscriptions.write().await;
        subscriptions.retain(|s| s.filter != topic);
        Ok(())
    }

    pub fn topic_matches(&self, topic: &str, filter: &str) -> bool {
        topic_matches(topic, filter)
    }

    pub async fn message_count(&self) -> usize {
        let messages = self.messages.read().await;
        messages.len()
    }

    pub async fn messages_for(&self, topic: &str) -> Vec<MqttMessage> {
        let messages = self.messages.read().await;
        messages
            .iter()
            .filter(|m| m.topic == topic)
            .cloned()
            .collect()
    }

    pub async fn messages_matching(&self, filter: &str) -> Vec<MqttMessage> {
        let messages = self.messages.read().await;
        messages
            .iter()
            .filter(|m| topic_matches(&m.topic, filter))
            .cloned()
            .collect()
    }

    pub async fn subscription_count(&self) -> usize {
        let subscriptions = self.subscriptions.read().await;
        subscriptions.len()
    }

    pub async fn is_subscribed(&self, topic: &str) -> bool {
        let subscriptions = self.subscriptions.read().await;
        subscriptions.iter().any(|s| s.filter == topic)
    }

    pub async fn subscription_qos(&self, topic: &str) -> Option<QoS> {
        let subscriptions = self.subscriptions.read().await;
        subscriptions
            .iter()
            .find(|s| s.filter == topic)
            .map(|s| s.qos)
    }

    pub async fn retained_count(&self) -> usize {
        let retained = self.retained.read().await;
        retained.len()
    }

    pub async fn retained_get(&self, topic: &str) -> Option<MqttMessage> {
        let retained = self.retained.read().await;
        retained.get(topic).cloned()
    }

    pub async fn clear_messages(&self) {
        let mut messages = self.messages.write().await;
        messages.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_publish_stores_message() {
        let config = MqttConfig::default();
        let mut plugin = MqttPlugin::new(config);
        plugin.connect().await.unwrap();

        plugin
            .publish("sensor/temp", b"23.5".to_vec(), QoS::AtLeastOnce)
            .await
            .unwrap();
        assert_eq!(plugin.message_count().await, 1);

        let messages = plugin.messages_for("sensor/temp").await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].payload, b"23.5");
        assert_eq!(messages[0].qos, QoS::AtLeastOnce);
    }

    #[tokio::test]
    async fn test_publish_multiple_messages() {
        let config = MqttConfig::default();
        let mut plugin = MqttPlugin::new(config);
        plugin.connect().await.unwrap();

        plugin
            .publish("topic/a", b"1".to_vec(), QoS::AtMostOnce)
            .await
            .unwrap();
        plugin
            .publish("topic/b", b"2".to_vec(), QoS::AtMostOnce)
            .await
            .unwrap();
        plugin
            .publish("topic/a", b"3".to_vec(), QoS::AtMostOnce)
            .await
            .unwrap();

        assert_eq!(plugin.message_count().await, 3);
        assert_eq!(plugin.messages_for("topic/a").await.len(), 2);
        assert_eq!(plugin.messages_for("topic/b").await.len(), 1);
    }

    #[tokio::test]
    async fn test_publish_not_connected_fails() {
        let config = MqttConfig::default();
        let plugin = MqttPlugin::new(config);
        let result = plugin.publish("topic", vec![], QoS::AtMostOnce).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_subscribe_registers_subscription() {
        let config = MqttConfig::default();
        let mut plugin = MqttPlugin::new(config);
        plugin.connect().await.unwrap();

        plugin.subscribe("home/#", QoS::ExactlyOnce).await.unwrap();
        assert_eq!(plugin.subscription_count().await, 1);
        assert!(plugin.is_subscribed("home/#").await);
        assert_eq!(
            plugin.subscription_qos("home/#").await,
            Some(QoS::ExactlyOnce)
        );
    }

    #[tokio::test]
    async fn test_subscribe_updates_qos_for_existing() {
        let config = MqttConfig::default();
        let mut plugin = MqttPlugin::new(config);
        plugin.connect().await.unwrap();

        plugin.subscribe("home/#", QoS::AtMostOnce).await.unwrap();
        plugin.subscribe("home/#", QoS::ExactlyOnce).await.unwrap();
        assert_eq!(plugin.subscription_count().await, 1);
        assert_eq!(
            plugin.subscription_qos("home/#").await,
            Some(QoS::ExactlyOnce)
        );
    }

    #[tokio::test]
    async fn test_unsubscribe_removes_subscription() {
        let config = MqttConfig::default();
        let mut plugin = MqttPlugin::new(config);
        plugin.connect().await.unwrap();

        plugin.subscribe("home/#", QoS::AtMostOnce).await.unwrap();
        assert!(plugin.is_subscribed("home/#").await);

        plugin.unsubscribe("home/#").await.unwrap();
        assert!(!plugin.is_subscribed("home/#").await);
        assert_eq!(plugin.subscription_count().await, 0);
    }

    #[tokio::test]
    async fn test_subscribe_not_connected_fails() {
        let config = MqttConfig::default();
        let plugin = MqttPlugin::new(config);
        let result = plugin.subscribe("home/#", QoS::AtMostOnce).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_messages_matching_wildcard() {
        let config = MqttConfig::default();
        let mut plugin = MqttPlugin::new(config);
        plugin.connect().await.unwrap();

        plugin
            .publish("home/living/temp", b"23".to_vec(), QoS::AtMostOnce)
            .await
            .unwrap();
        plugin
            .publish("home/kitchen/temp", b"20".to_vec(), QoS::AtMostOnce)
            .await
            .unwrap();
        plugin
            .publish("office/temp", b"25".to_vec(), QoS::AtMostOnce)
            .await
            .unwrap();

        let matched = plugin.messages_matching("home/+/temp").await;
        assert_eq!(matched.len(), 2);

        let matched_all = plugin.messages_matching("home/#").await;
        assert_eq!(matched_all.len(), 2);
    }

    #[tokio::test]
    async fn test_retained_messages() {
        let config = MqttConfig::default();
        let mut plugin = MqttPlugin::new(config);
        plugin.connect().await.unwrap();

        plugin
            .publish_retain("config/version", b"1.0".to_vec(), QoS::AtLeastOnce)
            .await
            .unwrap();

        assert_eq!(plugin.retained_count().await, 1);
        let retained = plugin
            .retained_get("config/version")
            .await
            .expect("retained message should exist");
        assert_eq!(retained.payload, b"1.0");
        assert!(retained.retain);
    }

    #[tokio::test]
    async fn test_disconnect_clears_connection_state() {
        let config = MqttConfig::default();
        let mut plugin = MqttPlugin::new(config);
        plugin.connect().await.unwrap();
        assert!(plugin.is_connected());

        plugin.disconnect().await.unwrap();
        assert!(!plugin.is_connected());

        let result = plugin.publish("topic", vec![], QoS::AtMostOnce).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_clear_messages() {
        let config = MqttConfig::default();
        let mut plugin = MqttPlugin::new(config);
        plugin.connect().await.unwrap();

        plugin
            .publish("topic", b"data".to_vec(), QoS::AtMostOnce)
            .await
            .unwrap();
        assert_eq!(plugin.message_count().await, 1);

        plugin.clear_messages().await;
        assert_eq!(plugin.message_count().await, 0);
    }

    #[test]
    fn test_mqtt_topic_matches_method() {
        let topic = MqttTopic::new("home/+/temp");
        assert!(topic.matches("home/living/temp"));
        assert!(!topic.matches("home/living/humidity"));
    }
}
