//! Connection Pool
//!
//! Provides async connection pooling with configurable options

use async_trait::async_trait;
use std::collections::VecDeque;
use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify};

use crate::error::PoolError;

/// 查询结果行类型别名：避免 `Connection::query` 签名触发 `clippy::type_complexity`。
pub type QueryRows = Vec<std::collections::HashMap<String, crate::value::Value>>;

/// 数据库连接 trait
///
/// 注意：此 trait 手动解糖 async 方法（不使用 `#[async_trait]`），
/// 以避免 `&str` 参数触发 HRTB 与 sqlx::Executor 冲突。
/// 所有 async 方法使用单一生命周期 `'a`（绑定 `&'a mut self` 和 `&'a str`），
/// 而非 HRTB，从而允许 sqlx 适配器实现。
pub trait Connection: Send + Sync {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>>;
    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, crate::DbError>> + Send + 'a>>;
    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>>;
    fn commit<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>>;
    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>>;
    fn is_connected(&self) -> bool;
    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>>;
    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>>;
}

/// 连接池中的连接条目，记录创建时间和最后使用时间
///
/// - `created_at`：连接的原始创建时间，**不**随 acquire/release 重置，
///   用于 `max_lifetime` 过期判定。
/// - `last_used_at`：上次归还到池的时间，用于 `idle_timeout` 空闲超时判定。
pub struct PooledConnection {
    conn: Box<dyn Connection>,
    created_at: Instant,
    last_used_at: Instant,
}

impl PooledConnection {
    fn new(conn: Box<dyn Connection>) -> Self {
        let now = Instant::now();
        Self {
            conn,
            created_at: now,
            last_used_at: now,
        }
    }

    fn is_expired(&self, max_lifetime: Duration) -> bool {
        self.created_at.elapsed() >= max_lifetime
    }

    fn is_idle_too_long(&self, idle_timeout: Duration) -> bool {
        self.last_used_at.elapsed() >= idle_timeout
    }

    /// 连接的原始创建时间（不随 acquire/release 重置）
    pub fn created_at(&self) -> Instant {
        self.created_at
    }

    /// 提取内部连接（消费 PooledConnection）
    ///
    /// 用于将连接传递给 `Transaction::new` 等消费连接的 API。
    /// 调用此方法后，连接不再属于池，调用方需自行管理其生命周期。
    pub fn into_inner(self) -> Box<dyn Connection> {
        self.conn
    }
}

impl Deref for PooledConnection {
    type Target = dyn Connection;

    fn deref(&self) -> &Self::Target {
        self.conn.as_ref()
    }
}

impl DerefMut for PooledConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.conn.as_mut()
    }
}

pub struct PoolConfig {
    pub max_size: u32,
    pub min_idle: u32,
    pub acquire_timeout: Duration,
    pub idle_timeout: Duration,
    pub max_lifetime: Duration,
    pub connection_timeout: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_size: 100,
            min_idle: 0,
            acquire_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_secs(1800),
            connection_timeout: Duration::from_secs(10),
        }
    }
}

impl Clone for PoolConfig {
    fn clone(&self) -> Self {
        Self {
            max_size: self.max_size,
            min_idle: self.min_idle,
            acquire_timeout: self.acquire_timeout,
            idle_timeout: self.idle_timeout,
            max_lifetime: self.max_lifetime,
            connection_timeout: self.connection_timeout,
        }
    }
}

impl PoolConfig {
    /// 校验配置合法性
    pub fn validate(&self) -> Result<(), PoolError> {
        if self.max_size == 0 {
            return Err(PoolError::InvalidConfig("max_size cannot be 0".to_string()));
        }
        if self.min_idle > self.max_size {
            return Err(PoolError::InvalidConfig(
                "min_idle cannot exceed max_size".to_string(),
            ));
        }
        Ok(())
    }
}

pub struct PoolStatus {
    pub idle: u32,
    pub active: u32,
    pub max: u32,
    pub min: u32,
}

impl std::fmt::Debug for PoolStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoolStatus")
            .field("idle", &self.idle)
            .field("active", &self.active)
            .field("max", &self.max)
            .field("min", &self.min)
            .finish()
    }
}

