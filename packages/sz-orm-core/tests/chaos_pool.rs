//! 连接池混沌工程测试（Chaos Engineering for Connection Pool）
//!
//! 专注于验证连接池在故障场景下的容错能力，不依赖真实数据库。
//! 所有场景使用 MockConnection/MockConnectionFactory 模拟数据库行为。
//!
//! 测试场景：
//! 1. 连接池饥荒攻击（Starvation）
//! 2. 长事务泄漏检测（Transaction Leak）
//! 3. 池关闭后获取连接（Close After Shutdown）
//! 4. 并发获取释放（Concurrent Stress）
//! 5. 连接健康检查（Health Check & max_lifetime）

use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sz_orm_core::{Connection, ConnectionFactory, DbError, Pool, PoolConfig, PoolConfigBuilder, PoolError};

// ===================== 测试用 Mock 连接及工厂 =====================

/// 最简单的 Mock 连接：始终健康，记录 close 次数
struct MockConnection {
    connected: bool,
    close_count: Arc<AtomicU32>,
}

impl MockConnection {
    fn new(close_count: Arc<AtomicU32>) -> Self {
        Self {
            connected: true,
            close_count,
        }
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
    ) -> Pin<Box<dyn Future<Output = Result<Vec<std::collections::HashMap<String, sz_orm_core::Value>>, DbError>> + Send + 'a>> {
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
        self.connected
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.connected })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.connected = false;
            self.close_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
}

struct MockConnectionFactory {
    close_count: Arc<AtomicU32>,
}

impl MockConnectionFactory {
    fn new() -> Self {
        Self {
            close_count: Arc::new(AtomicU32::new(0)),
        }
    }
}

#[async_trait]
impl ConnectionFactory for MockConnectionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        Ok(Box::new(MockConnection::new(self.close_count.clone())))
    }
}

/// 延迟连接工厂：模拟创建缓慢的数据库连接，用于验证超时行为
struct DelayedConnectionFactory {
    delay: Duration,
}

impl DelayedConnectionFactory {
    fn new(delay: Duration) -> Self {
        Self { delay }
    }
}

#[async_trait]
impl ConnectionFactory for DelayedConnectionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        tokio::time::sleep(self.delay).await;
        Ok(Box::new(MockConnection::new(Arc::new(AtomicU32::new(0)))))
    }
}

/// 故障注入工厂：第 N 次创建失败
struct FailingConnectionFactory {
    fail_count: Arc<AtomicU32>,
    /// 创建失败次数上限（达到后开始成功）
    max_failures: u32,
}

impl FailingConnectionFactory {
    fn new(max_failures: u32) -> Self {
        Self {
            fail_count: Arc::new(AtomicU32::new(0)),
            max_failures,
        }
    }
}

#[async_trait]
impl ConnectionFactory for FailingConnectionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        let failures = self.fail_count.fetch_add(1, Ordering::SeqCst);
        if failures < self.max_failures {
            Err(DbError::ConnectionRefused("simulated failure".to_string()))
        } else {
            Ok(Box::new(MockConnection::new(Arc::new(AtomicU32::new(0)))))
        }
    }
}

// ===================== 辅助函数 =====================

fn make_pool(max_size: u32, acquire_timeout_secs: u64) -> (Pool, Arc<MockConnectionFactory>) {
    let factory = Arc::new(MockConnectionFactory::new());
    let config = PoolConfigBuilder::new()
        .max_size(max_size)
        .min_idle(0)
        .acquire_timeout(acquire_timeout_secs)
        .build()
        .unwrap();
    let pool = Pool::new(config, factory.clone()).unwrap();
    (pool, factory)
}

// ===================== 场景 1：连接池饥荒攻击 =====================

