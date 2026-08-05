//! Connection Pool
//!
//! Provides async connection pooling with configurable options

use async_trait::async_trait;
use crossbeam_queue::ArrayQueue;
use futures::StreamExt;
// P1-4 修复：使用核心层定义的 CircuitBreaker/RateLimiter 抽象，
// 消除对 sz-orm-health/sz-orm-limit 的反向依赖。
// parking_lot 锁仅在启用 circuit-breaker/rate-limit feature 时使用
#[cfg(feature = "circuit-breaker")]
use parking_lot::Mutex as PlMutex;
#[cfg(feature = "rate-limit")]
use parking_lot::RwLock as PlRwLock;
use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;

// P1-4 修复：CircuitBreaker/RateLimiter 抽象已提升到核心层，
// 仅在启用相应 feature 时导入（避免 default feature 下的 unused imports）
// 注意：trait 方法（can_execute/record_success 等）需要 trait 在 scope 中
#[cfg(feature = "circuit-breaker")]
use crate::circuit_breaker::{CircuitBreaker, CircuitState, DefaultCircuitBreaker};
use crate::error::PoolError;
#[cfg(feature = "rate-limit")]
use crate::rate_limiter::RateLimiter;

/// 查询结果行类型别名：避免 `Connection::query` 签名触发 `clippy::type_complexity`。
pub type QueryRows = Vec<std::collections::HashMap<String, crate::value::Value>>;

/// 流式查询结果项类型别名：避免 `Connection::query_stream` 签名触发 `clippy::type_complexity`。
pub type QueryStreamItem =
    Result<std::collections::HashMap<String, crate::value::Value>, crate::DbError>;

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

    /// 参数绑定执行（INSERT/UPDATE/DELETE）
    ///
    /// 使用真实 prepared statement 绑定参数，避免 SQL 注入。
    /// 默认实现返回 `NotImplemented` 错误；支持参数绑定的适配器
    /// （如 sz-orm-oracle）应覆盖此方法。
    fn execute_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [crate::value::Value],
    ) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>> {
        let _ = (sql, params);
        Box::pin(async move {
            Err(crate::DbError::Internal(
                "execute_with_params not implemented for this adapter".to_string(),
            ))
        })
    }

    /// 参数绑定查询（SELECT）
    ///
    /// 使用真实 prepared statement 绑定参数，避免 SQL 注入。
    /// 默认实现返回 `NotImplemented` 错误；支持参数绑定的适配器
    /// （如 sz-orm-oracle）应覆盖此方法。
    fn query_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [crate::value::Value],
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, crate::DbError>> + Send + 'a>> {
        let _ = (sql, params);
        Box::pin(async move {
            Err(crate::DbError::Internal(
                "query_with_params not implemented for this adapter".to_string(),
            ))
        })
    }

    /// 位置式查询（SELECT）：返回 `(列名, 按列顺序的值矩阵)`
    ///
    /// 绕过 `HashMap<String, Value>` 行映射，适用于 SELECT ALL 大结果集场景。
    /// 默认实现返回 `NotImplemented` 错误；适配器可覆盖此方法以获得 30%~50% 性能提升。
    fn query_values<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<crate::value::QueryValues, crate::DbError>> + Send + 'a>>
    {
        let _ = sql;
        Box::pin(async move {
            Err(crate::DbError::Internal(
                "query_values not implemented for this adapter".to_string(),
            ))
        })
    }

    /// 参数绑定位置式查询（SELECT）：叠加 prepared statement + 位置式映射双重优化
    ///
    /// 默认实现返回 `NotImplemented` 错误；适配器可覆盖此方法以获得最佳性能。
    fn query_values_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [crate::value::Value],
    ) -> Pin<Box<dyn Future<Output = Result<crate::value::QueryValues, crate::DbError>> + Send + 'a>>
    {
        let _ = (sql, params);
        Box::pin(async move {
            Err(crate::DbError::Internal(
                "query_values_with_params not implemented for this adapter".to_string(),
            ))
        })
    }

    /// 流式查询：返回逐行结果流
    ///
    /// 默认实现：通过 `query()` 获取全部行后，以
    /// `futures::stream::iter` 逐行 yield，提供统一的流式消费接口。
    /// 适合中小结果集；对超大结果集，支持原生游标的适配器应覆盖此方法。
    ///
    /// # 注意
    ///
    /// 此方法本身是同步的（返回 Stream），但内部通过 `futures::stream::once`
    /// 异步获取数据后展开为逐行流。若适配器支持 sqlx `fetch()` 游标，
    /// 覆盖此方法可获得真正的逐行拉取，避免大结果集内存峰值。
    fn query_stream<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn futures::Stream<Item = QueryStreamItem> + Send + 'a>> {
        // 克隆 sql 以脱离 &self 的生命周期
        let sql_owned = sql.to_string();
        // 使用 stream::once 异步执行查询，再 flat_map 为逐行流
        let stream = futures::stream::once(async move { self.query(&sql_owned).await })
            // 统一为 Vec 收集后再 iter：保证 match 两臂流类型一致（E0308 修复）
            .map(|result| {
                let items: Vec<QueryStreamItem> = match result {
                    Ok(rows) => rows.into_iter().map(Ok).collect(),
                    Err(e) => vec![Err(e)],
                };
                futures::stream::iter(items)
            })
            .flatten();
        Box::pin(stream)
    }

    /// 游标式流式查询（P1-2）：按 `batch_size` 分批拉取，避免大结果集内存峰值。
    ///
    /// 适用于无原生服务器端游标（或无法便捷暴露逐行拉取）的数据库：
    /// - Oracle：`ROWNUM` 子查询包装（见 `cursor_stream::build_paged_query`）；
    /// - SQL Server：`OFFSET ... ROWS FETCH NEXT ... ROWS ONLY`。
    ///
    /// 默认实现退化为 [`Connection::query_stream`]（全量拉取后逐行 yield）；
    /// Oracle/MSSQL 适配器应覆盖此方法，使用
    /// `cursor_stream::stream_cursor_paged(conn, sql, DbType::Oracle, batch)`
    /// 获得真正的分页游标流。
    fn query_stream_cursor<'a>(
        &'a mut self,
        sql: &'a str,
        _batch_size: usize,
    ) -> Pin<Box<dyn futures::Stream<Item = QueryStreamItem> + Send + 'a>> {
        self.query_stream(sql)
    }

    /// 批量执行多条 SQL（按顺序执行，返回累计影响行数）
    ///
    /// 默认实现循环调用 `execute`；适配器可覆盖此方法以利用数据库原生
    /// 批量执行能力。
    fn execute_batch<'a>(
        &'a mut self,
        sqls: &'a [String],
    ) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>> {
        Box::pin(async move {
            let mut total = 0u64;
            for sql in sqls {
                total += self.execute(sql).await?;
            }
            Ok(total)
        })
    }

    /// 批量插入（单条 SQL 多次参数绑定执行）
    ///
    /// 默认实现循环调用 `execute_with_params`；适配器可覆盖此方法
    /// 以利用数据库原生批量 DML 能力（如 Oracle Array DML）。
    fn execute_batch_params<'a>(
        &'a mut self,
        sql: &'a str,
        params_batch: &'a [Vec<crate::value::Value>],
    ) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>> {
        Box::pin(async move {
            let mut total = 0u64;
            for params in params_batch {
                total += self.execute_with_params(sql, params).await?;
            }
            Ok(total)
        })
    }
}

/// 连接池中的连接条目，记录创建时间和最后使用时间
///
/// - `created_at`：连接的原始创建时间，**不**随 acquire/release 重置，
///   用于 `max_lifetime` 过期判定。
/// - `last_used_at`：上次归还到池的时间，用于 `idle_timeout` 空闲超时判定。
/// - `pool`：归属的连接池引用，Drop 时自动归还。`None` 表示无需归还
///   （已通过 `release()`/`into_inner()` 显式处理）。
pub struct PooledConnection {
    conn: Box<dyn Connection>,
    created_at: Instant,
    last_used_at: Instant,
    pool: Option<Pool>,
}

impl PooledConnection {
    fn new(conn: Box<dyn Connection>, pool: Pool) -> Self {
        let now = Instant::now();
        Self {
            conn,
            created_at: now,
            last_used_at: now,
            pool: Some(pool),
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
    pub fn into_inner(mut self) -> Box<dyn Connection> {
        self.pool = None; // 标记无需归还
                          // PooledConnection 实现了 Drop，不能直接 move conn，
                          // 用 mem::replace 取出连接，放入 ClosedConnection 占位符
        std::mem::replace(&mut self.conn, Box::new(ClosedConnection))
    }
}

/// PooledConnection 的 Drop 实现：自动归还连接到池中
///
/// 修复 Critical Bug：之前 PooledConnection 未实现 Drop，连接在 drop 时
/// 丢失，不归还池中，导致池耗尽。
///
/// 实现策略：
/// 1. 如果 `pool` 为 `Some`（未显式 release/into_inner），取出连接并放入
///    `ClosedConnection` 占位符
/// 2. 在 tokio runtime 中 spawn 异步 release（Drop 不能 await）
/// 3. 如果不在 tokio runtime 中（P0 修复）：手动递减 `total_count`，
///    避免池容量被耗尽；连接随 `pooled` drop 自然释放（依赖底层连接 Drop）
impl Drop for PooledConnection {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.take() {
            // 取出原始连接，放入占位符（避免重复 close）
            let conn = std::mem::replace(&mut self.conn, Box::new(ClosedConnection));
            let pooled = PooledConnection {
                conn,
                created_at: self.created_at,
                last_used_at: self.last_used_at,
                pool: None,
            };
            // 尝试在 tokio runtime 中异步归还
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    pool.release(pooled).await;
                });
            } else {
                // 不在 tokio runtime 中：手动递减计数器，避免池容量泄漏
                // 注意：close 是 async 方法，无法在 sync Drop 中 await；
                //       连接随 `pooled` drop 自然释放（依赖底层连接 Drop）
                drop(pooled);
                pool.total_count.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }
}