pub struct PoolConfigBuilder {
    config: PoolConfig,
}

impl PoolConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: PoolConfig::default(),
        }
    }

    pub fn max_size(mut self, size: u32) -> Self {
        self.config.max_size = size;
        self
    }

    pub fn min_idle(mut self, count: u32) -> Self {
        self.config.min_idle = count;
        self
    }

    pub fn acquire_timeout(mut self, timeout_secs: u64) -> Self {
        self.config.acquire_timeout = Duration::from_secs(timeout_secs);
        self
    }

    pub fn idle_timeout(mut self, timeout_secs: u64) -> Self {
        self.config.idle_timeout = Duration::from_secs(timeout_secs);
        self
    }

    pub fn max_lifetime(mut self, lifetime_secs: u64) -> Self {
        self.config.max_lifetime = Duration::from_secs(lifetime_secs);
        self
    }

    pub fn build(self) -> Result<PoolConfig, PoolError> {
        self.config.validate()?;
        Ok(self.config)
    }
}

impl Default for PoolConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 连接工厂 trait，用于创建新连接
#[async_trait]
pub trait ConnectionFactory: Send + Sync {
    async fn create(&self) -> Result<Box<dyn Connection>, crate::DbError>;
}

/// 连接池核心实现
pub struct Pool {
    config: PoolConfig,
    factory: Arc<dyn ConnectionFactory>,
    idle: Arc<Mutex<VecDeque<PooledConnection>>>,
    /// 池中总连接数（idle + borrowed）
    total_count: Arc<Mutex<u32>>,
    /// 池是否已关闭（close_all 后设为 true，拒绝新 acquire/release）
    closed: Arc<AtomicBool>,
    notify: Notify,
}

impl Pool {
    /// 创建连接池
    pub fn new(config: PoolConfig, factory: Arc<dyn ConnectionFactory>) -> Result<Self, PoolError> {
        config.validate()?;
        Ok(Self {
            config,
            factory,
            idle: Arc::new(Mutex::new(VecDeque::new())),
            total_count: Arc::new(Mutex::new(0)),
            closed: Arc::new(AtomicBool::new(false)),
            notify: Notify::new(),
        })
    }

    /// 获取配置
    pub fn config(&self) -> &PoolConfig {
        &self.config
    }

    /// 从池中获取连接（带超时）
    pub async fn acquire(&self) -> Result<PooledConnection, PoolError> {
        // close_all 后拒绝新 acquire
        if self.closed.load(Ordering::Acquire) {
            return Err(PoolError::Closed);
        }

        let deadline = Instant::now() + self.config.acquire_timeout;

        loop {
            // 尝试从空闲连接中获取
            {
                let mut idle = self.idle.lock().await;
                while let Some(mut pooled) = idle.pop_front() {
                    // 检查连接是否过期
                    if pooled.is_expired(self.config.max_lifetime) {
                        let _ = pooled.conn.close().await;
                        let mut total = self.total_count.lock().await;
                        *total = total.saturating_sub(1);
                        continue;
                    }
                    // 检查连接是否空闲过久
                    if pooled.is_idle_too_long(self.config.idle_timeout) {
                        let _ = pooled.conn.close().await;
                        let mut total = self.total_count.lock().await;
                        *total = total.saturating_sub(1);
                        continue;
                    }
                    // 检查连接是否仍然连接
                    if !pooled.conn.is_connected() {
                        let _ = pooled.conn.close().await;
                        let mut total = self.total_count.lock().await;
                        *total = total.saturating_sub(1);
                        continue;
                    }
                    return Ok(pooled);
                }
            }

            // 尝试创建新连接
            {
                let mut total = self.total_count.lock().await;
                if *total < self.config.max_size {
                    *total += 1;
                    drop(total);
                    match tokio::time::timeout(
                        self.config.connection_timeout,
                        self.factory.create(),
                    )
                    .await
                    {
                        Ok(Ok(conn)) => return Ok(PooledConnection::new(conn)),
                        Ok(Err(e)) => {
                            let mut total = self.total_count.lock().await;
                            *total = total.saturating_sub(1);
                            return Err(PoolError::ConnectionFailed(e.to_string()));
                        }
                        Err(_) => {
                            // tokio::time::timeout 的 Err 必为超时
                            let mut total = self.total_count.lock().await;
                            *total = total.saturating_sub(1);
                            return Err(PoolError::Timeout);
                        }
                    }
                }
            }

            // 等待连接释放或超时
            let now = Instant::now();
            if now >= deadline {
                return Err(PoolError::Timeout);
            }
            let _ = tokio::time::timeout(deadline - now, self.notify.notified()).await;
        }
    }