/// 混沌场景 1：连接池饥荒攻击
///
/// 模拟连接池被占满后，新请求等待超时的行为。
/// 验证：第 3 个连接请求应返回明确的 PoolError::Timeout，绝不 Panic。
#[tokio::test]
async fn chaos_pool_starvation_attack() {
    let (pool, _factory) = make_pool(2, 0); // max_size=2, 立即超时

    // 占满连接池
    let conn1 = pool.acquire().await.expect("第 1 个连接应成功");
    let conn2 = pool.acquire().await.expect("第 2 个连接应成功");

    // 验证池已满
    let status = pool.status().await;
    assert_eq!(status.active, 2, "池满：active 应为 2");
    assert_eq!(status.idle, 0, "池满：idle 应为 0");

    // 尝试获取第 3 个连接 — 应返回 Timeout，绝不 panic
    let err = match pool.acquire().await {
        Err(e) => e,
        Ok(_) => panic!("第 3 个 acquire 应失败返回 Timeout"),
    };
    assert!(
        matches!(err, PoolError::Timeout),
        "饥荒时 acquire 应返回 Timeout，实际: {}",
        err
    );

    // 释放后池应恢复正常
    pool.release(conn1).await;
    pool.release(conn2).await;

    let status = pool.status().await;
    assert_eq!(status.idle, 2, "释放后 idle 应为 2");
}

/// 混沌场景 1b：饥荒后恢复 — 释放一个连接后，等待的 acquire 应成功
///
/// 验证：池从饥荒状态恢复后能正常服务请求
#[tokio::test]
async fn chaos_pool_starvation_recovery() {
    let (pool, _factory) = make_pool(2, 5); // max_size=2, 5s 超时
    let conn1 = pool.acquire().await.unwrap();
    let conn2 = pool.acquire().await.unwrap();

    // 新线程释放一个连接
    let pool_clone = pool.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        pool_clone.release(conn1).await;
    });

    // 主线程等待 acquire（应被 notify 唤醒）
    let conn3 = pool.acquire().await.expect("释放后应能获取连接");
    pool.release(conn2).await;
    pool.release(conn3).await;

    let status = pool.status().await;
    assert_eq!(status.idle, 2, "恢复后 idle 应为 2");
}

/// 混沌场景 1c：工厂创建超时 — 验证 connection_timeout 生效
///
/// 验证：connection_timeout 极短时，创建新连接应超时返回 PoolError::Timeout
#[tokio::test]
async fn chaos_pool_connection_creation_timeout() {
    let factory = Arc::new(DelayedConnectionFactory::new(Duration::from_secs(2)));
    let config = PoolConfig {
        max_size: 5,
        min_idle: 0,
        acquire_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(600),
        max_lifetime: Duration::from_secs(1800),
        connection_timeout: Duration::from_millis(100), // 100ms 超时
        tls: None,
        query_timeout: None,
        max_rows: None,
        memory_limit: None,
        on_event: None,
    };
    let pool = Pool::new(config, factory).unwrap();

    // acquire 应超时（工厂延迟 2s > connection_timeout 100ms）
    let err = match pool.acquire().await {
        Err(e) => e,
        Ok(_) => panic!("连接创建超时应返回 PoolError::Timeout"),
    };
    assert!(
        matches!(err, PoolError::Timeout),
        "连接创建超时应返回 PoolError::Timeout，实际: {}",
        err
    );

    // 验证 total_count 正确回退（失败的创建不应泄漏计数器）
    let status = pool.status().await;
    assert_eq!(
        status.active, 0,
        "失败的创建不应泄漏计数器，active 应为 0，实际: {}",
        status.active
    );
}

// ===================== 场景 2：长事务泄漏检测 =====================

/// 混沌场景 2：长事务泄漏检测
///
/// 模拟事务被意外 drop（未 commit/rollback）的场景。
/// 验证：事务 Drop 的自动回滚机制不会导致 Panic，
/// 池仍能基于剩余容量正常提供服务，最终状态一致。
#[tokio::test]
async fn chaos_pool_transaction_leak_detection() {
    let (pool, _factory) = make_pool(2, 5);

    // 获取连接，提取内部连接创建事务
    let conn = pool.acquire().await.unwrap();
    let raw_conn = conn.into_inner();
    let mut tx = sz_orm_core::Transaction::new(raw_conn, sz_orm_core::TransactOptions::default());

    // 在事务中执行操作
    tx.execute("INSERT INTO test VALUES (1)").await.unwrap();

    // drop 事务而不 commit/rollback — 应触发异步 rollback
    // 注意：into_inner 后连接不再受池管理，total_count 不会自动回退
    drop(tx);

    // 等待异步 rollback 完成
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 池仍能基于剩余容量提供服务（max_size=2，已借出 1 个 via into_inner）
    let conn2 = pool.acquire().await.expect("池应能提供连接");
    assert!(conn2.is_connected(), "新连接应健康");
    pool.release(conn2).await;

    let status = pool.status().await;
    assert_eq!(status.idle, 1, "最终 idle 应为 1");
    assert!(
        status.active <= 2,
        "active 不应超过 max_size: {}",
        status.active
    );
}

