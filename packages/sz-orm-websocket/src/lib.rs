//! # SZ-ORM WebSocket — 实时推送
//!
//! 提供 WebSocket 长连接管理、消息推送与认证支持，可选启用 `server` feature
//! 启动独立 WebSocket 服务。
//!
//! ## 主要模块
//!
//! - [`handler`] — 连接处理与会话管理
//! - [`pusher`] — 消息推送器
//! - [`server`] — WebSocket 服务端（feature = "server"）

pub mod error;
pub mod handler;
pub mod pusher;

pub use error::WsError;
pub use handler::*;
pub use pusher::*;

#[cfg(feature = "server")]
pub mod server;

#[cfg(feature = "server")]
pub use server::WsServer;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_connection_new() {
        let conn = WebSocketConnection::new("conn1");
        assert_eq!(conn.id, "conn1");
        assert!(!conn.is_authenticated);
        assert!(conn.user_id.is_none());
    }

    #[test]
    fn test_websocket_connection_with_user() {
        let conn = WebSocketConnection::new("conn1").with_user(123);
        assert_eq!(conn.user_id, Some(123));
        assert!(conn.is_authenticated);
    }

    #[test]
    fn test_websocket_connection_with_address() {
        let conn = WebSocketConnection::new("conn1").with_address("127.0.0.1:8080");
        assert_eq!(conn.remote_addr, Some("127.0.0.1:8080".to_string()));
    }

    #[test]
    fn test_websocket_connection_subscribe() {
        let mut conn = WebSocketConnection::new("conn1");
        conn.subscribe("room1");
        conn.subscribe("room2");
        conn.subscribe("room1");

        assert_eq!(conn.subscriptions.len(), 2);
        assert!(conn.subscriptions.contains(&"room1".to_string()));
        assert!(conn.subscriptions.contains(&"room2".to_string()));
    }

    #[test]
    fn test_websocket_connection_unsubscribe() {
        let mut conn = WebSocketConnection::new("conn1");
        conn.subscribe("room1");
        conn.subscribe("room2");
        conn.unsubscribe("room1");

        assert_eq!(conn.subscriptions.len(), 1);
        assert!(!conn.subscriptions.contains(&"room1".to_string()));
    }

    #[test]
    fn test_ws_message_builder_text() {
        let msg = WsMessageBuilder::new().text("hello").build();

        assert_eq!(msg.msg_type, MessageType::Text);
        assert_eq!(msg.payload, b"hello");
    }

    #[test]
    fn test_ws_message_builder_binary() {
        let msg = WsMessageBuilder::new().binary(vec![1, 2, 3, 4]).build();

        assert_eq!(msg.msg_type, MessageType::Binary);
        assert_eq!(msg.payload, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_ws_message_builder_json() {
        let msg = WsMessageBuilder::new()
            .json(&serde_json::json!({"key": "value"}))
            .unwrap()
            .build();

        assert_eq!(msg.msg_type, MessageType::Text);
        let parsed: serde_json::Value = serde_json::from_slice(&msg.payload).unwrap();
        assert_eq!(parsed["key"], "value");
    }

    #[test]
    fn test_ws_message_builder_with_sender() {
        let msg = WsMessageBuilder::new()
            .text("hello")
            .with_sender(123)
            .build();

        assert_eq!(msg.sender_id, Some(123));
    }

    #[test]
    fn test_ws_message_builder_with_room() {
        let msg = WsMessageBuilder::new()
            .text("hello")
            .with_room("room1")
            .build();

        assert_eq!(msg.room_id, Some("room1".to_string()));
    }

    #[test]
    fn test_ws_message_builder_notification() {
        let msg = WsMessageBuilder::new()
            .text("notice")
            .notification()
            .build();

        assert_eq!(msg.msg_type, MessageType::Notification);
    }

    #[test]
    fn test_ws_message_builder_system() {
        let msg = WsMessageBuilder::new().text("system").system().build();

        assert_eq!(msg.msg_type, MessageType::System);
    }

    #[test]
    fn test_ws_context_new() {
        let ctx = WsContext::new("conn1");
        assert_eq!(ctx.connection_id, "conn1");
        assert!(ctx.user_id.is_none());
    }

    #[test]
    fn test_ws_context_with_user() {
        let ctx = WsContext::new("conn1").with_user(123);
        assert_eq!(ctx.user_id, Some(123));
    }

    #[test]
    fn test_ws_context_with_metadata() {
        let ctx = WsContext::new("conn1")
            .with_metadata("key1", "value1")
            .with_metadata("key2", "value2");

        assert_eq!(ctx.metadata.get("key1"), Some(&"value1".to_string()));
        assert_eq!(ctx.metadata.get("key2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_push_result_new() {
        let result = PushResult::new(10);
        assert_eq!(result.total, 10);
        assert_eq!(result.success, 0);
        assert_eq!(result.failed, 0);
    }

    #[test]
    fn test_push_result_add() {
        let mut result = PushResult::new(10);
        result.add_success();
        result.add_success();
        result.add_failure();

        assert_eq!(result.success, 2);
        assert_eq!(result.failed, 1);
    }

    #[test]
    fn test_push_result_success_rate() {
        let mut result = PushResult::new(10);
        result.add_success();
        result.add_success();
        result.add_success();

        assert_eq!(result.success_rate(), 30.0);
    }

    #[test]
    fn test_push_result_zero_total() {
        let result = PushResult::new(0);
        assert_eq!(result.success_rate(), 0.0);
    }

    #[tokio::test]
    async fn test_realtime_pusher_new() {
        let pusher = RealtimePusher::new();
        let count = pusher.connection_count().await;
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_realtime_pusher_register_connection() {
        let pusher = RealtimePusher::new();
        pusher.register_connection("conn1").await;

        let count = pusher.connection_count().await;
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_realtime_pusher_unregister_connection() {
        let pusher = RealtimePusher::new();
        pusher.register_connection("conn1").await;
        pusher.unregister_connection("conn1").await;

        let count = pusher.connection_count().await;
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_realtime_pusher_subscribe() {
        let pusher = RealtimePusher::new();
        pusher.register_connection("conn1").await;
        pusher.subscribe("conn1", "room1").await.unwrap();

        let count = pusher.room_count("room1").await;
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_realtime_pusher_unsubscribe() {
        let pusher = RealtimePusher::new();
        pusher.register_connection("conn1").await;
        pusher.subscribe("conn1", "room1").await.unwrap();
        pusher.unsubscribe("conn1", "room1").await;

        let count = pusher.room_count("room1").await;
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_realtime_pusher_room_list() {
        let pusher = RealtimePusher::new();
        pusher.register_connection("conn1").await;
        pusher.subscribe("conn1", "room1").await.unwrap();
        pusher.subscribe("conn1", "room2").await.unwrap();

        let rooms = pusher.room_list().await;
        assert!(rooms.contains(&"room1".to_string()));
        assert!(rooms.contains(&"room2".to_string()));
    }

    #[tokio::test]
    async fn test_realtime_pusher_push_to_room() {
        let pusher = RealtimePusher::new();
        pusher.register_connection("conn1").await;
        pusher.subscribe("conn1", "room1").await.unwrap();

        let result = pusher.push_to_room("room1", vec![1, 2, 3]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_realtime_pusher_broadcast() {
        let pusher = RealtimePusher::new();
        pusher.register_connection("conn1").await;
        pusher.register_connection("conn2").await;

        let result = pusher.broadcast(vec![1, 2, 3]).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_realtime_pusher_push_order_status() {
        let pusher = RealtimePusher::new();
        pusher.register_connection("conn1").await;

        let result = pusher.push_order_status(123, 456, "shipped").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_realtime_pusher_push_customer_message() {
        let pusher = RealtimePusher::new();
        pusher.register_connection("conn1").await;
        pusher.subscribe("conn1", "room1").await.unwrap();

        let result = pusher.push_customer_message("room1", 123, "hello").await;
        assert!(result.is_ok());
    }
}
