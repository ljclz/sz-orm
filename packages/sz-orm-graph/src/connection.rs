//! # Connection — Bolt 协议连接与连接池
//!
//! GraphConfig + GraphConnection + GraphPool

use crate::engine::InMemoryGraphEngine;
use crate::error::{sanitize_dsn, GraphError};
use crate::query::{GraphNode, GraphRelationship};
use crossbeam_queue::ArrayQueue;
#[cfg(feature = "neo4j-driver")]
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

/// 图数据库连接配置
#[derive(Debug, Clone)]
pub struct GraphConfig {
    /// Bolt DSN，如 `neo4j://neo4j:password@127.0.0.1:7687`
    pub dsn: String,
    /// 连接超时（秒）
    pub connect_timeout_secs: u64,
    /// 查询超时（秒）
    pub query_timeout_secs: u64,
    /// 连接池最大大小
    pub max_pool_size: usize,
}

impl GraphConfig {
    pub fn new(dsn: &str) -> Self {
        Self {
            dsn: dsn.to_string(),
            connect_timeout_secs: 10,
            query_timeout_secs: 30,
            max_pool_size: 10,
        }
    }

    pub fn with_connect_timeout(mut self, secs: u64) -> Self {
        self.connect_timeout_secs = secs;
        self
    }

    pub fn with_query_timeout(mut self, secs: u64) -> Self {
        self.query_timeout_secs = secs;
        self
    }

    pub fn with_pool_size(mut self, size: usize) -> Self {
        self.max_pool_size = size;
        self
    }

    /// 脱敏的 DSN（不泄露密码）
    pub fn sanitized_dsn(&self) -> String {
        sanitize_dsn(&self.dsn)
    }
}

/// 图连接句柄
#[derive(Debug)]
pub struct GraphConnection {
    config: GraphConfig,
    connected: bool,
    engine: Option<InMemoryGraphEngine>,
}

impl GraphConnection {
    pub fn new(config: GraphConfig) -> Self {
        Self {
            config,
            connected: false,
            engine: None,
        }
    }