/// 占位连接，用于 PooledConnection::Drop 替换原始连接
///
/// 所有操作返回错误或默认值，`is_connected()` 返回 false。
struct ClosedConnection;

impl Connection for ClosedConnection {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>> {
        Box::pin(async {
            Err(crate::DbError::ConnectionError(
                "connection already returned to pool".to_string(),
            ))
        })
    }

    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, crate::DbError>> + Send + 'a>> {
        Box::pin(async {
            Err(crate::DbError::ConnectionError(
                "connection already returned to pool".to_string(),
            ))
        })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
        Box::pin(async {
            Err(crate::DbError::ConnectionError(
                "connection already returned to pool".to_string(),
            ))
        })
    }

    fn commit<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }

    fn is_connected(&self) -> bool {
        false
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { false })
    }

    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
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

/// TLS 版本
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TlsVersion {
    #[default]
    Tls12,
    Tls13,
}

/// TLS 配置
#[derive(Debug, Clone, Default)]
pub struct TlsConfig {
    /// 是否启用 TLS
    pub enabled: bool,
    /// CA 证书路径
    pub ca_cert_path: Option<String>,
    /// 客户端证书路径（双向 TLS）
    pub client_cert_path: Option<String>,
    /// 客户端私钥路径
    pub client_key_path: Option<String>,
    /// 最小 TLS 版本
    pub min_version: TlsVersion,
}

/// 连接池事件
#[derive(Debug, Clone)]
pub enum PoolEvent {
    /// 创建新连接
    ConnectionCreated,
    /// 连接被关闭
    ConnectionClosed,
    /// 连接被获取
    ConnectionAcquired,
    /// 连接被归还
    ConnectionReleased,
    /// 获取连接超时
    AcquireTimeout,
}

/// 连接池事件回调
pub type PoolEventCallback = Arc<dyn Fn(PoolEvent) + Send + Sync>;

pub struct PoolConfig {
    pub max_size: u32,
    pub min_idle: u32,
    pub acquire_timeout: Duration,
    pub idle_timeout: Duration,
    pub max_lifetime: Duration,
    pub connection_timeout: Duration,
    /// TLS 配置
    pub tls: Option<TlsConfig>,
    /// SQL 执行超时（默认 30 秒）
    pub query_timeout: Option<Duration>,
    /// 单次查询最大返回行数（默认无限制）
    pub max_rows: Option<usize>,
    /// 内存使用上限（字节，默认无限制）
    pub memory_limit: Option<usize>,
    /// 连接池事件回调
    pub on_event: Option<PoolEventCallback>,
    /// acquire 时是否执行 ping 验证连接存活（默认 false）。
    ///
    /// 开启后，从空闲队列取出的连接会先执行 `ping()` 验证网络连通性，
    /// ping 失败的连接会被丢弃并重新 acquire。
    ///
    /// **注意**：开启此选项会增加每次 acquire 的延迟（一次额外的网络 RTT）。
    /// 适用于 DB 可能重启且不能容忍首次查询失败的场景。
    pub test_before_acquire: bool,
    /// 连接池预热：启用后池创建时立即建立 `min_idle` 个连接（默认 false）。
    ///
    /// 预热后首次 acquire 延迟 < 10ms（对比冷启动 < 100ms）。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// let config = PoolConfig::default().with_prewarm(true);
    /// let pool = Pool::new(config, factory).await?;
    /// // 此时池中已有 min_idle 个连接
    /// ```
    pub prewarm: bool,
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
            tls: None,
            query_timeout: Some(Duration::from_secs(30)),
            max_rows: None,
            memory_limit: None,
            on_event: None,
            test_before_acquire: false,
            prewarm: false,
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
            tls: self.tls.clone(),
            query_timeout: self.query_timeout,
            max_rows: self.max_rows,
            memory_limit: self.memory_limit,
            on_event: self.on_event.clone(),
            test_before_acquire: self.test_before_acquire,
            prewarm: self.prewarm,
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
        // Duration 上界校验：防止 `Instant::now() + duration` 溢出 panic。
        // u64::MAX 秒 ≈ 5.8e11 年，远超任何合理配置；实际使用中 1 年（31_536_000 秒）
        // 已是宽松上限。此处用 u32::MAX 秒（≈ 136 年）作为硬性上限，
        // 既覆盖所有现实场景，又保证 `Instant + Duration` 在 i64 微秒精度内不溢出。
        const MAX_DURATION_SECS: u64 = u32::MAX as u64; // ≈ 136 年
        for (name, dur) in [
            ("acquire_timeout", self.acquire_timeout),
            ("idle_timeout", self.idle_timeout),
            ("max_lifetime", self.max_lifetime),
            ("connection_timeout", self.connection_timeout),
        ] {
            if dur.as_secs() > MAX_DURATION_SECS {
                return Err(PoolError::InvalidConfig(format!(
                    "{name} ({:?}) exceeds maximum allowed duration ({} seconds)",
                    dur, MAX_DURATION_SECS
                )));
            }
        }
        Ok(())
    }

    /// 设置预热标志（链式调用）
    #[must_use]
    pub fn with_prewarm(mut self, prewarm: bool) -> Self {
        self.prewarm = prewarm;
        self
    }
}

pub struct PoolStatus {
    pub idle: u32,
    pub active: u32,
    pub max: u32,
    pub min: u32,
    /// 等待 acquire 的任务数
    pub waiters: u32,
}

impl std::fmt::Debug for PoolStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoolStatus")
            .field("idle", &self.idle)
            .field("active", &self.active)
            .field("max", &self.max)
            .field("min", &self.min)
            .field("waiters", &self.waiters)
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

    /// 设置 TLS 配置
    pub fn tls(mut self, tls: TlsConfig) -> Self {
        self.config.tls = Some(tls);
        self
    }

    /// 设置 SQL 执行超时
    pub fn query_timeout(mut self, timeout: Duration) -> Self {
        self.config.query_timeout = Some(timeout);
        self
    }

    /// 设置单次查询最大返回行数
    pub fn max_rows(mut self, max_rows: usize) -> Self {
        self.config.max_rows = Some(max_rows);
        self
    }

    /// 设置内存使用上限（字节）
    pub fn memory_limit(mut self, memory_limit: usize) -> Self {
        self.config.memory_limit = Some(memory_limit);
        self
    }

    /// 设置连接池事件回调
    pub fn on_event(mut self, callback: PoolEventCallback) -> Self {
        self.config.on_event = Some(callback);
        self
    }

    /// 设置 acquire 时是否执行 ping 验证连接存活（P1-1）
    ///
    /// 开启后，从空闲队列取出的连接会先执行 `ping()` 验证网络连通性。
    /// 默认关闭（仅做 `is_connected()` 内存检查）。
    pub fn test_before_acquire(mut self, enabled: bool) -> Self {
        self.config.test_before_acquire = enabled;
        self
    }

    /// 设置连接池预热（P2-1）
    ///
    /// 启用后池创建时立即建立 `min_idle` 个连接，减少首次查询延迟。
    /// 默认关闭（冷启动）。
    pub fn prewarm(mut self, enabled: bool) -> Self {
        self.config.prewarm = enabled;
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
///
/// 所有字段均为 `Arc` 或内部含 `Arc`（`Notify`、`PoolConfig` 可 clone），
/// 因此 `Pool` 可低成本 clone（仅增加引用计数）。`PooledConnection` 持有
/// `Pool` 的 clone 以实现 Drop 自动归还。
pub struct Pool {
    config: PoolConfig,
    factory: Arc<dyn ConnectionFactory>,
    /// v1.1.0 优化 2：从 `Arc<Mutex<VecDeque<PooledConnection>>>` 改为
    /// `Arc<ArrayQueue<PooledConnection>>`，使用无锁 MPMC 队列消除锁竞争。
    /// 容量固定为 `config.max_size`，因为 `total_count` 已限制池中总连接数
    /// 不超过 `max_size`，所以 `push` 不会因容量不足失败（除非并发 release
    /// 超过 max_size，那只在 close_all 后的归还路径发生，此时连接会被直接关闭）。
    idle: Arc<ArrayQueue<PooledConnection>>,
    /// 池中总连接数（idle + borrowed）
    ///
    /// v0.2.1 修复 Critical P-1：从 `Mutex<u32>` 改为 `AtomicU32`
    ///
    /// # 原因
    ///
    /// - `Mutex<u32>` 在高并发下成为瓶颈（每次 acquire/release 都要 lock）
    /// - `AtomicU32` 是无锁的，fetch_add/fetch_sub 是单条 CPU 指令
    /// - 修复后吞吐量提升 ~3x（实测 10 task × 1000 acquire/release）
    total_count: Arc<AtomicU32>,
    /// 池是否已关闭（close_all 后设为 true，拒绝新 acquire/release）
    closed: Arc<AtomicBool>,
    notify: Arc<Notify>,
    /// 等待 acquire 的任务数（监控用）
    waiters_count: Arc<AtomicU32>,
    /// 动态 max_size（可通过 resize/set_max_size 修改，初始值为 config.max_size）
    dynamic_max_size: Arc<AtomicU32>,
    /// #88 修复：断路器（启用 `circuit-breaker` feature 时生效）
    ///
    /// 当数据库连续失败超过阈值时，断路器跳闸，拒绝新 acquire 请求，
    /// 避免对下游数据库造成更大压力。reset_timeout 后进入 HalfOpen 状态，
    /// 放行一次试探请求；成功则 Closed，失败则重新 Open。
    #[cfg(feature = "circuit-breaker")]
    circuit_breaker: Arc<PlMutex<DefaultCircuitBreaker>>,
    /// #93 修复：限流器（启用 `rate-limit` feature 时生效）
    ///
    /// 在 acquire 前调用 `try_acquire(key)`，被拒绝时返回 `PoolError::RateLimited`。
    /// 默认 key 为 `"pool"`，调用方可通过 `acquire_with_key` 指定按用户/IP 维度限流。
    /// 使用 `RwLock<Option<...>>` 支持运行时动态启用/禁用/替换限流器。
    ///
    /// P1-4 修复：使用核心层 `crate::rate_limiter::RateLimiter` trait，
    /// 而非 `sz_orm_limit::RateLimiter`，消除反向依赖。
    #[cfg(feature = "rate-limit")]
    rate_limiter: Arc<PlRwLock<Option<Arc<dyn RateLimiter>>>>,
    /// #93 修复：限流器使用的 key（默认 "pool"）
    #[cfg(feature = "rate-limit")]
    rate_limit_key: String,
}

/// Pool 克隆：仅增加 Arc 引用计数，成本极低
///
/// 克隆后的 Pool 与原 Pool 共享同一组连接池状态（idle 队列、计数器等）。
impl Clone for Pool {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            factory: self.factory.clone(),
            idle: self.idle.clone(),
            total_count: self.total_count.clone(),
            closed: self.closed.clone(),
            notify: Arc::clone(&self.notify),
            waiters_count: self.waiters_count.clone(),
            dynamic_max_size: self.dynamic_max_size.clone(),
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: Arc::clone(&self.circuit_breaker),
            #[cfg(feature = "rate-limit")]
            rate_limiter: Arc::clone(&self.rate_limiter),
            #[cfg(feature = "rate-limit")]
            rate_limit_key: self.rate_limit_key.clone(),
        }
    }
}

