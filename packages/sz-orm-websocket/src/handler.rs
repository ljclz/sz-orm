use crate::error::WsError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct WebSocketMessage {
    pub msg_type: MessageType,
    pub payload: Vec<u8>,
    pub sender_id: Option<i64>,
    pub room_id: Option<String>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MessageType {
    #[default]
    Text,
    Binary,
    Ping,
    Pong,
    Join,
    Leave,
    Subscribe,
    Unsubscribe,
    Notification,
    System,
}

#[derive(Debug, Clone)]
pub struct WebSocketConnection {
    pub id: String,
    pub user_id: Option<i64>,
    pub remote_addr: Option<String>,
    pub is_authenticated: bool,
    pub subscriptions: Vec<String>,
}

impl WebSocketConnection {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            user_id: None,
            remote_addr: None,
            is_authenticated: false,
            subscriptions: Vec::new(),
        }
    }

    pub fn with_user(mut self, user_id: i64) -> Self {
        self.user_id = Some(user_id);
        self.is_authenticated = true;
        self
    }

    pub fn with_address(mut self, addr: impl Into<String>) -> Self {
        self.remote_addr = Some(addr.into());
        self
    }

    pub fn subscribe(&mut self, room: impl Into<String>) {
        let room = room.into();
        if !self.subscriptions.contains(&room) {
            self.subscriptions.push(room);
        }
    }

    pub fn unsubscribe(&mut self, room: &str) {
        self.subscriptions.retain(|r| r != room);
    }
}

#[async_trait]
pub trait WebSocketHandler: Send + Sync {
    async fn on_message(
        &self,
        conn: &WebSocketConnection,
        msg: WebSocketMessage,
    ) -> Result<Option<WebSocketMessage>, WsError>;

    async fn on_connect(&self, conn: &WebSocketConnection) -> Result<(), WsError>;

    async fn on_disconnect(&self, conn: &WebSocketConnection);

    fn authenticate(&self, token: &str) -> Result<UserId, WsError>;
}

pub type UserId = i64;