    /// 释放连接回池中
    /// 如果池已关闭或连接已断开，则直接关闭连接而不是放回池中。
    ///
    /// 接收 `PooledConnection` 以保留原始 `created_at`，避免 `max_lifetime`
    /// 在每次归还后被重置（Critical bug fix）。
    pub async fn release(&self, mut pooled: PooledConnection) {
        // 检查池是否已关闭
        if self.closed.load(Ordering::Acquire) {
            let _ = pooled.conn.close().await;
            let mut total = self.total_count.lock().await;
            *total = total.saturating_sub(1);
            return;
        }

        // 检查连接是否仍然有效
        if !pooled.conn.is_connected() {
            let _ = pooled.conn.close().await;
            let mut total = self.total_count.lock().await;
            *total = total.saturating_sub(1);
            return;
        }

        // 更新 last_used_at（归还时间），但保留 created_at（原始创建时间）
        pooled.last_used_at = Instant::now();

        {
            let mut idle = self.idle.lock().await;
            idle.push_back(pooled);
        }
        self.notify.notify_one();
    }

    /// 获取池状态
    pub async fn status(&self) -> PoolStatus {
        let idle_count = self.idle.lock().await.len() as u32;
        let active = *self.total_count.lock().await;
        PoolStatus {
            idle: idle_count,
            active,
            max: self.config.max_size,
            min: self.config.min_idle,
        }
    }

    /// 回收空闲过久的连接
    pub async fn reap_idle(&self) {
        let mut idle = self.idle.lock().await;
        let mut to_close = Vec::new();
        let mut remaining = VecDeque::new();
        while let Some(pooled) = idle.pop_front() {
            if pooled.is_idle_too_long(self.config.idle_timeout)
                || pooled.is_expired(self.config.max_lifetime)
            {
                to_close.push(pooled);
            } else {
                remaining.push_back(pooled);
            }
        }
        *idle = remaining;
        drop(idle);
        for mut pooled in to_close {
            let _ = pooled.conn.close().await;
            let mut total = self.total_count.lock().await;
            *total = total.saturating_sub(1);
        }
    }

    /// 关闭所有空闲连接，并标记池为已关闭
    /// 注意：已借出未归还的连接不受影响，但归还时会被直接关闭；
    /// 同时 close_all 后的新 acquire 也会被拒绝。
    pub async fn close_all(&self) {
        // 标记为已关闭，阻止新 acquire/release
        self.closed.store(true, Ordering::Release);
        // 关闭所有空闲连接
        let mut idle = self.idle.lock().await;
        let mut closed_count: u32 = 0;
        while let Some(mut pooled) = idle.pop_front() {
            let _ = pooled.conn.close().await;
            closed_count = closed_count.saturating_add(1);
        }
        drop(idle);
        // 减少总连接计数（只减去已关闭的空闲连接数）
        let mut total = self.total_count.lock().await;
        *total = total.saturating_sub(closed_count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用的模拟连接
    struct MockConnection {
        connected: bool,
    }

    impl MockConnection {
        fn new() -> Self {
            Self { connected: true }
        }
    }

    impl Connection for MockConnection {
        fn execute<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(1) })
        }