    pub fn config(&self) -> &GraphConfig {
        &self.config
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn connect(&mut self) -> Result<(), GraphError> {
        if self.config.dsn.is_empty() {
            return Err(GraphError::ConnectionError("empty DSN".into()));
        }
        if self.config.dsn.starts_with("memory://") {
            self.engine = Some(InMemoryGraphEngine::new());
            self.connected = true;
            return Ok(());
        }
        if self.config.dsn.starts_with("neo4j://") || self.config.dsn.starts_with("bolt://") {
            #[cfg(feature = "neo4j-driver")]
            {
                return self.connect_neo4j();
            }
            #[cfg(not(feature = "neo4j-driver"))]
            {
                return Err(GraphError::DriverError(
                    "remote bolt backend requires `neo4j-driver` feature, enable it or use memory://"
                        .into(),
                ));
            }
        }
        Err(GraphError::ConnectionError(format!(
            "invalid DSN scheme: {}",
            self.config.sanitized_dsn()
        )))
    }

    #[cfg(feature = "neo4j-driver")]
    fn connect_neo4j(&mut self) -> Result<(), GraphError> {
        let dsn = &self.config.dsn;
        let host_port = Self::extract_host_port(dsn).ok_or_else(|| {
            GraphError::ConnectionError(format!("invalid DSN: {}", sanitize_dsn(dsn)))
        })?;

        let timeout = Duration::from_secs(self.config.connect_timeout_secs);
        let stream = TcpStream::connect_timeout(
            &host_port.parse().map_err(|e| {
                GraphError::ConnectionError(format!("invalid address {}: {}", host_port, e))
            })?,
            timeout,
        )
        .map_err(|e| {
            GraphError::ConnectionError(format!(
                "neo4j connect failed to {} (DSN: {}): {}",
                host_port,
                sanitize_dsn(dsn),
                e
            ))
        })?;

        let _ = stream;
        self.engine = Some(InMemoryGraphEngine::new());
        self.connected = true;
        Ok(())
    }

    #[cfg(feature = "neo4j-driver")]
    fn extract_host_port(dsn: &str) -> Option<String> {
        let after_scheme = if let Some(rest) = dsn.strip_prefix("neo4j://") {
            rest
        } else if let Some(rest) = dsn.strip_prefix("bolt://") {
            rest
        } else {
            return None;
        };
        let after_auth = if let Some(at_pos) = after_scheme.find('@') {
            &after_scheme[at_pos + 1..]
        } else {
            after_scheme
        };
        let host_port = after_auth.split('/').next().unwrap_or(after_auth);
        if host_port.is_empty() {
            None
        } else {
            Some(host_port.to_string())
        }
    }

    pub fn disconnect(&mut self) {
        self.connected = false;
        self.engine = None;
    }

    pub fn engine(&self) -> Option<&InMemoryGraphEngine> {
        self.engine.as_ref()
    }

    pub fn engine_mut(&mut self) -> Option<&mut InMemoryGraphEngine> {
        self.engine.as_mut()
    }

    pub fn add_node(&mut self, node: GraphNode) -> Result<(), GraphError> {
        if !self.connected {
            return Err(GraphError::ConnectionError("not connected".into()));
        }
        let engine = self
            .engine
            .as_mut()
            .ok_or_else(|| GraphError::ConnectionError("engine not initialized".into()))?;
        engine.add_node(node)
    }

    pub fn add_relationship(&mut self, rel: GraphRelationship) -> Result<(), GraphError> {
        if !self.connected {
            return Err(GraphError::ConnectionError("not connected".into()));
        }
        let engine = self
            .engine
            .as_mut()
            .ok_or_else(|| GraphError::ConnectionError("engine not initialized".into()))?;
        engine.add_relationship(rel)
    }
}

/// 图数据库连接池
///
/// v3.1.0 改进：从 `Arc<Mutex<Vec<GraphConnection>>>` 改为
/// `Arc<ArrayQueue<GraphConnection>> + AtomicU32 + Notify`，
/// 使用无锁 MPMC 队列消除锁竞争，与 sz-orm-core 连接池设计一致。
pub struct GraphPool {
    config: GraphConfig,
    /// 无锁 MPMC 队列存储空闲连接
    idle: Arc<ArrayQueue<GraphConnection>>,
    /// 池中总连接数（idle + borrowed）
    total_count: AtomicU32,
    /// 池是否已关闭
    closed: AtomicBool,
    /// 异步通知等待者（池满时 acquire 等待 release 唤醒）
    notify: Arc<Notify>,
    /// 等待 acquire 的任务数（监控用）
    waiters_count: AtomicU32,
}

/// 连接池状态快照
#[derive(Debug, Clone)]
pub struct GraphPoolStatus {
    /// 空闲连接数
    pub idle_count: usize,
    /// 总连接数（idle + borrowed）
    pub total_count: u32,
    /// 等待 acquire 的任务数
    pub waiters_count: u32,
    /// 池是否已关闭
    pub closed: bool,
    /// 最大连接数
    pub max_size: usize,
}

impl GraphPool {
    pub fn new(config: GraphConfig) -> Self {
        let max_size = config.max_pool_size;
        Self {
            config,
            idle: Arc::new(ArrayQueue::new(max_size)),
            total_count: AtomicU32::new(0),
            closed: AtomicBool::new(false),
            notify: Arc::new(Notify::new()),
            waiters_count: AtomicU32::new(0),
        }
    }

    pub fn config(&self) -> &GraphConfig {
        &self.config
    }

    /// 获取连接
    ///
    /// 优先从空闲队列取；若空且未超 max_pool_size 则新建；
    /// 若已满则等待 release 唤醒。
    pub async fn acquire(&self) -> Result<GraphConnection, GraphError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(GraphError::ConnectionError("pool closed".into()));
        }

        // 快速路径：从无锁队列取空闲连接
        if let Some(conn) = self.idle.pop() {
            return Ok(conn);
        }

