//! # 遗嘱消息（Last Will & Testament）
//!
//! 实现 MQTT 遗嘱消息功能：客户端连接时注册遗嘱消息，当客户端异常断开时
//! 由 broker 代为发布。支持 MQTT 5.0 的遗嘱延迟（Will Delay）特性。
//!
//! ## 主要类型
//!
//! - [`WillMessage`] — 遗嘱消息内容
//! - [`WillConfig`] — 遗嘱配置（含延迟发布间隔）
//! - [`WillRegistry`] — 遗嘱消息注册表（broker 端）
//! - [`WillDelivery`] — 遗嘱消息投递结果

use crate::broker::{MqttMessage, QoS};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 遗嘱消息内容
#[derive(Debug, Clone)]
pub struct WillMessage {
    /// 遗嘱主题
    pub topic: String,
    /// 遗嘱载荷
    pub payload: Vec<u8>,
    /// 遗嘱 QoS
    pub qos: QoS,
    /// 是否作为保留消息发布
    pub retain: bool,
}

impl WillMessage {
    /// 创建新的遗嘱消息
    pub fn new(topic: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            topic: topic.into(),
            payload,
            qos: QoS::default(),
            retain: false,
        }
    }

    /// 设置 QoS
    pub fn with_qos(mut self, qos: QoS) -> Self {
        self.qos = qos;
        self
    }

    /// 设置为保留消息
    pub fn retain(mut self) -> Self {
        self.retain = true;
        self
    }

    /// 从文本构造
    pub fn text(topic: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(topic, text.into().into_bytes())
    }

    /// 转换为 MqttMessage
    pub fn to_mqtt_message(&self, client_id: &str) -> MqttMessage {
        let mut msg = MqttMessage::new(self.topic.clone(), self.payload.clone())
            .with_qos(self.qos)
            .with_client(client_id);
        if self.retain {
            msg = msg.retain();
        }
        msg
    }

    /// 获取文本内容
    pub fn text_payload(&self) -> Option<&str> {
        std::str::from_utf8(&self.payload).ok()
    }
}

/// 遗嘱配置（包含遗嘱延迟等高级选项）
#[derive(Debug, Clone)]
pub struct WillConfig {
    /// 遗嘱消息本体
    pub message: WillMessage,
    /// 遗嘱延迟（秒）。0 表示立即发布；>0 表示延迟指定秒数后发布
    /// （MQTT 5.0 Will Delay Interval）
    pub delay_seconds: u32,
    /// 当客户端正常断开时是否仍发布遗嘱（默认 false，符合 MQTT 规范）
    pub publish_on_graceful_disconnect: bool,
}

impl WillConfig {
    /// 创建遗嘱配置
    pub fn new(message: WillMessage) -> Self {
        Self {
            message,
            delay_seconds: 0,
            publish_on_graceful_disconnect: false,
        }
    }

    /// 设置遗嘱延迟
    pub fn with_delay(mut self, seconds: u32) -> Self {
        self.delay_seconds = seconds;
        self
    }

    /// 设置正常断开时是否发布遗嘱
    pub fn publish_on_graceful(mut self, publish: bool) -> Self {
        self.publish_on_graceful_disconnect = publish;
        self
    }

    /// 是否有延迟
    pub fn has_delay(&self) -> bool {
        self.delay_seconds > 0
    }
}

/// 遗嘱消息投递结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WillDelivery {
    /// 遗嘱已发布
    Delivered { client_id: String },
    /// 遗嘱已调度（等待延迟到期）
    Scheduled { client_id: String, delay_seconds: u32 },
    /// 遗嘱未发布（客户端正常断开且配置为不发布）
    Skipped { client_id: String, reason: String },
    /// 遗嘱已被取消（客户端在延迟期内重新连接）
    Cancelled { client_id: String },
}

/// broker 端的遗嘱消息注册表
#[derive(Debug, Default)]
pub struct WillRegistry {
    /// 按 client_id 索引的遗嘱配置
    wills: Arc<RwLock<HashMap<String, WillConfig>>>,
    /// 待发布的延迟遗嘱队列：client_id -> (到期时间戳 millis, WillMessage)
    pending: Arc<RwLock<HashMap<String, (i64, WillMessage)>>>,
}

