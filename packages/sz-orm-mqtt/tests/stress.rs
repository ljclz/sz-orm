//! sz-orm-mqtt 压力测试套件
//!
//! 超大数据量验证：
//! - 10 万条消息发布
//! - 8 task × 1 万条并发 publish
//! - 1000 个订阅者
//! - retained 消息一致性
//! - 通配符匹配在海量 topic 下的性能

use std::sync::Arc;
use sz_orm_mqtt::topics::topic_matches;
use sz_orm_mqtt::{MqttConfig, MqttPlugin, QoS};

/// 辅助：构造一个已连接的 plugin
async fn make_plugin() -> MqttPlugin {
    let mut plugin = MqttPlugin::new(MqttConfig::default());
    plugin.connect().await.unwrap();
    plugin
}

/// 验证：10 万条消息发布 + 计数准确
#[tokio::test]
async fn stress_mqtt_100k_messages() {
    let plugin = make_plugin().await;
    let total: u64 = 100_000;

    for i in 0..total {
        let payload = format!("msg-{}", i);
        plugin
            .publish("bulk/topic", payload.into_bytes(), QoS::AtLeastOnce)
            .await
            .unwrap();
    }
    assert_eq!(plugin.message_count().await, total as usize);
}

/// 验证：8 task × 1 万条并发 publish（合计 8 万条）
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn stress_mqtt_concurrent_publish() {
    let plugin = Arc::new(make_plugin().await);
    let per_task: u64 = 10_000;
    let task_count: u64 = 8;
    let total = per_task * task_count;

    let mut handles = Vec::new();
    for task_id in 0..task_count {
        let p = plugin.clone();
        handles.push(tokio::spawn(async move {
            let topic = format!("task/{}/data", task_id);
            for i in 0..per_task {
                let payload = format!("t{}-m{}", task_id, i);
                p.publish(&topic, payload.into_bytes(), QoS::AtMostOnce)
                    .await
                    .unwrap();
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    assert_eq!(plugin.message_count().await, total as usize);
}

/// 验证：1000 个不同 topic 订阅 + unsubscribe 一致性
#[tokio::test]
async fn stress_mqtt_1000_subscribers() {
    let plugin = make_plugin().await;
    let n: usize = 1000;
    for i in 0..n {
        plugin
            .subscribe(&format!("topic-{}", i), QoS::AtLeastOnce)
            .await
            .unwrap();
    }
    assert_eq!(plugin.subscription_count().await, n);

    // unsubscribe 一半
    for i in 0..n / 2 {
        plugin.unsubscribe(&format!("topic-{}", i)).await.unwrap();
    }
    assert_eq!(plugin.subscription_count().await, n / 2);
}

/// 验证：同一 topic 多次 subscribe 是幂等的（更新 QoS，不增加计数）
#[tokio::test]
async fn stress_mqtt_subscribe_same_topic_idempotent() {
    let plugin = make_plugin().await;
    for _ in 0..1000 {
        plugin
            .subscribe("same/topic", QoS::AtLeastOnce)
            .await
            .unwrap();
    }
    assert_eq!(
        plugin.subscription_count().await,
        1,
        "duplicate subscribe must be idempotent"
    );
}

/// 验证：retained 消息在大量 publish 下唯一
#[tokio::test]
async fn stress_mqtt_retained_uniqueness() {
    let plugin = make_plugin().await;
    let n: u64 = 10_000;
    for i in 0..n {
        let topic = format!("config/{}", i % 100); // 100 个不同 topic
        plugin
            .publish_retain(&topic, format!("v{}", i).into_bytes(), QoS::AtLeastOnce)
            .await
            .unwrap();
    }
    // retained 应该只有 100 个（每个 topic 最后一个值覆盖）
    assert_eq!(plugin.retained_count().await, 100);
}

/// 验证：通配符匹配在海量 topic 下准确
#[tokio::test]
async fn stress_mqtt_wildcard_matching() {
    let plugin = make_plugin().await;
    // 发布到 1000 个不同 topic
    for i in 0..1000u64 {
        let topic = format!("home/room{}/temp", i);
        plugin
            .publish(&topic, b"23.5".to_vec(), QoS::AtMostOnce)
            .await
            .unwrap();
    }
    // 通配符查询
    let matched = plugin.messages_matching("home/+/temp").await;
    assert_eq!(matched.len(), 1000);

    let matched_all = plugin.messages_matching("home/#").await;
    assert_eq!(matched_all.len(), 1000);

    let matched_none = plugin.messages_matching("office/#").await;
    assert_eq!(matched_none.len(), 0);
}

/// 验证：大 payload（1MB）× 100 条
#[tokio::test]
async fn stress_mqtt_large_payload() {
    let plugin = make_plugin().await;
    let payload = vec![0xCDu8; 1_000_000];
    for _ in 0..100 {
        plugin
            .publish("large/topic", payload.clone(), QoS::AtMostOnce)
            .await
            .unwrap();
    }
    assert_eq!(plugin.message_count().await, 100);
}

/// 验证：topic_matches 函数在大量调用下一致
#[test]
fn stress_mqtt_topic_matches_function() {
    let topics: Vec<String> = (0..10_000)
        .map(|i| format!("home/room{}/temp", i))
        .collect();
    let mut match_count = 0usize;
    for t in &topics {
        if topic_matches(t, "home/+/temp") {
            match_count += 1;
        }
    }
    assert_eq!(match_count, 10_000);

    // 不匹配
    for t in &topics {
        assert!(!topic_matches(t, "office/#"));
    }
}

/// 验证：未连接时所有操作返回错误
#[tokio::test]
async fn stress_mqtt_not_connected_consistency() {
    let plugin = MqttPlugin::new(MqttConfig::default());
    for i in 0..100 {
        let result = plugin
            .publish(&format!("t/{}", i), vec![], QoS::AtMostOnce)
            .await;
        assert!(
            result.is_err(),
            "publish must fail when disconnected at iter {}",
            i
        );
    }
    for i in 0..100 {
        let result = plugin.subscribe(&format!("t/{}", i), QoS::AtMostOnce).await;
        assert!(
            result.is_err(),
            "subscribe must fail when disconnected at iter {}",
            i
        );
    }
}

/// 验证：disconnect 后 reconnect 可以继续使用
#[tokio::test]
async fn stress_mqtt_disconnect_reconnect_cycle() {
    let mut plugin = MqttPlugin::new(MqttConfig::default());
    for cycle in 0..10 {
        plugin.connect().await.unwrap();
        for i in 0..100 {
            plugin
                .publish(
                    &format!("cycle/{}/{}", cycle, i),
                    b"data".to_vec(),
                    QoS::AtMostOnce,
                )
                .await
                .unwrap();
        }
        plugin.disconnect().await.unwrap();
        // disconnect 后 publish 失败
        let result = plugin.publish("fail/topic", vec![], QoS::AtMostOnce).await;
        assert!(result.is_err());
    }
    // 累积 10 × 100 = 1000 条
    plugin.connect().await.unwrap();
    assert_eq!(plugin.message_count().await, 1000);
}