pub struct WsContext {
    pub connection_id: String,
    pub user_id: Option<i64>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl WsContext {
    pub fn new(connection_id: impl Into<String>) -> Self {
        Self {
            connection_id: connection_id.into(),
            user_id: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn with_user(mut self, user_id: i64) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

pub struct WsMessageBuilder {
    msg_type: MessageType,
    payload: Vec<u8>,
    sender_id: Option<i64>,
    room_id: Option<String>,
}

impl WsMessageBuilder {
    pub fn new() -> Self {
        Self {
            msg_type: MessageType::Text,
            payload: Vec::new(),
            sender_id: None,
            room_id: None,
        }
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.msg_type = MessageType::Text;
        self.payload = text.into().into_bytes();
        self
    }

    pub fn binary(mut self, data: Vec<u8>) -> Self {
        self.msg_type = MessageType::Binary;
        self.payload = data;
        self
    }

    pub fn json<T: serde::Serialize>(mut self, data: &T) -> Result<Self, WsError> {
        self.msg_type = MessageType::Text;
        self.payload = serde_json::to_vec(data)?;
        Ok(self)
    }

    pub fn with_sender(mut self, user_id: i64) -> Self {
        self.sender_id = Some(user_id);
        self
    }

    pub fn with_room(mut self, room: impl Into<String>) -> Self {
        self.room_id = Some(room.into());
        self
    }

    pub fn notification(mut self) -> Self {
        self.msg_type = MessageType::Notification;
        self
    }

    pub fn system(mut self) -> Self {
        self.msg_type = MessageType::System;
        self
    }

    pub fn build(self) -> WebSocketMessage {
        WebSocketMessage {
            msg_type: self.msg_type,
            payload: self.payload,
            sender_id: self.sender_id,
            room_id: self.room_id,
            timestamp: current_timestamp(),
        }
    }
}

impl Default for WsMessageBuilder {
    fn default() -> Self {
        Self::new()
    }
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Default WebSocket handler that tracks connections and echoes messages.
///
/// - Text messages are echoed back to the sender with the sender's user_id.
/// - Ping messages are answered with a Pong.
/// - Subscribe/Unsubscribe/Join/Leave messages return a System acknowledgement.
/// - All other messages are logged but produce no response.
pub struct DefaultWebSocketHandler {
    connections: Arc<RwLock<HashMap<String, WebSocketConnection>>>,
    message_log: Arc<RwLock<Vec<WebSocketMessage>>>,
}

impl DefaultWebSocketHandler {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            message_log: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn connection_count(&self) -> usize {
        self.connections.read().await.len()
    }

    pub async fn is_connected(&self, connection_id: &str) -> bool {
        self.connections.read().await.contains_key(connection_id)
    }

    pub async fn message_count(&self) -> usize {
        self.message_log.read().await.len()
    }

    pub async fn messages(&self) -> Vec<WebSocketMessage> {
        self.message_log.read().await.clone()
    }

    pub async fn get_connection(&self, connection_id: &str) -> Option<WebSocketConnection> {
        self.connections.read().await.get(connection_id).cloned()
    }
}

impl Default for DefaultWebSocketHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WebSocketHandler for DefaultWebSocketHandler {
    async fn on_message(
        &self,
        conn: &WebSocketConnection,
        msg: WebSocketMessage,
    ) -> Result<Option<WebSocketMessage>, WsError> {
        self.message_log.write().await.push(msg.clone());

        match msg.msg_type {
            MessageType::Text => {
                let response = WebSocketMessage {
                    msg_type: MessageType::Text,
                    payload: msg.payload.clone(),
                    sender_id: conn.user_id,
                    room_id: None,
                    timestamp: current_timestamp(),
                };
                Ok(Some(response))
            }
            MessageType::Ping => {
                let response = WebSocketMessage {
                    msg_type: MessageType::Pong,
                    payload: msg.payload.clone(),
                    sender_id: None,
                    room_id: None,
                    timestamp: current_timestamp(),
                };
                Ok(Some(response))
            }
            MessageType::Subscribe => {
                let room = String::from_utf8_lossy(&msg.payload).to_string();
                let ack = format!("subscribed:{}", room);
                let response = WebSocketMessage {
                    msg_type: MessageType::System,
                    payload: ack.into_bytes(),
                    sender_id: None,
                    room_id: Some(room),
                    timestamp: current_timestamp(),
                };
                Ok(Some(response))
            }
            MessageType::Unsubscribe => {
                let room = String::from_utf8_lossy(&msg.payload).to_string();
                let ack = format!("unsubscribed:{}", room);
                let response = WebSocketMessage {
                    msg_type: MessageType::System,
                    payload: ack.into_bytes(),
                    sender_id: None,
                    room_id: Some(room),
                    timestamp: current_timestamp(),
                };
                Ok(Some(response))
            }
            MessageType::Join => {
                let room = String::from_utf8_lossy(&msg.payload).to_string();
                let ack = format!("joined:{}", room);
                let response = WebSocketMessage {
                    msg_type: MessageType::System,
                    payload: ack.into_bytes(),
                    sender_id: conn.user_id,
                    room_id: Some(room),
                    timestamp: current_timestamp(),
                };
                Ok(Some(response))
            }
            MessageType::Leave => {
                let room = String::from_utf8_lossy(&msg.payload).to_string();
                let ack = format!("left:{}", room);
                let response = WebSocketMessage {
                    msg_type: MessageType::System,
                    payload: ack.into_bytes(),
                    sender_id: conn.user_id,
                    room_id: Some(room),
                    timestamp: current_timestamp(),
                };
                Ok(Some(response))
            }
            _ => Ok(None),
        }
    }

    async fn on_connect(&self, conn: &WebSocketConnection) -> Result<(), WsError> {
        self.connections
            .write()
            .await
            .insert(conn.id.clone(), conn.clone());
        Ok(())
    }

    async fn on_disconnect(&self, conn: &WebSocketConnection) {
        self.connections.write().await.remove(&conn.id);
    }

    fn authenticate(&self, token: &str) -> Result<UserId, WsError> {
        if let Some(id_str) = token.strip_prefix("user_id:") {
            id_str.parse::<i64>().map_err(|_| {
                WsError::Authentication(format!("invalid user_id in token: {}", token))
            })
        } else {
            token
                .parse::<i64>()
                .map_err(|_| WsError::Authentication(format!("invalid token: {}", token)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_conn(id: &str, user_id: Option<i64>) -> WebSocketConnection {
        let mut conn = WebSocketConnection::new(id);
        if let Some(uid) = user_id {
            conn = conn.with_user(uid);
        }
        conn
    }

    fn make_text(payload: &[u8]) -> WebSocketMessage {
        WebSocketMessage {
            msg_type: MessageType::Text,
            payload: payload.to_vec(),
            sender_id: None,
            room_id: None,
            timestamp: 1000,
        }
    }

    #[tokio::test]
    async fn test_on_connect_tracks_connection() {
        let handler = DefaultWebSocketHandler::new();
        assert_eq!(handler.connection_count().await, 0);

        let conn = make_conn("c1", Some(123));
        handler.on_connect(&conn).await.unwrap();

        assert_eq!(handler.connection_count().await, 1);
        assert!(handler.is_connected("c1").await);
    }

    #[tokio::test]
    async fn test_on_disconnect_removes_connection() {
        let handler = DefaultWebSocketHandler::new();
        let conn = make_conn("c1", Some(123));
        handler.on_connect(&conn).await.unwrap();
        assert!(handler.is_connected("c1").await);

        handler.on_disconnect(&conn).await;
        assert!(!handler.is_connected("c1").await);
        assert_eq!(handler.connection_count().await, 0);
    }

    #[tokio::test]
    async fn test_on_message_text_echoes_back() {
        let handler = DefaultWebSocketHandler::new();
        let conn = make_conn("c1", Some(42));
        let msg = make_text(b"hello");

        let response = handler.on_message(&conn, msg).await.unwrap();
        assert!(response.is_some());

        let resp = response.unwrap();
        assert_eq!(resp.msg_type, MessageType::Text);
        assert_eq!(resp.payload, b"hello");
        assert_eq!(resp.sender_id, Some(42));
    }

    #[tokio::test]
    async fn test_on_message_ping_responds_pong() {
        let handler = DefaultWebSocketHandler::new();
        let conn = make_conn("c1", None);
        let msg = WebSocketMessage {
            msg_type: MessageType::Ping,
            payload: b"ping".to_vec(),
            sender_id: None,
            room_id: None,
            timestamp: 1,
        };

        let response = handler.on_message(&conn, msg).await.unwrap();
        let resp = response.unwrap();
        assert_eq!(resp.msg_type, MessageType::Pong);
        assert_eq!(resp.payload, b"ping");
    }

    #[tokio::test]
    async fn test_on_message_subscribe_returns_system_ack() {
        let handler = DefaultWebSocketHandler::new();
        let conn = make_conn("c1", None);
        let msg = WebSocketMessage {
            msg_type: MessageType::Subscribe,
            payload: b"room1".to_vec(),
            sender_id: None,
            room_id: None,
            timestamp: 1,
        };

        let response = handler.on_message(&conn, msg).await.unwrap();
        let resp = response.unwrap();
        assert_eq!(resp.msg_type, MessageType::System);
        assert_eq!(resp.payload, b"subscribed:room1");
        assert_eq!(resp.room_id, Some("room1".to_string()));
    }

    #[tokio::test]
    async fn test_on_message_unsubscribe_returns_system_ack() {
        let handler = DefaultWebSocketHandler::new();
        let conn = make_conn("c1", None);
        let msg = WebSocketMessage {
            msg_type: MessageType::Unsubscribe,
            payload: b"room1".to_vec(),
            sender_id: None,
            room_id: None,
            timestamp: 1,
        };

        let response = handler.on_message(&conn, msg).await.unwrap();
        let resp = response.unwrap();
        assert_eq!(resp.msg_type, MessageType::System);
        assert_eq!(resp.payload, b"unsubscribed:room1");
    }

    #[tokio::test]
    async fn test_on_message_join_returns_system_ack() {
        let handler = DefaultWebSocketHandler::new();
        let conn = make_conn("c1", Some(7));
        let msg = WebSocketMessage {
            msg_type: MessageType::Join,
            payload: b"lobby".to_vec(),
            sender_id: None,
            room_id: None,
            timestamp: 1,
        };

        let response = handler.on_message(&conn, msg).await.unwrap();
        let resp = response.unwrap();
        assert_eq!(resp.msg_type, MessageType::System);
        assert_eq!(resp.payload, b"joined:lobby");
        assert_eq!(resp.sender_id, Some(7));
        assert_eq!(resp.room_id, Some("lobby".to_string()));
    }

    #[tokio::test]
    async fn test_on_message_leave_returns_system_ack() {
        let handler = DefaultWebSocketHandler::new();
        let conn = make_conn("c1", Some(7));
        let msg = WebSocketMessage {
            msg_type: MessageType::Leave,
            payload: b"lobby".to_vec(),
            sender_id: None,
            room_id: None,
            timestamp: 1,
        };

        let response = handler.on_message(&conn, msg).await.unwrap();
        let resp = response.unwrap();
        assert_eq!(resp.payload, b"left:lobby");
    }

    #[tokio::test]
    async fn test_on_message_binary_returns_none() {
        let handler = DefaultWebSocketHandler::new();
        let conn = make_conn("c1", None);
        let msg = WebSocketMessage {
            msg_type: MessageType::Binary,
            payload: vec![1, 2, 3],
            sender_id: None,
            room_id: None,
            timestamp: 1,
        };

        let response = handler.on_message(&conn, msg).await.unwrap();
        assert!(response.is_none());
    }

    #[tokio::test]
    async fn test_message_log_records_all_messages() {
        let handler = DefaultWebSocketHandler::new();
        let conn = make_conn("c1", None);

        handler.on_message(&conn, make_text(b"m1")).await.unwrap();
        handler
            .on_message(
                &conn,
                WebSocketMessage {
                    msg_type: MessageType::Binary,
                    payload: vec![1],
                    sender_id: None,
                    room_id: None,
                    timestamp: 2,
                },
            )
            .await
            .unwrap();
        handler.on_message(&conn, make_text(b"m3")).await.unwrap();

        assert_eq!(handler.message_count().await, 3);
        let msgs = handler.messages().await;
        assert_eq!(msgs[0].payload, b"m1");
        assert_eq!(msgs[1].msg_type, MessageType::Binary);
        assert_eq!(msgs[2].payload, b"m3");
    }

    #[tokio::test]
    async fn test_authenticate_valid_numeric_token() {
        let handler = DefaultWebSocketHandler::new();
        let user_id = handler.authenticate("12345").unwrap();
        assert_eq!(user_id, 12345);
    }

    #[tokio::test]
    async fn test_authenticate_valid_prefixed_token() {
        let handler = DefaultWebSocketHandler::new();
        let user_id = handler.authenticate("user_id:67890").unwrap();
        assert_eq!(user_id, 67890);
    }

    #[test]
    fn test_authenticate_invalid_token_returns_error() {
        let handler = DefaultWebSocketHandler::new();
        let result = handler.authenticate("not-a-number");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), WsError::Authentication(_)));
    }

    #[test]
    fn test_authenticate_invalid_prefixed_token_returns_error() {
        let handler = DefaultWebSocketHandler::new();
        let result = handler.authenticate("user_id:abc");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), WsError::Authentication(_)));
    }

    #[tokio::test]
    async fn test_get_connection_returns_stored_connection() {
        let handler = DefaultWebSocketHandler::new();
        let conn = make_conn("c1", Some(99));
        handler.on_connect(&conn).await.unwrap();

        let retrieved = handler.get_connection("c1").await.unwrap();
        assert_eq!(retrieved.id, "c1");
        assert_eq!(retrieved.user_id, Some(99));
        assert!(retrieved.is_authenticated);
    }

    #[tokio::test]
    async fn test_get_connection_not_found() {
        let handler = DefaultWebSocketHandler::new();
        assert!(handler.get_connection("missing").await.is_none());
    }

    #[tokio::test]
    async fn test_multiple_connections_tracked_independently() {
        let handler = DefaultWebSocketHandler::new();
        let conn1 = make_conn("c1", Some(1));
        let conn2 = make_conn("c2", Some(2));

        handler.on_connect(&conn1).await.unwrap();
        handler.on_connect(&conn2).await.unwrap();

        assert_eq!(handler.connection_count().await, 2);

        handler.on_disconnect(&conn1).await;
        assert_eq!(handler.connection_count().await, 1);
        assert!(!handler.is_connected("c1").await);
        assert!(handler.is_connected("c2").await);
    }

    #[tokio::test]
    async fn test_default_impl_creates_empty_handler() {
        let handler = DefaultWebSocketHandler::default();
        assert_eq!(handler.connection_count().await, 0);
        assert_eq!(handler.message_count().await, 0);
    }
}