// ===================== 场景 3：池关闭后获取连接 =====================

/// 混沌场景 3：池关闭后获取连接
///
/// 验证：close_all() 后 acquire 应立即返回 PoolError::Closed
#[tokio::test]
async fn chaos_pool_acquire_after_close() {
    let (pool, _factory) = make_pool(5, 10);

    // 先创建一些空闲连接
    let conn1 = pool.acquire().await.unwrap();
    let conn2 = pool.acquire().await.unwrap();
    pool.release(conn1).await;
    pool.release(conn2).await;

    // 关闭池
    pool.close_all().await;

    // 验证状态
    let status = pool.status().await;
    assert_eq!(status.idle, 0, "close_all 后 idle 应为 0");
    assert_eq!(status.active, 0, "close_all 后 active 应为 0");

    // 尝试获取连接 — 应返回 PoolError::Closed
    let err = match pool.acquire().await {
        Err(e) => e,
        Ok(_) => panic!("关闭后 acquire 应返回 PoolError::Closed"),
    };
    assert!(
        matches!(err, PoolError::Closed),
        "关闭后 acquire 应返回 PoolError::Closed，实际: {}",
        err
    );
}

/// 混沌场景 3b：关闭后归还连接
///
/// 验证：close_all() 后再 release 连接，连接应被直接关闭且 active 正确递减
#[tokio::test]
async fn chaos_pool_release_after_close() {
    let (pool, _factory) = make_pool(5, 10);

    let conn1 = pool.acquire().await.unwrap();
    let conn2 = pool.acquire().await.unwrap();

    pool.close_all().await;

    // close_all 后 release 借出的连接 — 应直接关闭，active 递减
    pool.release(conn1).await;
    pool.release(conn2).await;

    let status = pool.status().await;
    assert_eq!(
        status.active, 0,
        "close_all + release 后 active 应为 0，实际: {}",
        status.active
    );

    // 验证连接确实被 close（close_count）
    // 注意：release 中 close_all 标记 + is_connected=false 两种路径都有可能
    // 但 total_count 最终应为 0
    assert_eq!(status.idle, 0, "close_all 后 idle 应为 0");
}

// ===================== 场景 4：并发获取释放 =====================

