#![cfg(feature = "real-broker")]

use crate::broker::MqttConfig;
use crate::error::MqttError;
use crate::qos::QoS;
use rumqttc::{AsyncClient, MqttOptions, QoS as RumqttcQoS};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

pub struct RealMqttClient {
    config: MqttConfig,
    client: Option<AsyncClient>,
    subscriptions: Arc<RwLock<HashSet<String>>>,
    connected: Arc<RwLock<bool>>,
}

impl RealMqttClient {
    pub fn new(config: MqttConfig) -> Self {
        Self {
            config,
            client: None,
            subscriptions: Arc::new(RwLock::new(HashSet::new())),
            connected: Arc::new(RwLock::new(false)),
        }
    }

    pub fn check_connected(&self) -> bool {
        self.client.is_some()
    }

    pub async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }

    pub async fn subscription_count(&self) -> usize {
        self.subscriptions.read().await.len()
    }

    pub async fn is_subscribed(&self, topic: &str) -> bool {
        self.subscriptions.read().await.contains(topic)
    }

    pub async fn connect(&mut self) -> Result<(), MqttError> {
        if self.client.is_some() {
            return Err(MqttError::Connection("Already connected".to_string()));
        }

        let (host, port) = parse_broker_url(&self.config.broker_url)?;

        let client_id = self
            .config
            .client_id
            .clone()
            .unwrap_or_else(|| format!("sz-orm-mqtt-{}", std::process::id()));

        let mut mqttoptions = MqttOptions::new(client_id, host, port);
        mqttoptions.set_keep_alive(Duration::from_secs(self.config.keep_alive as u64));
        mqttoptions.set_clean_session(self.config.clean_session);

        if let (Some(user), Some(pass)) = (&self.config.username, &self.config.password) {
            mqttoptions.set_credentials(user, pass);
        }

        let (client, mut connection) = AsyncClient::new(mqttoptions, 10);

        let connected = self.connected.clone();
        tokio::spawn(async move {
            loop {
                match connection.poll().await {
                    Ok(event) => {
                        if let rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(_)) = event {
                            let mut c = connected.write().await;
                            *c = true;
                        }
                    }
                    Err(_) => {
                        let mut c = connected.write().await;
                        *c = false;
                        break;
                    }
                }
            }
        });

        self.client = Some(client);
        Ok(())
    }

    pub async fn disconnect(&mut self) -> Result<(), MqttError> {
        if let Some(client) = self.client.take() {
            client
                .disconnect()
                .await
                .map_err(|e| MqttError::Connection(e.to_string()))?;
        }
        *self.connected.write().await = false;
        self.subscriptions.write().await.clear();
        Ok(())
    }

    pub async fn publish(&self, topic: &str, payload: Vec<u8>, qos: QoS) -> Result<(), MqttError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| MqttError::Connection("Not connected".to_string()))?;

        client
            .publish(topic, map_qos(qos), false, payload)
            .await
            .map_err(|e| MqttError::Publish(e.to_string()))?;
        Ok(())
    }

    pub async fn publish_retain(
        &self,
        topic: &str,
        payload: Vec<u8>,
        qos: QoS,
    ) -> Result<(), MqttError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| MqttError::Connection("Not connected".to_string()))?;

        client
            .publish(topic, map_qos(qos), true, payload)
            .await
            .map_err(|e| MqttError::Publish(e.to_string()))?;
        Ok(())
    }

    pub async fn subscribe(&self, topic: &str, qos: QoS) -> Result<(), MqttError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| MqttError::Connection("Not connected".to_string()))?;

        client
            .subscribe(topic, map_qos(qos))
            .await
            .map_err(|e| MqttError::Subscribe(e.to_string()))?;

        self.subscriptions.write().await.insert(topic.to_string());
        Ok(())
    }

    pub async fn unsubscribe(&self, topic: &str) -> Result<(), MqttError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| MqttError::Connection("Not connected".to_string()))?;

        client
            .unsubscribe(topic)
            .await
            .map_err(|e| MqttError::Subscribe(e.to_string()))?;

        self.subscriptions.write().await.remove(topic);
        Ok(())
    }
}

fn map_qos(qos: QoS) -> RumqttcQoS {
    match qos {
        QoS::AtMostOnce => RumqttcQoS::AtMostOnce,
        QoS::AtLeastOnce => RumqttcQoS::AtLeastOnce,
        QoS::ExactlyOnce => RumqttcQoS::ExactlyOnce,
    }
}

