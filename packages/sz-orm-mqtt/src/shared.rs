//! # 共享订阅（Shared Subscriptions）
//!
//! 实现 MQTT 5.0 共享订阅功能：多个客户端以同一组名订阅同一过滤器时，
//! broker 将匹配的消息在该组内负载均衡分发（而非广播给每个订阅者）。
//!
//! 共享订阅语法：`$share/{group_name}/{topic_filter}`
//!
//! ## 主要类型
//!
//! - [`SharedSubscription`] — 共享订阅条目
//! - [`SharedSubscriber`] — 组内订阅者
//! - [`SharedSubscriptionRegistry`] — 共享订阅注册表
//! - [`LoadBalanceStrategy`] — 负载均衡策略

use crate::broker::QoS;
use crate::topics::topic_matches;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 共享订阅前缀
pub const SHARED_PREFIX: &str = "$share/";

/// 负载均衡策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadBalanceStrategy {
    /// 轮询（Round Robin）—— 默认
    #[default]
    RoundRobin,
    /// 随机
    Random,
    /// 最少未处理（选择当前未确认消息最少的订阅者）
    LeastPending,
}

/// 共享订阅中的单个订阅者
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedSubscriber {
    /// 客户端 ID
    pub client_id: String,
    /// 订阅 QoS
    pub qos: QoS,
}

impl SharedSubscriber {
    pub fn new(client_id: impl Into<String>, qos: QoS) -> Self {
        Self {
            client_id: client_id.into(),
            qos,
        }
    }
}

/// 共享订阅组
#[derive(Debug, Clone)]
pub struct SharedSubscription {
    /// 组名
    pub group_name: String,
    /// 主题过滤器（不含 `$share/{group}/` 前缀）
    pub topic_filter: String,
    /// 组内订阅者列表
    pub subscribers: Vec<SharedSubscriber>,
    /// 负载均衡策略
    pub strategy: LoadBalanceStrategy,
    /// 轮询索引（仅 RoundRobin 使用）
    pub rr_index: usize,
}

impl SharedSubscription {
    pub fn new(
        group_name: impl Into<String>,
        topic_filter: impl Into<String>,
    ) -> Self {
        Self {
            group_name: group_name.into(),
            topic_filter: topic_filter.into(),
            subscribers: Vec::new(),
            strategy: LoadBalanceStrategy::default(),
            rr_index: 0,
        }
    }

    /// 设置负载均衡策略
    pub fn with_strategy(mut self, strategy: LoadBalanceStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// 添加订阅者
    pub fn add_subscriber(&mut self, subscriber: SharedSubscriber) -> bool {
        if self.subscribers.iter().any(|s| s.client_id == subscriber.client_id) {
            return false; // 已存在
        }
        self.subscribers.push(subscriber);
        true
    }

    /// 移除订阅者
    pub fn remove_subscriber(&mut self, client_id: &str) -> bool {
        let before = self.subscribers.len();
        self.subscribers.retain(|s| s.client_id != client_id);
        self.subscribers.len() != before
    }

    /// 更新订阅者 QoS（若已存在则更新，不存在则添加）
    pub fn upsert_subscriber(&mut self, client_id: &str, qos: QoS) {
        if let Some(s) = self.subscribers.iter_mut().find(|s| s.client_id == client_id) {
            s.qos = qos;
        } else {
            self.subscribers.push(SharedSubscriber::new(client_id, qos));
        }
    }

    /// 当前订阅者数量
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }

    /// 是否包含某订阅者
    pub fn contains(&self, client_id: &str) -> bool {
        self.subscribers.iter().any(|s| s.client_id == client_id)
    }

    /// 主题是否匹配过滤器
    pub fn matches(&self, topic: &str) -> bool {
        topic_matches(topic, &self.topic_filter)
    }