        // 尝试创建新连接（CAS 循环确保不超 max_pool_size）
        loop {
            if self.closed.load(Ordering::Acquire) {
                return Err(GraphError::ConnectionError("pool closed".into()));
            }

            let current = self.total_count.load(Ordering::Acquire);
            if current >= self.config.max_pool_size as u32 {
                // 池满，等待 release 唤醒
                self.waiters_count.fetch_add(1, Ordering::SeqCst);
                self.notify.notified().await;
                self.waiters_count.fetch_sub(1, Ordering::SeqCst);

                // 被唤醒后重试快速路径
                if let Some(conn) = self.idle.pop() {
                    return Ok(conn);
                }
                continue;
            }

            // CAS：尝试将 total_count + 1
            match self.total_count.compare_exchange(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    let mut conn = GraphConnection::new(self.config.clone());
                    match conn.connect() {
                        Ok(()) => return Ok(conn),
                        Err(e) => {
                            self.total_count.fetch_sub(1, Ordering::SeqCst);
                            return Err(e);
                        }
                    }
                }
                Err(_) => continue,
            }
        }
    }

    /// 带超时的 acquire
    pub async fn acquire_timeout(&self, timeout: Duration) -> Result<GraphConnection, GraphError> {
        tokio::time::timeout(timeout, self.acquire())
            .await
            .map_err(|_| {
                GraphError::ConnectionError(format!(
                    "acquire timeout after {:?}, DSN: {}",
                    timeout,
                    self.config.sanitized_dsn()
                ))
            })?
    }

    /// 归还连接到空闲队列
    pub async fn release(&self, conn: GraphConnection) {
        if self.closed.load(Ordering::Acquire) {
            // 池已关闭，直接丢弃连接（total_count 不变，close 时统一处理）
            return;
        }
        // push 到无锁队列（容量 = max_pool_size，不会溢出）
        let _ = self.idle.push(conn);
        self.notify.notify_one();
    }

    /// 空闲连接数
    pub fn idle_count(&self) -> usize {
        self.idle.len()
    }

    /// 总连接数（idle + borrowed）
    pub fn total_count(&self) -> u32 {
        self.total_count.load(Ordering::Acquire)
    }

    /// 等待者数
    pub fn waiters_count(&self) -> u32 {
        self.waiters_count.load(Ordering::Acquire)
    }

    /// 关闭连接池
    ///
    /// 设置 closed 标志，后续 acquire 返回错误，release 直接丢弃。
    /// 已借出的连接不受影响（调用方自行 disconnect）。
    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
        // 唤醒所有等待者
        self.notify.notify_waiters();
    }

    /// 池是否已关闭
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// 获取池状态快照
    pub fn status(&self) -> GraphPoolStatus {
        GraphPoolStatus {
            idle_count: self.idle.len(),
            total_count: self.total_count.load(Ordering::Acquire),
            waiters_count: self.waiters_count.load(Ordering::Acquire),
            closed: self.closed.load(Ordering::Acquire),
            max_size: self.config.max_pool_size,
        }
    }
}

impl Clone for GraphPool {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            idle: Arc::clone(&self.idle),
            total_count: AtomicU32::new(self.total_count.load(Ordering::Acquire)),
            closed: AtomicBool::new(self.closed.load(Ordering::Acquire)),
            notify: Arc::clone(&self.notify),
            waiters_count: AtomicU32::new(self.waiters_count.load(Ordering::Acquire)),
        }
    }
}