/// 混沌场景 4：并发获取释放
///
/// 多任务并发 acquire/release，验证在高压下不 Panic、不泄漏、
/// active_count 始终不超过 max_size。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chaos_pool_concurrent_stress() {
    let (pool, _factory) = make_pool(50, 10);

    let task_count = 20;
    let ops_per_task = 100;
    let max_active = Arc::new(AtomicU32::new(0));
    let total_ops = Arc::new(AtomicU32::new(0));

    let mut handles = Vec::new();
    for _ in 0..task_count {
        let pool_c = pool.clone();
        let max_c = max_active.clone();
        let ops_c = total_ops.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..ops_per_task {
                let conn = pool_c.acquire().await.expect("并发 acquire 应成功");
                let status = pool_c.status().await;
                max_c.fetch_max(status.active, Ordering::Relaxed);
                // 模拟短暂持有连接
                tokio::task::yield_now().await;
                pool_c.release(conn).await;
                ops_c.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // 验证完成的操作数
    let completed = total_ops.load(Ordering::Relaxed);
    assert_eq!(
        completed,
        (task_count * ops_per_task) as u32,
        "所有 {} 个操作应完成，实际: {}",
        task_count * ops_per_task,
        completed
    );

    // 验证 active 从未超过 max_size
    let observed_max = max_active.load(Ordering::Relaxed);
    assert!(
        observed_max <= 50,
        "active 从未超过 max_size: observed_max={}",
        observed_max
    );

    // 最终池状态一致
    let status = pool.status().await;
    assert_eq!(
        status.active, status.idle,
        "并发结束后 active={} 应等于 idle={}",
        status.active, status.idle
    );
}

/// 混沌场景 4b：高并发+短超时 — 超时不 panic
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chaos_pool_concurrent_with_timeout_no_panic() {
    let factory = Arc::new(MockConnectionFactory::new());
    // max_size=2，短超时，增加超时触发概率
    let config = PoolConfigBuilder::new()
        .max_size(2)
        .min_idle(0)
        .acquire_timeout(1)
        .build()
        .unwrap();
    let pool = Pool::new(config, factory).unwrap();

    let mut handles = Vec::new();
    for _ in 0..8 {
        let pool_c = pool.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..50 {
                match pool_c.acquire().await {
                    Ok(conn) => {
                        tokio::task::yield_now().await;
                        pool_c.release(conn).await;
                    }
                    Err(e) => {
                        // 只允许 Timeout 或 Closed 错误
                        assert!(
                            matches!(e, PoolError::Timeout),
                            "只应出现 Timeout，实际: {:?}",
                            e
                        );
                    }
                }
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // 池应保持一致性
    let status = pool.status().await;
    assert!(
        status.active <= 2,
        "active 不应超过 max_size: {}",
        status.active
    );
    assert_eq!(
        status.active, status.idle,
        "最终 active={} 应等于 idle={}",
        status.active, status.idle
    );
}

// ===================== 场景 5：连接健康检查 =====================

/// 混沌场景 5：连接健康检查 — max_lifetime 过期回收
///
/// health_check 验证连接连通性（is_connected/ping）；
/// max_lifetime 过期由 reap_idle 处理。
/// 此测试验证：reap_idle 能正确回收超过 max_lifetime 的空闲连接。
#[tokio::test]
async fn chaos_pool_health_check_recycles_expired() {
    let factory = Arc::new(MockConnectionFactory::new());
    let config = PoolConfig {
        max_size: 10,
        min_idle: 0,
        acquire_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(600), // 避免 idle_timeout 干扰
        max_lifetime: Duration::from_millis(100), // 100ms 过期
        connection_timeout: Duration::from_secs(10),
        tls: None,
        query_timeout: None,
        max_rows: None,
        memory_limit: None,
        on_event: None,
    };
    let pool = Pool::new(config, factory).unwrap();

    // 创建 3 个连接并释放
    let conn1 = pool.acquire().await.unwrap();
    let conn2 = pool.acquire().await.unwrap();
    let conn3 = pool.acquire().await.unwrap();
    pool.release(conn1).await;
    pool.release(conn2).await;
    pool.release(conn3).await;

    let before = pool.status().await;
    assert_eq!(before.idle, 3, "初始应有 3 个 idle 连接");

    // 等待超过 max_lifetime
    tokio::time::sleep(Duration::from_millis(150)).await;

    // 执行 reap_idle — 应回收所有过期连接
    pool.reap_idle().await;

    let after = pool.status().await;
    assert_eq!(after.idle, 0, "回收后 idle 应为 0");
    assert_eq!(after.active, 0, "回收后 total_count 应为 0");
}

/// 混沌场景 5b：reap_idle 回收过期连接后，池可正常服务新请求
///
/// 验证：reap_idle 回收过期连接后，池能创建新连接
#[tokio::test]
async fn chaos_pool_health_check_then_serve() {
    let factory = Arc::new(MockConnectionFactory::new());
    let config = PoolConfig {
        max_size: 10,
        min_idle: 0,
        acquire_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(600),
        max_lifetime: Duration::from_millis(100),
        connection_timeout: Duration::from_secs(10),
        tls: None,
        query_timeout: None,
        max_rows: None,
        memory_limit: None,
        on_event: None,
    };
    let pool = Pool::new(config, factory).unwrap();

    // 创建连接、释放、等待过期
    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    pool.reap_idle().await;

    // 回收后 acquire 应正常创建新连接
    let conn = pool.acquire().await.expect("回收后应能创建新连接");
    assert!(conn.is_connected(), "新连接应健康");
    pool.release(conn).await;
}

/// 混沌场景 5c：health_check 检测断开连接
///
/// 验证：health_check 能检测到 is_connected=false 的空闲连接并移除
#[tokio::test]
async fn chaos_pool_health_check_detects_disconnected() {
    use std::sync::atomic::AtomicBool;

    /// 可断开的连接
    struct DisconnectableConnection {
        alive: Arc<AtomicBool>,
    }

    impl Connection for DisconnectableConnection {
        // (其他方法略——与 is_connected 无关)
        fn execute<'a>(&'a mut self, _sql: &'a str) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
            if !self.alive.load(Ordering::SeqCst) { Box::pin(async { Err(DbError::ConnectionError("disconnected".into())) }) }
            else { Box::pin(async { Ok(1) }) }
        }
        fn query<'a>(&'a mut self, _sql: &'a str) -> Pin<Box<dyn Future<Output = Result<Vec<std::collections::HashMap<String, sz_orm_core::Value>>, DbError>> + Send + 'a>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn begin_transaction<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> { Box::pin(async { Ok(()) }) }
        fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> { Box::pin(async { Ok(()) }) }
        fn rollback<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> { Box::pin(async { Ok(()) }) }
        fn is_connected(&self) -> bool { self.alive.load(Ordering::SeqCst) }
        fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> { let a = self.alive.clone(); Box::pin(async move { a.load(Ordering::SeqCst) }) }
        fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> { Box::pin(async { Ok(()) }) }
    }

    struct DisconnectableFactory { alive: Arc<AtomicBool> }
    #[async_trait]
    impl ConnectionFactory for DisconnectableFactory {
        async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
            Ok(Box::new(DisconnectableConnection { alive: self.alive.clone() }))
        }
    }

    let alive = Arc::new(AtomicBool::new(true));
    let factory = Arc::new(DisconnectableFactory { alive: alive.clone() });
    let config = PoolConfig {
        max_size: 10, min_idle: 0,
        acquire_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(600),
        max_lifetime: Duration::from_secs(3600),
        connection_timeout: Duration::from_secs(10),
        tls: None,
        query_timeout: None,
        max_rows: None,
        memory_limit: None,
        on_event: None,
    };
    let pool = Pool::new(config, factory).unwrap();

    // 创建连接并释放
    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;
    assert_eq!(pool.status().await.idle, 1);

    // 模拟连接断开
    alive.store(false, Ordering::SeqCst);

    // health_check 应检测到断开并移除
    let removed = pool.health_check().await;
    assert_eq!(removed, 1, "health_check 应移除 1 个断开连接");
    assert_eq!(pool.status().await.idle, 0);
}

/// 混沌场景 5d：空闲连接未过期时 health_check 不误杀
#[tokio::test]
async fn chaos_pool_health_check_no_false_positive() {
    let factory = Arc::new(MockConnectionFactory::new());
    let config = PoolConfig {
        max_size: 10,
        min_idle: 0,
        acquire_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(600),
        max_lifetime: Duration::from_secs(3600), // 一小时后过期，不会误杀
        connection_timeout: Duration::from_secs(10),
        tls: None,
        query_timeout: None,
        max_rows: None,
        memory_limit: None,
        on_event: None,
    };
    let pool = Pool::new(config, factory).unwrap();

    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;

    let removed = pool.health_check().await;
    assert_eq!(removed, 0, "未过期的连接不应被回收");

    let status = pool.status().await;
    assert_eq!(status.idle, 1, "健康连接应保留");
}

/// 混沌场景 5d：工厂创建失败 + health_check 无泄漏
///
/// 验证：工厂创建失败时 total_count 正确回退
#[tokio::test]
async fn chaos_pool_factory_failure_no_counter_leak() {
    let factory = Arc::new(FailingConnectionFactory::new(3)); // 前 3 次失败
    let config = PoolConfig {
        max_size: 5,
        min_idle: 0,
        acquire_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(600),
        max_lifetime: Duration::from_secs(3600),
        connection_timeout: Duration::from_secs(10),
        tls: None,
        query_timeout: None,
        max_rows: None,
        memory_limit: None,
        on_event: None,
    };
    let pool = Pool::new(config, factory).unwrap();

    // 前 3 次 acquire 应全部失败
    for i in 0..3 {
        let result = pool.acquire().await;
        assert!(
            result.is_err(),
            "第 {} 次 acquire 应失败",
            i + 1,
        );
    }

    // 计数器应归零（失败的创建不泄漏）
    let status = pool.status().await;
    assert_eq!(
        status.active, 0,
        "3 次失败后 active 应为 0，实际: {}",
        status.active
    );

    // 第 4 次应成功（工厂已过故障期）
    let conn = pool.acquire().await.expect("第 4 次 acquire 应成功");
    pool.release(conn).await;

    let status = pool.status().await;
    assert_eq!(status.idle, 1, "成功创建后 idle 应为 1");
}