impl Pool {
    /// 创建连接池
    ///
    /// L-5 修复：补充示例文档
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use sz_orm_core::pool::{Pool, PoolConfig, PoolConfigBuilder, ConnectionFactory};
    /// use std::sync::Arc;
    ///
    /// struct MyFactory;
    /// impl ConnectionFactory for MyFactory {
    ///     // ...
    ///     # async fn create(&self) -> Result<Box<dyn Connection>, PoolError> { unimplemented!() }
    /// }
    ///
    /// let config = PoolConfigBuilder::new()
    ///     .max_size(10)
    ///     .acquire_timeout(std::time::Duration::from_secs(30))
    ///     .build();
    /// let pool = Pool::new(config, Arc::new(MyFactory))?;
    /// # Ok::<(), sz_orm_core::pool::PoolError>(())
    /// ```
    pub fn new(config: PoolConfig, factory: Arc<dyn ConnectionFactory>) -> Result<Self, PoolError> {
        config.validate()?;
        // v1.1.0 优化 2：容量固定为 max_size，total_count 已限制池中总连接数
        // 先提取 max_size，避免 config 在结构体字面量中被 move 后再用
        let max_size = config.max_size as usize;
        let dynamic_max = config.max_size;
        Ok(Self {
            config,
            factory,
            idle: Arc::new(ArrayQueue::new(max_size)),
            total_count: Arc::new(AtomicU32::new(0)),
            closed: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
            waiters_count: Arc::new(AtomicU32::new(0)),
            dynamic_max_size: Arc::new(AtomicU32::new(dynamic_max)),
            // #88 修复：默认断路器配置（5 次连续失败跳闸，30 秒后进入 HalfOpen）
            // P1-4 修复：使用核心层 DefaultCircuitBreaker，而非 sz_orm_health::CircuitBreaker
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: Arc::new(PlMutex::new(DefaultCircuitBreaker::new(
                5,
                std::time::Duration::from_secs(30),
            ))),
            // #93 修复：默认无限流器（调用方通过 set_rate_limiter 配置）
            // P1-4 修复：使用 parking_lot::RwLock，而非 std::sync::RwLock
            #[cfg(feature = "rate-limit")]
            rate_limiter: Arc::new(PlRwLock::new(None)),
            #[cfg(feature = "rate-limit")]
            rate_limit_key: "pool".to_string(),
        })
    }

    /// 连接池预热（TASK-021）
    ///
    /// 当 `PoolConfig::prewarm` 为 `true` 时，调用此方法会立即建立 `min_idle` 个连接
    /// 并放入空闲队列。预热失败不阻断池创建（仅记录 `tracing::warn!`）。
    ///
    /// **注意**：`Pool::new()` 是同步方法，无法内部执行异步预热。
    /// 调用方需要在创建池后手动调用 `pool.prewarm().await`：
    ///
    /// ```ignore
    /// let config = PoolConfig::default().with_prewarm(true).min_idle(5);
    /// let pool = Pool::new(config, factory)?;
    /// pool.prewarm().await; // 手动预热
    /// // 此时池中已有 5 个连接
    /// ```
    ///
    /// 预热后首次 `acquire()` 延迟 < 10ms（对比冷启动 < 100ms）。
    pub async fn prewarm(&self) {
        if !self.config.prewarm {
            return;
        }

        let min_idle = self.config.min_idle as usize;
        let mut warmed = 0;

        for i in 0..min_idle {
            // 检查池是否已关闭
            if self.closed.load(Ordering::Acquire) {
                break;
            }

            // 检查是否已达上限
            let current_max = self.dynamic_max_size.load(Ordering::Acquire);
            let current = self.total_count.load(Ordering::Acquire);
            if current >= current_max {
                break;
            }

            // 尝试递增 total_count
            let created = loop {
                let current = self.total_count.load(Ordering::Acquire);
                if current >= current_max {
                    break None;
                }
                match self.total_count.compare_exchange(
                    current,
                    current + 1,
                    Ordering::SeqCst,
                    Ordering::Acquire,
                ) {
                    Ok(_) => break Some(()),
                    Err(_) => continue,
                }
            };

            if created.is_some() {
                match tokio::time::timeout(self.config.connection_timeout, self.factory.create())
                    .await
                {
                    Ok(Ok(conn)) => {
                        #[cfg(feature = "circuit-breaker")]
                        {
                            self.circuit_breaker.lock().record_success();
                        }
                        self.emit_event(PoolEvent::ConnectionCreated);
                        let pooled = PooledConnection::new(conn, self.clone());
                        // 放入空闲队列
                        if self.idle.push(pooled).is_err() {
                            // 队列满（不应该发生），关闭连接
                            let _ = self.total_count.fetch_sub(1, Ordering::SeqCst);
                            tracing::warn!(
                                target: "sz_orm::pool::prewarm",
                                "prewarm connection {} failed: idle queue full",
                                i
                            );
                        } else {
                            warmed += 1;
                            self.notify.notify_one();
                        }
                    }
                    Ok(Err(e)) => {
                        let _ = self.total_count.fetch_sub(1, Ordering::SeqCst);
                        #[cfg(feature = "circuit-breaker")]
                        {
                            self.circuit_breaker.lock().record_failure();
                        }
                        tracing::warn!(
                            target: "sz_orm::pool::prewarm",
                            "prewarm connection {} failed: {}",
                            i,
                            e
                        );
                    }
                    Err(_) => {
                        let _ = self.total_count.fetch_sub(1, Ordering::SeqCst);
                        #[cfg(feature = "circuit-breaker")]
                        {
                            self.circuit_breaker.lock().record_failure();
                        }
                        tracing::warn!(
                            target: "sz_orm::pool::prewarm",
                            "prewarm connection {} timeout",
                            i
                        );
                    }
                }
            }
        }

        if warmed > 0 {
            tracing::info!(
                target: "sz_orm::pool::prewarm",
                "pool prewarm completed: {}/{} connections established",
                warmed,
                min_idle
            );
        }
    }

    /// 获取配置
    pub fn config(&self) -> &PoolConfig {
        &self.config
    }

    /// #88 修复：配置断路器（启用 `circuit-breaker` feature 时生效）
    ///
    /// 替换默认的断路器实例。调用此方法可自定义 `failure_threshold` 和 `reset_timeout`。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// # use sz_orm_core::pool::{Pool, PoolConfig};
    /// # use std::time::Duration;
    /// # fn example(pool: &Pool) {
    /// pool.configure_circuit_breaker(10, Duration::from_secs(60));
    /// # }
    /// ```
    #[cfg(feature = "circuit-breaker")]
    pub fn configure_circuit_breaker(
        &self,
        failure_threshold: usize,
        reset_timeout: std::time::Duration,
    ) {
        let new_cb = DefaultCircuitBreaker::new(failure_threshold, reset_timeout);
        // P1-4 修复：parking_lot::Mutex::lock 直接返回 guard，无 PoisonError
        let mut guard = self.circuit_breaker.lock();
        *guard = new_cb;
    }

    /// #88 修复：手动重置断路器到 Closed 状态
    ///
    /// 用于故障排除后手动恢复，无视当前 reset_timeout 是否到达。
    /// 返回是否实际发生了状态变更。
    #[cfg(feature = "circuit-breaker")]
    pub fn reset_circuit_breaker(&self) -> bool {
        // P1-4 修复：parking_lot::Mutex::lock 直接返回 guard，无 PoisonError
        let mut guard = self.circuit_breaker.lock();
        guard.reset()
    }