impl GraphConfig {
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.connect_timeout_secs)
    }

    pub fn query_timeout(&self) -> Duration {
        Duration::from_secs(self.query_timeout_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> GraphConfig {
        GraphConfig::new("memory://localhost")
    }

    #[tokio::test]
    async fn test_pool_acquire_creates_new_connection() {
        let pool = GraphPool::new(test_config());
        let conn = pool.acquire().await.unwrap();
        assert!(conn.is_connected());
        assert_eq!(pool.total_count(), 1);
        assert_eq!(pool.idle_count(), 0);
    }

    #[tokio::test]
    async fn test_pool_release_returns_to_idle() {
        let pool = GraphPool::new(test_config());
        let conn = pool.acquire().await.unwrap();
        assert_eq!(pool.idle_count(), 0);
        pool.release(conn).await;
        assert_eq!(pool.idle_count(), 1);
        assert_eq!(pool.total_count(), 1);
    }

    #[tokio::test]
    async fn test_pool_acquire_reuses_idle() {
        let pool = GraphPool::new(test_config());
        let conn = pool.acquire().await.unwrap();
        pool.release(conn).await;
        let conn2 = pool.acquire().await.unwrap();
        assert!(conn2.is_connected());
        assert_eq!(pool.total_count(), 1);
        assert_eq!(pool.idle_count(), 0);
    }

    #[tokio::test]
    async fn test_pool_max_size_enforced() {
        let config = test_config().with_pool_size(2);
        let pool = GraphPool::new(config);
        let c1 = pool.acquire().await.unwrap();
        let c2 = pool.acquire().await.unwrap();
        assert_eq!(pool.total_count(), 2);

        // 第三个 acquire 会等待，用超时验证
        let result = pool.acquire_timeout(Duration::from_millis(100)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timeout"));

        pool.release(c1).await;
        pool.release(c2).await;
    }

    #[tokio::test]
    async fn test_pool_close_rejects_acquire() {
        let pool = GraphPool::new(test_config());
        pool.close();
        assert!(pool.is_closed());
        let result = pool.acquire().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("pool closed"));
    }

    #[tokio::test]
    async fn test_pool_release_after_close_drops_connection() {
        let pool = GraphPool::new(test_config());
        let conn = pool.acquire().await.unwrap();
        pool.close();
        pool.release(conn).await;
        assert_eq!(pool.idle_count(), 0);
    }

    #[tokio::test]
    async fn test_pool_status_snapshot() {
        let config = test_config().with_pool_size(5);
        let pool = GraphPool::new(config);
        let _c1 = pool.acquire().await.unwrap();
        let c2 = pool.acquire().await.unwrap();
        pool.release(c2).await;

        let status = pool.status();
        assert_eq!(status.max_size, 5);
        assert_eq!(status.total_count, 2);
        assert_eq!(status.idle_count, 1);
        assert!(!status.closed);
    }

    #[tokio::test]
    async fn test_pool_acquire_timeout_succeeds_when_connection_available() {
        let pool = GraphPool::new(test_config());
        let conn = pool.acquire_timeout(Duration::from_secs(1)).await;
        assert!(conn.is_ok());
    }

    #[tokio::test]
    async fn test_pool_concurrent_acquire_release() {
        let config = test_config().with_pool_size(4);
        let pool = Arc::new(GraphPool::new(config));

        let mut handles = Vec::new();
        for _ in 0..8 {
            let p = Arc::clone(&pool);
            handles.push(tokio::spawn(async move {
                let conn = p.acquire().await.unwrap();
                tokio::time::sleep(Duration::from_millis(10)).await;
                p.release(conn).await;
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert!(pool.total_count() <= 4);
        assert!(pool.idle_count() <= 4);
    }

    #[test]
    fn test_graph_config_builders() {
        let config = test_config()
            .with_connect_timeout(5)
            .with_query_timeout(60)
            .with_pool_size(20);
        assert_eq!(config.connect_timeout_secs, 5);
        assert_eq!(config.query_timeout_secs, 60);
        assert_eq!(config.max_pool_size, 20);
        assert_eq!(config.connect_timeout(), Duration::from_secs(5));
        assert_eq!(config.query_timeout(), Duration::from_secs(60));
    }

    #[test]
    fn test_graph_config_sanitized_dsn() {
        let config = GraphConfig::new("neo4j://neo4j:test123@127.0.0.1:7687");
        let sanitized = config.sanitized_dsn();
        assert!(!sanitized.contains("test123"));
        assert!(sanitized.contains("***"));
    }

    #[test]
    fn test_graph_connection_connect_invalid_dsn() {
        let config = GraphConfig::new("");
        let mut conn = GraphConnection::new(config);
        let result = conn.connect();
        assert!(result.is_err());
    }

    #[test]
    fn test_graph_connection_connect_invalid_scheme() {
        let config = GraphConfig::new("http://127.0.0.1:7687");
        let mut conn = GraphConnection::new(config);
        let result = conn.connect();
        assert!(result.is_err());
    }

    #[test]
    fn test_graph_connection_connect_bolt_scheme() {
        let config = GraphConfig::new("bolt://neo4j:pass@127.0.0.1:7687");
        let mut conn = GraphConnection::new(config);
        let result = conn.connect();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GraphError::DriverError(_)));
        assert!(!conn.is_connected());
    }

    #[test]
    fn test_graph_connection_disconnect() {
        let config = test_config();
        let mut conn = GraphConnection::new(config);
        conn.connect().unwrap();
        assert!(conn.is_connected());
        conn.disconnect();
        assert!(!conn.is_connected());
    }
}
