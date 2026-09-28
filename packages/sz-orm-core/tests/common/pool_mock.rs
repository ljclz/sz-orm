//! v9.2.0 M6：连接池测试共享 mock（纯单元测试，无 DB 依赖）

use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sz_orm_core::{Connection, ConnectionFactory, DbError, PoolConfig, QueryRows};

/// 简单 mock 连接：所有操作成功，is_connected 返回 true
pub struct MockConnection {
    connected: AtomicBool,
}

impl MockConnection {
    pub fn new() -> Self {
        Self {
            connected: AtomicBool::new(true),
        }
    }
}

impl Default for MockConnection {
    fn default() -> Self {
        Self::new()
    }
}

impl Connection for MockConnection {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(1) })
    }

    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(vec![]) })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        let connected = self.connected.load(Ordering::Relaxed);
        Box::pin(async move { connected })
    }

    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.connected.store(false, Ordering::Relaxed);
            Ok(())
        })
    }
}

/// mock 连接工厂：每次 create 返回新 MockConnection
pub struct MockConnectionFactory;

#[async_trait]
impl ConnectionFactory for MockConnectionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        Ok(Box::new(MockConnection::new()))
    }
}

/// 计数工厂：记录 create 调用次数
pub struct CountingConnectionFactory {
    pub count: AtomicU32,
}

impl CountingConnectionFactory {
    pub fn new() -> Self {
        Self {
            count: AtomicU32::new(0),
        }
    }

    pub fn create_count(&self) -> u32 {
        self.count.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl ConnectionFactory for CountingConnectionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        self.count.fetch_add(1, Ordering::Relaxed);
        Ok(Box::new(MockConnection::new()))
    }
}

/// 失败工厂：始终返回错误
pub struct FailingConnectionFactory;

#[async_trait]
impl ConnectionFactory for FailingConnectionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        Err(DbError::ConnectionError("injected failure".to_string()))
    }
}

/// 创建测试用连接池配置
pub fn test_config(max_size: u32) -> PoolConfig {
    PoolConfig {
        max_size,
        min_idle: 0,
        acquire_timeout: Duration::from_secs(2),
        idle_timeout: Duration::from_secs(60),
        max_lifetime: Duration::from_secs(300),
        connection_timeout: Duration::from_secs(2),
        tls: None,
        query_timeout: None,
        max_rows: None,
        memory_limit: None,
        on_event: None,
        test_before_acquire: false,
        prewarm: false,
    }
}

/// 创建带 prewarm 的测试配置
pub fn prewarm_config(max_size: u32, min_idle: u32) -> PoolConfig {
    PoolConfig {
        max_size,
        min_idle,
        acquire_timeout: Duration::from_secs(2),
        idle_timeout: Duration::from_secs(60),
        max_lifetime: Duration::from_secs(300),
        connection_timeout: Duration::from_secs(2),
        tls: None,
        query_timeout: None,
        max_rows: None,
        memory_limit: None,
        on_event: None,
        test_before_acquire: false,
        prewarm: true,
    }
}

/// 创建池（使用 MockConnectionFactory）
pub fn create_pool(max_size: u32) -> sz_orm_core::Pool {
    sz_orm_core::Pool::new(test_config(max_size), Arc::new(MockConnectionFactory)).unwrap()
}

/// 创建池（使用 CountingConnectionFactory）
pub fn create_counting_pool(max_size: u32) -> (sz_orm_core::Pool, Arc<CountingConnectionFactory>) {
    let factory = Arc::new(CountingConnectionFactory::new());
    let pool = sz_orm_core::Pool::new(test_config(max_size), factory.clone()).unwrap();
    (pool, factory)
}