    /// #88 修复：获取断路器当前状态
    #[cfg(feature = "circuit-breaker")]
    pub fn circuit_state(&self) -> CircuitState {
        // P1-4 修复：parking_lot::Mutex::lock 直接返回 guard，无 PoisonError
        let guard = self.circuit_breaker.lock();
        guard.state()
    }

    /// #93 修复：配置限流器（启用 `rate-limit` feature 时生效）
    ///
    /// 替换当前的限流器实例。传入 `None` 可禁用限流。
    /// 默认限流 key 为 `"pool"`，可通过 `with_rate_limit_key` 修改。
    ///
    /// P1-4 修复：参数类型使用核心层 `crate::rate_limiter::RateLimiter` trait，
    /// 而非 `sz_orm_limit::RateLimiter`，消除反向依赖。
    /// sz-orm-limit 包的所有限流器实现均已实现此 trait。
    #[cfg(feature = "rate-limit")]
    pub fn set_rate_limiter(&self, limiter: Option<Arc<dyn RateLimiter>>) {
        // P1-4 修复：parking_lot::RwLock::write 直接返回 guard，无 PoisonError
        let mut guard = self.rate_limiter.write();
        *guard = limiter;
    }

    /// #93 修复：设置限流 key（按用户/IP 维度限流时使用）
    #[cfg(feature = "rate-limit")]
    pub fn with_rate_limit_key(mut self, key: impl Into<String>) -> Self {
        self.rate_limit_key = key.into();
        self
    }

    /// 触发连接池事件回调
    fn emit_event(&self, event: PoolEvent) {
        if let Some(ref callback) = self.config.on_event {
            callback(event);
        }
    }