impl WillRegistry {
    pub fn new() -> Self {
        Self {
            wills: Arc::new(RwLock::new(HashMap::new())),
            pending: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册遗嘱消息
    pub async fn register(&self, client_id: impl Into<String>, config: WillConfig) {
        let mut wills = self.wills.write().await;
        wills.insert(client_id.into(), config);
    }

    /// 注销遗嘱消息（通常在客户端正常断开时调用）
    pub async fn unregister(&self, client_id: &str) -> Option<WillConfig> {
        let mut wills = self.wills.write().await;
        wills.remove(client_id)
    }

    /// 查询客户端是否注册了遗嘱
    pub async fn has_will(&self, client_id: &str) -> bool {
        let wills = self.wills.read().await;
        wills.contains_key(client_id)
    }

    /// 查询客户端的遗嘱配置
    pub async fn get(&self, client_id: &str) -> Option<WillConfig> {
        let wills = self.wills.read().await;
        wills.get(client_id).cloned()
    }

    /// 当前注册的遗嘱数量
    pub async fn count(&self) -> usize {
        let wills = self.wills.read().await;
        wills.len()
    }

    /// 触发遗嘱消息发布。`graceful` 表示是否为正常断开。
    /// 返回遗嘱投递结果描述。
    pub async fn trigger(
        &self,
        client_id: &str,
        graceful: bool,
        now_ms: i64,
    ) -> WillDelivery {
        let config = {
            let mut wills = self.wills.write().await;
            wills.remove(client_id)
        };
        let Some(config) = config else {
            return WillDelivery::Skipped {
                client_id: client_id.to_string(),
                reason: "no will registered".to_string(),
            };
        };

        // 正常断开且配置为不发布
        if graceful && !config.publish_on_graceful_disconnect {
            return WillDelivery::Skipped {
                client_id: client_id.to_string(),
                reason: "graceful disconnect, will suppressed".to_string(),
            };
        }

        // 延迟发布
        if config.has_delay() {
            let due = now_ms + (config.delay_seconds as i64) * 1000;
            let mut pending = self.pending.write().await;
            pending.insert(
                client_id.to_string(),
                (due, config.message.clone()),
            );
            return WillDelivery::Scheduled {
                client_id: client_id.to_string(),
                delay_seconds: config.delay_seconds,
            };
        }

        // 立即发布
        WillDelivery::Delivered {
            client_id: client_id.to_string(),
        }
    }

    /// 取消待发布的延迟遗嘱（客户端在延迟期内重新连接时调用）
    pub async fn cancel_pending(&self, client_id: &str) -> bool {
        let mut pending = self.pending.write().await;
        pending.remove(client_id).is_some()
    }

    /// 获取已到期的遗嘱消息列表（broker 应发布这些消息）
    pub async fn drain_due(&self, now_ms: i64) -> Vec<(String, WillMessage)> {
        let mut pending = self.pending.write().await;
        let mut due = Vec::new();
        let mut remove_keys = Vec::new();
        for (client_id, (deadline, msg)) in pending.iter() {
            if *deadline <= now_ms {
                due.push((client_id.clone(), msg.clone()));
                remove_keys.push(client_id.clone());
            }
        }
        for key in remove_keys {
            pending.remove(&key);
        }
        due
    }

    /// 当前待发布的延迟遗嘱数量
    pub async fn pending_count(&self) -> usize {
        let pending = self.pending.read().await;
        pending.len()
    }

    /// 将已触发的遗嘱转换为 MqttMessage
    pub fn to_mqtt_messages(items: Vec<(String, WillMessage)>) -> Vec<MqttMessage> {
        items
            .into_iter()
            .map(|(client_id, will)| will.to_mqtt_message(&client_id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now_ms() -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }

    #[test]
    fn test_will_message_new_defaults() {
        let will = WillMessage::new("client/status", b"offline".to_vec());
        assert_eq!(will.topic, "client/status");
        assert_eq!(will.payload, b"offline");
        assert_eq!(will.qos, QoS::AtMostOnce);
        assert!(!will.retain);
    }

    #[test]
    fn test_will_message_builder() {
        let will = WillMessage::text("status/topic", "dead")
            .with_qos(QoS::ExactlyOnce)
            .retain();
        assert_eq!(will.qos, QoS::ExactlyOnce);
        assert!(will.retain);
        assert_eq!(will.text_payload(), Some("dead"));
    }

    #[test]
    fn test_will_message_to_mqtt_message_sets_client() {
        let will = WillMessage::text("t", "p").with_qos(QoS::AtLeastOnce);
        let msg = will.to_mqtt_message("client-1");
        assert_eq!(msg.topic, "t");
        assert_eq!(msg.text(), Some("p"));
        assert_eq!(msg.qos, QoS::AtLeastOnce);
        assert_eq!(msg.client_id, Some("client-1".to_string()));
        assert!(!msg.retain);
    }

    #[test]
    fn test_will_message_to_mqtt_message_retain_flag() {
        let will = WillMessage::text("t", "p").retain();
        let msg = will.to_mqtt_message("c");
        assert!(msg.retain);
    }

    #[test]
    fn test_will_config_default_no_delay() {
        let cfg = WillConfig::new(WillMessage::text("t", "p"));
        assert!(!cfg.has_delay());
        assert_eq!(cfg.delay_seconds, 0);
        assert!(!cfg.publish_on_graceful_disconnect);
    }

    #[test]
    fn test_will_config_with_delay() {
        let cfg = WillConfig::new(WillMessage::text("t", "p")).with_delay(30);
        assert!(cfg.has_delay());
        assert_eq!(cfg.delay_seconds, 30);
    }

    #[test]
    fn test_will_config_publish_on_graceful() {
        let cfg = WillConfig::new(WillMessage::text("t", "p")).publish_on_graceful(true);
        assert!(cfg.publish_on_graceful_disconnect);
    }

    #[tokio::test]
    async fn test_registry_register_and_has() {
        let reg = WillRegistry::new();
        reg.register(
            "client-1",
            WillConfig::new(WillMessage::text("status", "offline")),
        )
        .await;
        assert!(reg.has_will("client-1").await);
        assert!(!reg.has_will("client-2").await);
        assert_eq!(reg.count().await, 1);
    }

    #[tokio::test]
    async fn test_registry_unregister() {
        let reg = WillRegistry::new();
        reg.register(
            "c1",
            WillConfig::new(WillMessage::text("t", "p")),
        )
        .await;
        let removed = reg.unregister("c1").await;
        assert!(removed.is_some());
        assert!(!reg.has_will("c1").await);
    }

    #[tokio::test]
    async fn test_registry_get() {
        let reg = WillRegistry::new();
        let cfg = WillConfig::new(WillMessage::text("t", "p")).with_delay(10);
        reg.register("c1", cfg).await;
        let got = reg.get("c1").await;
        assert!(got.is_some());
        assert_eq!(got.unwrap().delay_seconds, 10);
    }

    #[tokio::test]
    async fn test_trigger_ungraceful_immediate_delivery() {
        let reg = WillRegistry::new();
        reg.register(
            "c1",
            WillConfig::new(WillMessage::text("status", "down")),
        )
        .await;
        let result = reg.trigger("c1", false, now_ms()).await;
        match result {
            WillDelivery::Delivered { client_id } => {
                assert_eq!(client_id, "c1");
            }
            _ => panic!("expected Delivered, got {:?}", result),
        }
        // 已被消费
        assert!(!reg.has_will("c1").await);
    }

    #[tokio::test]
    async fn test_trigger_graceful_default_skipped() {
        let reg = WillRegistry::new();
        reg.register(
            "c1",
            WillConfig::new(WillMessage::text("t", "p")),
        )
        .await;
        let result = reg.trigger("c1", true, now_ms()).await;
        assert!(matches!(result, WillDelivery::Skipped { .. }));
    }

    #[tokio::test]
    async fn test_trigger_graceful_with_publish_flag() {
        let reg = WillRegistry::new();
        reg.register(
            "c1",
            WillConfig::new(WillMessage::text("t", "p")).publish_on_graceful(true),
        )
        .await;
        let result = reg.trigger("c1", true, now_ms()).await;
        assert!(matches!(result, WillDelivery::Delivered { .. }));
    }

    #[tokio::test]
    async fn test_trigger_with_delay_schedules() {
        let reg = WillRegistry::new();
        reg.register(
            "c1",
            WillConfig::new(WillMessage::text("t", "p")).with_delay(60),
        )
        .await;
        let result = reg.trigger("c1", false, now_ms()).await;
        match result {
            WillDelivery::Scheduled {
                delay_seconds, ..
            } => assert_eq!(delay_seconds, 60),
            _ => panic!("expected Scheduled"),
        }
        assert_eq!(reg.pending_count().await, 1);
    }

    #[tokio::test]
    async fn test_trigger_no_will_returns_skipped() {
        let reg = WillRegistry::new();
        let result = reg.trigger("ghost", false, now_ms()).await;
        assert!(matches!(result, WillDelivery::Skipped { .. }));
    }

    #[tokio::test]
    async fn test_cancel_pending() {
        let reg = WillRegistry::new();
        reg.register(
            "c1",
            WillConfig::new(WillMessage::text("t", "p")).with_delay(60),
        )
        .await;
        reg.trigger("c1", false, now_ms()).await;
        assert_eq!(reg.pending_count().await, 1);
        assert!(reg.cancel_pending("c1").await);
        assert_eq!(reg.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_cancel_pending_not_present_returns_false() {
        let reg = WillRegistry::new();
        assert!(!reg.cancel_pending("nope").await);
    }

    #[tokio::test]
    async fn test_drain_due_returns_only_expired() {
        let reg = WillRegistry::new();
        let now = now_ms();
        // c1: 立即到期
        reg.register(
            "c1",
            WillConfig::new(WillMessage::text("t1", "p1")).with_delay(0),
        )
        .await;
        // c2: 60 秒后到期
        reg.register(
            "c2",
            WillConfig::new(WillMessage::text("t2", "p2")).with_delay(60),
        )
        .await;
        // 触发 c1（无延迟）-> Delivered
        reg.trigger("c1", false, now).await;
        // 触发 c2（延迟）-> Scheduled
        reg.trigger("c2", false, now).await;

        // drain_due(now)：无到期项（c1 已交付，c2 未到期）
        let due_now = reg.drain_due(now).await;
        assert_eq!(due_now.len(), 0);

        // drain_due(now + 120s)：c2 应到期
        let future = now + 120_000;
        let due_future = reg.drain_due(future).await;
        assert_eq!(due_future.len(), 1);
        assert_eq!(due_future[0].0, "c2");
        assert_eq!(due_future[0].1.text_payload(), Some("p2"));
    }

    #[tokio::test]
    async fn test_drain_due_clears_returned_entries() {
        let reg = WillRegistry::new();
        let now = now_ms();
        reg.register(
            "c1",
            WillConfig::new(WillMessage::text("t", "p")).with_delay(1),
        )
        .await;
        reg.trigger("c1", false, now).await;
        let future = now + 2_000;
        let due = reg.drain_due(future).await;
        assert_eq!(due.len(), 1);
        // 第二次调用应返回空（已被清除）
        let due2 = reg.drain_due(future).await;
        assert_eq!(due2.len(), 0);
    }

    #[tokio::test]
    async fn test_to_mqtt_messages_conversion() {
        let items = vec![
            ("c1".to_string(), WillMessage::text("t1", "p1")),
            ("c2".to_string(), WillMessage::text("t2", "p2").with_qos(QoS::AtLeastOnce)),
        ];
        let msgs = WillRegistry::to_mqtt_messages(items);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].client_id, Some("c1".to_string()));
        assert_eq!(msgs[1].qos, QoS::AtLeastOnce);
    }

    #[tokio::test]
    async fn test_trigger_consumes_will_from_registry() {
        let reg = WillRegistry::new();
        reg.register(
            "c1",
            WillConfig::new(WillMessage::text("t", "p")),
        )
        .await;
        reg.trigger("c1", false, now_ms()).await;
        // 再次触发应返回 Skipped
        let result = reg.trigger("c1", false, now_ms()).await;
        assert!(matches!(result, WillDelivery::Skipped { .. }));
    }

    #[tokio::test]
    async fn test_multiple_clients_independent_wills() {
        let reg = WillRegistry::new();
        reg.register("c1", WillConfig::new(WillMessage::text("t1", "p1"))).await;
        reg.register("c2", WillConfig::new(WillMessage::text("t2", "p2"))).await;
        reg.register("c3", WillConfig::new(WillMessage::text("t3", "p3"))).await;
        assert_eq!(reg.count().await, 3);

        let r1 = reg.trigger("c1", false, now_ms()).await;
        let r2 = reg.trigger("c2", false, now_ms()).await;
        assert!(matches!(r1, WillDelivery::Delivered { .. }));
        assert!(matches!(r2, WillDelivery::Delivered { .. }));
        assert!(reg.has_will("c3").await);
        assert!(!reg.has_will("c1").await);
    }
}
