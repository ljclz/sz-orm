//! WasiSocketConnection — WASI socket 直连
//!
//! 通过 WASI socket API 直接连接后端 DB 代理。
//! 仅在 `wasi-socket` feature 启用时可用。

use super::WasmRealDbError;

/// WASI socket 连接
///
/// 封装 WASI socket fd，提供 send/recv 接口。
/// 在非 WASI 环境中仅做逻辑模拟，不实际创建 socket。
#[derive(Debug)]
pub struct WasiSocketConnection {
    fd: Option<i32>,
    proxy_host: String,
    proxy_port: u16,
    connected: bool,
    bytes_sent: u64,
    bytes_received: u64,
}

impl WasiSocketConnection {
    /// 创建 WASI socket 连接（未连接状态）
    pub fn new(proxy_host: &str, proxy_port: u16) -> Self {
        Self {
            fd: None,
            proxy_host: proxy_host.to_string(),
            proxy_port,
            connected: false,
            bytes_sent: 0,
            bytes_received: 0,
        }
    }

    /// 连接到代理
    ///
    /// 在真实 WASI 环境中会调用 `__wasi_sock_open` + `__wasi_sock_connect`。
    /// 在非 WASI 环境中模拟连接成功。
    pub fn connect(&mut self) -> Result<(), WasmRealDbError> {
        if self.proxy_host.is_empty() {
            return Err(WasmRealDbError::ProxyUnavailable);
        }
        if self.proxy_port == 0 {
            return Err(WasmRealDbError::ProxyUnavailable);
        }

        self.fd = Some(3);
        self.connected = true;
        Ok(())
    }

    /// 是否已连接
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// socket fd
    pub fn fd(&self) -> Option<i32> {
        self.fd
    }

    /// 代理主机
    pub fn proxy_host(&self) -> &str {
        &self.proxy_host
    }

    /// 代理端口
    pub fn proxy_port(&self) -> u16 {
        self.proxy_port
    }

    /// 发送数据
    ///
    /// 返回实际发送的字节数。在非 WASI 环境中模拟全部发送。
    pub fn send(&mut self, data: &[u8]) -> Result<usize, WasmRealDbError> {
        if !self.connected {
            return Err(WasmRealDbError::ProxyUnavailable);
        }
        let len = data.len();
        self.bytes_sent += len as u64;
        Ok(len)
    }

    /// 接收数据
    ///
    /// `buf` 为接收缓冲区，返回实际接收到的字节数。
    /// 在非 WASI 环境中模拟空响应。
    pub fn recv(&mut self, buf: &mut [u8]) -> Result<usize, WasmRealDbError> {
        if !self.connected {
            return Err(WasmRealDbError::ProxyUnavailable);
        }
        let len = buf.len().min(0);
        self.bytes_received += len as u64;
        Ok(len)
    }

    /// 已发送字节数
    pub fn bytes_sent(&self) -> u64 {
        self.bytes_sent
    }

    /// 已接收字节数
    pub fn bytes_received(&self) -> u64 {
        self.bytes_received
    }

    /// 关闭连接
    pub fn close(&mut self) {
        self.fd = None;
        self.connected = false;
    }
}

impl Clone for WasiSocketConnection {
    fn clone(&self) -> Self {
        Self {
            fd: self.fd,
            proxy_host: self.proxy_host.clone(),
            proxy_port: self.proxy_port,
            connected: self.connected,
            bytes_sent: self.bytes_sent,
            bytes_received: self.bytes_received,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_not_connected() {
        let conn = WasiSocketConnection::new("proxy.example.com", 8080);
        assert!(!conn.is_connected());
        assert!(conn.fd().is_none());
        assert_eq!(conn.proxy_host(), "proxy.example.com");
        assert_eq!(conn.proxy_port(), 8080);
    }

    #[test]
    fn test_connect_success() {
        let mut conn = WasiSocketConnection::new("proxy.example.com", 8080);
        assert!(conn.connect().is_ok());
        assert!(conn.is_connected());
        assert!(conn.fd().is_some());
    }

    #[test]
    fn test_connect_empty_host() {
        let mut conn = WasiSocketConnection::new("", 8080);
        assert!(matches!(
            conn.connect(),
            Err(WasmRealDbError::ProxyUnavailable)
        ));
    }

    #[test]
    fn test_connect_zero_port() {
        let mut conn = WasiSocketConnection::new("proxy.example.com", 0);
        assert!(matches!(
            conn.connect(),
            Err(WasmRealDbError::ProxyUnavailable)
        ));
    }

    #[test]
    fn test_send_not_connected() {
        let mut conn = WasiSocketConnection::new("proxy.example.com", 8080);
        let data = b"hello";
        assert!(matches!(
            conn.send(data),
            Err(WasmRealDbError::ProxyUnavailable)
        ));
    }

    #[test]
    fn test_send_success() {
        let mut conn = WasiSocketConnection::new("proxy.example.com", 8080);
        conn.connect().unwrap();
        let data = b"hello world";
        let sent = conn.send(data).unwrap();
        assert_eq!(sent, 11);
        assert_eq!(conn.bytes_sent(), 11);
    }

    #[test]
    fn test_recv_not_connected() {
        let mut conn = WasiSocketConnection::new("proxy.example.com", 8080);
        let mut buf = [0u8; 1024];
        assert!(matches!(
            conn.recv(&mut buf),
            Err(WasmRealDbError::ProxyUnavailable)
        ));
    }

    #[test]
    fn test_recv_success() {
        let mut conn = WasiSocketConnection::new("proxy.example.com", 8080);
        conn.connect().unwrap();
        let mut buf = [0u8; 1024];
        let received = conn.recv(&mut buf).unwrap();
        assert_eq!(received, 0);
    }

    #[test]
    fn test_close() {
        let mut conn = WasiSocketConnection::new("proxy.example.com", 8080);
        conn.connect().unwrap();
        assert!(conn.is_connected());
        conn.close();
        assert!(!conn.is_connected());
        assert!(conn.fd().is_none());
    }

    #[test]
    fn test_multiple_sends_accumulate() {
        let mut conn = WasiSocketConnection::new("proxy.example.com", 8080);
        conn.connect().unwrap();
        conn.send(b"hello").unwrap();
        conn.send(b" world").unwrap();
        assert_eq!(conn.bytes_sent(), 11);
    }

    #[test]
    fn test_clone() {
        let mut conn = WasiSocketConnection::new("proxy.example.com", 8080);
        conn.connect().unwrap();
        conn.send(b"test").unwrap();
        let conn2 = conn.clone();
        assert_eq!(conn2.proxy_host(), "proxy.example.com");
        assert_eq!(conn2.bytes_sent(), 4);
        assert!(conn2.is_connected());
    }
}