    /// 选择下一个接收消息的订阅者（按策略）
    /// 返回选中的订阅者索引；组为空时返回 None
    pub fn select_next(&mut self) -> Option<usize> {
        if self.subscribers.is_empty() {
            return None;
        }
        let idx = match self.strategy {
            LoadBalanceStrategy::RoundRobin => {
                let i = self.rr_index % self.subscribers.len();
                self.rr_index = self.rr_index.wrapping_add(1);
                i
            }
            LoadBalanceStrategy::Random => {
                // 简单确定性伪随机：基于 subscriber_count 与 rr_index
                // 不引入 rand 依赖
                let seed = self.rr_index.wrapping_add(self.subscribers.len());
                seed % self.subscribers.len()
            }
            LoadBalanceStrategy::LeastPending => {
                // 无 pending 跟踪时退化为 RoundRobin
                let i = self.rr_index % self.subscribers.len();
                self.rr_index = self.rr_index.wrapping_add(1);
                i
            }
        };
        Some(idx)
    }
}

/// 解析共享订阅主题过滤器。
/// 输入形如 `$share/group_name/topic/filter`，返回 `(group_name, topic_filter)`。
/// 若输入不以 `$share/` 开头，返回 None。
pub fn parse_shared_filter(filter: &str) -> Option<(&str, &str)> {
    let rest = filter.strip_prefix(SHARED_PREFIX)?;
    let slash = rest.find('/')?;
    let group = &rest[..slash];
    let topic = &rest[slash + 1..];
    if group.is_empty() || topic.is_empty() {
        return None;
    }
    Some((group, topic))
}

/// 判断是否为共享订阅过滤器
pub fn is_shared_filter(filter: &str) -> bool {
    parse_shared_filter(filter).is_some()
}

/// 共享订阅注册表
#[derive(Debug, Default)]
pub struct SharedSubscriptionRegistry {
    /// key: "{group_name}/{topic_filter}"
    subscriptions: Arc<RwLock<HashMap<String, SharedSubscription>>>,
}