        fn query<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            Vec<std::collections::HashMap<String, crate::value::Value>>,
                            crate::DbError,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move { Ok(vec![]) })
        }

        fn begin_transaction<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(()) })
        }

        fn commit<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(()) })
        }

        fn rollback<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(()) })
        }

        fn is_connected(&self) -> bool {
            self.connected
        }

        fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            Box::pin(async move { true })
        }

        fn close<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
            Box::pin(async move {
                self.connected = false;
                Ok(())
            })
        }
    }

    struct MockConnectionFactory;

    #[async_trait]
    impl ConnectionFactory for MockConnectionFactory {
        async fn create(&self) -> Result<Box<dyn Connection>, crate::DbError> {
            Ok(Box::new(MockConnection::new()))
        }
    }

    #[tokio::test]
    async fn test_pool_config_builder() {
        let config = PoolConfigBuilder::new()
            .max_size(50)
            .min_idle(10)
            .build()
            .unwrap();

        assert_eq!(config.max_size, 50);
        assert_eq!(config.min_idle, 10);
    }

    #[test]
    fn test_pool_status_display() {
        let status = PoolStatus {
            idle: 5,
            active: 10,
            max: 100,
            min: 5,
        };

        let display = format!("{:?}", status);
        assert!(display.contains("idle"));
        assert!(display.contains("active"));
    }

    #[test]
    fn test_default_pool_config() {
        let config = PoolConfig::default();
        assert_eq!(config.max_size, 100);
        assert_eq!(config.min_idle, 0);
        assert_eq!(config.acquire_timeout.as_secs(), 30);
        assert_eq!(config.idle_timeout.as_secs(), 600);
        assert_eq!(config.max_lifetime.as_secs(), 1800);
    }

    #[tokio::test]
    async fn test_pool_config_clone() {
        let config = PoolConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.max_size, config.max_size);
        assert_eq!(cloned.min_idle, config.min_idle);
    }

    #[test]
    fn test_pool_config_builder_default() {
        let builder = PoolConfigBuilder::new();
        let config = builder.build().unwrap();
        assert_eq!(config.max_size, 100);
    }

    #[test]
    fn test_pool_config_validate() {
        let result = PoolConfigBuilder::new().max_size(0).build();
        assert!(result.is_err());

        let result = PoolConfigBuilder::new().max_size(10).min_idle(20).build();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_pool_acquire_and_release() {
        let config = PoolConfigBuilder::new()
            .max_size(5)
            .min_idle(1)
            .build()
            .unwrap();
        let factory = Arc::new(MockConnectionFactory);
        let pool = Pool::new(config, factory).unwrap();

        let conn = pool.acquire().await.unwrap();
        let status = pool.status().await;
        assert_eq!(status.active, 1);
        assert_eq!(status.idle, 0);

        pool.release(conn).await;
        let status = pool.status().await;
        assert_eq!(status.idle, 1);

        // 再次获取应该复用空闲连接
        let _conn2 = pool.acquire().await.unwrap();
        let status = pool.status().await;
        assert_eq!(status.idle, 0);
    }

    #[tokio::test]
    async fn test_pool_status() {
        let config = PoolConfigBuilder::new()
            .max_size(10)
            .min_idle(2)
            .build()
            .unwrap();
        let factory = Arc::new(MockConnectionFactory);
        let pool = Pool::new(config, factory).unwrap();

        let status = pool.status().await;
        assert_eq!(status.max, 10);
        assert_eq!(status.min, 2);
        assert_eq!(status.active, 0);
    }

    #[tokio::test]
    async fn test_pool_close_all() {
        let config = PoolConfigBuilder::new().max_size(5).build().unwrap();
        let factory = Arc::new(MockConnectionFactory);
        let pool = Pool::new(config, factory).unwrap();

        // 创建几个连接然后释放
        let conn1 = pool.acquire().await.unwrap();
        let conn2 = pool.acquire().await.unwrap();
        pool.release(conn1).await;
        pool.release(conn2).await;

        pool.close_all().await;
        let status = pool.status().await;
        assert_eq!(status.idle, 0);
        assert_eq!(status.active, 0);
    }

    #[tokio::test]
    async fn test_pool_reap_idle() {
        let config = PoolConfigBuilder::new()
            .max_size(5)
            .idle_timeout(0) // 立即超时
            .build()
            .unwrap();
        let factory = Arc::new(MockConnectionFactory);
        let pool = Pool::new(config, factory).unwrap();

        let conn = pool.acquire().await.unwrap();
        pool.release(conn).await;

        // 等待一下确保空闲超时
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        pool.reap_idle().await;
        let status = pool.status().await;
        assert_eq!(status.idle, 0);
    }
}
