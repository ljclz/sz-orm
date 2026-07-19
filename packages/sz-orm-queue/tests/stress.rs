//! sz-orm-queue 压力测试套件
//!
//! 超大数据量验证：
//! - 10 万条消息并发发布/消费
//! - 8 task × 1 万条消息混合工作负载
//! - 验证消息不丢失、不重复、FIFO 顺序、in-flight 一致性

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use sz_orm_queue::{
    ActiveConfig, KafkaConfig, NatsConfig, PulsarConfig, RabbitConfig, RocketConfig,
};
use sz_orm_queue::{InMemoryQueue, MessageQueue, MqProvider, QueueWrapper};

/// 辅助：超大消息量（10 万条）单线程 publish/consume/ack
#[tokio::test]
async fn stress_queue_100k_messages_single_thread() {
    let queue = InMemoryQueue::new();
    let total: u64 = 100_000;

    // 发布 10 万条
    for i in 0..total {
        let payload = format!("msg-{}", i);
        queue
            .publish("bulk-topic", payload.as_bytes())
            .await
            .unwrap();
    }
    assert_eq!(queue.message_count("bulk-topic").await, total as usize);

    // 消费 10 万条 + 立即 ack，验证 FIFO 与无丢失
    for i in 0..total {
        let msg = queue
            .consume("bulk-topic")
            .await
            .unwrap()
            .expect("msg must exist");
        let expected = format!("msg-{}", i);
        assert_eq!(
            msg.payload,
            expected.as_bytes(),
            "FIFO broken at index {}",
            i
        );
        queue.ack(&msg.id).await.unwrap();
    }
    assert_eq!(queue.message_count("bulk-topic").await, 0);
    assert_eq!(
        queue.in_flight_count().await,
        0,
        "in_flight must be 0 after ack"
    );
}

/// 辅助：8 task 并发 publish/consume（每个 1 万条，合计 8 万条）
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn stress_queue_concurrent_publish_consume() {
    let queue = Arc::new(InMemoryQueue::new());
    let total_per_task: u64 = 10_000;
    let task_count: u64 = 8;
    let total = total_per_task * task_count;

    let mut handles = Vec::new();
    for task_id in 0..task_count {
        let q = queue.clone();
        handles.push(tokio::spawn(async move {
            let topic = format!("task-{}", task_id);
            for i in 0..total_per_task {
                let payload = format!("t{}-m{}", task_id, i);
                q.publish(&topic, payload.as_bytes()).await.unwrap();
            }
            // 立即消费并 ack
            for i in 0..total_per_task {
                let msg = q.consume(&topic).await.unwrap().expect("msg must exist");
                let expected = format!("t{}-m{}", task_id, i);
                assert_eq!(
                    msg.payload,
                    expected.as_bytes(),
                    "FIFO broken in task {}",
                    task_id
                );
                q.ack(&msg.id).await.unwrap();
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // 所有 task 完成后，所有 topic 应为空，in_flight 也应为空
    for task_id in 0..task_count {
        let topic = format!("task-{}", task_id);
        assert_eq!(
            queue.message_count(&topic).await,
            0,
            "topic {} not drained",
            topic
        );
    }
    assert_eq!(
        queue.in_flight_count().await,
        0,
        "in_flight must be 0 after ack"
    );
    let _ = total;
}

/// 验证：大量 topic（1000 个）下 message_count 准确
#[tokio::test]
async fn stress_queue_many_topics() {
    let queue = InMemoryQueue::new();
    let topic_count: u64 = 1000;
    let per_topic: u64 = 100;

    for t in 0..topic_count {
        let topic = format!("topic-{}", t);
        for i in 0..per_topic {
            queue
                .publish(&topic, format!("m{}", i).as_bytes())
                .await
                .unwrap();
        }
    }

    for t in 0..topic_count {
        let topic = format!("topic-{}", t);
        assert_eq!(
            queue.message_count(&topic).await,
            per_topic as usize,
            "topic {} count mismatch",
            topic
        );
    }
}

/// 验证：超大 payload（1MB）× 1000 条
#[tokio::test]
async fn stress_queue_large_payload() {
    let queue = InMemoryQueue::new();
    let payload_size: usize = 1_000_000; // 1 MB
    let count: usize = 1000;
    let payload = vec![0xABu8; payload_size];

    for _ in 0..count {
        queue.publish("large", &payload).await.unwrap();
    }
    assert_eq!(queue.message_count("large").await, count);

    for _ in 0..count {
        let msg = queue.consume("large").await.unwrap().unwrap();
        assert_eq!(msg.payload.len(), payload_size);
        queue.ack(&msg.id).await.unwrap();
    }
    assert_eq!(queue.in_flight_count().await, 0);
}

/// 验证：6 种 provider wrapper 在大并发下一致性
#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn stress_queue_all_providers_concurrent() {
    let providers = vec![
        MqProvider::Kafka(KafkaConfig::default()),
        MqProvider::RabbitMQ(RabbitConfig::default()),
        MqProvider::RocketMQ(RocketConfig::default()),
        MqProvider::ActiveMQ(ActiveConfig::default()),
        MqProvider::Nats(NatsConfig::default()),
        MqProvider::Pulsar(PulsarConfig::default()),
    ];

    let success = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();
    for provider in providers {
        let s = success.clone();
        handles.push(tokio::spawn(async move {
            let wrapper = QueueWrapper::new(provider);
            for i in 0..1000 {
                let payload = format!("msg-{}", i);
                wrapper.publish("test", payload.as_bytes()).await.unwrap();
                let msg = wrapper.consume("test").await.unwrap().expect("msg");
                assert_eq!(msg.payload, payload.as_bytes());
                wrapper.ack(&msg.id).await.unwrap();
                s.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    assert_eq!(success.load(Ordering::Relaxed), 6000);
}

/// 验证：订阅者计数在大量 subscribe 下准确
#[tokio::test]
async fn stress_queue_subscriber_count() {
    let queue = InMemoryQueue::new();
    let n: usize = 10_000;
    for _ in 0..n {
        queue.subscribe("hot-topic").await.unwrap();
    }
    assert_eq!(queue.subscriber_count("hot-topic").await, n);
}

/// 验证：消费空 topic 不会 panic
#[tokio::test]
async fn stress_queue_consume_empty_repeated() {
    let queue = InMemoryQueue::new();
    for _ in 0..10_000 {
        let result = queue.consume("never-published").await.unwrap();
        assert!(result.is_none());
    }
}

/// 验证：ack 未知 id 始终返回错误
#[tokio::test]
async fn stress_queue_ack_unknown_repeated() {
    let queue = InMemoryQueue::new();
    for i in 0..1000 {
        let result = queue.ack(&format!("unknown-{}", i)).await;
        assert!(result.is_err(), "ack unknown must fail at iter {}", i);
    }
}
