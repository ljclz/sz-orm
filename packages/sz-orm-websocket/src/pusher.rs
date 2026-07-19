use crate::error::WsError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared in-memory message buffer that captures messages sent to each connection.
/// Each connection_id maps to a list of messages that have been pushed to it.
pub struct InMemorySender {
    messages: Arc<RwLock<HashMap<String, Vec<Vec<u8>>>>>,
}

impl InMemorySender {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn send(&self, connection_id: &str, message: Vec<u8>) -> Result<(), WsError> {
        let mut messages = self.messages.write().await;
        messages
            .entry(connection_id.to_string())
            .or_insert_with(Vec::new)
            .push(message);
        Ok(())
    }

    pub async fn close(&self, connection_id: &str) -> Result<(), WsError> {
        let mut messages = self.messages.write().await;
        messages.remove(connection_id);
        Ok(())
    }

    pub async fn messages_for(&self, connection_id: &str) -> Vec<Vec<u8>> {
        let messages = self.messages.read().await;
        messages.get(connection_id).cloned().unwrap_or_default()
    }

    pub async fn message_count(&self, connection_id: &str) -> usize {
        let messages = self.messages.read().await;
        messages.get(connection_id).map(|v| v.len()).unwrap_or(0)
    }

    pub async fn total_message_count(&self) -> usize {
        let messages = self.messages.read().await;
        messages.values().map(|v| v.len()).sum()
    }
}

impl Default for InMemorySender {
    fn default() -> Self {
        Self::new()
    }
}

struct ConnectionState {
    user_id: Option<i64>,
    subscriptions: Vec<String>,
}

pub struct RealtimePusher {
    connections: Arc<RwLock<HashMap<String, ConnectionState>>>,
    rooms: Arc<RwLock<HashMap<String, Vec<String>>>>,
    sender: InMemorySender,
}

