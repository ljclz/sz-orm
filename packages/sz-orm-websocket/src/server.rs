use crate::error::WsError;
use crate::handler::{MessageType, WebSocketConnection, WebSocketHandler, WebSocketMessage};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, Sender};
use tokio::sync::{Mutex, RwLock};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

#[derive(Clone)]
pub struct WebSocketSender {
    inner: Sender<Vec<u8>>,
}

impl WebSocketSender {
    pub fn new(inner: Sender<Vec<u8>>) -> Self {
        Self { inner }
    }

    pub async fn send(&self, data: Vec<u8>) -> Result<(), WsError> {
        self.inner
            .send(data)
            .await
            .map_err(|e| WsError::Connection(format!("send failed: {}", e)))
    }
}

impl std::fmt::Debug for WebSocketSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSocketSender").finish()
    }
}

pub struct WsServer {
    connections: Arc<RwLock<HashMap<String, WebSocketSender>>>,
    listen_addr: String,
    shutdown_tx: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl WsServer {
    pub fn new(listen_addr: impl Into<String>) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            listen_addr: listen_addr.into(),
            shutdown_tx: Mutex::new(None),
        }
    }

    pub async fn is_running(&self) -> bool {
        self.shutdown_tx.lock().await.is_some()
    }

    pub async fn connection_count(&self) -> usize {
        self.connections.read().await.len()
    }

    pub async fn start(&self, handler: Arc<dyn WebSocketHandler>) -> Result<(), WsError> {
        let listener = TcpListener::bind(&self.listen_addr).await?;

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        let connections = self.connections.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        break;
                    }
                    accept_result = listener.accept() => {
                        match accept_result {
                            Ok((socket, addr)) => {
                                let handler = handler.clone();
                                let connections = connections.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = handle_connection(socket, addr, handler, connections).await {
                                        eprintln!("ws connection error: {}", e);
                                    }
                                });
                            }
                            Err(e) => {
                                eprintln!("ws accept error: {}", e);
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn stop(&self) -> Result<(), WsError> {
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(());
        }
        self.connections.write().await.clear();
        Ok(())
    }

    pub async fn broadcast_to_all(&self, data: Vec<u8>) -> Result<usize, WsError> {
        let senders: Vec<WebSocketSender> =
            self.connections.read().await.values().cloned().collect();
        let mut count = 0;
        for sender in senders {
            if sender.send(data.clone()).await.is_ok() {
                count += 1;
            }
        }
        Ok(count)
    }
}

async fn handle_connection(
    socket: TcpStream,
    addr: std::net::SocketAddr,
    handler: Arc<dyn WebSocketHandler>,
    connections: Arc<RwLock<HashMap<String, WebSocketSender>>>,
) -> Result<(), WsError> {
    let ws_stream = accept_async(socket)
        .await
        .map_err(|e| WsError::Connection(format!("accept failed: {}", e)))?;

    let conn_id = generate_connection_id();
    let conn = WebSocketConnection::new(conn_id.clone()).with_address(addr.to_string());

    handler.on_connect(&conn).await?;

    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(64);
    let sender = WebSocketSender::new(tx);
    connections.write().await.insert(conn_id.clone(), sender);

    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    loop {
        tokio::select! {
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(TungsteniteMessage::Text(text))) => {
                        let ws_msg = WebSocketMessage {
                            msg_type: MessageType::Text,
                            payload: text.as_bytes().to_vec(),
                            sender_id: conn.user_id,
                            room_id: None,
                            timestamp: current_timestamp(),
                        };
                        if let Some(resp) = handler.on_message(&conn, ws_msg).await? {
                            if ws_sink.send(make_tungstenite_msg(resp)).await.is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(TungsteniteMessage::Binary(data))) => {
                        let ws_msg = WebSocketMessage {
                            msg_type: MessageType::Binary,
                            payload: data.to_vec(),
                            sender_id: conn.user_id,
                            room_id: None,
                            timestamp: current_timestamp(),
                        };
                        if let Some(resp) = handler.on_message(&conn, ws_msg).await? {
                            if ws_sink.send(make_tungstenite_msg(resp)).await.is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(TungsteniteMessage::Ping(p))) => {
                        if ws_sink.send(TungsteniteMessage::Pong(p)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(TungsteniteMessage::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            outgoing = rx.recv() => {
                match outgoing {
                    Some(data) => {
                        if ws_sink.send(TungsteniteMessage::Binary(data.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
    }

    connections.write().await.remove(&conn_id);
    handler.on_disconnect(&conn).await;

    Ok(())
}

fn make_tungstenite_msg(resp: WebSocketMessage) -> TungsteniteMessage {
    match resp.msg_type {
        MessageType::Text => {
            TungsteniteMessage::Text(String::from_utf8_lossy(&resp.payload).into_owned().into())
        }
        MessageType::Ping => TungsteniteMessage::Ping(resp.payload.into()),
        MessageType::Pong => TungsteniteMessage::Pong(resp.payload.into()),
        _ => TungsteniteMessage::Binary(resp.payload.into()),
    }
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn generate_connection_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("conn-{:x}-{}", ts, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_websocket_sender_new() {
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(8);
        let sender = WebSocketSender::new(tx);
        sender.send(b"hello".to_vec()).await.unwrap();
        let received = rx.recv().await.unwrap();
        assert_eq!(received, b"hello");
    }

    #[tokio::test]
    async fn test_ws_server_new() {
        let server = WsServer::new("127.0.0.1:0");
        assert!(!server.is_running().await);
        assert_eq!(server.connection_count().await, 0);
    }

    #[tokio::test]
    #[ignore = "requires port availability"]
    async fn test_ws_server_start_stop() {
        let server = WsServer::new("127.0.0.1:0");
        let handler = Arc::new(crate::DefaultWebSocketHandler::new()) as Arc<dyn WebSocketHandler>;

        server.start(handler).await.unwrap();
        assert!(server.is_running().await);

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        server.stop().await.unwrap();
        assert!(!server.is_running().await);
    }

    #[tokio::test]
    async fn test_ws_server_broadcast_empty() {
        let server = WsServer::new("127.0.0.1:0");
        let count = server.broadcast_to_all(b"hello".to_vec()).await.unwrap();
        assert_eq!(count, 0);
    }
}
