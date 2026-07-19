//! sz-orm-websocket 压力测试套件
//!
//! 超大数据量验证：
//! - 1 万个连接注册
//! - 8 task × 1000 次房间推送
//! - 大房间（1000 个连接）推送一致性
//! - broadcast 到 1 万个连接

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use sz_orm_websocket::{InMemorySender, RealtimePusher};

/// 验证：1 万个连接注册 + 推送
#[tokio::test]
async fn stress_ws_10k_connections() {
    let pusher = RealtimePusher::new();
    let n: u64 = 10_000;

    for i in 0..n {
        pusher.register_connection(format!("conn-{}", i)).await;
    }
    assert_eq!(pusher.connection_count().await, n as usize);

    // 推送到每个连接
    for i in 0..n {
        pusher
            .push_to_connection(&format!("conn-{}", i), b"hello".to_vec())
            .await
            .unwrap();
    }
    for i in 0..n {
        let count = pusher.message_count(&format!("conn-{}", i)).await;
        assert_eq!(count, 1, "conn-{} should have 1 message", i);
    }
}

/// 验证：大房间（1000 个连接）推送一致性
#[tokio::test]
async fn stress_ws_large_room_push() {
    let pusher = Arc::new(RealtimePusher::new());
    let room_size: usize = 1000;

    for i in 0..room_size {
        pusher.register_connection(format!("conn-{}", i)).await;
        pusher
            .subscribe(&format!("conn-{}", i), "broadcast-room")
            .await
            .unwrap();
    }
    assert_eq!(pusher.room_count("broadcast-room").await, room_size);

    // 推送到房间
    let success = pusher
        .push_to_room("broadcast-room", b"room-msg".to_vec())
        .await
        .unwrap();
    assert_eq!(success, room_size);

    // 验证每个连接都收到
    for i in 0..room_size {
        let count = pusher.message_count(&format!("conn-{}", i)).await;
        assert_eq!(count, 1, "conn-{} did not receive room msg", i);
    }
}

/// 验证：8 task 并发 broadcast（每个 1000 次，合计 8000 次）
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn stress_ws_concurrent_broadcast() {
    let pusher = Arc::new(RealtimePusher::new());
    let conn_count: usize = 100;
    for i in 0..conn_count {
        pusher.register_connection(format!("conn-{}", i)).await;
    }

    let total = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();
    for task_id in 0..8 {
        let p = pusher.clone();
        let t = total.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..1000u64 {
                let msg = format!("t{}-m{}", task_id, i).into_bytes();
                let success = p.broadcast(msg).await.unwrap();
                assert_eq!(success, 100, "broadcast must reach all conns");
                t.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    assert_eq!(total.load(Ordering::Relaxed), 8000);
    // 每个连接应该收到 8000 条
    for i in 0..conn_count {
        let count = pusher.message_count(&format!("conn-{}", i)).await;
        assert_eq!(count, 8000, "conn-{} should have 8000 msgs", i);
    }
}

/// 验证：push_to_user 一致性
#[tokio::test]
async fn stress_ws_push_to_user() {
    let pusher = RealtimePusher::new();
    let user_id: i64 = 42;
    let conn_per_user: usize = 100;

    for i in 0..conn_per_user {
        pusher
            .register_connection_with_user(format!("conn-{}", i), user_id)
            .await;
    }

    // 推送到用户
    for i in 0..100 {
        let success = pusher
            .push_to_user(user_id, format!("msg-{}", i).into_bytes())
            .await
            .unwrap();
        assert_eq!(
            success, conn_per_user,
            "push_to_user must reach all conns at iter {}",
            i
        );
    }

    // 验证每个连接收到 100 条
    for i in 0..conn_per_user {
        let count = pusher.message_count(&format!("conn-{}", i)).await;
        assert_eq!(count, 100, "conn-{} should have 100 msgs", i);
    }
}

/// 验证：unsubscribe 后不再收到房间消息
#[tokio::test]
async fn stress_ws_unsubscribe_stops_push() {
    let pusher = RealtimePusher::new();
    pusher.register_connection("conn-1").await;
    pusher.subscribe("conn-1", "room-1").await.unwrap();
    pusher
        .push_to_room("room-1", b"first".to_vec())
        .await
        .unwrap();
    assert_eq!(pusher.message_count("conn-1").await, 1);

    pusher.unsubscribe("conn-1", "room-1").await;
    let success = pusher
        .push_to_room("room-1", b"second".to_vec())
        .await
        .unwrap();
    assert_eq!(success, 0, "no subscribers in room");
    assert_eq!(
        pusher.message_count("conn-1").await,
        1,
        "should still be 1 after unsubscribe"
    );
}

/// 验证：unregister_connection 后 push_to_connection 失败
#[tokio::test]
async fn stress_ws_unregister_then_push_fails() {
    let pusher = RealtimePusher::new();
    pusher.register_connection("conn-1").await;
    pusher
        .push_to_connection("conn-1", b"hello".to_vec())
        .await
        .unwrap();

    pusher.unregister_connection("conn-1").await;
    let result = pusher
        .push_to_connection("conn-1", b"hello-again".to_vec())
        .await;
    assert!(result.is_err(), "push to unregistered must fail");
}

/// 验证：InMemorySender 在 1 万条消息下一致性
#[tokio::test]
async fn stress_ws_sender_10k_messages() {
    let sender = InMemorySender::new();
    for i in 0..10_000 {
        sender
            .send("conn-1", format!("msg-{}", i).into_bytes())
            .await
            .unwrap();
    }
    assert_eq!(sender.message_count("conn-1").await, 10_000);
    assert_eq!(sender.total_message_count().await, 10_000);

    sender.close("conn-1").await.unwrap();
    assert_eq!(sender.message_count("conn-1").await, 0);
    assert_eq!(sender.total_message_count().await, 0);
}

/// 验证：push_order_status 与 push_customer_message 推送路径
#[tokio::test]
async fn stress_ws_push_order_and_customer() {
    let pusher = RealtimePusher::new();
    pusher.register_connection_with_user("conn-1", 100).await;
    pusher.register_connection_with_user("conn-2", 200).await;
    pusher.subscribe("conn-1", "room-a").await.unwrap();
    pusher.subscribe("conn-2", "room-a").await.unwrap();

    let success = pusher
        .push_order_status(100, 1001, "shipped")
        .await
        .unwrap();
    assert_eq!(success, 1, "user 100 has 1 conn");

    let success = pusher
        .push_customer_message("room-a", 300, "hello")
        .await
        .unwrap();
    assert_eq!(success, 2, "room-a has 2 conns");
}

/// 验证：register 同一 connection_id 多次（覆盖）
#[tokio::test]
async fn stress_ws_register_overwrite() {
    let pusher = RealtimePusher::new();
    for _ in 0..1000 {
        pusher.register_connection("same-conn").await;
    }
    assert_eq!(
        pusher.connection_count().await,
        1,
        "duplicate register should overwrite"
    );
}