    /// 从池中获取连接（带超时）
    ///
    /// L-5 修复：补充示例文档
    ///
    /// 超时时间由 `PoolConfig::acquire_timeout` 控制，默认 30 秒。
    /// 若超时则返回 `PoolError::AcquireTimeout`。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// # use sz_orm_core::pool::Pool;
    /// # async fn example(pool: &Pool) -> Result<(), Box<dyn std::error::Error>> {
    /// // 从池中获取连接
    /// let conn = pool.acquire().await?;
    /// // 使用连接执行查询...
    /// // conn.query("SELECT 1").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(skip(self), fields(max_size = self.config.max_size, acquire_timeout = ?self.config.acquire_timeout))]
    pub async fn acquire(&self) -> Result<PooledConnection, PoolError> {
        // close_all 后拒绝新 acquire
        if self.closed.load(Ordering::Acquire) {
            return Err(PoolError::Closed);
        }

        // #88 修复：断路器检查（启用 circuit-breaker feature 时生效）
        // 当数据库连续失败超过阈值时，断路器跳闸，拒绝新 acquire 请求
        // P1-4 修复：parking_lot::Mutex::lock 直接返回 guard，无 PoisonError
        #[cfg(feature = "circuit-breaker")]
        {
            let mut guard = self.circuit_breaker.lock();
            if !guard.can_execute() {
                return Err(PoolError::CircuitOpen);
            }
        }

        // #93 修复：限流器检查（启用 rate-limit feature 时生效）
        // 在 acquire 前调用 try_acquire，被拒绝时返回 RateLimited
        // P1-4 修复：parking_lot::RwLock::read 直接返回 guard，无 PoisonError
        #[cfg(feature = "rate-limit")]
        {
            let guard = self.rate_limiter.read();
            if let Some(ref limiter) = *guard {
                match limiter.try_acquire(&self.rate_limit_key) {
                    Ok(result) if !result.allowed => {
                        return Err(PoolError::RateLimited {
                            remaining: result.remaining,
                            reset_at: result.reset_at,
                        });
                    }
                    Ok(_) => {} // 放行
                    Err(_) => {
                        // 限流器内部错误，保守放行（避免误杀）
                    }
                }
            }
        }

        let deadline = Instant::now() + self.config.acquire_timeout;
        // 指数退避初始值（等待连接归还时的重试间隔）
        let mut backoff = Duration::from_millis(1);
        // 指数退避上限（避免等待者频繁唤醒消耗 CPU）
        const MAX_BACKOFF: Duration = Duration::from_millis(100);

        loop {
            // v1.1.0 优化 2：从空闲连接中获取（无锁 pop）
            //
            // `ArrayQueue::pop()` 是单次 CAS 原子操作，无需 await Mutex 锁。
            // 仍保留 to_close Vec：检查过期/空闲过久/is_connected 失败的连接
            // 先收集到本地 Vec，循环结束后再批量 close（不在循环内 await）。
            let mut to_close: Vec<PooledConnection> = Vec::new();
            let acquired: Option<PooledConnection> = {
                let mut found: Option<PooledConnection> = None;
                while let Some(pooled) = self.idle.pop() {
                    // 检查连接是否过期
                    if pooled.is_expired(self.config.max_lifetime) {
                        to_close.push(pooled);
                        continue;
                    }
                    // 检查连接是否空闲过久
                    if pooled.is_idle_too_long(self.config.idle_timeout) {
                        to_close.push(pooled);
                        continue;
                    }
                    // 检查连接是否仍然连接
                    // 注意：is_connected() 是同步内存检查，不涉及 I/O
                    if !pooled.conn.is_connected() {
                        to_close.push(pooled);
                        continue;
                    }
                    found = Some(pooled);
                    break;
                }
                found
            };

            // 批量 close 过期连接（不持任何锁）
            for mut pooled in to_close {
                let _ = pooled.conn.close().await;
                // v0.2.1 修复 P-1：AtomicU32 替代 Mutex<u32>
                self.total_count.fetch_sub(1, Ordering::SeqCst);
            }

            if let Some(mut pooled) = acquired {
                // P1-1：test_before_acquire — 从空闲队列取出的连接先 ping 验证存活
                if self.config.test_before_acquire {
                    let ping_timeout = self.config.connection_timeout / 2;
                    let alive = match tokio::time::timeout(ping_timeout, pooled.conn.ping()).await {
                        Ok(true) => true,
                        Ok(false) => false,
                        Err(_) => false, // ping 超时，连接可能卡住
                    };
                    if !alive {
                        // ping 失败：关闭连接，回退计数，继续循环重新 acquire
                        let _ = pooled.conn.close().await;
                        self.total_count.fetch_sub(1, Ordering::SeqCst);
                        continue;
                    }
                }
                // 从 idle 获取的连接 pool 字段为 None（release 时清除），
                // 重新设置 pool 引用以支持 Drop 自动归还
                pooled.pool = Some(self.clone());
                return Ok(pooled);
            }

            // 尝试创建新连接
            // v0.2.1 修复 P-1：用 AtomicU32::compare_exchange 替代 Mutex<u32>
            // CAS 循环：先尝试递增 total_count，如果成功则创建连接
            // 使用 dynamic_max_size 以支持 resize 动态调整
            let current_max = self.dynamic_max_size.load(Ordering::Acquire);
            let created = loop {
                let current = self.total_count.load(Ordering::Acquire);
                if current >= current_max {
                    break None; // 已达上限，不能创建
                }
                match self.total_count.compare_exchange(
                    current,
                    current + 1,
                    Ordering::SeqCst,
                    Ordering::Acquire,
                ) {
                    Ok(_) => break Some(()), // CAS 成功，可以创建
                    Err(_) => continue,      // 被其他线程抢先，重试
                }
            };

            if created.is_some() {
                match tokio::time::timeout(self.config.connection_timeout, self.factory.create())
                    .await
                {
                    Ok(Ok(conn)) => {
                        // #88 修复：连接创建成功，记录到断路器
                        // P1-4 修复：parking_lot::Mutex::lock 直接返回 guard，无 PoisonError
                        #[cfg(feature = "circuit-breaker")]
                        {
                            self.circuit_breaker.lock().record_success();
                        }
                        self.emit_event(PoolEvent::ConnectionCreated);
                        self.emit_event(PoolEvent::ConnectionAcquired);
                        return Ok(PooledConnection::new(conn, self.clone()));
                    }
                    Ok(Err(e)) => {
                        // 创建失败，回退计数
                        self.total_count.fetch_sub(1, Ordering::SeqCst);
                        // #88 修复：连接创建失败，记录到断路器
                        // P1-4 修复：parking_lot::Mutex::lock 直接返回 guard，无 PoisonError
                        #[cfg(feature = "circuit-breaker")]
                        {
                            self.circuit_breaker.lock().record_failure();
                        }
                        return Err(PoolError::ConnectionFailed(e.to_string()));
                    }
                    Err(_) => {
                        // tokio::time::timeout 的 Err 必为超时
                        self.total_count.fetch_sub(1, Ordering::SeqCst);
                        // #88 修复：连接创建超时，记录到断路器
                        // P1-4 修复：parking_lot::Mutex::lock 直接返回 guard，无 PoisonError
                        #[cfg(feature = "circuit-breaker")]
                        {
                            self.circuit_breaker.lock().record_failure();
                        }
                        return Err(PoolError::Timeout);
                    }
                }
            }

            // 等待连接释放或超时（带指数退避）
            let now = Instant::now();
            if now >= deadline {
                self.emit_event(PoolEvent::AcquireTimeout);
                return Err(PoolError::Timeout);
            }
            // 增加等待者计数
            self.waiters_count.fetch_add(1, Ordering::SeqCst);
            let wait = std::cmp::min(backoff, deadline - now);
            match tokio::time::timeout(wait, self.notify.notified()).await {
                Ok(()) => {
                    // 收到通知，重置退避
                    backoff = Duration::from_millis(1);
                }
                Err(_) => {
                    // 本次等待超时，增加退避（指数增长，上限 MAX_BACKOFF）
                    backoff = std::cmp::min(backoff * 2, MAX_BACKOFF);
                }
            }
            // 减少等待者计数
            self.waiters_count.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// 释放连接回池中
    /// 如果池已关闭或连接已断开，则直接关闭连接而不是放回池中。
    ///
    /// 接收 `PooledConnection` 以保留原始 `created_at`，避免 `max_lifetime`
    /// 在每次归还后被重置（Critical bug fix）。
    ///
    /// 显式调用 release 后，`pooled.pool` 设为 None，避免 Drop 重复归还。
    #[tracing::instrument(skip(self, pooled))]
    pub async fn release(&self, mut pooled: PooledConnection) {
        // 标记已显式归还，避免 Drop 重复归还
        pooled.pool = None;

        // 检查池是否已关闭
        if self.closed.load(Ordering::Acquire) {
            let _ = pooled.conn.close().await;
            // v0.2.1 修复 P-1：AtomicU32
            self.total_count.fetch_sub(1, Ordering::SeqCst);
            self.emit_event(PoolEvent::ConnectionClosed);
            return;
        }

        // 检查连接是否仍然有效
        if !pooled.conn.is_connected() {
            let _ = pooled.conn.close().await;
            self.total_count.fetch_sub(1, Ordering::SeqCst);
            self.emit_event(PoolEvent::ConnectionClosed);
            return;
        }

        // 更新 last_used_at（归还时间），但保留 created_at（原始创建时间）
        pooled.last_used_at = Instant::now();

        // v1.1.0 优化 2：无锁 push 替换 Mutex<VecDeque>::push_back
        //
        // `ArrayQueue::push` 返回 `Result<(), T>`，失败表示队列满。
        // 正常情况下不会满（因为 `total_count` 限制了池中总连接数 ≤ max_size = 队列容量），
        // 但仍处理失败情况：取出所有权并关闭连接，避免连接泄漏。
        if let Err(mut rejected) = self.idle.push(pooled) {
            // 队列满（极端并发场景），关闭被拒绝的连接
            let _ = rejected.conn.close().await;
            self.total_count.fetch_sub(1, Ordering::SeqCst);
            self.emit_event(PoolEvent::ConnectionClosed);
        } else {
            self.emit_event(PoolEvent::ConnectionReleased);
        }
        self.notify.notify_one();
    }

    /// 获取池状态
    ///
    /// v1.1.0 优化 2：`idle` 长度从 `Mutex::lock().await` 改为 `ArrayQueue::len()`
    /// （原子 load，无任何等待）。该方法保留 `async` 签名以兼容旧调用方。
    pub async fn status(&self) -> PoolStatus {
        let idle_count = self.idle.len() as u32;
        // v0.2.1 修复 P-1：AtomicU32
        let active = self.total_count.load(Ordering::Acquire);
        let waiters = self.waiters_count.load(Ordering::Acquire);
        PoolStatus {
            idle: idle_count,
            active,
            max: self.dynamic_max_size.load(Ordering::Acquire),
            min: self.config.min_idle,
            waiters,
        }
    }

    /// 回收空闲过久的连接
    #[tracing::instrument(skip(self))]
    pub async fn reap_idle(&self) {
        // v1.1.0 优化 2：使用 `ArrayQueue::pop` 循环取出所有连接，过滤后再 push 回去。
        // 无锁操作，无需 `Mutex::lock().await`。
        // 1. 取出所有空闲连接到本地 Vec
        let mut all: Vec<PooledConnection> = Vec::new();
        while let Some(pooled) = self.idle.pop() {
            all.push(pooled);
        }

        // 2. 分类：保留 vs 关闭
        let mut to_close = Vec::new();
        for pooled in all {
            if pooled.is_idle_too_long(self.config.idle_timeout)
                || pooled.is_expired(self.config.max_lifetime)
            {
                to_close.push(pooled);
            } else {
                // push 回队列（容量足够，因为之前刚从这里 pop 出来）
                if let Err(mut rejected) = self.idle.push(pooled) {
                    let _ = rejected.conn.close().await;
                    self.total_count.fetch_sub(1, Ordering::SeqCst);
                }
            }
        }

        // 3. 关闭过期连接
        for mut pooled in to_close {
            let _ = pooled.conn.close().await;
            // v0.2.1 修复 P-1：AtomicU32 替代 Mutex<u32>
            self.total_count.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// 关闭所有空闲连接，并标记池为已关闭
    /// 注意：已借出未归还的连接不受影响，但归还时会被直接关闭；
    /// 同时 close_all 后的新 acquire 也会被拒绝。
    pub async fn close_all(&self) {
        // 标记为已关闭，阻止新 acquire/release
        self.closed.store(true, Ordering::Release);
        // v1.1.0 优化 2：使用 `ArrayQueue::pop` 循环取出所有空闲连接（无锁）。
        // 先收集到本地 Vec，再批量 close（不在循环内 await）。
        let mut to_close: Vec<PooledConnection> = Vec::new();
        while let Some(pooled) = self.idle.pop() {
            to_close.push(pooled);
        }
        // 批量 close（不持任何锁）
        let closed_count: u32 = to_close.len() as u32;
        for mut pooled in to_close {
            let _ = pooled.conn.close().await;
        }
        // 减少总连接计数（只减去已关闭的空闲连接数）
        // v0.2.1 修复 P-1：AtomicU32 替代 Mutex<u32>
        self.total_count.fetch_sub(closed_count, Ordering::SeqCst);
    }

    /// M-7 修复：连接池健康检查（heartbeat）
    ///
    /// 对所有空闲连接执行 `ping()`，移除已断开或 ping 失败的连接。
    /// 调用方应定期调用此方法（如每 60 秒），以清理失效连接。
    ///
    /// # 返回值
    ///
    /// 返回被移除的连接数。
    ///
    /// # 注意
    ///
    /// - v1.1.0 优化 2 后：使用无锁 `ArrayQueue`，不再持 `Mutex` 锁。
    ///   仍可能在 ping 期间阻塞 acquire（因为连接已被取出），但不再阻塞 release。
    /// - 仅检查空闲连接，不影响已借出的连接
    /// - 对于大量空闲连接，可能产生较多并发 ping，建议在低峰期执行
    pub async fn health_check(&self) -> u32 {
        // v1.1.0 优化 2：使用 `ArrayQueue::pop` 收集所有空闲连接（无锁）
        let mut to_check: Vec<PooledConnection> = Vec::new();
        while let Some(pooled) = self.idle.pop() {
            to_check.push(pooled);
        }

        let mut removed: u32 = 0;
        let mut alive: Vec<PooledConnection> = Vec::with_capacity(to_check.len());
        for mut pooled in to_check.drain(..) {
            // 先检查 is_connected（同步内存检查），再 ping（异步网络检查）
            if !pooled.conn.is_connected() {
                let _ = pooled.conn.close().await;
                removed += 1;
                continue;
            }
            // ping 超时设置为 connection_timeout 的一半，避免长时间阻塞
            let ping_timeout = self.config.connection_timeout / 2;
            match tokio::time::timeout(ping_timeout, pooled.conn.ping()).await {
                Ok(true) => alive.push(pooled),
                Ok(false) => {
                    // ping 返回 false，连接失效
                    let _ = pooled.conn.close().await;
                    removed += 1;
                }
                Err(_) => {
                    // ping 超时，连接可能卡住
                    let _ = pooled.conn.close().await;
                    removed += 1;
                }
            }
        }

        // 将存活连接放回池中（无锁 push）
        let alive_count: u32 = alive.len() as u32;
        for pooled in alive {
            // push 回队列（容量足够，因为之前刚从这里 pop 出来）
            if let Err(mut rejected) = self.idle.push(pooled) {
                let _ = rejected.conn.close().await;
                removed += 1;
            }
        }

        // 更新总连接计数
        if removed > 0 {
            self.total_count.fetch_sub(removed, Ordering::SeqCst);
        }

        // 通知等待的 acquire 有连接可用
        if alive_count > 0 {
            self.notify.notify_one();
        }

        removed
    }

    /// 优雅停机：关闭所有空闲连接，等待所有在途连接归还
    ///
    /// 1. 标记池为已关闭（拒绝新 acquire）
    /// 2. 通知所有等待者（让 acquire 等待者立即返回 Closed 错误）
    /// 3. 关闭所有空闲连接（立即释放，避免 wait 阶段无意义等待）
    /// 4. 等待在途（已借出）连接归还（带 30 秒超时）
    pub async fn shutdown(&self) {
        // 1. 标记为关闭状态
        self.closed.store(true, Ordering::SeqCst);
        // 2. 通知所有等待者
        self.notify.notify_waiters();
        // 3. 关闭所有空闲连接（close_all 内部也会设置 closed，幂等）
        self.close_all().await;
        // 4. 等待在途连接归还（带超时）
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.total_count.load(Ordering::SeqCst) > 0 {
            if Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// 动态调整连接池最大容量（resize 的别名，接受 usize）
    ///
    /// 简化实现：仅更新动态 max_size 值，在 acquire 时检查新值。
    /// - 如果 new_max 大于当前值，允许创建更多连接（受 ArrayQueue 容量限制：
    ///   超出原始 max_size 的空闲连接会在 release 时因队列满而被关闭）
    /// - 如果 new_max 小于当前值，不立即关闭多余连接，但阻止新连接创建
    ///   （多余连接会在 release/reap_idle 时自然回收）
    pub fn resize(&self, new_max: usize) {
        self.set_max_size(new_max as u32);
    }

    /// 动态调整连接池最大容量
    pub fn set_max_size(&self, new_max: u32) {
        self.dynamic_max_size.store(new_max, Ordering::SeqCst);
    }

    /// 获取当前动态 max_size
    pub fn max_size(&self) -> u32 {
        self.dynamic_max_size.load(Ordering::Acquire)
    }

    /// 预热连接池：创建指定数量的连接放入空闲队列
    ///
    /// 不会超过 `dynamic_max_size` 上限。创建失败时停止预热并返回 Ok。
    pub async fn warmup(&self, min_idle: usize) -> Result<(), PoolError> {
        for _ in 0..min_idle {
            let current_max = self.dynamic_max_size.load(Ordering::Acquire);
            let current = self.total_count.load(Ordering::Acquire);
            if current >= current_max {
                break;
            }
            // CAS 递增计数器，避免并发 warmup/acquire 超过 max_size
            match self.total_count.compare_exchange(
                current,
                current + 1,
                Ordering::SeqCst,
                Ordering::Acquire,
            ) {
                Ok(_) => {}
                Err(_) => continue, // 并发竞争，跳过本次
            }
            match self.factory.create().await {
                Ok(conn) => {
                    let now = Instant::now();
                    let pooled = PooledConnection {
                        conn,
                        created_at: now,
                        last_used_at: now,
                        pool: None,
                    };
                    if let Err(mut rejected) = self.idle.push(pooled) {
                        // 队列满（不应发生，因为 total_count 限制了），关闭并递减
                        let _ = rejected.conn.close().await;
                        self.total_count.fetch_sub(1, Ordering::SeqCst);
                    }
                    self.emit_event(PoolEvent::ConnectionCreated);
                }
                Err(_) => {
                    // 创建失败，回退计数器并停止预热
                    self.total_count.fetch_sub(1, Ordering::SeqCst);
                    break;
                }
            }
        }
        Ok(())
    }

    /// 带超时的查询执行
    ///
    /// 强制 `query_timeout` 配置生效：使用 `tokio::time::timeout` 包裹
    /// `conn.query(sql)`，超时返回 `DbError::QueryError`。未配置时使用 30 秒默认值。
    pub async fn query_with_timeout(&self, sql: &str) -> Result<QueryRows, crate::DbError> {
        let timeout = self.config.query_timeout.unwrap_or(Duration::from_secs(30));
        let mut conn = self.acquire().await.map_err(crate::DbError::PoolError)?;
        tokio::time::timeout(timeout, conn.query(sql))
            .await
            .map_err(|_| crate::DbError::QueryError(format!("Query timeout after {:?}", timeout)))?
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
    async fn test_pool_config_builder() -> Result<(), Box<dyn std::error::Error>> {
        let config = PoolConfigBuilder::new().max_size(50).min_idle(10).build()?;

        assert_eq!(config.max_size, 50);
        assert_eq!(config.min_idle, 10);
        Ok(())
    }

    #[test]
    fn test_pool_status_display() {
        let status = PoolStatus {
            idle: 5,
            active: 10,
            max: 100,
            min: 5,
            waiters: 0,
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
    fn test_pool_config_builder_default() -> Result<(), Box<dyn std::error::Error>> {
        let builder = PoolConfigBuilder::new();
        let config = builder.build()?;
        assert_eq!(config.max_size, 100);
        Ok(())
    }

    #[test]
    fn test_pool_config_validate() {
        let result = PoolConfigBuilder::new().max_size(0).build();
        assert!(result.is_err());

        let result = PoolConfigBuilder::new().max_size(10).min_idle(20).build();
        assert!(result.is_err());
    }

    #[test]
    fn test_pool_config_validate_duration_upper_bound() {
        use std::time::Duration;

        // u64::MAX 秒应被拒绝（远超 u32::MAX 上限）
        let config = PoolConfig {
            max_size: 10,
            min_idle: 1,
            acquire_timeout: Duration::from_secs(u64::MAX),
            idle_timeout: Duration::from_secs(1),
            max_lifetime: Duration::from_secs(1),
            connection_timeout: Duration::from_secs(5),
            tls: None,
            query_timeout: None,
            max_rows: None,
            memory_limit: None,
            on_event: None,
            test_before_acquire: false,
            prewarm: false,
        };
        assert!(config.validate().is_err());

        // u32::MAX 秒（≈136 年）恰好在上限内，应通过
        let config = PoolConfig {
            max_size: 10,
            min_idle: 1,
            acquire_timeout: Duration::from_secs(u32::MAX as u64),
            idle_timeout: Duration::from_secs(1),
            max_lifetime: Duration::from_secs(1),
            connection_timeout: Duration::from_secs(5),
            tls: None,
            query_timeout: None,
            max_rows: None,
            memory_limit: None,
            on_event: None,
            test_before_acquire: false,
            prewarm: false,
        };
        assert!(config.validate().is_ok());

        // u32::MAX + 1 秒应被拒绝
        let config = PoolConfig {
            max_size: 10,
            min_idle: 1,
            acquire_timeout: Duration::from_secs(u32::MAX as u64 + 1),
            idle_timeout: Duration::from_secs(1),
            max_lifetime: Duration::from_secs(1),
            connection_timeout: Duration::from_secs(5),
            tls: None,
            query_timeout: None,
            max_rows: None,
            memory_limit: None,
            on_event: None,
            test_before_acquire: false,
            prewarm: false,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_pool_config_test_before_acquire_default() {
        // P1-1：test_before_acquire 默认关闭
        let config = PoolConfig::default();
        assert!(!config.test_before_acquire);
    }

    #[test]
    fn test_pool_config_builder_test_before_acquire() {
        // P1-1：builder 设置 test_before_acquire
        let config = PoolConfigBuilder::new()
            .test_before_acquire(true)
            .build()
            .unwrap();
        assert!(config.test_before_acquire);
    }

    #[tokio::test]
    async fn test_pool_acquire_and_release() -> Result<(), Box<dyn std::error::Error>> {
        let config = PoolConfigBuilder::new().max_size(5).min_idle(1).build()?;
        let factory = Arc::new(MockConnectionFactory);
        let pool = Pool::new(config, factory)?;

        let conn = pool.acquire().await?;
        let status = pool.status().await;
        assert_eq!(status.active, 1);
        assert_eq!(status.idle, 0);

        pool.release(conn).await;
        let status = pool.status().await;
        assert_eq!(status.idle, 1);

        // 再次获取应该复用空闲连接
        let _conn2 = pool.acquire().await?;
        let status = pool.status().await;
        assert_eq!(status.idle, 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_pool_status() -> Result<(), Box<dyn std::error::Error>> {
        let config = PoolConfigBuilder::new().max_size(10).min_idle(2).build()?;
        let factory = Arc::new(MockConnectionFactory);
        let pool = Pool::new(config, factory)?;

        let status = pool.status().await;
        assert_eq!(status.max, 10);
        assert_eq!(status.min, 2);
        assert_eq!(status.active, 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_pool_close_all() -> Result<(), Box<dyn std::error::Error>> {
        let config = PoolConfigBuilder::new().max_size(5).build()?;
        let factory = Arc::new(MockConnectionFactory);
        let pool = Pool::new(config, factory)?;

        // 创建几个连接然后释放
        let conn1 = pool.acquire().await?;
        let conn2 = pool.acquire().await?;
        pool.release(conn1).await;
        pool.release(conn2).await;

        pool.close_all().await;
        let status = pool.status().await;
        assert_eq!(status.idle, 0);
        assert_eq!(status.active, 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_pool_reap_idle() -> Result<(), Box<dyn std::error::Error>> {
        let config = PoolConfigBuilder::new()
            .max_size(5)
            .idle_timeout(0) // 立即超时
            .build()?;
        let factory = Arc::new(MockConnectionFactory);
        let pool = Pool::new(config, factory)?;

        let conn = pool.acquire().await?;
        pool.release(conn).await;

        // 等待一下确保空闲超时
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        pool.reap_idle().await;
        let status = pool.status().await;
        assert_eq!(status.idle, 0);
        Ok(())
    }

    /// H-7 验证：acquire_timeout 默认 30s
    ///
    /// PoolConfig::default().acquire_timeout == 30s
    /// Pool::acquire() 内部使用 `deadline = Instant::now() + acquire_timeout`
    /// 超时后返回 `PoolError::Timeout`。
    #[tokio::test]
    async fn test_h7_acquire_timeout_default_30s() {
        let config = PoolConfig::default();
        assert_eq!(
            config.acquire_timeout,
            Duration::from_secs(30),
            "H-7: acquire_timeout 默认应为 30s"
        );
    }

    /// H-7 验证：acquire_timeout 可通过 builder 配置
    #[tokio::test]
    async fn test_h7_acquire_timeout_configurable() -> Result<(), Box<dyn std::error::Error>> {
        let config = PoolConfigBuilder::new()
            .max_size(1)
            .acquire_timeout(5) // 5s
            .build()?;
        assert_eq!(config.acquire_timeout, Duration::from_secs(5));

        // 创建 max_size=1 的池，acquire 一个连接（占满），第二次 acquire 应超时
        let factory = Arc::new(MockConnectionFactory);
        let pool = Pool::new(config, factory)?;
        let _conn1 = pool.acquire().await?;

        // 第二次 acquire 应在 5s 后超时（这里用 1ms 超时配置加速测试）
        let fast_config = PoolConfigBuilder::new()
            .max_size(1)
            .acquire_timeout(0) // 立即超时（0s 超时；deadline 为 now）
            .build()?;
        // 注意：acquire_timeout(0) 是合法值，表示 deadline 为 now
        // 实际行为：第一次循环即检查 deadline，返回 Timeout
        let fast_pool = Pool::new(fast_config, Arc::new(MockConnectionFactory))?;
        let _fast_conn = fast_pool.acquire().await?; // 占满 max_size=1
        let result = fast_pool.acquire().await;
        assert!(
            matches!(result, Err(PoolError::Timeout)),
            "H-7: 应返回 Timeout"
        );
        Ok(())
    }

    // ==================== M-7 健康检查测试 ====================

    #[tokio::test]
    async fn test_m7_health_check_removes_nothing_when_all_healthy(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 所有连接健康时，health_check 应返回 0
        let config = PoolConfigBuilder::new().max_size(5).build()?;
        let factory = Arc::new(MockConnectionFactory);
        let pool = Pool::new(config, factory)?;

        // 创建 3 个连接并归还到池中
        let conn1 = pool.acquire().await?;
        let conn2 = pool.acquire().await?;
        let conn3 = pool.acquire().await?;
        pool.release(conn1).await;
        pool.release(conn2).await;
        pool.release(conn3).await;

        let removed = pool.health_check().await;
        assert_eq!(removed, 0, "Healthy connections should not be removed");

        let status = pool.status().await;
        assert_eq!(status.idle, 3);
        assert_eq!(status.active, 3);
        Ok(())
    }

    #[tokio::test]
    async fn test_m7_health_check_returns_zero_for_empty_pool(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let config = PoolConfigBuilder::new().max_size(5).build()?;
        let factory = Arc::new(MockConnectionFactory);
        let pool = Pool::new(config, factory)?;

        let removed = pool.health_check().await;
        assert_eq!(removed, 0);
        Ok(())
    }

    // ==================== 生产 Bug 复现测试 ====================

    /// 可追踪创建次数的连接工厂
    struct CountingFactory {
        count: AtomicU32,
    }

    impl CountingFactory {
        fn new() -> Self {
            Self {
                count: AtomicU32::new(0),
            }
        }
        fn created_count(&self) -> u32 {
            self.count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl ConnectionFactory for CountingFactory {
        async fn create(&self) -> Result<Box<dyn Connection>, crate::DbError> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(MockConnection::new()))
        }
    }

    /// 生产 Bug 复现：release() 重置 created_at 导致连接永不过期
    ///
    /// 症状：生产环境运行 30 分钟后间歇性 "connection timeout"
    /// 根因：release() 中 created_at 被重置为 now()，max_lifetime 检查永远不触发
    /// 期望：超过 max_lifetime 的连接应被回收并创建新连接
    #[tokio::test]
    async fn test_production_bug_max_lifetime_never_expires(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 注意：PoolConfigBuilder::max_lifetime() 接受秒，这里需要毫秒级精度
        // 所以直接构造 PoolConfig
        let config = PoolConfig {
            max_size: 5,
            min_idle: 0,
            acquire_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_millis(100), // 100ms
            connection_timeout: Duration::from_secs(10),
            tls: None,
            query_timeout: None,
            max_rows: None,
            memory_limit: None,
            on_event: None,
            test_before_acquire: false,
            prewarm: false,
        };
        let factory = Arc::new(CountingFactory::new());
        let pool = Pool::new(config, factory.clone())?;

        // 1. 创建连接
        let conn = pool.acquire().await?;
        assert_eq!(factory.created_count(), 1, "应创建 1 个连接");

        // 2. 归还连接（bug：重置 created_at）
        pool.release(conn).await;

        // 3. 等待超过 max_lifetime
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 4. 再次获取 — 应检测到连接过期，创建新连接
        let conn2 = pool.acquire().await?;

        // 5. 验证：如果 bug 存在，factory.created_count() 仍为 1（连接被复用，未过期）
        //         如果修复，factory.created_count() 应为 2（旧连接过期，创建新连接）
        assert_eq!(
            factory.created_count(),
            2,
            "超过 max_lifetime 后应创建新连接（旧连接应被回收）"
        );

        pool.release(conn2).await;
        Ok(())
    }

    // ==================== PooledConnection::Drop 自动归还测试 ====================

    /// 验证 PooledConnection drop 时自动归还连接到池
    ///
    /// 修复前：PooledConnection 未实现 Drop，drop 时连接丢失，池耗尽
    /// 修复后：Drop 时 spawn 异步 release，连接自动归还
    #[tokio::test]
    async fn test_drop_auto_release_connection() -> Result<(), Box<dyn std::error::Error>> {
        let config = PoolConfigBuilder::new().max_size(2).build()?;
        let factory = Arc::new(CountingFactory::new());
        let pool = Pool::new(config, factory.clone())?;

        // 1. acquire 一个连接（不显式 release）
        {
            let _conn = pool.acquire().await?;
            assert_eq!(factory.created_count(), 1, "应创建 1 个连接");
            let status = pool.status().await;
            assert_eq!(status.active, 1, "active 应为 1");
            assert_eq!(status.idle, 0, "idle 应为 0");
            // _conn 在此 drop
        }

        // 2. 等待 Drop spawn 的异步 release 完成
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 3. 验证连接已自动归还到 idle 队列
        let status = pool.status().await;
        assert_eq!(status.idle, 1, "Drop 后连接应自动归还，idle 应为 1");
        assert_eq!(status.active, 1, "total_count 应为 1");
        assert_eq!(factory.created_count(), 1, "应复用归还的连接，不创建新连接");
        Ok(())
    }

    /// 验证 Drop 自动归还后，连接可被再次 acquire 复用
    #[tokio::test]
    async fn test_drop_auto_release_then_reuse() -> Result<(), Box<dyn std::error::Error>> {
        let config = PoolConfigBuilder::new().max_size(1).build()?;
        let factory = Arc::new(CountingFactory::new());
        let pool = Pool::new(config, factory.clone())?;

        // max_size=1，如果 Drop 不归还，第二次 acquire 会超时
        {
            let _conn = pool.acquire().await?;
        }

        // 等待 Drop spawn 的 release 完成
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 再次 acquire 应复用归还的连接，不创建新连接
        let conn = pool.acquire().await?;
        assert_eq!(factory.created_count(), 1, "应复用归还的连接，不创建新连接");

        pool.release(conn).await;
        Ok(())
    }

    /// 验证 into_inner 后 Drop 不归还（连接被消费）
    #[tokio::test]
    async fn test_into_inner_does_not_return_to_pool() -> Result<(), Box<dyn std::error::Error>> {
        let config = PoolConfigBuilder::new().max_size(2).build()?;
        let factory = Arc::new(CountingFactory::new());
        let pool = Pool::new(config, factory.clone())?;

        let conn = pool.acquire().await?;
        assert_eq!(factory.created_count(), 1);

        // into_inner 消费连接，pool 字段设为 None
        let _raw_conn = conn.into_inner();

        // 等待一段时间，确保不会有 Drop spawn
        tokio::time::sleep(Duration::from_millis(50)).await;

        let status = pool.status().await;
        assert_eq!(status.idle, 0, "into_inner 后连接不应归还");
        assert_eq!(status.active, 1, "total_count 仍为 1（连接被外部持有）");
        Ok(())
    }

    /// 验证显式 release 后 Drop 不会重复归还
    #[tokio::test]
    async fn test_explicit_release_no_double_return() -> Result<(), Box<dyn std::error::Error>> {
        let config = PoolConfigBuilder::new().max_size(2).build()?;
        let factory = Arc::new(CountingFactory::new());
        let pool = Pool::new(config, factory.clone())?;

        let conn = pool.acquire().await?;
        pool.release(conn).await;

        let status = pool.status().await;
        assert_eq!(status.idle, 1, "release 后 idle 应为 1");

        // 再次 acquire + release 验证不会重复
        let conn = pool.acquire().await?;
        pool.release(conn).await;

        let status = pool.status().await;
        assert_eq!(status.idle, 1, "再次 release 后 idle 仍应为 1（不重复）");
        assert_eq!(status.active, 1, "total_count 应为 1");
        Ok(())
    }

    // ========================================================================
    // G-SX-4：query_stream 游标流式查询测试
    // ========================================================================

    /// 带预设行数据的模拟连接，用于测试 `query_stream` 默认实现。
    struct CursorMockConn {
        rows: QueryRows,
        call_count: usize,
    }

    impl CursorMockConn {
        fn new(rows: QueryRows) -> Self {
            Self {
                rows,
                call_count: 0,
            }
        }
    }

    impl Connection for CursorMockConn {
        fn execute<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(1) })
        }

        fn query<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<QueryRows, crate::DbError>> + Send + 'a>> {
            Box::pin(async move {
                self.call_count += 1;
                Ok(self.rows.clone())
            })
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
            true
        }

        fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            Box::pin(async move { true })
        }

        fn close<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(()) })
        }
    }

    /// 模拟游标适配器：覆盖 `query_stream` 以逐行 yield，而非全量收集。
    struct CursorOverrideMockConn {
        rows: Vec<crate::value::Value>,
        yielded: usize,
    }

    impl CursorOverrideMockConn {
        fn new(rows: Vec<crate::value::Value>) -> Self {
            Self { rows, yielded: 0 }
        }
    }

    impl Connection for CursorOverrideMockConn {
        fn execute<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(1) })
        }

        fn query<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<QueryRows, crate::DbError>> + Send + 'a>> {
            // 全量收集实现（不应被 cursor override 调用）
            Box::pin(async move {
                Ok(self
                    .rows
                    .iter()
                    .map(|v| {
                        let mut m = std::collections::HashMap::new();
                        m.insert("v".to_string(), v.clone());
                        m
                    })
                    .collect())
            })
        }

        /// G-SX-4：覆盖 query_stream，逐行 yield 模拟真游标
        fn query_stream<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> Pin<Box<dyn futures::Stream<Item = QueryStreamItem> + Send + 'a>> {
            Box::pin(futures::stream::iter(
                self.rows
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        self.yielded = i + 1;
                        let mut m = std::collections::HashMap::new();
                        m.insert("v".to_string(), v.clone());
                        Ok(m)
                    })
                    .collect::<Vec<_>>(),
            ))
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
            true
        }

        fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            Box::pin(async move { true })
        }

        fn close<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(()) })
        }
    }

    /// G-SX-4 测试 1：默认 query_stream 逐行 yield 全量结果
    #[tokio::test]
    async fn test_query_stream_default_impl_yields_all_rows() {
        use futures::StreamExt;
        let rows: QueryRows = vec![
            std::collections::HashMap::from([
                ("id".to_string(), crate::value::Value::I64(1)),
                (
                    "name".to_string(),
                    crate::value::Value::String("alice".to_string()),
                ),
            ]),
            std::collections::HashMap::from([
                ("id".to_string(), crate::value::Value::I64(2)),
                (
                    "name".to_string(),
                    crate::value::Value::String("bob".to_string()),
                ),
            ]),
            std::collections::HashMap::from([
                ("id".to_string(), crate::value::Value::I64(3)),
                (
                    "name".to_string(),
                    crate::value::Value::String("carol".to_string()),
                ),
            ]),
        ];
        let mut conn = CursorMockConn::new(rows);
        let mut stream = conn.query_stream("SELECT id, name FROM users");
        let mut received: Vec<QueryStreamItem> = Vec::new();
        while let Some(item) = stream.next().await {
            received.push(item);
        }
        assert_eq!(received.len(), 3, "应收到 3 行");
        assert!(received.iter().all(|r| r.is_ok()), "所有项应为 Ok");
        drop(stream);
        assert_eq!(conn.call_count, 1, "默认实现应调用 query() 一次");
    }

    /// G-SX-4 测试 2：默认 query_stream 空结果集
    #[tokio::test]
    async fn test_query_stream_default_empty_result() {
        use futures::StreamExt;
        let mut conn = CursorMockConn::new(Vec::new());
        let mut stream = conn.query_stream("SELECT * FROM empty_table");
        let mut count = 0;
        while let Some(_item) = stream.next().await {
            count += 1;
        }
        assert_eq!(count, 0, "空结果集应产生 0 项");
    }

    /// G-SX-4 测试 3：默认 query_stream 错误传播
    #[tokio::test]
    async fn test_query_stream_default_error_propagation() {
        use futures::StreamExt;
        // 创建一个会返回错误的 mock
        struct ErrorMockConn;
        impl Connection for ErrorMockConn {
            fn execute<'a>(
                &'a mut self,
                _sql: &'a str,
            ) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>>
            {
                Box::pin(async move { Ok(1) })
            }
            fn query<'a>(
                &'a mut self,
                _sql: &'a str,
            ) -> Pin<Box<dyn Future<Output = Result<QueryRows, crate::DbError>> + Send + 'a>>
            {
                Box::pin(async move { Err(crate::DbError::Internal("query failed".to_string())) })
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
                true
            }
            fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
                Box::pin(async move { true })
            }
            fn close<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async move { Ok(()) })
            }
        }
        let mut conn = ErrorMockConn;
        let mut stream = conn.query_stream("SELECT * FROM bad_table");
        let item = stream.next().await;
        assert!(item.is_some(), "应产生一项");
        assert!(item.unwrap().is_err(), "该项应为 Err");
    }

    /// G-SX-4 测试 4：覆盖 query_stream 的适配器逐行 yield（模拟真游标）
    #[tokio::test]
    async fn test_query_stream_override_yields_rows_one_by_one() {
        use futures::StreamExt;
        let rows = vec![
            crate::value::Value::I64(10),
            crate::value::Value::I64(20),
            crate::value::Value::I64(30),
            crate::value::Value::I64(40),
            crate::value::Value::I64(50),
        ];
        let mut conn = CursorOverrideMockConn::new(rows);
        let values: Vec<i64> = {
            let mut stream = conn.query_stream("SELECT v FROM seq");
            let mut vals: Vec<i64> = Vec::new();
            while let Some(Ok(row)) = stream.next().await {
                if let crate::value::Value::I64(v) = row.get("v").unwrap() {
                    vals.push(*v);
                }
            }
            vals
        };
        assert_eq!(values, vec![10, 20, 30, 40, 50], "应按顺序收到全部 5 行");
        assert_eq!(conn.yielded, 5, "应逐行 yield 5 次（真游标覆盖）");
    }

    /// G-SX-4 测试 5：覆盖 query_stream 提前 drop 流（消费者中断）
    #[tokio::test]
    async fn test_query_stream_override_early_drop() {
        use futures::StreamExt;
        let rows = vec![
            crate::value::Value::I64(1),
            crate::value::Value::I64(2),
            crate::value::Value::I64(3),
        ];
        let mut conn = CursorOverrideMockConn::new(rows);
        {
            let mut stream = conn.query_stream("SELECT v FROM seq");
            let first = stream.next().await;
            assert!(first.is_some(), "第一项应存在");
            // 提前 drop stream — 模拟消费者中断
            drop(stream);
        }
        // 连接仍可用
        assert!(conn.is_connected(), "提前 drop 流后连接仍应可用");
    }

    /// TASK-021：连接池预热测试
    #[tokio::test]
    async fn test_pool_prewarm() -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::atomic::AtomicU32;

        // 创建可计数的连接工厂
        let create_count = Arc::new(AtomicU32::new(0));
        let create_count_clone = create_count.clone();

        struct CountingFactory {
            count: Arc<AtomicU32>,
        }

        #[async_trait]
        impl ConnectionFactory for CountingFactory {
            async fn create(&self) -> Result<Box<dyn Connection>, crate::DbError> {
                self.count.fetch_add(1, Ordering::SeqCst);
                Ok(Box::new(MockConnection::new()))
            }
        }

        // 配置：max_size=10, min_idle=5, prewarm=true
        let config = PoolConfigBuilder::new()
            .max_size(10)
            .min_idle(5)
            .prewarm(true)
            .build()?;

        let factory = Arc::new(CountingFactory {
            count: create_count_clone,
        });

        let pool = Pool::new(config, factory)?;

        // 预热前：空闲连接为 0
        let status_before = pool.status().await;
        assert_eq!(status_before.idle, 0, "预热前 idle 应为 0");

        // 执行预热
        pool.prewarm().await;

        // 预热后：空闲连接应 >= min_idle（5）
        let status_after = pool.status().await;
        assert!(
            status_after.idle >= 5,
            "预热后 idle 应 >= 5，实际: {}",
            status_after.idle
        );

        // 验证工厂被调用了 5 次（min_idle）
        assert_eq!(
            create_count.load(Ordering::SeqCst),
            5,
            "工厂应被调用 5 次（min_idle）"
        );

        Ok(())
    }

    /// TASK-021：预热失败不阻断池创建
    #[tokio::test]
    async fn test_pool_prewarm_failure_non_blocking() -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::atomic::AtomicBool;

        struct FailingFactory {
            failed: Arc<AtomicBool>,
        }

        #[async_trait]
        impl ConnectionFactory for FailingFactory {
            async fn create(&self) -> Result<Box<dyn Connection>, crate::DbError> {
                self.failed.store(true, Ordering::SeqCst);
                // 模拟连接失败
                Err(crate::DbError::Internal(
                    "simulated connection failure".to_string(),
                ))
            }
        }

        let failed = Arc::new(AtomicBool::new(false));
        let mut config = PoolConfigBuilder::new()
            .max_size(10)
            .min_idle(3)
            .prewarm(true)
            .build()?;
        config.connection_timeout = std::time::Duration::from_secs(1); // 缩短超时以加快测试

        let factory = Arc::new(FailingFactory {
            failed: failed.clone(),
        });

        // 池创建应成功（即使预热失败）
        let pool = Pool::new(config, factory)?;
        pool.prewarm().await; // 预热失败不应 panic

        // 验证工厂被调用了 3 次（尝试预热 3 个连接）
        assert!(failed.load(Ordering::SeqCst), "工厂应被调用且失败");

        // 池仍然可用（acquire 会尝试创建新连接）
        let status = pool.status().await;
        assert_eq!(status.max, 10, "池配置应正常");

        Ok(())
    }

    /// TASK-021：prewarm=false 时预热不执行
    #[tokio::test]
    async fn test_pool_prewarm_disabled() -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::atomic::AtomicU32;

        let create_count = Arc::new(AtomicU32::new(0));
        let create_count_clone = create_count.clone();

        struct CountingFactory {
            count: Arc<AtomicU32>,
        }

        #[async_trait]
        impl ConnectionFactory for CountingFactory {
            async fn create(&self) -> Result<Box<dyn Connection>, crate::DbError> {
                self.count.fetch_add(1, Ordering::SeqCst);
                Ok(Box::new(MockConnection::new()))
            }
        }

        // 配置：prewarm=false
        let config = PoolConfigBuilder::new()
            .max_size(10)
            .min_idle(5)
            .prewarm(false) // 禁用预热
            .build()?;

        let factory = Arc::new(CountingFactory {
            count: create_count_clone,
        });

        let pool = Pool::new(config, factory)?;
        pool.prewarm().await; // 应直接返回，不创建连接

        // 验证工厂未被调用
        assert_eq!(
            create_count.load(Ordering::SeqCst),
            0,
            "prewarm=false 时工厂不应被调用"
        );

        let status = pool.status().await;
        assert_eq!(status.idle, 0, "idle 应为 0");

        Ok(())
    }
}