impl SharedSubscriptionRegistry {
    pub fn new() -> Self {
        Self {
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn key(group: &str, filter: &str) -> String {
        format!("{}/{}", group, filter)
    }

    /// 注册共享订阅（若组不存在则创建，存在则添加订阅者）
    pub async fn subscribe(
        &self,
        group: &str,
        filter: &str,
        client_id: &str,
        qos: QoS,
    ) -> Result<(), String> {
        // 校验过滤器合法性（复用 TopicFilter 校验逻辑）
        let _ = crate::topics::TopicFilter::new(filter)
            .map_err(|e| format!("invalid topic filter: {}", e))?;

        let mut subs = self.subscriptions.write().await;
        let key = Self::key(group, filter);
        let entry = subs.entry(key).or_insert_with(|| {
            SharedSubscription::new(group, filter)
        });
        entry.upsert_subscriber(client_id, qos);
        Ok(())
    }

    /// 取消某客户端在某组+过滤器下的订阅
    /// 返回是否成功移除（false 表示订阅者不存在）
    pub async fn unsubscribe(
        &self,
        group: &str,
        filter: &str,
        client_id: &str,
    ) -> bool {
        let mut subs = self.subscriptions.write().await;
        let key = Self::key(group, filter);
        if let Some(sub) = subs.get_mut(&key) {
            let removed = sub.remove_subscriber(client_id);
            // 若组已空，移除整个条目
            if sub.subscriber_count() == 0 {
                subs.remove(&key);
            }
            return removed;
        }
        false
    }

    /// 移除某客户端在所有共享订阅组中的订阅（用于客户端断开连接）
    /// 返回被移除的组数量
    pub async fn unsubscribe_all(&self, client_id: &str) -> usize {
        let mut subs = self.subscriptions.write().await;
        let mut removed_groups = 0;
        let mut empty_keys = Vec::new();
        for sub in subs.values_mut() {
            if sub.remove_subscriber(client_id) {
                removed_groups += 1;
            }
            if sub.subscriber_count() == 0 {
                empty_keys.push(Self::key(&sub.group_name, &sub.topic_filter));
            }
        }
        for key in empty_keys {
            subs.remove(&key);
        }
        removed_groups
    }

    /// 查询某组+过滤器下的订阅者数量
    pub async fn subscriber_count(&self, group: &str, filter: &str) -> usize {
        let subs = self.subscriptions.read().await;
        let key = Self::key(group, filter);
        subs.get(&key).map(|s| s.subscriber_count()).unwrap_or(0)
    }

    /// 查询所有匹配指定主题的共享订阅组（用于消息分发）
    /// 返回 (group_name, topic_filter, subscriber_count) 列表
    pub async fn groups_matching(&self, topic: &str) -> Vec<(String, String, usize)> {
        let subs = self.subscriptions.read().await;
        let mut result: Vec<(String, String, usize)> = subs
            .values()
            .filter(|s| s.matches(topic))
            .map(|s| (s.group_name.clone(), s.topic_filter.clone(), s.subscriber_count()))
            .collect();
        result.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        result
    }

    /// 为消息选择接收者：对每个匹配的共享订阅组，按策略选一个订阅者。
    /// 返回 (client_id, qos) 列表。
    pub async fn select_recipients(&self, topic: &str) -> Vec<(String, QoS)> {
        let mut subs = self.subscriptions.write().await;
        let mut recipients = Vec::new();
        // 收集匹配的 key（避免持有可变借用同时遍历）
        let matching_keys: Vec<String> = subs
            .iter()
            .filter(|(_, s)| s.matches(topic))
            .map(|(k, _)| k.clone())
            .collect();
        for key in matching_keys {
            if let Some(sub) = subs.get_mut(&key) {
                if let Some(idx) = sub.select_next() {
                    let s = &sub.subscribers[idx];
                    recipients.push((s.client_id.clone(), s.qos));
                }
            }
        }
        recipients.sort_by(|a, b| a.0.cmp(&b.0));
        recipients
    }

    /// 当前注册的共享订阅组数量
    pub async fn group_count(&self) -> usize {
        let subs = self.subscriptions.read().await;
        subs.len()
    }

    /// 列出所有组信息
    pub async fn list_groups(&self) -> Vec<(String, String, usize)> {
        let subs = self.subscriptions.read().await;
        let mut result: Vec<(String, String, usize)> = subs
            .values()
            .map(|s| (s.group_name.clone(), s.topic_filter.clone(), s.subscriber_count()))
            .collect();
        result.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_shared_filter_valid() {
        let (group, filter) = parse_shared_filter("$share/g1/home/+/temp").unwrap();
        assert_eq!(group, "g1");
        assert_eq!(filter, "home/+/temp");
    }

    #[test]
    fn test_parse_shared_filter_no_prefix() {
        assert!(parse_shared_filter("home/temp").is_none());
    }

    #[test]
    fn test_parse_shared_filter_empty_group() {
        assert!(parse_shared_filter("$share//home/temp").is_none());
    }

    #[test]
    fn test_parse_shared_filter_empty_topic() {
        assert!(parse_shared_filter("$share/g1/").is_none());
    }

    #[test]
    fn test_is_shared_filter() {
        assert!(is_shared_filter("$share/g/home/#"));
        assert!(!is_shared_filter("home/#"));
    }

    #[test]
    fn test_shared_subscriber_new() {
        let s = SharedSubscriber::new("c1", QoS::AtLeastOnce);
        assert_eq!(s.client_id, "c1");
        assert_eq!(s.qos, QoS::AtLeastOnce);
    }

    #[test]
    fn test_shared_subscription_new() {
        let sub = SharedSubscription::new("g1", "home/#");
        assert_eq!(sub.group_name, "g1");
        assert_eq!(sub.topic_filter, "home/#");
        assert_eq!(sub.strategy, LoadBalanceStrategy::RoundRobin);
        assert_eq!(sub.subscriber_count(), 0);
    }

    #[test]
    fn test_shared_subscription_add_subscriber() {
        let mut sub = SharedSubscription::new("g1", "t");
        assert!(sub.add_subscriber(SharedSubscriber::new("c1", QoS::AtMostOnce)));
        assert!(sub.add_subscriber(SharedSubscriber::new("c2", QoS::AtLeastOnce)));
        assert_eq!(sub.subscriber_count(), 2);
        assert!(sub.contains("c1"));
        assert!(sub.contains("c2"));
    }

    #[test]
    fn test_shared_subscription_add_duplicate_returns_false() {
        let mut sub = SharedSubscription::new("g1", "t");
        sub.add_subscriber(SharedSubscriber::new("c1", QoS::AtMostOnce));
        assert!(!sub.add_subscriber(SharedSubscriber::new("c1", QoS::AtLeastOnce)));
        assert_eq!(sub.subscriber_count(), 1);
    }

    #[test]
    fn test_shared_subscription_remove_subscriber() {
        let mut sub = SharedSubscription::new("g1", "t");
        sub.add_subscriber(SharedSubscriber::new("c1", QoS::AtMostOnce));
        assert!(sub.remove_subscriber("c1"));
        assert!(!sub.contains("c1"));
        assert_eq!(sub.subscriber_count(), 0);
    }

    #[test]
    fn test_shared_subscription_remove_missing_returns_false() {
        let mut sub = SharedSubscription::new("g1", "t");
        assert!(!sub.remove_subscriber("ghost"));
    }

    #[test]
    fn test_shared_subscription_upsert_updates_qos() {
        let mut sub = SharedSubscription::new("g1", "t");
        sub.upsert_subscriber("c1", QoS::AtMostOnce);
        sub.upsert_subscriber("c1", QoS::ExactlyOnce); // 更新
        assert_eq!(sub.subscriber_count(), 1);
        assert_eq!(sub.subscribers[0].qos, QoS::ExactlyOnce);
    }

    #[test]
    fn test_shared_subscription_upsert_adds_new() {
        let mut sub = SharedSubscription::new("g1", "t");
        sub.upsert_subscriber("c1", QoS::AtMostOnce);
        sub.upsert_subscriber("c2", QoS::AtLeastOnce);
        assert_eq!(sub.subscriber_count(), 2);
    }

    #[test]
    fn test_shared_subscription_matches() {
        let sub = SharedSubscription::new("g1", "home/+/temp");
        assert!(sub.matches("home/living/temp"));
        assert!(!sub.matches("office/temp"));
    }

    #[test]
    fn test_select_next_empty_returns_none() {
        let mut sub = SharedSubscription::new("g1", "t");
        assert!(sub.select_next().is_none());
    }

    #[test]
    fn test_select_next_round_robin_cycles() {
        let mut sub = SharedSubscription::new("g1", "t");
        sub.add_subscriber(SharedSubscriber::new("c1", QoS::AtMostOnce));
        sub.add_subscriber(SharedSubscriber::new("c2", QoS::AtMostOnce));
        sub.add_subscriber(SharedSubscriber::new("c3", QoS::AtMostOnce));

        let i1 = sub.select_next().unwrap();
        let i2 = sub.select_next().unwrap();
        let i3 = sub.select_next().unwrap();
        let i4 = sub.select_next().unwrap();
        assert_eq!(i1, 0);
        assert_eq!(i2, 1);
        assert_eq!(i3, 2);
        assert_eq!(i4, 0); // 回到 c1
    }

    #[test]
    fn test_select_next_random_returns_valid_index() {
        let mut sub = SharedSubscription::new("g1", "t")
            .with_strategy(LoadBalanceStrategy::Random);
        sub.add_subscriber(SharedSubscriber::new("c1", QoS::AtMostOnce));
        sub.add_subscriber(SharedSubscriber::new("c2", QoS::AtMostOnce));
        let idx = sub.select_next().unwrap();
        assert!(idx < 2);
    }

    #[test]
    fn test_with_strategy() {
        let sub = SharedSubscription::new("g1", "t")
            .with_strategy(LoadBalanceStrategy::Random);
        assert_eq!(sub.strategy, LoadBalanceStrategy::Random);
    }

    #[tokio::test]
    async fn test_registry_subscribe_creates_group() {
        let reg = SharedSubscriptionRegistry::new();
        reg.subscribe("g1", "home/#", "c1", QoS::AtMostOnce).await.unwrap();
        assert_eq!(reg.group_count().await, 1);
        assert_eq!(reg.subscriber_count("g1", "home/#").await, 1);
    }

    #[tokio::test]
    async fn test_registry_subscribe_adds_to_existing_group() {
        let reg = SharedSubscriptionRegistry::new();
        reg.subscribe("g1", "home/#", "c1", QoS::AtMostOnce).await.unwrap();
        reg.subscribe("g1", "home/#", "c2", QoS::AtLeastOnce).await.unwrap();
        assert_eq!(reg.subscriber_count("g1", "home/#").await, 2);
        assert_eq!(reg.group_count().await, 1);
    }

    #[tokio::test]
    async fn test_registry_subscribe_invalid_filter_fails() {
        let reg = SharedSubscriptionRegistry::new();
        let result = reg.subscribe("g1", "", "c1", QoS::AtMostOnce).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_subscribe_updates_qos_for_existing() {
        let reg = SharedSubscriptionRegistry::new();
        reg.subscribe("g1", "t", "c1", QoS::AtMostOnce).await.unwrap();
        reg.subscribe("g1", "t", "c1", QoS::ExactlyOnce).await.unwrap();
        assert_eq!(reg.subscriber_count("g1", "t").await, 1);
        // 验证 QoS 已更新（通过 select_recipients）
        let recipients = reg.select_recipients("t").await;
        assert_eq!(recipients.len(), 1);
        assert_eq!(recipients[0].1, QoS::ExactlyOnce);
    }

    #[tokio::test]
    async fn test_registry_unsubscribe_removes_subscriber() {
        let reg = SharedSubscriptionRegistry::new();
        reg.subscribe("g1", "t", "c1", QoS::AtMostOnce).await.unwrap();
        reg.subscribe("g1", "t", "c2", QoS::AtMostOnce).await.unwrap();
        assert!(reg.unsubscribe("g1", "t", "c1").await);
        assert_eq!(reg.subscriber_count("g1", "t").await, 1);
    }

    #[tokio::test]
    async fn test_registry_unsubscribe_removes_empty_group() {
        let reg = SharedSubscriptionRegistry::new();
        reg.subscribe("g1", "t", "c1", QoS::AtMostOnce).await.unwrap();
        assert!(reg.unsubscribe("g1", "t", "c1").await);
        assert_eq!(reg.group_count().await, 0);
    }

    #[tokio::test]
    async fn test_registry_unsubscribe_missing_returns_false() {
        let reg = SharedSubscriptionRegistry::new();
        assert!(!reg.unsubscribe("g1", "t", "c1").await);
    }

    #[tokio::test]
    async fn test_registry_unsubscribe_all() {
        let reg = SharedSubscriptionRegistry::new();
        reg.subscribe("g1", "t1", "c1", QoS::AtMostOnce).await.unwrap();
        reg.subscribe("g2", "t2", "c1", QoS::AtMostOnce).await.unwrap();
        reg.subscribe("g1", "t1", "c2", QoS::AtMostOnce).await.unwrap();
        let removed = reg.unsubscribe_all("c1").await;
        assert_eq!(removed, 2);
        assert_eq!(reg.group_count().await, 1); // g1/t1 仍有 c2
    }

    #[tokio::test]
    async fn test_registry_groups_matching() {
        let reg = SharedSubscriptionRegistry::new();
        reg.subscribe("g1", "home/#", "c1", QoS::AtMostOnce).await.unwrap();
        reg.subscribe("g2", "office/#", "c2", QoS::AtMostOnce).await.unwrap();
        reg.subscribe("g3", "home/+/temp", "c3", QoS::AtMostOnce).await.unwrap();

        let matched = reg.groups_matching("home/living/temp").await;
        assert_eq!(matched.len(), 2);
        // 排序后 g1 在前
        assert_eq!(matched[0].0, "g1");
        assert_eq!(matched[1].0, "g3");
    }

    #[tokio::test]
    async fn test_registry_groups_matching_none() {
        let reg = SharedSubscriptionRegistry::new();
        reg.subscribe("g1", "home/#", "c1", QoS::AtMostOnce).await.unwrap();
        let matched = reg.groups_matching("office/temp").await;
        assert!(matched.is_empty());
    }

    #[tokio::test]
    async fn test_registry_select_recipients_round_robin() {
        let reg = SharedSubscriptionRegistry::new();
        reg.subscribe("g1", "t", "c1", QoS::AtMostOnce).await.unwrap();
        reg.subscribe("g1", "t", "c2", QoS::AtMostOnce).await.unwrap();
        reg.subscribe("g1", "t", "c3", QoS::AtMostOnce).await.unwrap();

        let r1 = reg.select_recipients("t").await;
        let r2 = reg.select_recipients("t").await;
        let r3 = reg.select_recipients("t").await;
        // 每次应只选一个订阅者（单组）
        assert_eq!(r1.len(), 1);
        assert_eq!(r2.len(), 1);
        assert_eq!(r3.len(), 1);
        // 三次应覆盖三个不同订阅者
        let mut clients: Vec<String> = vec![r1[0].0.clone(), r2[0].0.clone(), r3[0].0.clone()];
        clients.sort();
        assert_eq!(clients, vec!["c1", "c2", "c3"]);
    }

    #[tokio::test]
    async fn test_registry_select_recipients_multiple_groups() {
        let reg = SharedSubscriptionRegistry::new();
        reg.subscribe("g1", "t", "c1", QoS::AtMostOnce).await.unwrap();
        reg.subscribe("g2", "t", "c2", QoS::AtLeastOnce).await.unwrap();

        let recipients = reg.select_recipients("t").await;
        assert_eq!(recipients.len(), 2); // 每组各选一个
    }

    #[tokio::test]
    async fn test_registry_select_recipients_no_match() {
        let reg = SharedSubscriptionRegistry::new();
        reg.subscribe("g1", "home/#", "c1", QoS::AtMostOnce).await.unwrap();
        let recipients = reg.select_recipients("office/temp").await;
        assert!(recipients.is_empty());
    }

    #[tokio::test]
    async fn test_registry_list_groups_sorted() {
        let reg = SharedSubscriptionRegistry::new();
        reg.subscribe("g2", "t2", "c1", QoS::AtMostOnce).await.unwrap();
        reg.subscribe("g1", "t1", "c2", QoS::AtMostOnce).await.unwrap();
        let groups = reg.list_groups().await;
        assert_eq!(groups[0].0, "g1");
        assert_eq!(groups[1].0, "g2");
    }

    #[tokio::test]
    async fn test_registry_multiple_filters_same_group() {
        let reg = SharedSubscriptionRegistry::new();
        reg.subscribe("g1", "home/#", "c1", QoS::AtMostOnce).await.unwrap();
        reg.subscribe("g1", "office/#", "c1", QoS::AtMostOnce).await.unwrap();
        // 同组不同过滤器 -> 两个条目
        assert_eq!(reg.group_count().await, 2);
        let removed = reg.unsubscribe_all("c1").await;
        assert_eq!(removed, 2);
        assert_eq!(reg.group_count().await, 0);
    }
}