impl RealtimePusher {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            rooms: Arc::new(RwLock::new(HashMap::new())),
            sender: InMemorySender::new(),
        }
    }

    pub async fn register_connection(&self, connection_id: impl Into<String>) {
        let mut connections = self.connections.write().await;
        connections.insert(
            connection_id.into(),
            ConnectionState {
                user_id: None,
                subscriptions: Vec::new(),
            },
        );
    }

    pub async fn register_connection_with_user(
        &self,
        connection_id: impl Into<String>,
        user_id: i64,
    ) {
        let mut connections = self.connections.write().await;
        connections.insert(
            connection_id.into(),
            ConnectionState {
                user_id: Some(user_id),
                subscriptions: Vec::new(),
            },
        );
    }

    pub async fn unregister_connection(&self, connection_id: &str) {
        let mut connections = self.connections.write().await;
        connections.remove(connection_id);
        drop(connections);

        let mut rooms = self.rooms.write().await;
        for (_, ids) in rooms.iter_mut() {
            ids.retain(|id| id != connection_id);
        }
        drop(rooms);

        let _ = self.sender.close(connection_id).await;
    }

    pub async fn subscribe(&self, connection_id: &str, room: &str) -> Result<(), WsError> {
        let mut connections = self.connections.write().await;
        let conn = connections
            .get_mut(connection_id)
            .ok_or_else(|| WsError::Connection("Connection not found".to_string()))?;

        if !conn.subscriptions.contains(&room.to_string()) {
            conn.subscriptions.push(room.to_string());
        }

        drop(connections);

        let mut rooms = self.rooms.write().await;
        let entry = rooms.entry(room.to_string()).or_insert_with(Vec::new);
        if !entry.contains(&connection_id.to_string()) {
            entry.push(connection_id.to_string());
        }

        Ok(())
    }

    pub async fn unsubscribe(&self, connection_id: &str, room: &str) {
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.get_mut(connection_id) {
            conn.subscriptions.retain(|r| r != room);
        }

        drop(connections);

        let mut rooms = self.rooms.write().await;
        if let Some(ids) = rooms.get_mut(room) {
            ids.retain(|id| id != connection_id);
        }
    }

    pub async fn push_to_connection(
        &self,
        connection_id: &str,
        message: Vec<u8>,
    ) -> Result<(), WsError> {
        let connections = self.connections.read().await;
        let _conn = connections
            .get(connection_id)
            .ok_or_else(|| WsError::Connection("Connection not found".to_string()))?;
        drop(connections);

        self.sender.send(connection_id, message).await
    }

    pub async fn push_to_room(&self, room: &str, message: Vec<u8>) -> Result<usize, WsError> {
        let rooms = self.rooms.read().await;
        let connection_ids = rooms.get(room).cloned().unwrap_or_default();
        drop(rooms);

        let mut success_count = 0;

        for conn_id in connection_ids {
            if self.sender.send(&conn_id, message.clone()).await.is_ok() {
                success_count += 1;
            }
        }

        Ok(success_count)
    }

    pub async fn push_to_user(&self, user_id: i64, message: Vec<u8>) -> Result<usize, WsError> {
        let connections = self.connections.read().await;
        let matching_ids: Vec<String> = connections
            .iter()
            .filter(|(_, conn)| conn.user_id == Some(user_id))
            .map(|(id, _)| id.clone())
            .collect();
        drop(connections);

        let mut success_count = 0;

        for conn_id in matching_ids {
            if self.sender.send(&conn_id, message.clone()).await.is_ok() {
                success_count += 1;
            }
        }

        Ok(success_count)
    }

    pub async fn push_order_status(
        &self,
        user_id: i64,
        order_id: i64,
        status: &str,
    ) -> Result<usize, WsError> {
        let payload = serde_json::json!({
            "type": "order_status",
            "order_id": order_id,
            "status": status,
        });

        let message = serde_json::to_vec(&payload)?;
        self.push_to_user(user_id, message).await
    }

    pub async fn push_customer_message(
        &self,
        room_id: &str,
        sender_id: i64,
        content: &str,
    ) -> Result<usize, WsError> {
        let payload = serde_json::json!({
            "type": "customer_message",
            "sender_id": sender_id,
            "content": content,
        });

        let message = serde_json::to_vec(&payload)?;
        self.push_to_room(room_id, message).await
    }

    pub async fn broadcast(&self, message: Vec<u8>) -> Result<usize, WsError> {
        let connections = self.connections.read().await;
        let conn_ids: Vec<String> = connections.keys().cloned().collect();
        drop(connections);

        let mut success_count = 0;

        for conn_id in conn_ids {
            if self.sender.send(&conn_id, message.clone()).await.is_ok() {
                success_count += 1;
            }
        }

        Ok(success_count)
    }

    pub async fn connection_count(&self) -> usize {
        let connections = self.connections.read().await;
        connections.len()
    }

    pub async fn room_count(&self, room: &str) -> usize {
        let rooms = self.rooms.read().await;
        rooms.get(room).map(|v| v.len()).unwrap_or(0)
    }

    pub async fn room_list(&self) -> Vec<String> {
        let rooms = self.rooms.read().await;
        rooms.keys().cloned().collect()
    }

    /// Returns all messages sent to the given connection, in delivery order.
    pub async fn messages_for(&self, connection_id: &str) -> Vec<Vec<u8>> {
        self.sender.messages_for(connection_id).await
    }

    /// Returns the number of messages sent to the given connection.
    pub async fn message_count(&self, connection_id: &str) -> usize {
        self.sender.message_count(connection_id).await
    }
}

impl Default for RealtimePusher {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResult {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
}

impl PushResult {
    pub fn new(total: usize) -> Self {
        Self {
            total,
            success: 0,
            failed: 0,
        }
    }

    pub fn add_success(&mut self) {
        self.success += 1;
    }