fn parse_broker_url(url: &str) -> Result<(String, u16), MqttError> {
    let rest = url
        .strip_prefix("tcp://")
        .or_else(|| url.strip_prefix("mqtt://"))
        .or_else(|| url.strip_prefix("ssl://"))
        .or_else(|| url.strip_prefix("mqtts://"))
        .ok_or_else(|| MqttError::Connection(format!("Invalid broker URL scheme: {}", url)))?;

    let (host, port) = if let Some(idx) = rest.rfind(':') {
        let host = &rest[..idx];
        let port_str = &rest[idx + 1..];
        let port: u16 = port_str
            .parse()
            .map_err(|_| MqttError::Connection(format!("Invalid port in broker URL: {}", url)))?;
        (host.to_string(), port)
    } else {
        (rest.to_string(), 1883)
    };

    if host.is_empty() {
        return Err(MqttError::Connection(format!(
            "Empty host in broker URL: {}",
            url
        )));
    }

    Ok((host, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_broker_url_with_port() {
        let (host, port) = parse_broker_url("tcp://broker.local:1884").unwrap();
        assert_eq!(host, "broker.local");
        assert_eq!(port, 1884);
    }

    #[test]
    fn test_parse_broker_url_without_port() {
        let (host, port) = parse_broker_url("tcp://broker.local").unwrap();
        assert_eq!(host, "broker.local");
        assert_eq!(port, 1883);
    }

    #[test]
    fn test_parse_broker_url_mqtt_scheme() {
        let (host, port) = parse_broker_url("mqtt://broker.local:8883").unwrap();
        assert_eq!(host, "broker.local");
        assert_eq!(port, 8883);

        let (host, port) = parse_broker_url("ssl://broker.local").unwrap();
        assert_eq!(host, "broker.local");
        assert_eq!(port, 1883);

        let (host, port) = parse_broker_url("mqtts://broker.local:8883").unwrap();
        assert_eq!(host, "broker.local");
        assert_eq!(port, 8883);
    }

    #[test]
    fn test_parse_broker_url_invalid_port() {
        let result = parse_broker_url("tcp://broker.local:abc");
        assert!(result.is_err());

        let result = parse_broker_url("tcp://broker.local:99999");
        assert!(result.is_err());
    }

    #[test]
    fn test_map_qos() {
        assert_eq!(map_qos(QoS::AtMostOnce), RumqttcQoS::AtMostOnce);
        assert_eq!(map_qos(QoS::AtLeastOnce), RumqttcQoS::AtLeastOnce);
        assert_eq!(map_qos(QoS::ExactlyOnce), RumqttcQoS::ExactlyOnce);
    }

    #[test]
    fn test_real_mqtt_client_new() {
        let config = MqttConfig::default();
        let client = RealMqttClient::new(config);
        assert!(!client.check_connected());
    }

    #[tokio::test]
    async fn test_publish_not_connected_fails() {
        let config = MqttConfig::default();
        let client = RealMqttClient::new(config);
        let result = client.publish("test", vec![], QoS::AtMostOnce).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_subscribe_not_connected_fails() {
        let config = MqttConfig::default();
        let client = RealMqttClient::new(config);
        let result = client.subscribe("test/#", QoS::AtMostOnce).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore = "requires a real MQTT broker at localhost:1883"]
    async fn test_real_broker_connect_publish_subscribe() {
        let config = MqttConfig::default();
        let mut client = RealMqttClient::new(config);

        client.connect().await.unwrap();
        assert!(client.check_connected());

        for _ in 0..50 {
            if client.is_connected().await {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(client.is_connected().await);

        client
            .subscribe("test/topic", QoS::AtLeastOnce)
            .await
            .unwrap();
        assert_eq!(client.subscription_count().await, 1);
        assert!(client.is_subscribed("test/topic").await);

        client
            .publish("test/topic", b"hello".to_vec(), QoS::AtLeastOnce)
            .await
            .unwrap();

        client.unsubscribe("test/topic").await.unwrap();
        assert_eq!(client.subscription_count().await, 0);
        assert!(!client.is_subscribed("test/topic").await);

        client.disconnect().await.unwrap();
        assert!(!client.check_connected());
    }
}