    pub fn add_failure(&mut self) {
        self.failed += 1;
    }

    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.success as f64 / self.total as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_sender_send_and_messages_for() {
        let sender = InMemorySender::new();
        sender.send("conn1", b"hello".to_vec()).await.unwrap();
        sender.send("conn1", b"world".to_vec()).await.unwrap();

        let messages = sender.messages_for("conn1").await;
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], b"hello");
        assert_eq!(messages[1], b"world");
    }

    #[tokio::test]
    async fn test_in_memory_sender_message_count() {
        let sender = InMemorySender::new();
        assert_eq!(sender.message_count("conn1").await, 0);

        sender.send("conn1", b"a".to_vec()).await.unwrap();
        sender.send("conn1", b"b".to_vec()).await.unwrap();
        assert_eq!(sender.message_count("conn1").await, 2);
    }

    #[tokio::test]
    async fn test_in_memory_sender_close() {
        let sender = InMemorySender::new();
        sender.send("conn1", b"data".to_vec()).await.unwrap();
        assert_eq!(sender.message_count("conn1").await, 1);

        sender.close("conn1").await.unwrap();
        assert_eq!(sender.message_count("conn1").await, 0);
    }

    #[tokio::test]
    async fn test_in_memory_sender_isolation() {
        let sender = InMemorySender::new();
        sender.send("conn1", b"a".to_vec()).await.unwrap();
        sender.send("conn2", b"b".to_vec()).await.unwrap();

        assert_eq!(sender.messages_for("conn1").await.len(), 1);
        assert_eq!(sender.messages_for("conn2").await.len(), 1);
        assert_eq!(sender.messages_for("conn3").await.len(), 0);
    }

    #[tokio::test]
    async fn test_push_to_user_filters_by_user_id() {
        let pusher = RealtimePusher::new();
        pusher.register_connection_with_user("conn1", 100).await;
        pusher.register_connection_with_user("conn2", 200).await;
        pusher.register_connection_with_user("conn3", 100).await;
        pusher.register_connection("conn4").await;

        let count = pusher
            .push_to_user(100, b"hi-user-100".to_vec())
            .await
            .unwrap();
        assert_eq!(count, 2);

        assert_eq!(pusher.message_count("conn1").await, 1);
        assert_eq!(pusher.message_count("conn2").await, 0);
        assert_eq!(pusher.message_count("conn3").await, 1);
        assert_eq!(pusher.message_count("conn4").await, 0);

        let messages = pusher.messages_for("conn1").await;
        assert_eq!(messages[0], b"hi-user-100");
    }

    #[tokio::test]
    async fn test_push_to_user_no_matching_connections() {
        let pusher = RealtimePusher::new();
        pusher.register_connection_with_user("conn1", 100).await;

        let count = pusher.push_to_user(999, b"nope".to_vec()).await.unwrap();
        assert_eq!(count, 0);
        assert_eq!(pusher.message_count("conn1").await, 0);
    }

    #[tokio::test]
    async fn test_push_to_user_with_unregistered_connections() {
        let pusher = RealtimePusher::new();
        pusher.register_connection("conn1").await;
        pusher.register_connection("conn2").await;

        let count = pusher
            .push_to_user(100, b"no-users".to_vec())
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_push_to_connection_delivers_message() {
        let pusher = RealtimePusher::new();
        pusher.register_connection("conn1").await;

        pusher
            .push_to_connection("conn1", b"direct-msg".to_vec())
            .await
            .unwrap();

        let messages = pusher.messages_for("conn1").await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], b"direct-msg");
    }

    #[tokio::test]
    async fn test_push_to_connection_not_found() {
        let pusher = RealtimePusher::new();
        let result = pusher.push_to_connection("missing", b"data".to_vec()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), WsError::Connection(_)));
    }

    #[tokio::test]
    async fn test_push_to_room_delivers_to_all_members() {
        let pusher = RealtimePusher::new();
        pusher.register_connection("conn1").await;
        pusher.register_connection("conn2").await;
        pusher.register_connection("conn3").await;

        pusher.subscribe("conn1", "room1").await.unwrap();
        pusher.subscribe("conn2", "room1").await.unwrap();
        pusher.subscribe("conn3", "room2").await.unwrap();

        let count = pusher
            .push_to_room("room1", b"room-msg".to_vec())
            .await
            .unwrap();
        assert_eq!(count, 2);

        assert_eq!(pusher.message_count("conn1").await, 1);
        assert_eq!(pusher.message_count("conn2").await, 1);
        assert_eq!(pusher.message_count("conn3").await, 0);
    }

    #[tokio::test]
    async fn test_push_to_room_empty_room() {
        let pusher = RealtimePusher::new();
        let count = pusher
            .push_to_room("nonexistent", b"data".to_vec())
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_broadcast_delivers_to_all() {
        let pusher = RealtimePusher::new();
        pusher.register_connection("conn1").await;
        pusher.register_connection("conn2").await;

        let count = pusher.broadcast(b"broadcast".to_vec()).await.unwrap();
        assert_eq!(count, 2);

        assert_eq!(pusher.messages_for("conn1").await[0], b"broadcast");
        assert_eq!(pusher.messages_for("conn2").await[0], b"broadcast");
    }

    #[tokio::test]
    async fn test_unregister_clears_messages() {
        let pusher = RealtimePusher::new();
        pusher.register_connection("conn1").await;
        pusher
            .push_to_connection("conn1", b"data".to_vec())
            .await
            .unwrap();
        assert_eq!(pusher.message_count("conn1").await, 1);

        pusher.unregister_connection("conn1").await;
        assert_eq!(pusher.message_count("conn1").await, 0);
        assert_eq!(pusher.connection_count().await, 0);
    }

    #[tokio::test]
    async fn test_push_order_status_delivers_to_matching_user() {
        let pusher = RealtimePusher::new();
        pusher.register_connection_with_user("conn1", 123).await;
        pusher.register_connection_with_user("conn2", 456).await;

        let count = pusher.push_order_status(123, 789, "shipped").await.unwrap();
        assert_eq!(count, 1);

        let messages = pusher.messages_for("conn1").await;
        assert_eq!(messages.len(), 1);
        let parsed: serde_json::Value = serde_json::from_slice(&messages[0]).unwrap();
        assert_eq!(parsed["type"], "order_status");
        assert_eq!(parsed["order_id"], 789);
        assert_eq!(parsed["status"], "shipped");
    }

    #[tokio::test]
    async fn test_push_customer_message_delivers_to_room() {
        let pusher = RealtimePusher::new();
        pusher.register_connection_with_user("conn1", 100).await;
        pusher.register_connection_with_user("conn2", 200).await;
        pusher.subscribe("conn1", "room1").await.unwrap();
        pusher.subscribe("conn2", "room1").await.unwrap();

        let count = pusher
            .push_customer_message("room1", 100, "hello room")
            .await
            .unwrap();
        assert_eq!(count, 2);

        for conn_id in &["conn1", "conn2"] {
            let messages = pusher.messages_for(conn_id).await;
            assert_eq!(messages.len(), 1);
            let parsed: serde_json::Value = serde_json::from_slice(&messages[0]).unwrap();
            assert_eq!(parsed["type"], "customer_message");
            assert_eq!(parsed["sender_id"], 100);
            assert_eq!(parsed["content"], "hello room");
        }
    }

    #[tokio::test]
    async fn test_multiple_pushes_to_same_connection() {
        let pusher = RealtimePusher::new();
        pusher.register_connection("conn1").await;

        pusher
            .push_to_connection("conn1", b"msg1".to_vec())
            .await
            .unwrap();
        pusher
            .push_to_connection("conn1", b"msg2".to_vec())
            .await
            .unwrap();
        pusher
            .push_to_connection("conn1", b"msg3".to_vec())
            .await
            .unwrap();

        let messages = pusher.messages_for("conn1").await;
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0], b"msg1");
        assert_eq!(messages[1], b"msg2");
        assert_eq!(messages[2], b"msg3");
    }

    #[tokio::test]
    async fn test_subscribe_not_found() {
        let pusher = RealtimePusher::new();
        let result = pusher.subscribe("missing", "room1").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), WsError::Connection(_)));
    }
}
